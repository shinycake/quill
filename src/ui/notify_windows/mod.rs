//! Quill's own notification windows (`parity:notify-desktop-options`),
//! tdesktop's `Window::Notifications::Default::Manager`
//! (`window/notifications_manager_default.cpp`): a stack of small
//! always-on-top windows in the chosen screen corner, at most "Notifications
//! count" of them with the rest queued, a "Hide all" button below the
//! stack, a fade in, a slide when the stack changes, and a slow fade out a
//! while after the user is back at the computer. Hovering any of them
//! keeps them all up; the Reply button opens a field right in the window.
//!
//! Where they are used instead of the system's notifications is decided by
//! `quill::notify::custom` (Windows by default, X11 when the daemon falls
//! short, never on macOS or Wayland unless forced). The pure stacking and
//! queue rules live there; this module owns the windows and their clocks.
//!
//! Each window is a GPUI `PopUp` window with a transparent background:
//! the fade is the content's opacity, as GPUI has no window opacity
//! (`view.rs`). GPUI cannot move a window, so `position.rs` does it
//! natively. The clock is a task that wakes every frame while something
//! animates and otherwise only for the next deadline, and stops when no
//! window is left.
#![deny(clippy::indexing_slicing)]

mod position;
mod view;

use super::app::QuillApp;
use gpui_kit::*;
use quill::ids::{AccountKey, ChatId};
use quill::notify::custom::{
    self, DELTA_Y, FAST_ANIM_MS, HIDE_ALL_HEIGHT, INPUT_CHECK_MS, MIN_HEIGHT, NativeService,
    NotifyPlatform, NotifyQueue, Rect, SLOW_HIDE_MS, ScreenCorner, WAIT_LONG_HIDE_MS, WIDTH,
};
use quill::notify::{NotificationAction, OsNotification};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use view::{Content, NotifyView, ViewEvent};

/// One frame of an animation.
const FRAME: Duration = Duration::from_millis(16);

/// The chat a notification belongs to: the account (`None` = on screen)
/// and the chat.
type Key = (Option<AccountKey>, ChatId);

/// A value moving from `from` to `to` (tdesktop `anim::value`).
#[derive(Debug, Clone, Copy)]
struct Anim {
    from: f32,
    to: f32,
    start: Instant,
    ms: u64,
    slow: bool,
}

impl Anim {
    fn still(value: f32, now: Instant) -> Self {
        Self {
            from: value,
            to: value,
            start: now,
            ms: 0,
            slow: false,
        }
    }

    fn progress(&self, now: Instant) -> f32 {
        if self.ms == 0 {
            return 1.0;
        }
        let elapsed = now.saturating_duration_since(self.start).as_millis() as f32;
        (elapsed / self.ms as f32).clamp(0.0, 1.0)
    }

    fn value(&self, now: Instant) -> f32 {
        let t = self.progress(now);
        let eased = if self.slow {
            custom::ease_in_circ(t)
        } else {
            t
        };
        self.from + (self.to - self.from) * eased
    }

    fn done(&self, now: Instant) -> bool {
        self.progress(now) >= 1.0
    }

    /// Head for `to` from wherever the value is now.
    fn retarget(&mut self, to: f32, ms: u64, slow: bool, now: Instant) {
        self.from = self.value(now);
        self.to = to;
        self.start = now;
        self.ms = ms;
        self.slow = slow;
    }
}

/// One open window and its motion.
struct Popup {
    handle: WindowHandle<NotifyView>,
    view: Entity<NotifyView>,
    height: f32,
    shift: Anim,
    opacity: Anim,
    hiding: bool,
    /// Where the window was last put, so a still frame moves nothing.
    placed: Option<Point<Pixels>>,
    drawn_opacity: f32,
}

impl Popup {
    fn hide(&mut self, slow: bool, now: Instant) {
        self.hiding = true;
        let ms = if slow { SLOW_HIDE_MS } else { FAST_ANIM_MS };
        // Restart from fully shown, as tdesktop does (its #28811).
        self.opacity = Anim::still(1.0, now);
        self.opacity.retarget(0.0, ms, slow, now);
    }

    fn unhide(&mut self, now: Instant) {
        if self.hiding {
            self.hiding = false;
            self.opacity.retarget(1.0, FAST_ANIM_MS, false, now);
        }
    }

