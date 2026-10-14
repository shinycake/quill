//! Quill's own notification windows (`parity:notify-desktop-options`):
//! the pure half of tdesktop's `Window::Notifications::Default::Manager`
//! (`window/notifications_manager_default.cpp`). Which backend shows a
//! notification, where each window sits on the screen, and the queue
//! behind the "Notifications count" limit. The windows themselves are
//! `ui::notify_windows`.
//!
//! tdesktop shows these windows where the system's own notifications fall
//! short (`Platform::Notifications::ByDefault`): by default on Windows,
//! and on X11 when the notification daemon lacks body, actions, inline
//! reply and sound or inhibitions. The screen corner, the count and the
//! display only apply to them. macOS and Wayland always use the system's
//! notifications (`Enforced`); there the options are hidden, as in
//! tdesktop.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// tdesktop `st::notifyWidth`.
pub const WIDTH: f32 = 320.0;
/// tdesktop `st::notifyMinHeight`: a notification without the reply field.
pub const MIN_HEIGHT: f32 = 80.0;
/// tdesktop `st::notifyDeltaX`: gap to the screen's side.
pub const DELTA_X: f32 = 6.0;
/// tdesktop `st::notifyDeltaY`: gap to the screen's edge and between
/// notifications.
pub const DELTA_Y: f32 = 7.0;
/// tdesktop `st::notifyHideAllHeight`.
pub const HIDE_ALL_HEIGHT: f32 = 36.0;
/// tdesktop `st::notifyPhotoSize` and `st::notifyPhotoPos`.
pub const PHOTO_SIZE: f32 = 62.0;
pub const PHOTO_POS: f32 = 9.0;
/// tdesktop `st::notifyFastAnim`: appear, move, fast hide (ms).
pub const FAST_ANIM_MS: u64 = 150;
/// tdesktop `st::notifySlowHide`: the slow fade out (ms).
pub const SLOW_HIDE_MS: u64 = 4000;
/// tdesktop `st::notifyWaitLongHide`: shown this long after the user is
/// back before the slow fade starts (ms).
pub const WAIT_LONG_HIDE_MS: u64 = 3000;
/// tdesktop `st::notifyActionsDuration`: the Reply button's fade (ms).
pub const ACTIONS_MS: u64 = 200;
/// How often a notification waiting for the user to come back checks
/// (tdesktop `_inputCheckTimer`, ms).
pub const INPUT_CHECK_MS: u64 = 300;
/// tdesktop `kMaxNotificationsCount` and the default count.
pub const MAX_COUNT: u8 = 5;
pub const DEFAULT_COUNT: u8 = 3;

/// tdesktop `Core::Settings::ScreenCorner`, in its stored order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ScreenCorner {
    TopLeft,
    TopRight,
    #[default]
    BottomRight,
    BottomLeft,
}

impl ScreenCorner {
    pub const ALL: [ScreenCorner; 4] = [
        ScreenCorner::TopLeft,
        ScreenCorner::TopRight,
        ScreenCorner::BottomRight,
        ScreenCorner::BottomLeft,
    ];

    pub fn is_left(self) -> bool {
        matches!(self, ScreenCorner::TopLeft | ScreenCorner::BottomLeft)
    }

    pub fn is_top(self) -> bool {
        matches!(self, ScreenCorner::TopLeft | ScreenCorner::TopRight)
    }
}

/// A rectangle in screen points (the display's usable area).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// The windowing system Quill runs on, as far as notifications care.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyPlatform {
    MacOS,
    Windows,
    X11,
    Wayland,
    Other,
}

/// What the system's notification service offers (Linux: the D-Bus
/// daemon's capabilities, once known).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeService {
    /// Not asked yet (the probe runs off the UI thread).
    Unknown,
    /// No service: nothing to hand notifications to.
    Missing,
    /// A service; `full` when it matches the custom windows feature for
    /// feature (tdesktop `ByDefault`).
    Present { full: bool },
}

/// tdesktop's `Platform::Notifications::{Supported, Enforced, ByDefault}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeSupport {
    pub supported: bool,
    pub enforced: bool,
    pub by_default: bool,
}

/// `force_custom` is tdesktop's "Force non-native notifications
/// availability" option (`OptionCustomNotification`), which lifts the
/// enforcement; Quill reads it from `QUILL_CUSTOM_NOTIFICATIONS=1`.
pub fn native_support(
    platform: NotifyPlatform,
    service: NativeService,
    force_custom: bool,
) -> NativeSupport {
    let present = matches!(service, NativeService::Present { .. });
    let full = matches!(service, NativeService::Present { full: true });
    let (supported, enforced, by_default) = match platform {
        NotifyPlatform::MacOS => (true, true, true),
        NotifyPlatform::Windows => (true, false, false),
        NotifyPlatform::X11 => (present, false, full),
        // Wayland has no window positioning: custom windows cannot work.
        NotifyPlatform::Wayland => (present, true, full),
        NotifyPlatform::Other => (false, true, false),
    };
    NativeSupport {
        supported,
        enforced: enforced && !(force_custom && platform != NotifyPlatform::Wayland),
        by_default,
    }
}

