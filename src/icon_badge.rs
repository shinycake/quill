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
//! The count is [`crate::tray::badge_count`] summed over every account
//! (`notify_accounts::total_unread`) — the same `BadgePrefs`-governed
//! total as the system tray badge (TDLib's server-side totals; muted and
//! archived chats included by default, like Telegram Desktop). There is deliberately no second
//! unread definition here.
//!
//! `sync_icon_badge` is called from the 1s UI-thread timer in `main.rs`
//! and emits only when the count changes. Unread = 0 sends
//! `count-visible: false`, which also clears any stale badge left by a
//! previous run on the first tick.
//!
//! The dock resolves the badge against the app's installed desktop file,
//! named by [`app_uri`].
#![deny(clippy::indexing_slicing)]

use crate::state::Session;

/// App URI carried by the LauncherEntry signal: `application://` +
/// the desktop-file id docks match against.
pub fn app_uri() -> String {
    format!(
        "application://{}",
        crate::desktop_app_id::desktop_file_name()
    )
}

/// How long a failed push waits before the same count is pushed again, so a
/// missing session bus (or a taskbar button that does not exist yet) does not
/// wake the worker on every pass.
const FAILED_RETRY_AFTER: std::time::Duration = std::time::Duration::from_secs(10);

/// What the dock shows, from the UI's point of view: one stage for every
/// platform. The count the UI wants is asked of the platform through
/// [`BadgeStage::request`]; a platform that answers later (the Linux
/// worker) settles it with [`BadgeStage::answered`], so a bus that is down
/// or a failed emit is asked again on the next tick instead of leaving the
/// badge wrong until the count changes. It starts `Idle`, so the first tick
/// always pushes (clearing a stale badge when unread is 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BadgeStage {
    /// Nothing shown that we know of, or the OS may have lost it.
    Idle,
    /// Handed to the platform, not answered.
    Pending(u32),
    /// The platform showed it.
    Delivered(u32),
    /// The platform could not show it at `at`: the same count is asked again
    /// after [`FAILED_RETRY_AFTER`], a different one at once.
    Failed { count: u32, at: std::time::Instant },
}

impl BadgeStage {
    /// The UI wants `unread` shown at `now`: whether to push it now. A count
    /// already shown, or in flight, or one that just failed, is not pushed
    /// again.
    fn request(&mut self, unread: u32, now: std::time::Instant) -> bool {
        match *self {
            BadgeStage::Delivered(shown) if shown == unread => false,
            BadgeStage::Pending(asked) if asked == unread => false,
            BadgeStage::Failed { count, at }
                if count == unread && now.saturating_duration_since(at) < FAILED_RETRY_AFTER =>
            {
                false
            }
            BadgeStage::Idle
            | BadgeStage::Pending(_)
            | BadgeStage::Delivered(_)
            | BadgeStage::Failed { .. } => {
                *self = BadgeStage::Pending(unread);
                true
            }
        }
    }

    /// The platform's answer for `count`; a newer request that overtook it
    /// stays pending.
    fn answered(&mut self, count: u32, shown: bool, now: std::time::Instant) {
        if *self == BadgeStage::Pending(count) {
            *self = if shown {
                BadgeStage::Delivered(count)
            } else {
                BadgeStage::Failed { count, at: now }
            };
        }
    }

    /// The platform can no longer answer what is in flight (its worker is
    /// gone): ask again after the wait.
    #[cfg(any(all(target_os = "linux", feature = "ui"), test))]
    fn abandoned(&mut self, now: std::time::Instant) {
        if let BadgeStage::Pending(count) = *self {
            *self = BadgeStage::Failed { count, at: now };
        }
    }

    /// The OS may have lost the badge: forget that anything was shown, so
    /// the next `request` pushes the count again even if it is equal. A
    /// count still in flight stays in flight.
    fn invalidate(&mut self) {
        if matches!(*self, BadgeStage::Delivered(_)) {
            *self = BadgeStage::Idle;
        }
    }
}

static BADGE: std::sync::Mutex<BadgeStage> = std::sync::Mutex::new(BadgeStage::Idle);