    fn close(&self, cx: &mut App) {
        let _ = self
            .handle
            .update(cx, |_, window, _| window.remove_window());
    }
}

struct Shown {
    id: u64,
    key: Key,
    popup: Popup,
    /// Read, closed or replaced: fading out, no longer counted.
    unlinked: bool,
    started: Instant,
    waiting_for_input: bool,
    hide_at: Option<Instant>,
    replying: bool,
}

struct Pending {
    notification: OsNotification,
    icon: Option<PathBuf>,
    preview: Option<PathBuf>,
    locked: bool,
    reply: bool,
}

/// The notification windows' state (`NotifyUi::windows`).
pub(crate) struct NotifyWindows {
    shown: Vec<Shown>,
    hide_all: Option<Popup>,
    queue: NotifyQueue<Key, Pending>,
    /// The settings' corner preview (tdesktop `SampleWidget`s).
    samples: Vec<(ScreenCorner, Popup)>,
    /// tdesktop `demoMasterOpacity`: the real notifications dim while the
    /// preview shows.
    demo: Anim,
    next_id: u64,
    ticking: bool,
    /// The main window's windowing system and display, noted every frame.
    pub(super) platform: NotifyPlatform,
    pub(super) main_display: Option<DisplayId>,
}

impl NotifyWindows {
    pub(crate) fn new() -> Self {
        Self {
            shown: Vec::new(),
            hide_all: None,
            queue: NotifyQueue::default(),
            samples: Vec::new(),
            demo: Anim::still(1.0, Instant::now()),
            next_id: 0,
            ticking: false,
            platform: NotifyPlatform::Other,
            main_display: None,
        }
    }

    fn live(&self) -> usize {
        self.shown.iter().filter(|s| !s.unlinked).count()
    }

    fn any_replying(&self) -> bool {
        self.shown.iter().any(|s| s.replying)
    }
}

/// The platform of a window, for `custom::native_support`.
pub(super) fn platform_of(window: &Window) -> NotifyPlatform {
    use super::window_control::Backend;
    match super::window_control::backend(window) {
        Backend::MacOS => NotifyPlatform::MacOS,
        Backend::Windows => NotifyPlatform::Windows,
        Backend::X11 => NotifyPlatform::X11,
        Backend::Wayland => NotifyPlatform::Wayland,
        Backend::Unknown => NotifyPlatform::Other,
    }
}

/// tdesktop's "Force non-native notifications availability" option.
fn force_custom() -> bool {
    std::env::var("QUILL_CUSTOM_NOTIFICATIONS").is_ok_and(|v| v == "1")
}

/// What the system's notification service can do, as far as known.
pub(super) fn native_service(platform: NotifyPlatform) -> NativeService {
    match platform {
        #[cfg(target_os = "linux")]
        NotifyPlatform::X11 | NotifyPlatform::Wayland => {
            super::notifications::linux_service_capabilities()
        }
        _ => NativeService::Present { full: true },
    }
}

impl QuillApp {
    /// tdesktop's native-notification rules for this platform.
    pub(in crate::ui) fn native_notify_support(&self) -> custom::NativeSupport {
        let platform = self.notify.windows.platform;
        custom::native_support(platform, native_service(platform), force_custom())
    }

    /// Whether notifications go to Quill's own windows right now.
    pub(in crate::ui) fn custom_notifications_active(&self) -> bool {
        let platform = self.notify.windows.platform;
        custom::use_custom(
            quill::notify_prefs::current().native,
            self.native_notify_support(),
            native_service(platform),
        )
    }

    /// The usable area of the display notifications go to: the one picked
    /// in the settings, else the main window's (tdesktop
    /// `NotificationDisplayRect`).
    fn notify_display(&self, cx: &App) -> Option<(DisplayId, Rect)> {
        let wanted = quill::notify_prefs::current().display;
        let displays = cx.displays();
        let picked = displays
            .iter()
            .find(|d| wanted != 0 && display_checksum(d.as_ref()) == wanted)
            .or_else(|| {
                self.notify
                    .windows
                    .main_display
                    .and_then(|id| displays.iter().find(|d| d.id() == id))
            })
            .cloned()
            .or_else(|| cx.primary_display())?;
        let area = picked.visible_bounds();
        Some((
            picked.id(),
            Rect {
                x: area.origin.x.as_f32(),
                y: area.origin.y.as_f32(),
                width: area.size.width.as_f32(),
                height: area.size.height.as_f32(),
            },
        ))
    }

