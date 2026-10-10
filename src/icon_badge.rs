//! Unread badge on the app/taskbar icon (parity:platform-app-icon-badge).
//!
//! Per platform, matching Telegram Desktop (`platform/*/main_window_*`):
//! - Linux: the Unity `com.canonical.Unity.LauncherEntry` `Update`
//!   signal, the de-facto standard Telegram Desktop uses on Linux, honored
//!   by Ubuntu Dock, Dash-to-Dock, and KDE's task manager. A worker thread
//!   owns a zbus session connection and emits the signal; the UI thread only
//!   drops the count into a latest-wins slot, so D-Bus can never stall it.
//!   Silent no-op when the session bus is absent (same convention as the
//!   tray module).
//! - macOS: `NSApplication.dockTile.badgeLabel` (the count text, capped
//!   "99+"; cleared at zero). Main-thread only, which the UI timer is.
//! - Windows: `ITaskbarList3::SetOverlayIcon` on the main window's taskbar
//!   button with the rendered count pill (`tray::render_overlay_icon`).
//!   The window handle is registered once by `set_native_window`.
//!
//! The count is [`tray::badge_count`] — the same `BadgePrefs`-governed
//! total as the system tray badge (TDLib's server-side totals; muted and
//! archived chats included by default, like Telegram Desktop). There is deliberately no second
//! unread definition here.
//!
//! `sync_icon_badge` is called from the 1s UI-thread timer in `main.rs`
//! and emits only when the count changes. Unread = 0 sends
//! `count-visible: false`, which also clears any stale badge left by a
//! previous run on the first tick.
//!
//! The dock resolves the badge against the app's installed desktop file;
//! Quill ships no `.desktop` file yet (packaging is a separate slice), so
//! [`APP_URI`] names the `quill.desktop` id a future install will satisfy.

use crate::state::Session;
use crate::tray;

/// App URI carried by the LauncherEntry signal: `application://` +
/// the desktop-file id docks match against.
pub const APP_URI: &str = "application://quill.desktop";

thread_local! {
    /// Last count pushed to the OS. Starts unset so the first tick always
    /// applies (clearing a stale badge when unread is 0). Only updated after
    /// a successful push, so a failure retries on the next tick.
    static LAST_SENT: std::cell::Cell<Option<u32>> = const { std::cell::Cell::new(None) };
}

/// Text for the macOS dock tile: the count capped at "99+", `None` (clear
/// the badge) when nothing is unread.
#[cfg(any(all(target_os = "macos", feature = "ui"), test))]
fn dock_badge_label(unread: u32) -> Option<String> {
    (unread > 0).then(|| tray::badge_text(unread))
}

/// Accessible description of the Windows overlay icon.
#[cfg(any(windows, test))]
fn overlay_description(unread: u32) -> String {
    match unread {
        0 => String::new(),
        n => format!("{} unread", tray::badge_text(n)),
    }
}

/// `QUILL_TRACE_STATUS=1`: print badge pushes (same switch as status notes).
fn trace(line: &str) {
    if std::env::var_os("QUILL_TRACE_STATUS").is_some() {
        eprintln!("{line}");
    }
}

/// Sync the taskbar/dock app-icon badge with the session's unread count.
/// Call from the UI thread (the 1s tray timer); emits only on change.
pub fn sync_icon_badge(session: Option<&Session>) {
    let unread = session
        .map(|s| tray::badge_count(s, &s.badge_prefs))
        .unwrap_or(0);
    LAST_SENT.with(|last| {
        if last.get() == Some(unread) {
            return;
        }
        let pushed = push_badge(unread);
        trace(&format!("icon-badge: unread={unread} pushed={pushed}"));
        if pushed {
            last.set(Some(unread));
        }
    });
}

#[cfg(target_os = "linux")]
fn push_badge(unread: u32) -> bool {
    emit_launcher_entry(unread);
    true
}

#[cfg(all(target_os = "macos", feature = "ui"))]
fn push_badge(unread: u32) -> bool {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;
    use objc2_foundation::NSString;
    request_badge_authorization();
    let Some(mtm) = MainThreadMarker::new() else {
        trace("icon-badge: not on the main thread, dock label skipped");
        return false;
    };
    let label = dock_badge_label(unread).map(|text| NSString::from_str(&text));
    let tile = NSApplication::sharedApplication(mtm).dockTile();
    tile.setBadgeLabel(label.as_deref());
    // Read back what AppKit now holds, so a trace shows whether it stuck.
    let held = tile.badgeLabel().map(|l| l.to_string());
    trace(&format!(
        "icon-badge: dock label set={label:?} readback={held:?}",
        label = dock_badge_label(unread)
    ));
    true
}

