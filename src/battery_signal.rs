//! Linux: follow the power-saver switch over D-Bus instead of asking for it.
//!
//! The battery watcher used to run `gdbus` every half minute (one or two
//! processes a probe, thousands a day). The desktop already announces the
//! change: the portal's `PowerProfileMonitor` and power-profiles-daemon both
//! emit `PropertiesChanged`. A worker subscribes to whichever answers,
//! keeps the latest value here and pokes [`crate::wake::BATTERY`]. While it
//! listens, [`crate::battery_saving::probe`] reads that value and spawns
//! nothing; when neither service answers, or the bus closes, the watcher
//! falls back to polling.
#![cfg(all(target_os = "linux", feature = "ui"))]
#![deny(clippy::indexing_slicing)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use zbus::blocking::{Connection, MessageIterator, fdo::PropertiesProxy};
use zbus::zvariant::OwnedValue;

const NOT_LISTENING: u32 = 0;
const UNKNOWN: u32 = 1;
const OFF: u32 = 2;
const ON: u32 = 3;

static LATEST: AtomicU32 = AtomicU32::new(NOT_LISTENING);

/// The switch as last announced: `None` while nothing is subscribed, else
/// the answer (`Some(None)` when the service gave a value we cannot read).
pub fn latest() -> Option<Option<bool>> {
    match LATEST.load(Ordering::Acquire) {
        NOT_LISTENING => None,
        UNKNOWN => Some(None),
        value => Some(Some(value == ON)),
    }
}

fn record(answer: Option<Option<bool>>) {
    let value = match answer {
        None => NOT_LISTENING,
        Some(None) => UNKNOWN,
        Some(Some(false)) => OFF,
        Some(Some(true)) => ON,
    };
    LATEST.store(value, Ordering::Release);
    crate::wake::BATTERY.poke();
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
const SOURCES: [Source; 2] = [
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
            record(None);
        });
    });
}

/// Subscribe, read the current value, then follow every change until the
/// bus closes. Errors when the service does not answer.
fn listen(source: &Source) -> zbus::Result<()> {
    let connection = if source.session {
        Connection::session()?
    } else {
        Connection::system()?
    };
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
    let read = |properties: &PropertiesProxy| -> zbus::Result<Option<bool>> {
        let interface = zbus::names::InterfaceName::try_from(source.interface)?;
        Ok((source.read)(&properties.get(interface, source.property)?))
    };
    record(Some(read(&properties)?));
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
            Some(None) => record(Some(read(&properties).ok().flatten())),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::Value;

    fn owned(value: Value<'_>) -> OwnedValue {
        value.try_to_owned().unwrap()
    }

    #[test]
    fn a_signal_wakes_the_watcher_and_the_probe_spawns_nothing() {
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