    /// Queue one notification for the windows (tdesktop
    /// `doShowNotification`). A newer notification of the same chat
    /// replaces the shown one, as the system notifications do by tag.
    pub(in crate::ui) fn show_custom_notification(
        &mut self,
        account: Option<&AccountKey>,
        notification: OsNotification,
        icon: Option<PathBuf>,
        preview: Option<PathBuf>,
        locked: bool,
        reply: bool,
        cx: &mut Context<Self>,
    ) {
        let key: Key = (account.cloned(), notification.chat_id);
        let now = Instant::now();
        let windows = &mut self.notify.windows;
        windows.queue.remove_key(&key);
        for shown in windows
            .shown
            .iter_mut()
            .filter(|s| s.key == key && !s.unlinked)
        {
            shown.unlinked = true;
            shown.popup.hide(false, now);
        }
        windows.queue.push(
            key,
            Pending {
                notification,
                icon,
                preview,
                locked,
                reply,
            },
        );
        self.notify_show_next(cx);
    }

    /// Withdraw a chat's notifications: it was read or opened
    /// (tdesktop `doClearFromHistory`).
    pub(in crate::ui) fn clear_custom_notifications(
        &mut self,
        account: Option<&AccountKey>,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) {
        let key: Key = (account.cloned(), chat_id);
        let now = Instant::now();
        let windows = &mut self.notify.windows;
        windows.queue.remove_key(&key);
        let mut changed = false;
        for shown in windows
            .shown
            .iter_mut()
            .filter(|s| s.key == key && !s.unlinked)
        {
            shown.unlinked = true;
            shown.popup.hide(false, now);
            changed = true;
        }
        if changed {
            self.notify_show_next(cx);
        }
    }

    /// Hide everything ("Hide all", tdesktop `doClearAll`).
    fn clear_all_custom_notifications(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        let windows = &mut self.notify.windows;
        windows.queue.clear();
        for shown in windows.shown.iter_mut().filter(|s| !s.unlinked) {
            shown.unlinked = true;
            shown.popup.hide(false, now);
        }
        self.notify_show_next(cx);
    }

    /// Open windows for queued notifications while there is room, then
    /// restack (tdesktop `showNextFromQueue` + `moveWidgets`).
    fn notify_show_next(&mut self, cx: &mut Context<Self>) {
        let limit = custom::clamp_count(quill::notify_prefs::current().count);
        let free = limit.saturating_sub(self.notify.windows.live());
        // With no display to show them on, they wait for one to be
        // connected (`custom_notify_displays_changed`; tdesktop keeps its
        // queue for `screenAdded`).
        if let Some((display_id, area)) = self.notify_display(cx) {
            for (key, pending) in self.notify.windows.queue.take(free) {
                self.open_notification(key, pending, display_id, area, cx);
            }
        }
        self.notify_restack(cx);
        self.ensure_notify_clock(cx);
    }

    /// A display was connected, disconnected or resized: put the windows
    /// back in place on the notification display (the next clock step
    /// moves them) and show what waited for a display.
    pub(in crate::ui) fn custom_notify_displays_changed(&mut self, cx: &mut Context<Self>) {
        let windows = &self.notify.windows;
        if windows.shown.is_empty()
            && windows.hide_all.is_none()
            && windows.samples.is_empty()
            && windows.queue.is_empty()
        {
            return;
        }
        for popup in self.all_popups_mut() {
            popup.placed = None;
        }
        self.notify_show_next(cx);
    }

