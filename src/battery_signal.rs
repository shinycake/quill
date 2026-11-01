//! Linux: follow the power-saver switch over D-Bus instead of asking for it.
//!
//! The battery watcher used to run `gdbus` every half minute (one or two
//! processes a probe, thousands a day). The desktop already announces the
//! change: the portal's `PowerProfileMonitor` and power-profiles-daemon both
//! emit `PropertiesChanged`. A worker subscribes to whichever answers,
//! keeps the latest value here and pokes [`crate::wake::BATTERY`]. While it
//! listens, [`crate::battery_saving::probe`] reads the property again over
//! the open connection ([`refresh`]) and spawns nothing: a change made
//! while the service restarted sends no signal, and the watcher's parked
//! wake corrects it. When neither service answers, or the bus closes, the
//! watcher falls back to polling.
#![cfg(all(target_os = "linux", feature = "ui"))]
#![deny(clippy::indexing_slicing)]

use crate::locks::LockExt as _;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use zbus::blocking::{Connection, MessageIterator, connection, fdo::PropertiesProxy};
use zbus::zvariant::OwnedValue;

const NOT_LISTENING: u64 = 0;
const UNKNOWN: u64 = 1;
const OFF: u64 = 2;
const ON: u64 = 3;
const VALUE: u64 = 0b11;
/// The listener stored an answer the watcher has not taken yet: the wake it
/// poked needs no read of its own.
const FRESH: u64 = 0b100;
/// One more answer from the listener.
const ANSWER: u64 = 0b1000;

/// The switch, the [`FRESH`] flag, and a count of the listener's answers
/// above them. One word, so a [`refresh`] read that went out before a
/// signal cannot overwrite the signal's newer value.
static STATE: AtomicU64 = AtomicU64::new(NOT_LISTENING);

/// The switch as last announced: `None` while nothing is subscribed, else
/// the answer (`Some(None)` when the service gave a value we cannot read).
pub fn latest() -> Option<Option<bool>> {
    decode(STATE.load(Ordering::Acquire))
}

fn decode(state: u64) -> Option<Option<bool>> {
    match state & VALUE {
        NOT_LISTENING => None,
        UNKNOWN => Some(None),
        value => Some(Some(value == ON)),
    }
}

fn encode(answer: Option<Option<bool>>) -> u64 {
    match answer {
        None => NOT_LISTENING,
        Some(None) => UNKNOWN,
        Some(Some(false)) => OFF,
        Some(Some(true)) => ON,
    }
}

fn record(answer: Option<Option<bool>>) {
    let fresh = if answer.is_some() { FRESH } else { 0 };
    let next =
        |state: u64| (state & !(VALUE | FRESH)).wrapping_add(ANSWER) | fresh | encode(answer);
    STATE.update(Ordering::AcqRel, Ordering::Acquire, next);
    crate::wake::BATTERY.poke();
}

/// The listener's connection to the property, while it is subscribed.
static OPEN: Mutex<Option<(PropertiesProxy<'static>, &'static Source)>> = Mutex::new(None);

/// Read the switch again over the listener's open connection, without a
/// process; `None` while nothing listens. Keeps the answer for [`latest`]
/// without waking the watcher, which is the one asking. Right after the
/// listener recorded an answer it is that answer, with no D-Bus call.
pub fn refresh() -> Option<Option<bool>> {
    let (properties, source) = OPEN.lock_or_recover().clone()?;
    let taken = STATE.fetch_and(!FRESH, Ordering::AcqRel);
    if taken & FRESH != 0
        && let Some(answer) = decode(taken)
    {
        return Some(answer);
    }
    let seen = taken & !FRESH;
    let answer = read(&properties, source).ok().flatten();
    let ours = (seen & !VALUE) | encode(Some(answer));
    match STATE.compare_exchange(seen, ours, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => Some(answer),
        // The listener recorded an answer while this read was out: it is
        // newer, so it stays, and the wake it poked takes it.
        Err(_) => latest().or(Some(answer)),
    }
}

fn read(properties: &PropertiesProxy<'_>, source: &Source) -> zbus::Result<Option<bool>> {
    let interface = zbus::names::InterfaceName::try_from(source.interface)?;
    Ok((source.read)(&properties.get(interface, source.property)?))
}

/// One D-Bus property that says whether the power saver is on.
struct Source {
    session: bool,
    destination: &'static str,
    path: &'static str,
    interface: &'static str,
    property: &'static str,
    read: fn(&OwnedValue) -> Option<bool>,
}

