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
//!   can stall, so a background thread reads it once and then waits for
//!   changes; [`running`] returns the last value it stored and never blocks.
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

#[cfg(all(feature = "ui", target_os = "linux"))]
mod platform {
    use std::sync::Once;
    use std::sync::atomic::{AtomicBool, Ordering};

    static ON: AtomicBool = AtomicBool::new(false);
    static WATCH: Once = Once::new();

    pub(super) fn running() -> bool {
        // A thread that does not start leaves "no screen reader" in place.
        WATCH.call_once(|| {
            let _ = crate::thread_guard::spawn_guarded("quill-screen-reader", watch);
        });
        ON.load(Ordering::Relaxed)
    }

    /// Reads the property, then blocks on its change signal for the life of
    /// the process. Returns at once without a session bus or AT-SPI.
    fn watch() {
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
