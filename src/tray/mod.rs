//! System tray icon with an unread-count badge (parity:platform-tray-icon).
//!
//! The icon is the Quill app icon at 64x64 RGBA (`assets/icons/tray-64.rgba`),
//! plus a large red pill badge with the unread count (capped at "99+") in the
//! top-right corner whenever any chat is unread.
//!
//! What counts is governed by [`BadgePrefs`]
//! (parity:chatlist-badge-settings), edited in the notification defaults
//! dialog and persisted to `badge_prefs.json`:
//! - muted chats are included unless `include_muted` is off (Telegram
//!   Desktop's default: `_includeMutedCounter = true`, tdesktop
//!   `Telegram/SourceFiles/core/core_settings.h`, i.e. excluding muted is
//!   the opt-out, not the opt-in; muteness is [`Session::effective_muted`]);
//! - archived chats count as muted (tdesktop folds the archive folder into
//!   the main list, all-muted), so they are included with muted chats unless
//!   `include_archived` is turned off;
//! - the badge shows either the unread-message sum or the unread-chat count
//!   (`count_messages`). The sum saturates instead of overflowing.
//!
//! The OS tray itself is managed behind `#[cfg(feature = "ui")]`: the
//! `tray-icon` crate on macOS and Windows, a StatusNotifierItem over D-Bus
//! (`ksni`, [`crate::tray_sni`]) on Linux, where `tray-icon` would need a
//! GTK main loop. [`sync_tray`] is called from a 1s timer in `main.rs`;
//! it re-renders only when the count changes and silently no-ops when the OS
//! has no system tray (a tray appearing later is picked up on the next sync —
//! the handle is re-created until it succeeds).

use crate::settings::BadgePrefs;
use crate::state::Session;

/// Icon edge length in pixels.
pub const ICON_SIZE: u32 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    Open,
    /// Lock behind the local passcode (opens the passcode settings when
    /// none is set).
    Lock,
    /// tdesktop tray "Disable notifications" / "Enable notifications".
    ToggleNotifications,
    /// Quill's "Disable/Enable notification sounds" (tdesktop "Play sound").
    ToggleSounds,
    Quit,
}

/// Tray menu labels for the two notification toggles, tdesktop wording
/// (`lng_disable_notifications_from_tray` / `lng_enable_…`).
pub fn notifications_label(enabled: bool) -> &'static str {
    if enabled {
        "Disable notifications"
    } else {
        "Enable notifications"
    }
}

pub fn sounds_label(enabled: bool) -> &'static str {
    if enabled {
        "Disable notification sounds"
    } else {
        "Enable notification sounds"
    }
}

pub fn menu_action(id: &str) -> Option<TrayAction> {
    match id {
        "quill-tray-open" => Some(TrayAction::Open),
        "quill-tray-lock" => Some(TrayAction::Lock),
        "quill-tray-notifications" => Some(TrayAction::ToggleNotifications),
        "quill-tray-sounds" => Some(TrayAction::ToggleSounds),
        "quill-tray-quit" => Some(TrayAction::Quit),
        _ => None,
    }
}

/// Badge count, mirroring Telegram Desktop's `Session::computeUnreadBadge`
/// (`data/data_session.cpp`) over TDLib's server-side totals.
///
/// - The main list contributes TDLib's `updateUnreadMessageCount` (or
///   `updateUnreadChatCount` when `count_messages` is off): the unmuted
///   subset when `include_muted` is off, the full total otherwise.
/// - tdesktop folds the archive folder into the main list with everything
///   counted as muted, so the archive contributes its full total, and only
///   when muted chats are included. `include_archived` is Quill's extra
///   opt-out for it (default on, like tdesktop).
/// - A list whose totals haven't arrived yet (TDLib sends them only with a
///   message database, and after the first chat load) falls back to summing
///   the loaded chats of that list; a chat marked as unread counts as one.
///
/// The sum saturates.
pub fn badge_count(session: &Session, prefs: &BadgePrefs) -> u32 {
    let main = list_badge(session, prefs, false);
    let archive = if prefs.include_archived && prefs.include_muted {
        list_badge(session, prefs, true)
    } else {
        0
    };
    main.saturating_add(archive)
}

