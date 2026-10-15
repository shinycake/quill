//! caBLE advertisements from BlueZ over D-Bus (tdesktop
//! `cable_scanner_linux.cpp`): the first adapter from the object manager,
//! powered or "Bluetooth unavailable", a discovery filter for LE with
//! duplicates, `StartDiscovery`, then the `ServiceData` of every device
//! already known, added (`InterfacesAdded`) or updated
//! (`PropertiesChanged`) under FDE2 or FFF9.
//!
//! zbus is pure Rust: no libdbus or GLib is needed, only a running
//! `bluetoothd`. The scan runs on its own thread and looks at the stop
//! flag between messages.
#![deny(clippy::indexing_slicing)]

use super::ble::{ScanEvent, Scanner, is_cable_uuid};
use super::eid::ADVERT_LEN;
use crate::webauthn::log;
use futures_lite::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::Duration;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

const BLUEZ: &str = "org.bluez";
const ADAPTER: &str = "org.bluez.Adapter1";
const DEVICE: &str = "org.bluez.Device1";
/// How often the scan thread looks at the stop flag.
const POLL: Duration = Duration::from_millis(200);
/// How deep a `ServiceData` value may nest variants.
const MAX_VARIANT_DEPTH: usize = 4;

/// One scan on its own thread.
#[derive(Default)]
pub struct LinuxScanner {
    stop: Option<Arc<AtomicBool>>,
}

impl Scanner for LinuxScanner {
    fn start(&mut self, events: Sender<ScanEvent>) -> bool {
        self.stop();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let spawned = crate::thread_guard::spawn_guarded("quill-cable-scan", move || {
            zbus::block_on(scan(&events, &flag));
        });
        if spawned.is_err() {
            return false;
        }
        self.stop = Some(stop);
        true
    }

    fn stop(&mut self) {
        if let Some(stop) = self.stop.take() {
            stop.store(true, Ordering::SeqCst);
        }
    }
}

impl Drop for LinuxScanner {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The bytes of an `ay`, through any variants around it.
fn bytes_of(value: &Value<'_>, depth: usize) -> Option<Vec<u8>> {
    match value {
        Value::Value(inner) if depth < MAX_VARIANT_DEPTH => bytes_of(inner, depth + 1),
        Value::Array(array) => array
            .iter()
            .map(|item| match item {
                Value::U8(byte) => Some(*byte),
                _ => None,
            })
            .collect(),
        _ => None,
    }
}

/// The caBLE entries of a device's `ServiceData` (`a{sv}`, UUID strings
/// to byte arrays).
pub fn cable_service_data(value: &Value<'_>, depth: usize) -> Vec<Vec<u8>> {
    match value {
        Value::Value(inner) if depth < MAX_VARIANT_DEPTH => cable_service_data(inner, depth + 1),
        Value::Dict(dict) => dict
            .iter()
            .filter_map(|(uuid, data)| match uuid {
                Value::Str(uuid) if is_cable_uuid(uuid.as_str()) => bytes_of(data, 0),
                _ => None,
            })
            .filter(|data| data.len() >= ADVERT_LEN)
            .collect(),
        _ => Vec::new(),
    }
}

fn send_device(events: &Sender<ScanEvent>, properties: &HashMap<String, OwnedValue>) {
    if let Some(service_data) = properties.get("ServiceData") {
        for blob in cable_service_data(service_data, 0) {
            let _ = events.send(ScanEvent::Advert(blob));
        }
    }
}

type Interfaces = HashMap<String, HashMap<String, OwnedValue>>;

/// One BlueZ signal: a device added or its properties changed.
fn handle(events: &Sender<ScanEvent>, message: &zbus::Message) {
    let header = message.header();
    let Some(member) = header.member() else {
        return;
    };
    match member.as_str() {
        "PropertiesChanged" => {
            if let Ok((interface, changed, _)) =
                message
                    .body()
                    .deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>()
                && interface == DEVICE
            {
                send_device(events, &changed);
            }
        }
        "InterfacesAdded" => {
            if let Ok((_, interfaces)) = message
                .body()
                .deserialize::<(OwnedObjectPath, Interfaces)>()
                && let Some(device) = interfaces.get(DEVICE)
            {
                send_device(events, device);
            }
        }
        _ => {}
    }
}

async fn scan(events: &Sender<ScanEvent>, stop: &AtomicBool) {
    let tell = |available| {
        let _ = events.send(ScanEvent::Available(available));
    };
    let connection = match zbus::Connection::system().await {
        Ok(connection) => connection,
        Err(error) => {
            log(&format!("system bus: {error}"));
            return tell(false);
        }
    };
    let manager = match zbus::fdo::ObjectManagerProxy::builder(&connection)
        .destination(BLUEZ)
        .and_then(|builder| builder.path("/"))
    {
        Ok(builder) => builder.build().await,
        Err(error) => Err(error),
    };
    let objects = match manager {
        Ok(manager) => manager.get_managed_objects().await,
        Err(error) => Err(error.into()),
    };
    let objects = match objects {
        Ok(objects) => objects,
        Err(error) => {
            log(&format!("BlueZ object manager: {error}"));
            return tell(false);
        }
    };
    let mut paths: Vec<_> = objects.keys().collect();
    paths.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    let adapter = paths.iter().find_map(|path| {
        let interfaces = objects.get(*path)?;
        let adapter = interfaces
            .iter()
            .find(|(name, _)| name.as_str() == ADAPTER)?
            .1;
        Some(((*path).clone(), adapter))
    });
    let Some((adapter_path, adapter)) = adapter else {
        log("no Bluetooth adapter in BlueZ");
        return tell(false);
    };
    let powered = adapter
        .get("Powered")
        .and_then(|value| bool::try_from(value).ok())
        .unwrap_or(false);
    if !powered {
        log("the Bluetooth adapter is off");
        return tell(false);
    }
    let proxy = match zbus::Proxy::new(&connection, BLUEZ, adapter_path, ADAPTER).await {
        Ok(proxy) => proxy,
        Err(error) => {
            log(&format!("BlueZ adapter: {error}"));
            return tell(false);
        }
    };
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(BLUEZ)
        .map(|builder| builder.build());
    let stream = match rule {
        Ok(rule) => zbus::MessageStream::for_match_rule(rule, &connection, Some(256)).await,
        Err(error) => Err(error),
    };
    let mut stream = match stream {
        Ok(stream) => stream,
        Err(error) => {
            log(&format!("BlueZ signals: {error}"));
            return tell(false);
        }
    };
    let filter: HashMap<&str, Value<'_>> = HashMap::from([
        ("Transport", Value::from("le")),
        ("DuplicateData", Value::from(true)),
    ]);
    // tdesktop goes on to StartDiscovery whatever the filter call says.
    let _ = proxy.call_method("SetDiscoveryFilter", &(filter,)).await;
    if let Err(error) = proxy.call_method("StartDiscovery", &()).await {
        log(&format!("BlueZ StartDiscovery: {error}"));
        return tell(false);
    }
    tell(true);
    for interfaces in objects.values() {
        if let Some((_, device)) = interfaces.iter().find(|(name, _)| name.as_str() == DEVICE) {
            send_device(events, device);
        }
    }
    while !stop.load(Ordering::SeqCst) {
        let next = futures_lite::future::or(async { Some(stream.next().await) }, async {
            async_io::Timer::after(POLL).await;
            None
        })
        .await;
        match next {
            None => {}
            Some(None) => break,
            Some(Some(Ok(message))) => handle(events, &message),
            Some(Some(Err(_))) => {}
        }
    }
    let _ = proxy.call_method("StopDiscovery", &()).await;
}