/// The portal first (what GIO's `GPowerProfileMonitor` reads), then the
/// profile daemon itself, the order `battery_saving::probe` asks in.
static SOURCES: [Source; 2] = [
    Source {
        session: true,
        destination: "org.freedesktop.portal.Desktop",
        path: "/org/freedesktop/portal/desktop",
        interface: "org.freedesktop.portal.PowerProfileMonitor",
        property: "power-saver-enabled",
        read: read_bool,
    },
    Source {
        session: false,
        destination: "net.hadess.PowerProfiles",
        path: "/net/hadess/PowerProfiles",
        interface: "net.hadess.PowerProfiles",
        property: "ActiveProfile",
        read: read_profile,
    },
];

fn read_bool(value: &OwnedValue) -> Option<bool> {
    bool::try_from(value).ok()
}

fn read_profile(value: &OwnedValue) -> Option<bool> {
    <&str>::try_from(value)
        .ok()
        .map(|profile| profile == "power-saver")
}

/// What a `PropertiesChanged` signal means for `source`: `None` when it is
/// about something else, `Some(None)` when the property was invalidated
/// (it has to be read again).
fn signal_answer(
    source: &Source,
    interface: &str,
    changed: &HashMap<String, OwnedValue>,
    invalidated: &[String],
) -> Option<Option<bool>> {
    if interface != source.interface {
        return None;
    }
    if let Some(value) = changed.get(source.property) {
        return Some((source.read)(value));
    }
    invalidated
        .iter()
        .any(|name| name == source.property)
        .then_some(None)
}

/// Start the listener once; later calls do nothing.
pub fn start() {
    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(|| {
        let _ = crate::thread_guard::spawn_guarded("quill-battery-signal", || {
            for source in &SOURCES {
                if listen(source).is_ok() {
                    break;
                }
            }
            *OPEN.lock_or_recover() = None;
            record(None);
        });
    });
}

/// A connection whose calls give up after `limit`: [`refresh`] runs on the
/// background executor, and a service that never answers would hold that
/// thread for good.
fn connect(bus: connection::Builder<'_>, limit: Duration) -> zbus::Result<Connection> {
    bus.method_timeout(limit).build()
}