/// `QUILL_TRACE_STATUS=1`: note (once per list/mode) that the badge fell
/// back to summing loaded chats because TDLib's totals haven't arrived.
fn trace_fallback(archive: bool, messages: bool) {
    use std::sync::atomic::{AtomicU8, Ordering};
    static SEEN: AtomicU8 = AtomicU8::new(0);
    let bit = 1u8 << (u8::from(archive) * 2 + u8::from(messages));
    if std::env::var_os("QUILL_TRACE_STATUS").is_some()
        && SEEN.fetch_or(bit, Ordering::Relaxed) & bit == 0
    {
        eprintln!(
            "status: badge fallback to loaded-chat sum list={} count_messages={messages}",
            if archive { "archive" } else { "main" }
        );
    }
}

/// Unread of chats split into topics, on top of the list totals. tdesktop
/// replaces a forum chat's own state with its topics' (`AdjustedForumUnreadState`:
/// the topics' message sum, or one chat in chats mode), while TDLib's totals
/// know nothing of topics the user never opened. Whatever TDLib already counts
/// for the chat itself (`unread_count`, marked unread) is subtracted so
/// nothing is counted twice.
fn forum_topics_extra(session: &Session, prefs: &BadgePrefs, archive: bool) -> u32 {
    session
        .chats
        .values()
        .filter(|chat| chat.in_archive == archive)
        .filter(|chat| prefs.include_muted || !session.effective_muted(chat))
        .map(|chat| {
            let topics = session.forum_topics_unread(chat.id);
            if topics <= 0 {
                return 0;
            }
            if prefs.count_messages {
                (topics - chat.unread_count.max(0)).max(0) as u32
            } else {
                u32::from(chat.unread_count <= 0 && !chat.is_marked_as_unread)
            }
        })
        .fold(0u32, u32::saturating_add)
}

fn list_badge(session: &Session, prefs: &BadgePrefs, archive: bool) -> u32 {
    let totals = if archive {
        &session.chat_list.unread_totals.archive
    } else {
        &session.chat_list.unread_totals.main
    };
    let pair = if prefs.count_messages {
        totals.messages
    } else {
        totals.chats
    };
    let topics = forum_topics_extra(session, prefs, archive);
    if let Some(pair) = pair {
        // Archived chats always count as muted: take the full total.
        let value = if prefs.include_muted || archive {
            pair.all
        } else {
            pair.unmuted
        };
        return (value.max(0) as u32).saturating_add(topics);
    }
    trace_fallback(archive, prefs.count_messages);
    session
        .chats
        .values()
        .filter(|chat| chat.in_archive == archive)
        .filter(|chat| prefs.include_muted || !session.effective_muted(chat))
        .map(|chat| {
            if prefs.count_messages {
                if chat.unread_count > 0 {
                    chat.unread_count as u32
                } else {
                    u32::from(chat.is_marked_as_unread)
                }
            } else {
                u32::from(chat.unread_count > 0 || chat.is_marked_as_unread)
            }
        })
        .fold(0u32, u32::saturating_add)
        .saturating_add(topics)
}

/// The app icon downscaled to [`ICON_SIZE`] as straight (non-premultiplied)
/// RGBA, generated from `assets/icons/quill-1024-fullbleed.png` with
/// `magick quill-1024-fullbleed.png -resize 64x64 -depth 8 rgba:tray-64.rgba`.
/// Raw bytes keep the core crate free of an image decoder.
const BASE_ICON: &[u8; (ICON_SIZE * ICON_SIZE * 4) as usize] =
    include_bytes!("../../assets/icons/tray-64.rgba");

/// Monochrome glyph for the macOS menu bar as a 64x64 alpha mask, rasterized
/// from `assets/icons/tray-template.svg` with
/// `magick -background white -density 300 tray-template.svg -flatten
/// -resize 64x64 -colorspace Gray -negate -depth 8 gray:tray-template-64.a8`.
pub(crate) const TEMPLATE_MASK: &[u8; (ICON_SIZE * ICON_SIZE) as usize] =
    include_bytes!("../../assets/icons/tray-template-64.a8");

const BADGE_RED: [u8; 4] = [0xFF, 0x3B, 0x30, 0xFF];
const WHITE: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];
const BLACK: [u8; 4] = [0x00, 0x00, 0x00, 0xFF];
const CLEAR: [u8; 4] = [0x00, 0x00, 0x00, 0x00];

/// Colors of the unread pill: outer ring, fill, digits.
struct BadgeColors {
    ring: [u8; 4],
    fill: [u8; 4],
    digits: [u8; 4],
}