/// macOS draws a Dock badge only for apps the user allowed to badge
/// (System Settings > Notifications > Quill > "Badge application icon").
/// GPUI's own authorization request asks for alert + sound only, so ask for
/// badge as well, once, before the first label is pushed; macOS shows the
/// prompt only while the decision is still open, and an app that was already
/// decided keeps its choice (then the user must enable badges in Settings).
/// `QUILL_TRACE_STATUS=1` logs the resulting `badgeSetting`
/// (0 not supported, 1 disabled, 2 enabled).
#[cfg(all(target_os = "macos", feature = "ui"))]
fn request_badge_authorization() {
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_foundation::{NSBundle, NSError};
    use objc2_user_notifications::{
        UNAuthorizationOptions, UNNotificationSettings, UNUserNotificationCenter,
    };
    use std::ptr::NonNull;
    use std::sync::atomic::{AtomicBool, Ordering};

    static REQUESTED: AtomicBool = AtomicBool::new(false);
    if REQUESTED.swap(true, Ordering::SeqCst) {
        return;
    }
    // UNUserNotificationCenter aborts the process outside an app bundle.
    if NSBundle::mainBundle().bundleIdentifier().is_none() {
        trace("icon-badge: not an app bundle, badge authorization skipped");
        return;
    }
    let center = UNUserNotificationCenter::currentNotificationCenter();
    let reader = center.clone();
    let completion = RcBlock::new(move |granted: Bool, _error: *mut NSError| {
        trace(&format!(
            "icon-badge: authorization request finished, granted={}",
            granted.as_bool()
        ));
        let report = RcBlock::new(|settings: NonNull<UNNotificationSettings>| {
            // SAFETY: the system passes a valid settings object for the call.
            let settings = unsafe { settings.as_ref() };
            trace(&format!(
                "icon-badge: authorizationStatus={} badgeSetting={} (0 n/a, 1 disabled, 2 enabled)",
                settings.authorizationStatus().0,
                settings.badgeSetting().0
            ));
        });
        reader.getNotificationSettingsWithCompletionHandler(&report);
    });
    center.requestAuthorizationWithOptions_completionHandler(
        UNAuthorizationOptions::Badge
            | UNAuthorizationOptions::Alert
            | UNAuthorizationOptions::Sound,
        &completion,
    );
}

#[cfg(windows)]
fn push_badge(unread: u32) -> bool {
    windows_overlay::set(unread)
}

/// Core-only builds (no AppKit) and other targets have no app-icon badge.
#[cfg(not(any(target_os = "linux", all(target_os = "macos", feature = "ui"), windows)))]
fn push_badge(_unread: u32) -> bool {
    true
}

/// Windows: remember the main window so the overlay can target its taskbar
/// button. Called once after the window exists; the raw value is the HWND.
#[cfg(windows)]
pub fn set_native_window(hwnd: isize) {
    windows_overlay::set_window(hwnd);
}

#[cfg(windows)]
mod windows_overlay {
    use super::{overlay_description, tray};
    use std::cell::{Cell, RefCell};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
    use windows::Win32::UI::Shell::{ITaskbarList3, TaskbarList};
    use windows::Win32::UI::WindowsAndMessaging::{CreateIcon, DestroyIcon, HICON};
    use windows::core::PCWSTR;

    thread_local! {
        static WINDOW: Cell<isize> = const { Cell::new(0) };
        static TASKBAR: RefCell<Option<ITaskbarList3>> = const { RefCell::new(None) };
    }

    pub(super) fn set_window(hwnd: isize) {
        WINDOW.with(|w| w.set(hwnd));
    }

    /// Build an `HICON` from straight RGBA: BGRA XOR bits (32 bpp keeps the
    /// per-pixel alpha) with an all-opaque AND mask.
    fn icon_from_rgba(rgba: &[u8], width: u32, height: u32) -> windows::core::Result<HICON> {
        let mut bgra = rgba.to_vec();
        for px in bgra.as_chunks_mut::<4>().0.iter_mut() {
            px.swap(0, 2);
        }
        let and_mask = vec![0u8; (width.div_ceil(16) * 2 * height) as usize];
        // SAFETY: both buffers outlive the call and have the sizes
        // CreateIcon reads for a width x height, 1-plane, 32 bpp icon.
        unsafe {
            CreateIcon(
                None,
                width as i32,
                height as i32,
                1,
                32,
                and_mask.as_ptr(),
                bgra.as_ptr(),
            )
        }
    }

    /// Returns false when the taskbar button is not ready yet (window not
    /// registered, or the shell has not created the button), so the caller
    /// retries on the next tick.
    pub(super) fn set(unread: u32) -> bool {
        let hwnd = WINDOW.with(|w| w.get());
        if hwnd == 0 {
            return false;
        }
        let hwnd = HWND(hwnd as *mut _);
        TASKBAR.with(|slot| {
            let mut slot = slot.borrow_mut();
            if slot.is_none() {
                // SAFETY: plain COM activation on the UI thread, where GPUI
                // has already initialized COM.
                let list: windows::core::Result<ITaskbarList3> =
                    unsafe { CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER) };
                let Ok(list) = list else {
                    return false;
                };
                // SAFETY: freshly created, valid interface.
                if unsafe { list.HrInit() }.is_err() {
                    return false;
                }
                *slot = Some(list);
            }
            let Some(list) = slot.as_ref() else {
                return false;
            };
            let description: Vec<u16> = overlay_description(unread)
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            if unread == 0 {
                // SAFETY: a null icon removes the overlay.
                return unsafe {
                    list.SetOverlayIcon(hwnd, HICON::default(), PCWSTR(description.as_ptr()))
                }
                .is_ok();
            }
            let (rgba, w, h) = tray::render_overlay_icon(unread);
            let Ok(icon) = icon_from_rgba(&rgba, w, h) else {
                return false;
            };
            // SAFETY: `icon` and `description` are valid for the call; the
            // taskbar copies the icon, so it is destroyed right after.
            let ok =
                unsafe { list.SetOverlayIcon(hwnd, icon, PCWSTR(description.as_ptr())) }.is_ok();
            // SAFETY: `icon` was created above and is no longer referenced.
            let _ = unsafe { DestroyIcon(icon) };
            ok
        })
    }
}