/// Subscribe, read the current value, then follow every change until the
/// bus closes. Errors when the service does not answer.
fn listen(source: &'static Source) -> zbus::Result<()> {
    let bus = if source.session {
        connection::Builder::session()?
    } else {
        connection::Builder::system()?
    };
    let connection = connect(bus, crate::child_timeout::DESKTOP_QUERY_LIMIT)?;

    // Subscribe before the first read so no change is missed.
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(source.destination)?
        .path(source.path)?
        .interface("org.freedesktop.DBus.Properties")?
        .member("PropertiesChanged")?
        .build();
    let signals = MessageIterator::for_match_rule(rule, &connection, None)?;
    let properties = PropertiesProxy::builder(&connection)
        .destination(source.destination)?
        .path(source.path)?
        .build()?;
    record(Some(read(&properties, source)?));
    *OPEN.lock_or_recover() = Some((properties.clone(), source));
    for message in signals.flatten() {
        let Ok((interface, changed, invalidated)) =
            message
                .body()
                .deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>()
        else {
            continue;
        };
        match signal_answer(source, &interface, &changed, &invalidated) {
            None => {}
            Some(Some(saving)) => record(Some(Some(saving))),
            Some(None) => record(Some(read(&properties, source).ok().flatten())),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{OPEN, SOURCES, connect, latest, record, refresh, signal_answer};
    use crate::locks::LockExt as _;
    use std::collections::HashMap;
    use std::io::{BufRead as _, BufReader};
    use std::process::{Child, Command, Stdio};
    use std::sync::Mutex;
    use zbus::blocking::fdo::PropertiesProxy;
    use zbus::zvariant::{OwnedValue, Value};

    /// The tests that touch the listener's shared state take turns.
    static SERIAL: Mutex<()> = Mutex::new(());

    /// A stand-in for the portal's power profile monitor.
    struct Monitor {
        saving: bool,
    }

    #[zbus::interface(name = "org.freedesktop.portal.PowerProfileMonitor")]
    impl Monitor {
        #[zbus(property, name = "power-saver-enabled")]
        fn power_saver_enabled(&self) -> bool {
            self.saving
        }
    }

    /// A private session bus and its address.
    fn private_bus() -> (Child, String) {
        let mut daemon = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("dbus-daemon runs");
        let mut address = String::new();
        BufReader::new(daemon.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        (daemon, address.trim().to_owned())
    }

    #[test]
    fn a_parked_probe_reads_a_change_that_sent_no_signal() {
        let _serial = SERIAL.lock_or_recover();
        let (mut daemon, address) = private_bus();
        let [portal, _] = &SOURCES;
        let service = zbus::blocking::connection::Builder::address(&*address)
            .unwrap()
            .name(portal.destination)
            .unwrap()
            .serve_at(portal.path, Monitor { saving: false })
            .unwrap()
            .build()
            .unwrap();
        let watcher = zbus::blocking::connection::Builder::address(&*address)
            .unwrap()
            .build()
            .unwrap();
        let properties = PropertiesProxy::builder(&watcher)
            .destination(portal.destination)
            .unwrap()
            .path(portal.path)
            .unwrap()
            .build()
            .unwrap();
        *OPEN.lock_or_recover() = Some((properties, portal));
        assert_eq!(crate::battery_saving::probe(), Some(false));

        // The service came back from a restart with the saver on: no
        // PropertiesChanged was sent.
        service
            .object_server()
            .interface::<_, Monitor>(portal.path)
            .unwrap()
            .get_mut()
            .saving = true;
        assert_eq!(crate::battery_saving::probe(), Some(true));
        assert_eq!(latest(), Some(Some(true)));

        *OPEN.lock_or_recover() = None;
        record(None);
        assert_eq!(refresh(), None);
        let _ = daemon.kill();
        let _ = daemon.wait();
    }

    /// A portal that takes the call and never answers it.
    struct HungMonitor;

    #[zbus::interface(name = "org.freedesktop.portal.PowerProfileMonitor")]
    impl HungMonitor {
        #[zbus(property, name = "power-saver-enabled")]
        async fn power_saver_enabled(&self) -> bool {
            std::future::pending().await
        }
    }

    #[test]
    fn a_service_that_never_answers_does_not_hold_the_probe() {
        let _serial = SERIAL.lock_or_recover();
        let (mut daemon, address) = private_bus();
        let [portal, _] = &SOURCES;
        let _service = zbus::blocking::connection::Builder::address(&*address)
            .unwrap()
            .name(portal.destination)
            .unwrap()
            .serve_at(portal.path, HungMonitor)
            .unwrap()
            .build()
            .unwrap();
        let watcher = connect(
            zbus::blocking::connection::Builder::address(&*address).unwrap(),
            std::time::Duration::from_millis(200),
        )
        .unwrap();
        let properties = PropertiesProxy::builder(&watcher)
            .destination(portal.destination)
            .unwrap()
            .path(portal.path)
            .unwrap()
            .build()
            .unwrap();
        *OPEN.lock_or_recover() = Some((properties, portal));

        // Returns, with no answer, instead of waiting for the portal forever.
        assert_eq!(refresh(), Some(None));

        *OPEN.lock_or_recover() = None;
        record(None);
        let _ = daemon.kill();
        let _ = daemon.wait();
    }

    fn owned(value: Value<'_>) -> OwnedValue {
        value.try_to_owned().unwrap()
    }

    #[test]
    fn the_wake_a_signal_pokes_takes_its_value_without_a_call() {
        let _serial = SERIAL.lock_or_recover();
        let (mut daemon, address) = private_bus();
        let [portal, _] = &SOURCES;
        let _service = zbus::blocking::connection::Builder::address(&*address)
            .unwrap()
            .name(portal.destination)
            .unwrap()
            .serve_at(portal.path, Monitor { saving: false })
            .unwrap()
            .build()
            .unwrap();
        let watcher = zbus::blocking::connection::Builder::address(&*address)
            .unwrap()
            .build()
            .unwrap();
        let properties = PropertiesProxy::builder(&watcher)
            .destination(portal.destination)
            .unwrap()
            .path(portal.path)
            .unwrap()
            .build()
            .unwrap();
        *OPEN.lock_or_recover() = Some((properties, portal));

        // The signal said "on"; a call would have answered "off".
        record(Some(Some(true)));
        assert_eq!(crate::battery_saving::probe(), Some(true));
        // A later wake with no signal reads the service.
        assert_eq!(crate::battery_saving::probe(), Some(false));

        *OPEN.lock_or_recover() = None;
        record(None);
        let _ = daemon.kill();
        let _ = daemon.wait();
    }

    /// A portal whose getter reports that it was asked, then answers with
    /// whatever the test sends it.
    struct GatedMonitor {
        asked: Mutex<std::sync::mpsc::Sender<()>>,
        answer: Mutex<std::sync::mpsc::Receiver<bool>>,
    }

    #[zbus::interface(name = "org.freedesktop.portal.PowerProfileMonitor")]
    impl GatedMonitor {
        #[zbus(property, name = "power-saver-enabled")]
        fn power_saver_enabled(&self) -> bool {
            let _ = self.asked.lock_or_recover().send(());
            self.answer.lock_or_recover().recv().unwrap_or(false)
        }
    }

    #[test]
    fn a_read_that_went_out_before_a_signal_keeps_the_signals_value() {
        let _serial = SERIAL.lock_or_recover();
        let (mut daemon, address) = private_bus();
        let [portal, _] = &SOURCES;
        let (asked, was_asked) = std::sync::mpsc::channel();
        let (answer, answers) = std::sync::mpsc::channel();
        let _service = zbus::blocking::connection::Builder::address(&*address)
            .unwrap()
            .name(portal.destination)
            .unwrap()
            .serve_at(
                portal.path,
                GatedMonitor {
                    asked: Mutex::new(asked),
                    answer: Mutex::new(answers),
                },
            )
            .unwrap()
            .build()
            .unwrap();
        let watcher = zbus::blocking::connection::Builder::address(&*address)
            .unwrap()
            .build()
            .unwrap();
        let properties = PropertiesProxy::builder(&watcher)
            .destination(portal.destination)
            .unwrap()
            .path(portal.path)
            .unwrap()
            .build()
            .unwrap();
        *OPEN.lock_or_recover() = Some((properties, portal));

        // The parked probe asks; the service has the old value "off".
        let probe = std::thread::spawn(refresh);
        was_asked.recv().unwrap();
        // The saver goes on and the signal arrives before the answer does.
        record(Some(Some(true)));
        answer.send(false).unwrap();
        let _ = probe.join().unwrap();

        assert_eq!(latest(), Some(Some(true)));
        // The wake the signal poked takes "on" without asking again.
        assert_eq!(crate::battery_saving::probe(), Some(true));

        *OPEN.lock_or_recover() = None;
        record(None);
        drop(answer);
        let _ = daemon.kill();
        let _ = daemon.wait();
    }

    #[test]
    fn a_signal_wakes_the_watcher_and_the_probe_spawns_nothing() {
        let _serial = SERIAL.lock_or_recover();
        let before = crate::wake::BATTERY.epoch();
        record(Some(Some(true)));
        assert_ne!(crate::wake::BATTERY.epoch(), before);
        let spawned = std::cell::Cell::new(false);
        let answer = crate::battery_saving::probe_linux(latest(), |_| {
            spawned.set(true);
            None
        });
        assert_eq!(answer, Some(true));
        assert!(!spawned.get());
        record(None);
        assert_eq!(latest(), None);
    }

    #[test]
    fn a_portal_change_carries_the_new_value() {
        let [portal, _] = &SOURCES;
        let changed = HashMap::from([("power-saver-enabled".to_owned(), owned(true.into()))]);
        assert_eq!(
            signal_answer(portal, portal.interface, &changed, &[]),
            Some(Some(true))
        );
        assert_eq!(signal_answer(portal, "org.other", &changed, &[]), None);
    }

    #[test]
    fn a_profile_change_reads_the_profile_name() {
        let [_, daemon] = &SOURCES;
        let saver = HashMap::from([("ActiveProfile".to_owned(), owned("power-saver".into()))]);
        let balanced = HashMap::from([("ActiveProfile".to_owned(), owned("balanced".into()))]);
        assert_eq!(
            signal_answer(daemon, daemon.interface, &saver, &[]),
            Some(Some(true))
        );
        assert_eq!(
            signal_answer(daemon, daemon.interface, &balanced, &[]),
            Some(Some(false))
        );
    }

    #[test]
    fn an_invalidated_property_is_read_again() {
        let [portal, _] = &SOURCES;
        let invalidated = ["power-saver-enabled".to_owned()];
        assert_eq!(
            signal_answer(portal, portal.interface, &HashMap::new(), &invalidated),
            Some(None)
        );
        assert_eq!(
            signal_answer(portal, portal.interface, &HashMap::new(), &[]),
            None
        );
    }
}