/// Rendered icon: raw RGBA bytes plus `(width, height)`.
pub fn render_tray_icon(unread: u32) -> (Vec<u8>, u32, u32) {
    let mut px = Pixels::new(ICON_SIZE);
    px.buf.copy_from_slice(BASE_ICON);
    if unread > 0 {
        draw_badge(
            &mut px,
            unread,
            &BadgeColors {
                ring: WHITE,
                fill: BADGE_RED,
                digits: WHITE,
            },
        );
    }
    (px.buf, ICON_SIZE, ICON_SIZE)
}

/// macOS menu-bar variant: a black glyph with alpha, handed to AppKit as a
/// template image so the system tints it black or white to match the menu
/// bar (like every other status item). The count is a solid pill with the
/// digits and a surrounding gap cut out, so it stays legible in one color.
pub fn render_tray_template(unread: u32) -> (Vec<u8>, u32, u32) {
    let mut px = Pixels::new(ICON_SIZE);
    for (i, alpha) in TEMPLATE_MASK.iter().enumerate() {
        px.buf[i * 4 + 3] = *alpha;
    }
    if unread > 0 {
        draw_badge(
            &mut px,
            unread,
            &BadgeColors {
                ring: CLEAR,
                fill: BLACK,
                digits: CLEAR,
            },
        );
    }
    (px.buf, ICON_SIZE, ICON_SIZE)
}

/// 5x7 bitmap glyphs for the badge digits, rows top-to-bottom, bit 4 = left.
/// Strokes are two columns wide so the count stays legible when the menu bar
/// or taskbar shrinks the 64 px icon to ~20 px.
fn glyph(ch: char) -> Option<[u8; 7]> {
    Some(match ch {
        '0' => [
            0b01110, 0b11011, 0b11011, 0b11011, 0b11011, 0b11011, 0b01110,
        ],
        '1' => [
            0b00110, 0b01110, 0b11110, 0b00110, 0b00110, 0b00110, 0b00110,
        ],
        '2' => [
            0b01110, 0b11011, 0b00011, 0b00110, 0b01100, 0b11000, 0b11111,
        ],
        '3' => [
            0b11110, 0b00011, 0b00011, 0b01110, 0b00011, 0b00011, 0b11110,
        ],
        '4' => [
            0b00110, 0b01110, 0b11010, 0b11011, 0b11111, 0b00011, 0b00011,
        ],
        '5' => [
            0b11111, 0b11000, 0b11110, 0b00011, 0b00011, 0b11011, 0b01110,
        ],
        '6' => [
            0b01110, 0b11000, 0b11110, 0b11011, 0b11011, 0b11011, 0b01110,
        ],
        '7' => [
            0b11111, 0b00011, 0b00110, 0b00110, 0b01100, 0b01100, 0b01100,
        ],
        '8' => [
            0b01110, 0b11011, 0b11011, 0b01110, 0b11011, 0b11011, 0b01110,
        ],
        '9' => [
            0b01110, 0b11011, 0b11011, 0b01111, 0b00011, 0b00011, 0b01110,
        ],
        '+' => [
            0b00000, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0b00000,
        ],
        _ => return None,
    })
}

/// Text shown in the badge: the count, capped at "99+".
pub(crate) fn badge_text(unread: u32) -> String {
    if unread > 99 {
        "99+".to_string()
    } else {
        unread.to_string()
    }
}

/// Tray tooltip / title: "Quill", or "Quill - N unread".
pub fn tray_tooltip(unread: u32) -> String {
    if unread == 0 {
        "Quill".to_string()
    } else {
        format!("Quill \u{2014} {unread} unread")
    }
}

/// Convert straight RGBA to the StatusNotifierItem pixmap layout: ARGB32 in
/// network byte order, i.e. bytes `[A, R, G, B]` per pixel.
#[cfg(any(target_os = "linux", test))]
pub fn rgba_to_argb32(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len());
    for px in rgba.as_chunks::<4>().0 {
        out.extend_from_slice(&[px[3], px[0], px[1], px[2]]);
    }
    out
}