/// Latest-wins mailbox between the UI thread and the D-Bus worker: a burst
/// of unread changes while the worker is busy collapses to the newest count.
#[cfg(any(all(target_os = "linux", feature = "ui"), test))]
struct LatestSlot<T> {
    value: std::sync::Mutex<Option<T>>,
    ready: std::sync::Condvar,
}

#[cfg(any(all(target_os = "linux", feature = "ui"), test))]
impl<T> LatestSlot<T> {
    const fn new() -> Self {
        Self {
            value: std::sync::Mutex::new(None),
            ready: std::sync::Condvar::new(),
        }
    }

    /// Replace any pending value; never blocks on the consumer.
    fn put(&self, value: T) {
        *self.value.lock().unwrap_or_else(|e| e.into_inner()) = Some(value);
        self.ready.notify_one();
    }

    /// Block until a value is pending, then take it.
    fn take(&self) -> T {
        let mut guard = self.value.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some(value) = guard.take() {
                return value;
            }
            guard = self.ready.wait(guard).unwrap_or_else(|e| e.into_inner());
        }
    }
}

#[cfg(all(target_os = "linux", feature = "ui"))]
static LAUNCHER_SLOT: LatestSlot<u32> = LatestSlot::new();

/// Hands the count to a worker thread that owns the session-bus connection
/// and emits the signal, so the UI thread never waits on D-Bus. The worker
/// starts on first use; any failure (no session bus, no listening dock) is
/// silently ignored and retried on the next count change.
#[cfg(target_os = "linux")]
fn emit_launcher_entry(unread: u32) {
    #[cfg(feature = "ui")]
    {
        use std::sync::Once;
        static WORKER: Once = Once::new();
        WORKER.call_once(|| {
            let _ = std::thread::Builder::new()
                .name("quill-launcher-badge".into())
                .spawn(|| {
                    let mut connection: Option<zbus::blocking::Connection> = None;
                    loop {
                        let count = LAUNCHER_SLOT.take();
                        if connection.is_none() {
                            connection = zbus::blocking::Connection::session().ok();
                        }
                        let Some(conn) = &connection else { continue };
                        let mut props: std::collections::HashMap<&str, zbus::zvariant::Value> =
                            std::collections::HashMap::new();
                        props.insert("count", zbus::zvariant::Value::from(i64::from(count)));
                        props.insert("count-visible", zbus::zvariant::Value::from(count > 0));
                        let sent = conn.emit_signal(
                            None::<&str>,
                            "/",
                            "com.canonical.Unity.LauncherEntry",
                            "Update",
                            &(APP_URI, props),
                        );
                        if sent.is_err() {
                            connection = None;
                        }
                    }
                });
        });
        LAUNCHER_SLOT.put(unread);
    }
    #[cfg(not(feature = "ui"))]
    let _ = unread;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_slot_keeps_only_the_newest_value() {
        let slot = LatestSlot::new();
        slot.put(1u32);
        slot.put(2);
        slot.put(3);
        assert_eq!(slot.take(), 3);
    }

    #[test]
    fn latest_slot_wakes_a_blocked_consumer() {
        let slot = std::sync::Arc::new(LatestSlot::new());
        let reader = {
            let slot = slot.clone();
            std::thread::spawn(move || slot.take())
        };
        std::thread::sleep(std::time::Duration::from_millis(20));
        slot.put(9u32);
        assert_eq!(reader.join().unwrap(), 9);
    }

    #[test]
    fn dock_label_clears_at_zero_and_caps_at_99_plus() {
        assert_eq!(dock_badge_label(0), None);
        assert_eq!(dock_badge_label(7).as_deref(), Some("7"));
        assert_eq!(dock_badge_label(99).as_deref(), Some("99"));
        assert_eq!(dock_badge_label(100).as_deref(), Some("99+"));
        assert_eq!(dock_badge_label(u32::MAX).as_deref(), Some("99+"));
    }

    #[test]
    fn overlay_description_follows_the_badge_text() {
        assert_eq!(overlay_description(0), "");
        assert_eq!(overlay_description(3), "3 unread");
        assert_eq!(overlay_description(250), "99+ unread");
    }
}
