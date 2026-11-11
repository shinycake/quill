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

    /// How long after a watch that never reached the bus the next may start;
    /// it doubles with each miss in a row, up to `MAX_DOUBLINGS` times.
    pub(super) const RETRY_AFTER: Duration = Duration::from_secs(10);
    const MAX_DOUBLINGS: u32 = 6;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(super) enum Watch {
        /// Nothing started yet.
        Idle,
        /// A thread is reading the property and waiting for its changes;
        /// `misses` is the run of misses it follows.
        Watching { misses: u32 },
        /// The watch ended (no bus, AT-SPI gone, thread did not start) at
        /// this instant; `misses` watches in a row ended without ever
        /// reading the value.
        Ended { at: Instant, misses: u32 },
    }

    impl Watch {
        /// Whether a watch should start at `now`; when it should, the state
        /// is `Watching` from here on.
        pub(super) fn start_if_due(&mut self, now: Instant) -> bool {
            let due = match *self {
                Watch::Idle => true,
                Watch::Watching { .. } => false,
                Watch::Ended { at, misses } => {
                    now.saturating_duration_since(at)
                        >= RETRY_AFTER * (1 << misses.min(MAX_DOUBLINGS))
                }
            };
            if due {
                let misses = match *self {
                    Watch::Ended { misses, .. } => misses,
                    Watch::Idle | Watch::Watching { .. } => 0,
                };
                *self = Watch::Watching { misses };
            }
            due
        }

        /// The watch ended; `reached` is whether it read the value first.
        pub(super) fn ended(&mut self, now: Instant, reached: bool) {
            let misses = match *self {
                _ if reached => 0,
                Watch::Watching { misses } | Watch::Ended { misses, .. } => {
                    misses.saturating_add(1)
                }
                Watch::Idle => 1,
            };
            *self = Watch::Ended { at: now, misses };
        }
    }

    /// Ends a watch when it goes out of scope, so a watcher thread that
    /// panics (the thread guard catches that) or returns early still moves
    /// the stage out of `Watching`.
    pub(super) struct EndGuard<F: FnMut(bool)> {
        reached: bool,
        on_end: F,
    }

    impl<F: FnMut(bool)> EndGuard<F> {
        pub(super) fn new(on_end: F) -> Self {
            Self {
                reached: false,
                on_end,
            }
        }

        /// The value was read before the watch ended.
        pub(super) fn reached(&mut self) {
            self.reached = true;
        }
    }

    impl<F: FnMut(bool)> Drop for EndGuard<F> {
        fn drop(&mut self) {
            (self.on_end)(self.reached);
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{EndGuard, RETRY_AFTER, Watch};
        use std::time::Instant;

        #[test]
        fn a_watcher_that_panics_still_ends_its_watch() {
            let t0 = Instant::now();
            let mut watch = Watch::Idle;
            assert!(watch.start_if_due(t0));
            let ended = std::cell::Cell::new(None);
            let died = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _guard = EndGuard::new(|reached| ended.set(Some(reached)));
                std::panic::resume_unwind(Box::new("the watcher died"));
            }));
            assert!(died.is_err());
            assert_eq!(ended.get(), Some(false), "the guard ended it, unreached");
            let read = std::cell::Cell::new(None);
            {
                let mut guard = EndGuard::new(|reached| read.set(Some(reached)));
                guard.reached();
            }
            assert_eq!(read.get(), Some(true), "a read before the end is kept");
            watch.ended(t0, ended.get().unwrap_or(false));
            assert!(watch.start_if_due(t0 + RETRY_AFTER * 2), "asked again");
        }

        #[test]
        fn a_watch_that_ended_starts_again_after_the_pause() {
            let t0 = Instant::now();
            let mut watch = Watch::Idle;
            assert!(watch.start_if_due(t0), "the first call starts it");
            assert!(!watch.start_if_due(t0), "one at a time");
            watch.ended(t0, true);
            assert!(
                !watch.start_if_due(t0 + RETRY_AFTER / 2),
                "not hammering the bus"
            );
            assert!(watch.start_if_due(t0 + RETRY_AFTER), "then it tries again");
            assert_eq!(watch, Watch::Watching { misses: 0 });
        }

        #[test]
        fn a_bus_that_stays_away_is_tried_less_and_less_often() {
            let t0 = Instant::now();
            let mut watch = Watch::Idle;
            assert!(watch.start_if_due(t0));
            watch.ended(t0, false);
            assert!(watch.start_if_due(t0 + RETRY_AFTER * 2), "after one miss");
            watch.ended(t0, false);
            assert!(
                !watch.start_if_due(t0 + RETRY_AFTER * 2),
                "two misses wait longer"
            );
            assert!(watch.start_if_due(t0 + RETRY_AFTER * 4));
            watch.ended(t0, true);
            assert!(watch.start_if_due(t0 + RETRY_AFTER), "a read resets it");
        }
    }
}

#[cfg(all(feature = "ui", target_os = "linux"))]
mod platform {
    use super::watcher::{EndGuard, Watch};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Instant;

    static ON: AtomicBool = AtomicBool::new(false);
    static WATCH: Mutex<Watch> = Mutex::new(Watch::Idle);

    pub(super) fn running() -> bool {
        if crate::locks::lock_or_recover(&WATCH).start_if_due(Instant::now())
            && crate::thread_guard::spawn_guarded("quill-screen-reader", watch).is_err()
        {
            crate::locks::lock_or_recover(&WATCH).ended(Instant::now(), false);
        }
        ON.load(Ordering::Relaxed)
    }

    /// Reads the property, then blocks on its change signal until the bus
    /// goes away. Returns at once without a session bus or AT-SPI; either
    /// way [`running`] starts another watch after a pause.
    fn watch() {
        // However the thread ends, even by a panic, the guard ends the
        // watch, so `running()` starts another after the pause.
        let mut end = EndGuard::new(|reached| {
            // The value can no longer be trusted: the next watch reads it again.
            ON.store(false, Ordering::Relaxed);
            crate::locks::lock_or_recover(&WATCH).ended(Instant::now(), reached);
        });
        read_and_follow(&mut end);
    }

    /// Reads the property (`end.reached()` once it could be: the bus and
    /// AT-SPI were there), then follows its changes.
    fn read_and_follow(end: &mut EndGuard<impl FnMut(bool)>) {
        let Some(proxy) = proxy() else {
            return;
        };
        if let Ok(on) = proxy.get_property::<bool>("ScreenReaderEnabled") {
            ON.store(on, Ordering::Relaxed);
            end.reached();
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