/// Overlay badge for the Windows taskbar button (`SetOverlayIcon`): the red
/// count pill alone, vertically centered on a transparent canvas, so the
/// taskbar's ~16 px downscale keeps the digits as large as possible
/// (tdesktop draws its overlay the same way, `main_window_win.cpp`).
pub fn render_overlay_icon(unread: u32) -> (Vec<u8>, u32, u32) {
    let mut px = Pixels::new(ICON_SIZE);
    if unread > 0 {
        draw_badge_at(
            &mut px,
            unread,
            &BadgeColors {
                ring: WHITE,
                fill: BADGE_RED,
                digits: WHITE,
            },
            (ICON_SIZE - BADGE_HEIGHT) / 2,
        );
    }
    (px.buf, ICON_SIZE, ICON_SIZE)
}

/// Which tray-related switches the General settings may offer.
///
/// "Start in tray" and "run in the background" need a live tray icon: a
/// hidden or minimized window with no tray to reopen it from would strand
/// the user. The "Show tray icon" switch itself stays offered while the
/// user has turned the icon off (or it is up), so it can always be turned
/// back on; it is hidden only where the desktop has no tray host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraySettingSwitches {
    pub show_tray_icon: bool,
    pub start_in_tray: bool,
    pub run_in_background: bool,
}

pub fn tray_setting_switches(tray_available: bool, tray_enabled: bool) -> TraySettingSwitches {
    TraySettingSwitches {
        show_tray_icon: tray_available || !tray_enabled,
        start_in_tray: tray_available,
        run_in_background: tray_available,
    }
}

/// What closing the window does (the title-bar button, Ctrl/Cmd+W).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseOutcome {
    /// Close the window and quit.
    Quit,
    /// macOS: hide the whole app; the tray and Dock keep it reachable.
    HideApp,
    /// Windows and X11: hide the window (off the taskbar and the switcher);
    /// the tray icon's "Open Quill" shows it again
    /// (`ui/window_control.rs`).
    Hide,
    /// Wayland: xdg-shell cannot hide a toplevel, so the window is
    /// minimized and "Open Quill" asks the compositor to raise it.
    Minimize,
}

/// tdesktop "Run in the background" (`CloseBehavior::RunInBackground`): the
/// window close button keeps the app alive when a tray icon exists.
/// `hide_supported` is what `ui::window_control::hide_supported` says of
/// the window.
pub fn close_outcome(
    run_in_background: bool,
    tray_available: bool,
    macos: bool,
    hide_supported: bool,
) -> CloseOutcome {
    match (run_in_background && tray_available, macos, hide_supported) {
        (false, _, _) => CloseOutcome::Quit,
        (true, true, _) => CloseOutcome::HideApp,
        (true, false, true) => CloseOutcome::Hide,
        (true, false, false) => CloseOutcome::Minimize,
    }
}

/// The "Show tray icon" preference, mirrored here so the 1s tray timer needs
/// no handle on the app. Set from `set_appearance` and at startup.
static TRAY_ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub fn set_tray_enabled(enabled: bool) {
    TRAY_ENABLED.store(enabled, std::sync::atomic::Ordering::Relaxed);
}

pub fn tray_enabled() -> bool {
    TRAY_ENABLED.load(std::sync::atomic::Ordering::Relaxed)
}

/// Red pill in the top-right corner, sized to take up most of the icon so the
/// count reads at menu-bar size: 4 px glyph cells for one or two characters,
/// 3 px for "99+", with a ring to separate it from the icon art.
fn draw_badge(px: &mut Pixels, unread: u32, colors: &BadgeColors) {
    draw_badge_at(px, unread, colors, 0);
}

/// Height of the unread pill in pixels.
const BADGE_HEIGHT: u32 = 38;

/// [`draw_badge`] with the pill's top edge at row `y0`.
fn draw_badge_at(px: &mut Pixels, unread: u32, colors: &BadgeColors, y0: u32) {
    let text = badge_text(unread);
    let n = text.len() as u32;
    let scale: u32 = if n >= 3 { 3 } else { 4 };
    let gap = scale;
    let glyph_w = 5 * scale;
    let text_w = n * glyph_w + (n - 1) * gap;
    let text_h = 7 * scale;
    let pill_h: u32 = BADGE_HEIGHT;
    let pill_w = (text_w + 14).max(pill_h).min(ICON_SIZE);
    let ring: u32 = 2;
    let x0 = ICON_SIZE - pill_w;
    px.rounded_rect(x0, y0, pill_w, pill_h, pill_h / 2, colors.ring);
    px.rounded_rect(
        x0 + ring,
        y0 + ring,
        pill_w - 2 * ring,
        pill_h - 2 * ring,
        (pill_h - 2 * ring) / 2,
        colors.fill,
    );
    let gx = x0 + (pill_w - text_w) / 2;
    let gy = y0 + (pill_h - text_h) / 2;
    for (i, ch) in text.chars().enumerate() {
        let x = gx + i as u32 * (glyph_w + gap);
        draw_glyph(px, ch, x, gy, scale, colors.digits);
    }
}

