//! Whether a screen reader is running, as the OS reports it.
//!
//! GPUI's accessibility tree goes live whenever any accessibility client
//! asks the window for it, and many that are not screen readers do: on
//! macOS `AXVisualSupportAgent`, window managers and launchers; on Linux
//! anything on the AT-SPI bus. So "the tree is active" cannot stand for "a
//! screen reader is reading". This asks the OS for the screen reader itself:
//!
//! - macOS: `NSWorkspace.isVoiceOverEnabled` or `isSwitchControlEnabled`
//!   (Switch Control drives the window through the same tree).
//! - Windows: `SPI_GETSCREENREADER`, which Narrator, NVDA and JAWS set.
//! - Linux: AT-SPI's `org.a11y.Status.ScreenReaderEnabled` on the session
//!   bus, which GNOME's and KDE's screen reader switches set. A D-Bus call
//!   can stall, so a background thread reads it and then waits for changes
//!   (starting again, after a pause, when the bus was not there or went
//!   away); [`running`] returns the last value it stored and never blocks.
//!
//! `QUILL_SCREEN_READER=1` (or `0`) overrides the OS for testing.
#![deny(clippy::indexing_slicing)]

/// Whether a screen reader (or Switch Control) is on.
pub fn running() -> bool {
    if let Some(forced) = env_override() {
        return forced;
    }
    platform::running()
}

fn env_override() -> Option<bool> {
    let value = std::env::var("QUILL_SCREEN_READER").ok()?;
    Some(value == "1" || value.eq_ignore_ascii_case("true"))
}

#[cfg(all(feature = "ui", target_os = "macos"))]
mod platform {
    use objc2_app_kit::NSWorkspace;

    pub(super) fn running() -> bool {
        // NSWorkspace is documented as safe to use from any thread; agent
        // tools read this off the main thread.
        let workspace = NSWorkspace::sharedWorkspace();
        workspace.isVoiceOverEnabled() || workspace.isSwitchControlEnabled()
    }
}

#[cfg(windows)]
mod platform {
    pub(super) fn running() -> bool {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SPI_GETSCREENREADER, SystemParametersInfoW,
        };
        let mut on: i32 = 0;
        // SAFETY: SPI_GETSCREENREADER writes one BOOL to the pointer.
        let ok = unsafe {
            SystemParametersInfoW(SPI_GETSCREENREADER, 0, (&mut on as *mut i32).cast(), 0)
        };
        ok != 0 && on != 0
    }
}

/// The Linux watcher's lifecycle. `running()` starts it when it is not
/// running and the last attempt is old enough, so a session bus that was not
/// up yet, or that restarted, is picked up on a later call instead of never.
#[cfg(any(all(feature = "ui", target_os = "linux"), test))]
mod watcher {
    use std::time::{Duration, Instant};

    /// How long after a failed or ended watch the next may start.
    pub(super) const RETRY_AFTER: Duration = Duration::from_secs(10);

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(super) enum Watch {
        /// Nothing started yet.
        Idle,
        /// A thread is reading the property and waiting for its changes.
        Watching,
        /// The watch ended (no bus, AT-SPI gone, thread did not start) at
        /// this instant.
        Ended(Instant),
    }

    impl Watch {
        /// Whether a watch should start at `now`; when it should, the state
        /// is `Watching` from here on.
        pub(super) fn start_if_due(&mut self, now: Instant) -> bool {
            let due = match *self {
                Watch::Idle => true,
                Watch::Watching => false,
                Watch::Ended(at) => now.saturating_duration_since(at) >= RETRY_AFTER,
            };
            if due {
                *self = Watch::Watching;
            }
            due
        }

        pub(super) fn ended(&mut self, now: Instant) {
            *self = Watch::Ended(now);
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{RETRY_AFTER, Watch};
        use std::time::Instant;

        #[test]
        fn a_watch_that_ended_starts_again_after_the_pause() {
            let t0 = Instant::now();
            let mut watch = Watch::Idle;
            assert!(watch.start_if_due(t0), "the first call starts it");
            assert!(!watch.start_if_due(t0), "one at a time");
            watch.ended(t0);
            assert!(
                !watch.start_if_due(t0 + RETRY_AFTER / 2),
                "not hammering the bus"
            );
            assert!(watch.start_if_due(t0 + RETRY_AFTER), "then it tries again");
            assert_eq!(watch, Watch::Watching);
        }
    }
}

#[cfg(all(feature = "ui", target_os = "linux"))]
mod platform {
    use super::watcher::Watch;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Instant;

    static ON: AtomicBool = AtomicBool::new(false);
    static WATCH: Mutex<Watch> = Mutex::new(Watch::Idle);

    pub(super) fn running() -> bool {
        if crate::locks::lock_or_recover(&WATCH).start_if_due(Instant::now())
            && crate::thread_guard::spawn_guarded("quill-screen-reader", watch).is_err()
        {
            crate::locks::lock_or_recover(&WATCH).ended(Instant::now());
        }
        ON.load(Ordering::Relaxed)
    }

    /// Reads the property, then blocks on its change signal until the bus
    /// goes away. Returns at once without a session bus or AT-SPI; either
    /// way [`running`] starts another watch after a pause.
    fn watch() {
        read_and_follow();
        // The value can no longer be trusted: the next watch reads it again.
        ON.store(false, Ordering::Relaxed);
        crate::locks::lock_or_recover(&WATCH).ended(Instant::now());
    }

    fn read_and_follow() {
        let Some(proxy) = proxy() else {
            return;
        };
        if let Ok(on) = proxy.get_property::<bool>("ScreenReaderEnabled") {
            ON.store(on, Ordering::Relaxed);
        }
        for change in proxy.receive_property_changed::<bool>("ScreenReaderEnabled") {
            if let Ok(on) = change.get() {
                ON.store(on, Ordering::Relaxed);
            }
        }
    }

    fn proxy() -> Option<zbus::blocking::Proxy<'static>> {
        let bus = zbus::blocking::Connection::session().ok()?;
        zbus::blocking::Proxy::new_owned(bus, "org.a11y.Bus", "/org/a11y/bus", "org.a11y.Status")
            .ok()
    }
}

#[cfg(not(any(
    windows,
    all(feature = "ui", any(target_os = "macos", target_os = "linux"))
)))]
mod platform {
    pub(super) fn running() -> bool {
        false
    }
}