/// How a platform took a push.
#[allow(dead_code, reason = "each target returns only some of the variants")]
enum Push {
    /// Shown (or nothing to show on this target).
    Done,
    /// Handed to a worker that settles it through `BADGE`.
    Handed,
    /// Could not be shown now: asked again after the retry wait.
    Failed,
}

/// Text for the macOS dock tile: the count capped at "99+", `None` (clear
/// the badge) when nothing is unread.
#[cfg(any(all(target_os = "macos", feature = "ui"), test))]
fn dock_badge_label(unread: u32) -> Option<String> {
    (unread > 0).then(|| crate::tray::badge_text(unread))
}

/// Accessible description of the Windows overlay icon.
#[cfg(any(windows, test))]
fn overlay_description(unread: u32) -> String {
    match unread {
        0 => String::new(),
        n => format!("{} unread", crate::tray::badge_text(n)),
    }
}

/// `QUILL_TRACE_STATUS=1`: print badge pushes (same switch as status notes).
fn trace(line: &str) {
    if std::env::var_os("QUILL_TRACE_STATUS").is_some() {
        eprintln!("{line}");
    }
}

/// Sync the taskbar/dock app-icon badge with the unread count of every
/// account (tdesktop `Domain::unreadBadge`). Call from the UI thread (the 1s
/// tray timer); emits only on change.
pub fn sync_icon_badge(session: Option<&Session>) {
    let unread = crate::notify_accounts::total_unread(session).count;
    sync_with(&BADGE, unread, std::time::Instant::now(), push_badge);
}

/// The OS may have lost or reset the badge without telling us (the app
/// came back to the front, the account on screen changed, the user
/// toggled badges in system settings), so the next sync pushes the count
/// again even if it equals the last one pushed. The stage only dedupes.
pub fn invalidate_icon_badge() {
    crate::locks::lock_or_recover(&BADGE).invalidate();
    crate::wake::CHROME.poke();
}

fn sync_with(
    badge: &std::sync::Mutex<BadgeStage>,
    unread: u32,
    now: std::time::Instant,
    push: impl FnOnce(u32) -> Push,
) {
    if !crate::locks::lock_or_recover(badge).request(unread, now) {
        return;
    }
    // The push runs without the lock: a worker answers through it.
    let pushed = push(unread);
    trace(&format!(
        "icon-badge: unread={unread} pushed={}",
        !matches!(pushed, Push::Failed)
    ));
    match pushed {
        Push::Done => crate::locks::lock_or_recover(badge).answered(unread, true, now),
        Push::Failed => crate::locks::lock_or_recover(badge).answered(unread, false, now),
        Push::Handed => {}
    }
}

#[cfg(target_os = "linux")]
fn push_badge(unread: u32) -> Push {
    emit_launcher_entry(unread)
}