fn draw_glyph(px: &mut Pixels, ch: char, x0: u32, y0: u32, scale: u32, color: [u8; 4]) {
    let rows = glyph(ch).unwrap_or([0; 7]);
    for (ry, row) in rows.iter().enumerate() {
        for rx in 0..5 {
            if (row >> (4 - rx)) & 1 == 1 {
                for dy in 0..scale {
                    for dx in 0..scale {
                        px.set(x0 + rx * scale + dx, y0 + ry as u32 * scale + dy, color);
                    }
                }
            }
        }
    }
}

/// Minimal RGBA canvas; every pixel starts fully transparent.
struct Pixels {
    buf: Vec<u8>,
    size: u32,
}

impl Pixels {
    fn new(size: u32) -> Self {
        Self {
            buf: vec![0; (size * size * 4) as usize],
            size,
        }
    }

    fn set(&mut self, x: u32, y: u32, c: [u8; 4]) {
        if x < self.size && y < self.size {
            let i = ((y * self.size + x) * 4) as usize;
            self.buf[i..i + 4].copy_from_slice(&c);
        }
    }

    fn rounded_rect(&mut self, x0: u32, y0: u32, w: u32, h: u32, r: u32, c: [u8; 4]) {
        let (x0, y0, w, h, r) = (x0 as i64, y0 as i64, w as i64, h as i64, r as i64);
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                let dx = (x0 + r - x).max(0).max(x - (x0 + w - 1 - r));
                let dy = (y0 + r - y).max(0).max(y - (y0 + h - 1 - r));
                if dx * dx + dy * dy <= r * r {
                    self.set(x as u32, y as u32, c);
                }
            }
        }
    }
}

/// Live tray handle (macOS / Windows via `tray-icon`). Created lazily on the
/// UI thread; construction fails (returning `None`) when the OS has no
/// system tray. Linux uses [`crate::tray_sni`] instead: `tray-icon` there
/// needs a GTK main loop that a GPUI process does not run.
#[cfg(all(feature = "ui", not(target_os = "linux")))]
pub struct Tray {
    icon: tray_icon::TrayIcon,
    notifications_item: tray_icon::menu::MenuItem,
    sounds_item: tray_icon::menu::MenuItem,
    /// Last labels set: (notifications on, sounds on).
    toggles_shown: Option<(bool, bool)>,
    /// Last drawn (unread, all-muted, dark menu bar) — redraw on change.
    last_shown: Option<(u32, bool, bool)>,
}