/// Whether a notification goes to Quill's own windows (tdesktop
/// `System::createManager` picking `Default::Manager`). `native_pref` is
/// "Use native notifications" when the user set it. While the Linux
/// service is still being probed (`NativeService::Unknown`) the system
/// path is kept.
pub fn use_custom(
    native_pref: Option<bool>,
    support: NativeSupport,
    service: NativeService,
) -> bool {
    if support.enforced || service == NativeService::Unknown {
        return false;
    }
    !(support.supported && native_pref.unwrap_or(support.by_default))
}

/// Whether the settings show the "Use native notifications" switch.
pub fn native_switch_shown(support: NativeSupport) -> bool {
    support.supported && !support.enforced
}

/// tdesktop `notificationStartPosition`: the x of every window and the y
/// of the screen edge the stack grows from.
pub fn start_position(display: Rect, corner: ScreenCorner) -> (f32, f32) {
    let x = if corner.is_left() {
        display.x + DELTA_X
    } else {
        display.x + display.width - WIDTH - DELTA_X
    };
    let y = if corner.is_top() {
        display.y
    } else {
        display.y + display.height
    };
    (x, y)
}

/// The top-left corner of a window `shift` points from the start edge
/// (tdesktop `Widget::computePosition`).
pub fn window_origin(display: Rect, corner: ScreenCorner, shift: f32, height: f32) -> (f32, f32) {
    let (x, y) = start_position(display, corner);
    if corner.is_top() {
        (x, y + shift)
    } else {
        (x, y - shift - height)
    }
}

/// tdesktop `Manager::moveWidgets`: each live notification's shift, newest
/// nearest the corner, and the "Hide all" button's after the oldest.
/// `heights` are in display order, oldest first.
pub fn stack_shifts(heights: &[f32]) -> (Vec<f32>, f32) {
    let mut shifts = vec![0.0; heights.len()];
    let mut shift = DELTA_Y;
    for (slot, height) in shifts.iter_mut().zip(heights).rev() {
        *slot = shift;
        shift += height + DELTA_Y;
    }
    (shifts, shift)
}

/// The "Hide all" button is up while more than one notification is live
/// or more wait in the queue (tdesktop `moveWidgets`).
pub fn hide_all_wanted(live: usize, queued: usize) -> bool {
    live > 1 || queued > 0
}

/// The count setting clamped to 1-5 (tdesktop `CurrentNotificationsCount`).
pub fn clamp_count(count: u8) -> usize {
    usize::from(count.clamp(1, MAX_COUNT))
}

/// Notifications waiting for a free slot (tdesktop `_queuedNotifications`).
/// `K` identifies a chat (account and chat id) so a chat's queued entries
/// can be dropped when it is read.
#[derive(Debug, Clone)]
pub struct NotifyQueue<K, T> {
    items: VecDeque<(K, T)>,
}

impl<K: PartialEq, T> Default for NotifyQueue<K, T> {
    fn default() -> Self {
        Self {
            items: VecDeque::new(),
        }
    }
}

impl<K: PartialEq, T> NotifyQueue<K, T> {
    pub fn push(&mut self, key: K, item: T) {
        self.items.push_back((key, item));
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Take as many as `free` slots allow, oldest first
    /// (`showNextFromQueue`).
    pub fn take(&mut self, free: usize) -> Vec<(K, T)> {
        let count = free.min(self.items.len());
        self.items.drain(..count).collect()
    }

    /// Drop the queued entries of one chat (`doClearFromHistory`).
    pub fn remove_key(&mut self, key: &K) {
        self.items.retain(|(k, _)| k != key);
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }
}

/// tdesktop's `anim::easeInCirc`, the slow hide's curve.
pub fn ease_in_circ(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t * t).max(0.0).sqrt()
}

/// A display's stable number for the settings file (tdesktop
/// `ScreenNameChecksum`): FNV-1a of its identifier. Never 0, which means
/// "Default".
pub fn display_checksum(identifier: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in identifier.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash.max(1)
}

/// tdesktop's display choice label: "Display (1920×1080)".
pub fn display_label(width: f32, height: f32) -> String {
    format!("Display ({}\u{00D7}{})", width.round(), height.round())
}

#[cfg(test)]
#[path = "custom_tests.rs"]
mod tests;