#[cfg(all(target_os = "macos", feature = "ui"))]
fn push_badge(unread: u32) -> Push {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;
    use objc2_foundation::NSString;
    request_badge_authorization();
    let Some(mtm) = MainThreadMarker::new() else {
        trace("icon-badge: not on the main thread, dock label skipped");
        return Push::Failed;
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
    Push::Done
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
fn push_badge(unread: u32) -> Push {
    if windows_overlay::set(unread) {
        Push::Done
    } else {
        Push::Failed
    }
}

/// Core-only builds (no AppKit) and other targets have no app-icon badge.
#[cfg(not(any(target_os = "linux", all(target_os = "macos", feature = "ui"), windows)))]
fn push_badge(_unread: u32) -> Push {
    Push::Done
}

/// Windows: remember the main window so the overlay can target its taskbar
/// button. Called once after the window exists; the raw value is the HWND.
#[cfg(windows)]
pub fn set_native_window(hwnd: isize) {
    windows_overlay::set_window(hwnd);
}

#[cfg(windows)]
mod windows_overlay {
    use super::overlay_description;
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
            // A COM call that came back into this one finds the cell taken:
            // the next tick asks again.
            let Ok(mut slot) = slot.try_borrow_mut() else {
                return false;
            };
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
            let (rgba, w, h) = crate::tray::render_overlay_icon(unread);
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
pub(crate) struct LatestSlot<T> {
    value: std::sync::Mutex<Option<T>>,
    ready: std::sync::Condvar,
}

#[cfg(any(all(target_os = "linux", feature = "ui"), test))]
impl<T> LatestSlot<T> {
    pub(crate) const fn new() -> Self {
        Self {
            value: std::sync::Mutex::new(None),
            ready: std::sync::Condvar::new(),
        }
    }

    /// Replace any pending value; never blocks on the consumer.
    pub(crate) fn put(&self, value: T) {
        *crate::locks::lock_or_recover(&self.value) = Some(value);
        self.ready.notify_one();
    }

    /// Block until a value is pending, then take it.
    pub(crate) fn take(&self) -> T {
        let mut guard = crate::locks::lock_or_recover(&self.value);
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

/// The launcher worker, while it runs.
#[cfg(all(target_os = "linux", feature = "ui"))]
static LAUNCHER_WORKER: std::sync::Mutex<Option<std::thread::JoinHandle<Option<()>>>> =
    std::sync::Mutex::new(None);

/// Settles what the worker had in flight when it ends, normal or by a panic
/// that `spawn_guarded` catches: the badge is asked again on the next tick.
#[cfg(any(all(target_os = "linux", feature = "ui"), test))]
struct WorkerEnd<'a>(&'a std::sync::Mutex<BadgeStage>);

#[cfg(any(all(target_os = "linux", feature = "ui"), test))]
impl Drop for WorkerEnd<'_> {
    fn drop(&mut self) {
        crate::locks::lock_or_recover(self.0).abandoned(std::time::Instant::now());
    }
}

/// Whether the launcher worker runs, starting it when it does not (never
/// started, died by a panic, or the thread could not be spawned earlier).
#[cfg(all(target_os = "linux", feature = "ui"))]
fn launcher_worker_runs() -> bool {
    let mut worker = crate::locks::lock_or_recover(&LAUNCHER_WORKER);
    if worker.as_ref().is_some_and(|handle| !handle.is_finished()) {
        return true;
    }
    *worker = crate::thread_guard::spawn_guarded("quill-launcher-badge", launcher_loop).ok();
    worker.is_some()
}

/// The worker: owns the session-bus connection and emits the signal for the
/// newest count handed over, answering through `BADGE`.
#[cfg(all(target_os = "linux", feature = "ui"))]
fn launcher_loop() {
    let _end = WorkerEnd(&BADGE);
    let mut connection: Option<zbus::blocking::Connection> = None;
    let app_uri = app_uri();
    loop {
        let count = LAUNCHER_SLOT.take();
        if connection.is_none() {
            connection = zbus::blocking::Connection::session().ok();
        }
        let Some(conn) = &connection else {
            crate::locks::lock_or_recover(&BADGE).answered(count, false, std::time::Instant::now());
            continue;
        };
        let mut props: std::collections::HashMap<&str, zbus::zvariant::Value> =
            std::collections::HashMap::new();
        props.insert("count", zbus::zvariant::Value::from(i64::from(count)));
        props.insert("count-visible", zbus::zvariant::Value::from(count > 0));
        let sent = conn.emit_signal(
            None::<&str>,
            "/",
            "com.canonical.Unity.LauncherEntry",
            "Update",
            &(app_uri.as_str(), props),
        );
        if sent.is_err() {
            connection = None;
        }
        crate::locks::lock_or_recover(&BADGE).answered(count, sent.is_ok(), std::time::Instant::now());
    }
}

/// Hands the count to a worker thread that owns the session-bus connection
/// and emits the signal, so the UI thread never waits on D-Bus. The worker
/// starts on first use and again after it died; any failure (no session
/// bus, no listening dock, no thread) is asked again after a wait
/// (`BadgeStage::Failed`).
#[cfg(target_os = "linux")]
fn emit_launcher_entry(unread: u32) -> Push {
    #[cfg(feature = "ui")]
    {
        if !launcher_worker_runs() {
            return Push::Failed;
        }
        LAUNCHER_SLOT.put(unread);
        Push::Handed
    }
    #[cfg(not(feature = "ui"))]
    {
        let _ = unread;
        Push::Done
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Mutex;

    fn t0() -> std::time::Instant {
        static T0: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        *T0.get_or_init(std::time::Instant::now)
    }

    #[test]
    fn badge_is_repushed_after_invalidation_but_deduped_otherwise() {
        let badge = Mutex::new(BadgeStage::Idle);
        let pushes = std::cell::Cell::new(0);
        let push = |_| {
            pushes.set(pushes.get() + 1);
            Push::Done
        };
        sync_with(&badge, 4, t0(), push);
        sync_with(&badge, 4, t0(), push);
        assert_eq!(pushes.get(), 1, "same count is deduped");
        badge.lock().unwrap().invalidate();
        sync_with(&badge, 4, t0(), push);
        assert_eq!(pushes.get(), 2, "re-activation pushes the same count again");
        sync_with(&badge, 4, t0(), push);
        assert_eq!(pushes.get(), 2);
    }

    #[test]
    fn a_push_that_failed_is_asked_again_after_the_wait() {
        let badge = Mutex::new(BadgeStage::Idle);
        let pushes = std::cell::Cell::new(0);
        sync_with(&badge, 2, t0(), |_| {
            pushes.set(pushes.get() + 1);
            Push::Failed
        });
        sync_with(&badge, 2, t0() + FAILED_RETRY_AFTER, |_| {
            pushes.set(pushes.get() + 1);
            Push::Done
        });
        assert_eq!(pushes.get(), 2);
        assert_eq!(*badge.lock().unwrap(), BadgeStage::Delivered(2));
    }

    #[test]
    fn a_count_is_shown_only_once_the_worker_emitted_it() {
        let badge = Mutex::new(BadgeStage::Idle);
        let handed = std::cell::Cell::new(0);
        let push = |_| {
            handed.set(handed.get() + 1);
            Push::Handed
        };
        sync_with(&badge, 3, t0(), push);
        sync_with(&badge, 3, t0(), push);
        assert_eq!(handed.get(), 1, "in flight: not asked twice");
        badge.lock().unwrap().answered(3, false, t0());
        sync_with(&badge, 3, t0(), push);
        assert_eq!(handed.get(), 1, "failed: not asked on every pass");
        sync_with(&badge, 3, t0() + FAILED_RETRY_AFTER, push);
        assert_eq!(handed.get(), 2, "failed: asked again after the wait");
        badge.lock().unwrap().answered(3, true, t0());
        sync_with(&badge, 3, t0(), push);
        assert_eq!(handed.get(), 2, "emitted: known shown");
        sync_with(&badge, 0, t0(), push);
        assert_eq!(handed.get(), 3, "a new count goes out");
    }

    #[test]
    fn a_failed_count_waits_but_a_new_count_goes_out_at_once() {
        let badge = Mutex::new(BadgeStage::Idle);
        let pushes = std::cell::Cell::new(0);
        let fail = |_| {
            pushes.set(pushes.get() + 1);
            Push::Failed
        };
        for _ in 0..5 {
            sync_with(&badge, 4, t0(), fail);
        }
        assert_eq!(pushes.get(), 1, "a missing bus is not asked on every pass");
        sync_with(&badge, 5, t0(), fail);
        assert_eq!(pushes.get(), 2, "a new count is not held back");
    }

    #[test]
    fn an_old_answer_does_not_settle_a_newer_request() {
        let mut stage = BadgeStage::Idle;
        assert!(stage.request(1, t0()));
        assert!(stage.request(2, t0()));
        stage.answered(1, true, t0());
        assert!(!stage.request(2, t0()), "2 is still pending");
        stage.answered(2, true, t0());
        assert!(!stage.request(2, t0()));
    }

    #[test]
    fn a_worker_that_ends_hands_its_pending_count_back() {
        let badge = Mutex::new(BadgeStage::Idle);
        sync_with(&badge, 5, t0(), |_| Push::Handed);
        drop(WorkerEnd(&badge));
        assert!(matches!(
            *badge.lock().unwrap(),
            BadgeStage::Failed { count: 5, .. }
        ));
        let pushes = std::cell::Cell::new(0);
        sync_with(&badge, 5, t0() + FAILED_RETRY_AFTER * 2, |_| {
            pushes.set(pushes.get() + 1);
            Push::Handed
        });
        assert_eq!(pushes.get(), 1, "asked again after the worker was lost");
    }

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
        let (started, hear) = std::sync::mpsc::channel();
        let reader = {
            let slot = slot.clone();
            std::thread::spawn(move || {
                let _ = started.send(());
                slot.take()
            })
        };
        hear.recv().unwrap();
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