#[cfg(all(feature = "ui", not(target_os = "linux")))]
impl Tray {
    fn new() -> Option<Self> {
        #[cfg(target_os = "macos")]
        use tray_icon::menu::ContextMenu;
        use tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem};
        let menu = Menu::new();
        #[cfg(target_os = "macos")]
        unsafe {
            // Tray commands stay enabled even while the app has no active window.
            let native = &*menu.ns_menu().cast::<objc2_app_kit::NSMenu>();
            native.setAutoenablesItems(false);
        }
        // tdesktop's tray menu: Open / Disable notifications / Quit, plus
        // Quill's notification-sounds toggle next to it.
        let notifications_item = MenuItem::with_id(
            "quill-tray-notifications",
            notifications_label(true),
            true,
            None,
        );
        let sounds_item = MenuItem::with_id("quill-tray-sounds", sounds_label(true), true, None);
        menu.append_items(&[
            &MenuItem::with_id("quill-tray-open", "Open Quill", true, None),
            &MenuItem::with_id("quill-tray-lock", "Lock Quill", true, None),
            &notifications_item,
            &sounds_item,
            &PredefinedMenuItem::separator(),
            &MenuItem::with_id("quill-tray-quit", "Quit Quill", true, None),
        ])
        .ok()?;
        let (rgba, w, h, _) = render_platform_icon(0, false, true);
        let icon = tray_icon::Icon::from_rgba(rgba, w, h).ok()?;
        let builder = tray_icon::TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Quill");
        // Templated icons exist on macOS only.
        #[cfg(target_os = "macos")]
        let builder = builder.with_icon_templated(icon);
        #[cfg(not(target_os = "macos"))]
        let builder = builder.with_icon(icon);
        let tray = builder.build().ok()?;
        Some(Self {
            icon: tray,
            notifications_item,
            sounds_item,
            toggles_shown: None,
            last_shown: None,
        })
    }

    fn set_toggles(&mut self, notifications: bool, sounds: bool) {
        if self.toggles_shown == Some((notifications, sounds)) {
            return;
        }
        self.toggles_shown = Some((notifications, sounds));
        self.notifications_item
            .set_text(notifications_label(notifications));
        self.sounds_item.set_text(sounds_label(sounds));
    }

    fn set_unread(&mut self, unread: u32, muted: bool) {
        #[cfg(target_os = "macos")]
        let dark = self
            .icon
            .ns_status_item()
            .is_none_or(|item| crate::tray_mac::menu_bar_is_dark(&item));
        #[cfg(not(target_os = "macos"))]
        let dark = false;
        let key = (unread, muted, dark);
        if self.last_shown == Some(key) {
            return;
        }
        self.last_shown = Some(key);
        let (rgba, w, h, template) = render_platform_icon(unread, muted, dark);
        if let Ok(icon) = tray_icon::Icon::from_rgba(rgba, w, h) {
            #[cfg(target_os = "macos")]
            let _ = if template {
                self.icon.set_icon_templated(Some(icon))
            } else {
                self.icon.set_icon(Some(icon))
            };
            #[cfg(not(target_os = "macos"))]
            let _ = {
                let _ = template;
                self.icon.set_icon(Some(icon))
            };
        }
        let _ = self.icon.set_tooltip(Some(tray_tooltip(unread)));
    }
}

/// macOS: the monochrome template while nothing is unread, and tdesktop's
/// tinted glyph + red counter otherwise ([`crate::tray_mac`]). Windows trays
/// show the full-color app icon. Returns the RGBA, size and whether AppKit
/// should treat it as a template.
#[cfg(all(feature = "ui", not(target_os = "linux")))]
fn render_platform_icon(unread: u32, muted: bool, dark: bool) -> (Vec<u8>, u32, u32, bool) {
    #[cfg(target_os = "macos")]
    {
        if unread == 0 {
            let (rgba, w, h) = render_tray_template(0);
            return (rgba, w, h, true);
        }
        let (rgba, w, h) = crate::tray_mac::render(TEMPLATE_MASK, unread, muted, dark);
        (rgba, w, h, false)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (muted, dark);
        let (rgba, w, h) = render_tray_icon(unread);
        (rgba, w, h, false)
    }
}

/// tdesktop's `unreadBadgeMuted`: every counted unread chat is muted.
#[cfg(feature = "ui")]
fn badge_all_muted(session: &Session, unread: u32) -> bool {
    if unread == 0 || !session.settings.badge_prefs.include_muted {
        return false;
    }
    let unmuted = BadgePrefs {
        include_muted: false,
        ..session.settings.badge_prefs
    };
    badge_count(session, &unmuted) == 0
}

#[cfg(all(feature = "ui", not(target_os = "linux")))]
thread_local! {
    /// UI-thread tray handle. `tray_icon::TrayIcon` is `!Send` on some
    /// platforms (macOS), so a process-wide static cannot hold it; every
    /// call to `sync_tray` comes from the GPUI main thread anyway.
    static TRAY: std::cell::RefCell<Option<Tray>> = const { std::cell::RefCell::new(None) };
}

#[cfg(all(feature = "ui", target_os = "linux"))]
thread_local! {
    /// UI-thread StatusNotifierItem lifecycle (registration runs on a worker).
    static TRAY: std::cell::RefCell<crate::tray_sni::TrayState> =
        std::cell::RefCell::new(crate::tray_sni::TrayState::new());
}