    fn open_notification(
        &mut self,
        key: Key,
        pending: Pending,
        display_id: DisplayId,
        area: Rect,
        cx: &mut Context<Self>,
    ) {
        let windows = &mut self.notify.windows;
        windows.next_id += 1;
        let id = windows.next_id;
        let content = Content::Message {
            title: pending.notification.title.into(),
            body: pending.notification.body.into(),
            photo: pending.icon,
            media: pending.preview,
            hide_sender: pending.locked,
            can_reply: pending.reply && pending.notification.message_id.is_some(),
            reply_inline: self.notify.windows.platform != NotifyPlatform::X11,
        };
        let corner = quill::notify_prefs::current().corner;
        let Some(popup) =
            self.open_popup(id, content, MIN_HEIGHT, 0.0, corner, display_id, area, cx)
        else {
            eprintln!("quill: could not open a notification window");
            return;
        };
        let now = Instant::now();
        self.notify.windows.shown.push(Shown {
            id,
            key,
            popup,
            unlinked: false,
            started: now,
            waiting_for_input: true,
            hide_at: None,
            replying: false,
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn open_popup(
        &mut self,
        id: u64,
        content: Content,
        height: f32,
        shift: f32,
        corner: ScreenCorner,
        display_id: DisplayId,
        area: Rect,
        cx: &mut Context<Self>,
    ) -> Option<Popup> {
        let (x, y) = custom::window_origin(area, corner, shift, height);
        let owner = cx.entity().downgrade();
        let view = cx.new(|_| NotifyView::new(id, content, owner));
        let root = view.clone();
        let handle = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: point(px(x), px(y)),
                        size: size(px(WIDTH), px(height)),
                    })),
                    titlebar: None,
                    focus: false,
                    show: true,
                    kind: WindowKind::PopUp,
                    is_movable: false,
                    is_resizable: false,
                    is_minimizable: false,
                    display_id: Some(display_id),
                    window_background: WindowBackgroundAppearance::Transparent,
                    ..Default::default()
                },
                move |_, _| root,
            )
            .ok()?;
        let now = Instant::now();
        let mut opacity = Anim::still(0.0, now);
        opacity.retarget(1.0, FAST_ANIM_MS, false, now);
        Some(Popup {
            handle,
            view,
            height,
            shift: Anim::still(shift, now),
            opacity,
            hiding: false,
            placed: Some(point(px(x), px(y))),
            drawn_opacity: 0.0,
        })
    }

    /// Give every live window its place in the stack and put the "Hide
    /// all" button below it (tdesktop `moveWidgets`).
    fn notify_restack(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        let windows = &mut self.notify.windows;
        let heights: Vec<f32> = windows
            .shown
            .iter()
            .filter(|s| !s.unlinked)
            .map(|s| s.popup.height)
            .collect();
        let (shifts, below) = custom::stack_shifts(&heights);
        let mut last_current = 0.0;
        for (shown, shift) in windows.shown.iter_mut().filter(|s| !s.unlinked).zip(shifts) {
            if (shown.popup.shift.to - shift).abs() > f32::EPSILON {
                shown.popup.shift.retarget(shift, FAST_ANIM_MS, false, now);
            }
            last_current = shown.popup.shift.value(now);
        }
        let wanted = custom::hide_all_wanted(heights.len(), windows.queue.len());
        if wanted {
            if windows.hide_all.is_none() {
                let corner = quill::notify_prefs::current().corner;
                if let Some((display_id, area)) = self.notify_display(cx) {
                    self.notify.windows.next_id += 1;
                    let id = self.notify.windows.next_id;
                    self.notify.windows.hide_all = self.open_popup(
                        id,
                        Content::HideAll,
                        HIDE_ALL_HEIGHT,
                        last_current,
                        corner,
                        display_id,
                        area,
                        cx,
                    );
                }
            }
            if let Some(hide_all) = self.notify.windows.hide_all.as_mut() {
                hide_all.shift.retarget(below, FAST_ANIM_MS, false, now);
                hide_all.unhide(now);
            }
        } else if let Some(hide_all) = self.notify.windows.hide_all.as_mut()
            && !hide_all.hiding
        {
            hide_all.hide(false, now);
        }
    }

    /// Settings changed: corner or display (restack in place), count
    /// (close the oldest over the limit, or show queued ones).
    pub(in crate::ui) fn custom_notify_settings_changed(&mut self, cx: &mut Context<Self>) {
        let limit = custom::clamp_count(quill::notify_prefs::current().count);
        let now = Instant::now();
        let mut allow = limit;
        for shown in self.notify.windows.shown.iter_mut().rev() {
            if shown.unlinked {
                continue;
            }
            if allow == 0 {
                shown.unlinked = true;
                shown.popup.hide(false, now);
            } else {
                allow -= 1;
            }
        }
        for popup in self.all_popups_mut() {
            popup.placed = None;
        }
        self.notify_show_next(cx);
    }

    fn all_popups_mut(&mut self) -> impl Iterator<Item = &mut Popup> {
        let windows = &mut self.notify.windows;
        windows
            .shown
            .iter_mut()
            .map(|s| &mut s.popup)
            .chain(windows.hide_all.iter_mut())
            .chain(windows.samples.iter_mut().map(|(_, p)| p))
    }

    /// The settings' corner preview: sample windows in `corner` (as many
    /// as the count), the real ones dimmed; `None` takes them away
    /// (tdesktop `setOverCorner` / `clearOverCorner`).
    pub(in crate::ui) fn notify_corner_preview(
        &mut self,
        corner: Option<ScreenCorner>,
        cx: &mut Context<Self>,
    ) {
        let now = Instant::now();
        let windows = &mut self.notify.windows;
        let dim = if corner.is_some() { 0.0 } else { 1.0 };
        if (windows.demo.to - dim).abs() > f32::EPSILON {
            windows.demo.retarget(dim, FAST_ANIM_MS, false, now);
        }
        for (sample_corner, popup) in windows.samples.iter_mut() {
            if Some(*sample_corner) != corner {
                popup.hide(false, now);
            }
        }
        if let Some(corner) = corner {
            let count = custom::clamp_count(quill::notify_prefs::current().count);
            let mut present = 0;
            for (sample_corner, popup) in windows.samples.iter_mut() {
                if *sample_corner == corner {
                    if present < count {
                        popup.unhide(now);
                        present += 1;
                    } else {
                        popup.hide(false, now);
                    }
                }
            }
            if let Some((display_id, area)) = self.notify_display(cx) {
                for index in present..count {
                    let shift = DELTA_Y + index as f32 * (MIN_HEIGHT + DELTA_Y);
                    self.notify.windows.next_id += 1;
                    let id = self.notify.windows.next_id;
                    if let Some(popup) = self.open_popup(
                        id,
                        Content::Sample,
                        MIN_HEIGHT,
                        shift,
                        corner,
                        display_id,
                        area,
                        cx,
                    ) {
                        self.notify.windows.samples.push((corner, popup));
                    }
                }
            }
        }
        self.ensure_notify_clock(cx);
    }

    /// Something happened in a window (`view::ViewEvent`).
    fn notify_window_event(&mut self, id: u64, event: ViewEvent, cx: &mut Context<Self>) {
        let now = Instant::now();
        match event {
            ViewEvent::Hover(true) => {
                // tdesktop `stopAllHiding`.
                let windows = &mut self.notify.windows;
                for shown in windows.shown.iter_mut().filter(|s| !s.unlinked) {
                    shown.hide_at = None;
                    shown.popup.unhide(now);
                }
                if let Some(hide_all) = windows.hide_all.as_mut() {
                    hide_all.unhide(now);
                }
            }
            ViewEvent::Hover(false) => self.notify_start_all_hiding(now),
            ViewEvent::Activate => {
                if let Some(key) = self.notify_key(id) {
                    self.push_notification_click(&key, NotificationAction::Open);
                    self.clear_custom_notifications(key.0.as_ref(), key.1, cx);
                }
            }
            ViewEvent::Close => self.unlink_notification(id, cx),
            ViewEvent::ReplyInApp => {
                if let Some(key) = self.notify_key(id) {
                    self.push_notification_click(&key, NotificationAction::Reply);
                    self.clear_custom_notifications(key.0.as_ref(), key.1, cx);
                }
            }
            ViewEvent::ReplyOpened => {
                if let Some(shown) = self.notify.windows.shown.iter_mut().find(|s| s.id == id) {
                    shown.replying = true;
                    shown.hide_at = None;
                    shown.popup.unhide(now);
                }
            }
            ViewEvent::ReplySent(text) => {
                if let Some((None, chat_id)) = self.notify_key(id) {
                    self.send_inline_reply(chat_id, &text, cx);
                }
                self.unlink_notification(id, cx);
                self.notify_start_all_hiding(now);
            }
            ViewEvent::ReplyHeight(field) => {
                let height = MIN_HEIGHT + field;
                if let Some(shown) = self.notify.windows.shown.iter_mut().find(|s| s.id == id)
                    && (shown.popup.height - height).abs() > 0.5
                {
                    shown.popup.height = height;
                    shown.popup.placed = None;
                    let _ = shown.popup.handle.update(cx, |_, window, _| {
                        window.resize(size(px(WIDTH), px(height)));
                    });
                    self.notify_restack(cx);
                }
            }
            ViewEvent::HideAll => self.clear_all_custom_notifications(cx),
        }
        self.ensure_notify_clock(cx);
        cx.notify();
    }

    fn notify_key(&self, id: u64) -> Option<Key> {
        self.notify
            .windows
            .shown
            .iter()
            .find(|s| s.id == id && !s.unlinked)
            .map(|s| s.key.clone())
    }

    /// Hand a click to the path the system notifications use: the next
    /// render opens the chat (and switches accounts if needed).
    fn push_notification_click(&mut self, key: &Key, action: NotificationAction) {
        match &key.0 {
            None => {
                if let Ok(mut clicks) = self.notify.notify_clicks.lock() {
                    clicks.push((key.1, action));
                }
            }
            Some(account) => {
                if let Ok(mut clicks) = self.background.clicks.lock() {
                    clicks.push((account.clone(), key.1, action));
                }
            }
        }
        quill::wake::UPDATES.poke();
    }

    fn unlink_notification(&mut self, id: u64, cx: &mut Context<Self>) {
        let now = Instant::now();
        if let Some(shown) = self
            .notify
            .windows
            .shown
            .iter_mut()
            .find(|s| s.id == id && !s.unlinked)
        {
            shown.unlinked = true;
            shown.replying = false;
            shown.popup.hide(false, now);
            self.notify_show_next(cx);
        }
    }

    /// tdesktop `startAllHiding`: the pointer left, so everything fades
    /// slowly, unless someone is typing a reply.
    fn notify_start_all_hiding(&mut self, now: Instant) {
        let windows = &mut self.notify.windows;
        if windows.any_replying() {
            return;
        }
        for shown in windows.shown.iter_mut().filter(|s| !s.unlinked) {
            shown.popup.hide(true, now);
        }
        if windows.queue.len() < 2
            && let Some(hide_all) = windows.hide_all.as_mut()
        {
            hide_all.hide(true, now);
        }
    }

    fn ensure_notify_clock(&mut self, cx: &mut Context<Self>) {
        if self.notify.windows.ticking {
            return;
        }
        self.notify.windows.ticking = true;
        cx.spawn(async move |this, cx| {
            loop {
                let next = this
                    .update(cx, |app, cx| app.notify_windows_tick(cx))
                    .ok()
                    .flatten();
                let Some(delay) = next else {
                    break;
                };
                cx.background_executor().timer(delay).await;
            }
        })
        .detach();
    }

    /// One clock step: timers, fades, slides. Returns when to run again,
    /// `None` (and the clock stops) when nothing is left to do.
    fn notify_windows_tick(&mut self, cx: &mut Context<Self>) -> Option<Duration> {
        let now = Instant::now();
        self.notify_check_input(now);
        let windows = &mut self.notify.windows;
        let replying = windows.any_replying();
        for shown in windows.shown.iter_mut().filter(|s| !s.unlinked) {
            if shown.hide_at.is_some_and(|at| at <= now) && !replying {
                shown.hide_at = None;
                shown.popup.hide(true, now);
            }
        }
        let demo = windows.demo.value(now);
        let mut animating = !windows.demo.done(now);
        let display = self.notify_display(cx);
        let corner = quill::notify_prefs::current().corner;
        let mut closed = false;
        let windows = &mut self.notify.windows;
        let popups = windows
            .shown
            .iter_mut()
            .map(|s| (&mut s.popup, demo))
            .chain(windows.hide_all.iter_mut().map(|p| (p, demo)))
            .chain(windows.samples.iter_mut().map(|(_, p)| (p, 1.0)));
        for (popup, master) in popups {
            animating |= !popup.shift.done(now) || !popup.opacity.done(now);
            if let Some((_, area)) = display {
                let (x, y) =
                    custom::window_origin(area, corner, popup.shift.value(now), popup.height);
                let origin = point(px(x.round()), px(y.round()));
                if popup.placed != Some(origin) {
                    popup.placed = Some(origin);
                    let _ = popup
                        .handle
                        .update(cx, |_, window, _| position::set_origin(window, origin));
                }
            }
            let opacity = popup.opacity.value(now) * master;
            if (popup.drawn_opacity - opacity).abs() > 0.004 {
                popup.drawn_opacity = opacity;
                popup.view.update(cx, |view, cx| {
                    view.opacity = opacity;
                    cx.notify();
                });
            }
            if popup.hiding && popup.opacity.done(now) {
                popup.close(cx);
                closed = true;
            }
        }
        if closed {
            let windows = &mut self.notify.windows;
            windows
                .shown
                .retain(|s| !(s.popup.hiding && s.popup.opacity.done(now)));
            if windows
                .hide_all
                .as_ref()
                .is_some_and(|p| p.hiding && p.opacity.done(now))
            {
                windows.hide_all = None;
            }
            windows
                .samples
                .retain(|(_, p)| !(p.hiding && p.opacity.done(now)));
            self.notify_show_next(cx);
        }
        let windows = &mut self.notify.windows;
        if animating {
            return Some(FRAME);
        }
        let deadline = windows
            .shown
            .iter()
            .filter_map(|s| s.hide_at)
            .min()
            .map(|at| at.saturating_duration_since(now));
        let waiting = windows
            .shown
            .iter()
            .any(|s| s.waiting_for_input && !s.unlinked);
        let next = match (deadline, waiting) {
            (Some(deadline), true) => Some(deadline.min(Duration::from_millis(INPUT_CHECK_MS))),
            (Some(deadline), false) => Some(deadline),
            (None, true) => Some(Duration::from_millis(INPUT_CHECK_MS)),
            (None, false) => None,
        };
        let open =
            !windows.shown.is_empty() || windows.hide_all.is_some() || !windows.samples.is_empty();
        match next {
            Some(delay) => Some(delay.max(FRAME)),
            None if open => Some(Duration::from_millis(INPUT_CHECK_MS)),
            None => {
                windows.ticking = false;
                None
            }
        }
    }

    /// tdesktop `checkLastInput`: a notification stays until the user has
    /// touched the computer after it appeared, then lingers
    /// `WAIT_LONG_HIDE_MS` before the slow fade. Without an idle clock it
    /// starts the timer at once.
    fn notify_check_input(&mut self, now: Instant) {
        let idle = quill::passcode::os_idle_ms().map(Duration::from_millis);
        let windows = &mut self.notify.windows;
        let replying = windows.any_replying();
        for shown in windows.shown.iter_mut().filter(|s| s.waiting_for_input) {
            let back = match idle {
                None => true,
                Some(idle) => now
                    .checked_sub(idle)
                    .is_some_and(|input| input > shown.started),
            };
            if back {
                shown.waiting_for_input = false;
                if !replying && !shown.unlinked {
                    shown.hide_at = Some(now + Duration::from_millis(WAIT_LONG_HIDE_MS));
                }
            }
        }
    }

    /// Close every window at once (sign out, quit).
    pub(in crate::ui) fn close_all_notify_windows(&mut self, cx: &mut Context<Self>) {
        let windows = &mut self.notify.windows;
        windows.queue.clear();
        let shown = std::mem::take(&mut windows.shown);
        let hide_all = windows.hide_all.take();
        let samples = std::mem::take(&mut windows.samples);
        for popup in shown
            .iter()
            .map(|s| &s.popup)
            .chain(hide_all.iter())
            .chain(samples.iter().map(|(_, p)| p))
        {
            popup.close(cx);
        }
    }
}

/// A display's number for the settings file: its stable id when the
/// platform has one, else its index-like id.
pub(super) fn display_checksum(display: &dyn PlatformDisplay) -> u64 {
    match display.uuid() {
        Ok(uuid) if !uuid.is_nil() => custom::display_checksum(&uuid.to_string()),
        _ => custom::display_checksum(&format!("{:?}", display.id())),
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod tests;