/// Sync the system tray icon with the session's unread count. Called from a
/// 1s timer in `main.rs` on the UI thread; a no-op when the count is
/// unchanged or no system tray exists.
#[cfg(all(feature = "ui", not(target_os = "linux")))]
pub fn sync_tray(session: Option<&Session>) {
    let unread = session
        .map(|s| badge_count(s, &s.settings.badge_prefs))
        .unwrap_or(0);
    let muted = session.is_some_and(|s| badge_all_muted(s, unread));
    let notifications = session.is_none_or(|s| s.settings.desktop_notifications);
    let sounds = session.is_none_or(|s| s.settings.inapp_sounds_enabled);
    TRAY.with(|cell| {
        let mut slot = cell.borrow_mut();
        if !tray_enabled() {
            // "Show tray icon" is off: dropping the handle removes the icon.
            *slot = None;
            return;
        }
        // AppKit can return a status-item handle before its native window
        // exists during launch. Retry on the running event loop instead of
        // treating an invisible handle as a reachable tray.
        #[cfg(target_os = "macos")]
        if slot.as_ref().is_some_and(|tray| tray.icon.rect().is_none()) {
            *slot = None;
        }
        if slot.is_none() {
            // Retry until it succeeds: a system tray that appears after startup
            // (e.g. the notifier host starting late) is picked up on the next tick.
            *slot = Tray::new();
        }
        if let Some(tray) = slot.as_mut() {
            tray.set_unread(unread, muted);
            tray.set_toggles(notifications, sounds);
        }
    });
}

/// Linux: drive the StatusNotifierItem lifecycle (see [`crate::tray_sni`]).
#[cfg(all(feature = "ui", target_os = "linux"))]
pub fn sync_tray(session: Option<&Session>) {
    let unread = session
        .map(|s| badge_count(s, &s.settings.badge_prefs))
        .unwrap_or(0);
    let toggles = (
        session.is_none_or(|s| s.settings.desktop_notifications),
        session.is_none_or(|s| s.settings.inapp_sounds_enabled),
    );
    TRAY.with(|cell| {
        let mut state = cell.borrow_mut();
        if tray_enabled() {
            state.poll(unread, toggles);
        } else {
            state.hide();
        }
    });
}

/// First sync at window creation. macOS/Windows create the tray
/// synchronously; Linux only starts the StatusNotifierItem registration on a
/// worker thread (never blocking the UI thread) and the caller watches
/// [`tray_registering`] / [`tray_available`] to learn the outcome.
#[cfg(all(feature = "ui", not(target_os = "linux")))]
pub fn sync_tray_startup(session: Option<&Session>) {
    sync_tray(session);
}

#[cfg(all(feature = "ui", target_os = "linux"))]
pub fn sync_tray_startup(session: Option<&Session>) {
    sync_tray(session);
}

/// Whether the tray registration is still in flight (Linux only; the other
/// platforms create the tray synchronously).
#[cfg(all(feature = "ui", not(target_os = "linux")))]
pub fn tray_registering() -> bool {
    false
}

#[cfg(all(feature = "ui", target_os = "linux"))]
pub fn tray_registering() -> bool {
    TRAY.with(|cell| cell.borrow().registering())
}

/// How long start-in-tray waits for the tray host before revealing the window.
pub const TRAY_STARTUP_WAIT_MS: u64 = 1500;

/// Start-in-tray outcome check, run repeatedly after launch. `Some(true)`:
/// no tray host, reveal the window so it is never unreachable. `Some(false)`:
/// the tray is up, stay hidden. `None`: still registering, keep waiting.
pub fn tray_startup_reveal(available: bool, registering: bool, waited_ms: u64) -> Option<bool> {
    if available {
        Some(false)
    } else if !registering || waited_ms >= TRAY_STARTUP_WAIT_MS {
        Some(true)
    } else {
        None
    }
}

/// Whether a tray icon is currently shown. The close/minimize/start-in-tray
/// switches are only offered when this is true ([`tray_setting_switches`]).
#[cfg(all(feature = "ui", not(target_os = "linux")))]
pub fn tray_available() -> bool {
    TRAY.with(|cell| cell.borrow().is_some())
}

#[cfg(all(feature = "ui", target_os = "linux"))]
pub fn tray_available() -> bool {
    TRAY.with(|cell| cell.borrow().available())
}

#[cfg(all(feature = "ui", not(target_os = "linux")))]
pub fn take_tray_actions() -> Vec<TrayAction> {
    tray_icon::menu::MenuEvent::receiver()
        .try_iter()
        .filter_map(|event| menu_action(event.id.as_ref()))
        .collect()
}

#[cfg(all(feature = "ui", target_os = "linux"))]
pub fn take_tray_actions() -> Vec<TrayAction> {
    crate::tray_sni::take_actions()
}

#[cfg(test)]
mod tests;
