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
//! The OS tray itself is managed behind `#[cfg(feature = "ui")]` with the
//! `tray-icon` crate. [`sync_tray`] is called from a 1s timer in `main.rs`;
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
    Quit,
}

pub fn menu_action(id: &str) -> Option<TrayAction> {
    match id {
        "quill-tray-open" => Some(TrayAction::Open),
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

fn list_badge(session: &Session, prefs: &BadgePrefs, archive: bool) -> u32 {
    let totals = if archive {
        &session.unread_totals.archive
    } else {
        &session.unread_totals.main
    };
    let pair = if prefs.count_messages {
        totals.messages
    } else {
        totals.chats
    };
    if let Some(pair) = pair {
        // Archived chats always count as muted: take the full total.
        let value = if prefs.include_muted || archive {
            pair.all
        } else {
            pair.unmuted
        };
        return value.max(0) as u32;
    }
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
}

/// The app icon downscaled to [`ICON_SIZE`] as straight (non-premultiplied)
/// RGBA, generated from `assets/icons/quill-1024-fullbleed.png` with
/// `magick quill-1024-fullbleed.png -resize 64x64 -depth 8 rgba:tray-64.rgba`.
/// Raw bytes keep the core crate free of an image decoder.
const BASE_ICON: &[u8; (ICON_SIZE * ICON_SIZE * 4) as usize] =
    include_bytes!("../assets/icons/tray-64.rgba");

/// Monochrome glyph for the macOS menu bar as a 64x64 alpha mask, rasterized
/// from `assets/icons/tray-template.svg` with
/// `magick -background white -density 300 tray-template.svg -flatten
/// -resize 64x64 -colorspace Gray -negate -depth 8 gray:tray-template-64.a8`.
const TEMPLATE_MASK: &[u8; (ICON_SIZE * ICON_SIZE) as usize] =
    include_bytes!("../assets/icons/tray-template-64.a8");

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
fn badge_text(unread: u32) -> String {
    if unread > 99 {
        "99+".to_string()
    } else {
        unread.to_string()
    }
}

/// Red pill in the top-right corner, sized to take up most of the icon so the
/// count reads at menu-bar size: 4 px glyph cells for one or two characters,
/// 3 px for "99+", with a ring to separate it from the icon art.
fn draw_badge(px: &mut Pixels, unread: u32, colors: &BadgeColors) {
    let text = badge_text(unread);
    let n = text.len() as u32;
    let scale: u32 = if n >= 3 { 3 } else { 4 };
    let gap = scale;
    let glyph_w = 5 * scale;
    let text_w = n * glyph_w + (n - 1) * gap;
    let text_h = 7 * scale;
    let pill_h: u32 = 38;
    let pill_w = (text_w + 14).max(pill_h).min(ICON_SIZE);
    let ring: u32 = 2;
    let x0 = ICON_SIZE - pill_w;
    let y0 = 0;
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

/// Live tray handle. Created lazily on the UI thread; construction fails
/// (returning `None`) when the OS has no system tray.
#[cfg(feature = "ui")]
pub struct Tray {
    icon: tray_icon::TrayIcon,
    last_shown: Option<u32>,
}

#[cfg(feature = "ui")]
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
        menu.append_items(&[
            &MenuItem::with_id("quill-tray-open", "Open Quill", true, None),
            &PredefinedMenuItem::separator(),
            &MenuItem::with_id("quill-tray-quit", "Quit Quill", true, None),
        ])
        .ok()?;
        let (rgba, w, h) = render_platform_icon(0);
        let icon = tray_icon::Icon::from_rgba(rgba, w, h).ok()?;
        let tray = tray_icon::TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Quill")
            .with_icon(icon)
            .with_icon_as_template(cfg!(target_os = "macos"))
            .build()
            .ok()?;
        Some(Self {
            icon: tray,
            last_shown: Some(0),
        })
    }

    fn set_unread(&mut self, unread: u32) {
        if self.last_shown == Some(unread) {
            return;
        }
        self.last_shown = Some(unread);
        let (rgba, w, h) = render_platform_icon(unread);
        if let Ok(icon) = tray_icon::Icon::from_rgba(rgba, w, h) {
            let _ = self
                .icon
                .set_icon_with_as_template(Some(icon), cfg!(target_os = "macos"));
        }
        let tooltip = if unread == 0 {
            "Quill".to_string()
        } else {
            format!("Quill — {unread} unread")
        };
        let _ = self.icon.set_tooltip(Some(tooltip));
    }
}

/// The macOS menu bar takes a monochrome template; Windows and Linux trays
/// show the full-color app icon.
#[cfg(feature = "ui")]
fn render_platform_icon(unread: u32) -> (Vec<u8>, u32, u32) {
    if cfg!(target_os = "macos") {
        render_tray_template(unread)
    } else {
        render_tray_icon(unread)
    }
}

#[cfg(feature = "ui")]
thread_local! {
    /// UI-thread tray handle. `tray_icon::TrayIcon` is `!Send` on some
    /// platforms (macOS), so a process-wide static cannot hold it; every
    /// call to `sync_tray` comes from the GPUI main thread anyway.
    static TRAY: std::cell::RefCell<Option<Tray>> = const { std::cell::RefCell::new(None) };
}

/// Sync the system tray icon with the session's unread count. Called from a
/// 1s timer in `main.rs` on the UI thread; a no-op when the count is
/// unchanged or no system tray exists.
#[cfg(feature = "ui")]
pub fn sync_tray(session: Option<&Session>) {
    let unread = session.map(|s| badge_count(s, &s.badge_prefs)).unwrap_or(0);
    TRAY.with(|cell| {
        let mut slot = cell.borrow_mut();
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
            tray.set_unread(unread);
        }
    });
}

#[cfg(feature = "ui")]
pub fn tray_available() -> bool {
    TRAY.with(|cell| cell.borrow().is_some())
}

#[cfg(feature = "ui")]
pub fn take_tray_actions() -> Vec<TrayAction> {
    tray_icon::menu::MenuEvent::receiver()
        .try_iter()
        .filter_map(|event| menu_action(event.id.as_ref()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tray_menu_routes_only_its_own_actions() {
        assert_eq!(menu_action("quill-tray-open"), Some(TrayAction::Open));
        assert_eq!(menu_action("quill-tray-quit"), Some(TrayAction::Quit));
        assert_eq!(menu_action("quit"), None);
        assert_eq!(menu_action(""), None);
    }
    use crate::chatlist_style::ChatPreviewStyle;
    use crate::diagnostics::MemorySink;
    use crate::ids::{AccountKey, ChatId, MessageId};
    use crate::settings::BadgePrefs;
    use crate::state::{ChatSummary, ListUnreadTotals, Session, UnreadPair};
    use crate::telegram::envelope::{ChatKind, ChatNotificationSettings};
    use std::collections::BTreeMap;
    use std::sync::Arc;

    fn chat(id: i64, unread_count: i32) -> ChatSummary {
        ChatSummary {
            id: ChatId(id),
            title: format!("chat {id}"),
            kind: ChatKind::Unknown,
            unread_count,
            last_read_inbox_message_id: MessageId(0),
            last_read_outbox_message_id: MessageId(0),
            order: 0,
            is_pinned: false,
            in_main_list: false,
            in_archive: false,
            archive_order: 0,
            archive_is_pinned: false,
            folder_positions: BTreeMap::new(),
            notification_settings: ChatNotificationSettings::default(),
            last_preview: String::new(),
            last_preview_style: ChatPreviewStyle::default(),
            last_preview_sender: String::new(),
            last_preview_thumb: None,
            last_message: None,
            sender_actions: Vec::new(),
            draft: None,
            my_member_status: None,
            my_admin_can_post_messages: None,
            my_admin_can_invite_users: None,
            my_admin_can_change_info: None,
            my_admin_can_send_welcome_messages: None,
            my_admin_can_promote_members: None,
            my_admin_can_restrict_members: None,
            my_admin_can_pin_messages: None,
            is_forum: None,
            photo_file_id: None,
            can_send_basic_messages: true,
            permissions: None,
            can_be_deleted_for_all_users: false,
            can_be_deleted_only_for_self: false,
            is_marked_as_unread: false,
            unread_mention_count: 0,
            unread_reaction_count: 0,
            can_be_reported: false,
            blocked: false,
            secret_state: None,
            message_auto_delete_time: 0,
            video_chat: None,
        }
    }

    fn session_with(counts: &[i32]) -> Session {
        let mut session = Session::new(AccountKey("tray-test".into()), Arc::new(MemorySink::new()));
        for (i, &unread_count) in counts.iter().enumerate() {
            session.chats.insert(i as i64, chat(i as i64, unread_count));
        }
        session
    }

    fn muted_chat(id: i64, unread_count: i32) -> ChatSummary {
        let mut c = chat(id, unread_count);
        c.notification_settings = c.notification_settings.with_mute_for(i32::MAX);
        c
    }

    fn archived_chat(id: i64, unread_count: i32) -> ChatSummary {
        let mut c = chat(id, unread_count);
        c.in_archive = true;
        c
    }

    #[test]
    fn badge_count_sums_and_ignores_negative() {
        let session = session_with(&[3, 0, 7, -5]);
        assert_eq!(badge_count(&session, &BadgePrefs::default()), 10);
    }

    #[test]
    fn badge_count_includes_muted_chats_by_default() {
        // Muted chats count: Telegram Desktop's badge includes them by
        // default (`_includeMutedCounter = true`).
        let mut session = Session::new(
            AccountKey("tray-test-muted".into()),
            Arc::new(MemorySink::new()),
        );
        session.chats.insert(0, chat(0, 5));
        session.chats.insert(1, muted_chat(1, 9));
        assert_eq!(badge_count(&session, &BadgePrefs::default()), 14);
    }

    #[test]
    fn badge_count_excludes_muted_when_opted_out() {
        let mut session = Session::new(
            AccountKey("tray-test-no-muted".into()),
            Arc::new(MemorySink::new()),
        );
        session.chats.insert(0, chat(0, 5));
        session.chats.insert(1, muted_chat(1, 9));
        let prefs = BadgePrefs {
            include_muted: false,
            ..BadgePrefs::default()
        };
        assert_eq!(badge_count(&session, &prefs), 5);
    }

    #[test]
    fn badge_count_includes_archived_chats_by_default() {
        // tdesktop folds the all-muted archive into the main list, so it
        // counts while muted chats do.
        let mut session = Session::new(
            AccountKey("tray-test-archived".into()),
            Arc::new(MemorySink::new()),
        );
        session.chats.insert(0, chat(0, 5));
        session.chats.insert(1, archived_chat(1, 9));
        assert_eq!(badge_count(&session, &BadgePrefs::default()), 14);
    }

    #[test]
    fn badge_count_excludes_archived_when_opted_out_or_muted_excluded() {
        let mut session = Session::new(
            AccountKey("tray-test-archived-out".into()),
            Arc::new(MemorySink::new()),
        );
        session.chats.insert(0, chat(0, 5));
        session.chats.insert(1, archived_chat(1, 9));
        let no_archive = BadgePrefs {
            include_archived: false,
            ..BadgePrefs::default()
        };
        assert_eq!(badge_count(&session, &no_archive), 5);
        let no_muted = BadgePrefs {
            include_muted: false,
            ..BadgePrefs::default()
        };
        assert_eq!(badge_count(&session, &no_muted), 5);
    }

    #[test]
    fn badge_count_marked_as_unread_counts_one_in_fallback() {
        let mut session = session_with(&[0, 2]);
        session.chats.get_mut(&0).unwrap().is_marked_as_unread = true;
        assert_eq!(badge_count(&session, &BadgePrefs::default()), 3);
        let prefs = BadgePrefs {
            count_messages: false,
            ..BadgePrefs::default()
        };
        assert_eq!(badge_count(&session, &prefs), 2);
    }

    fn with_totals(session: &mut Session) {
        // Loaded chats are a small slice; the server totals are what count.
        session.chats.insert(0, chat(0, 1));
        session.unread_totals.main = ListUnreadTotals {
            messages: Some(UnreadPair {
                all: 44,
                unmuted: 13,
            }),
            chats: Some(UnreadPair {
                all: 20,
                unmuted: 6,
            }),
        };
        session.unread_totals.archive = ListUnreadTotals {
            messages: Some(UnreadPair {
                all: 30,
                unmuted: 0,
            }),
            chats: Some(UnreadPair { all: 9, unmuted: 0 }),
        };
    }

    #[test]
    fn badge_count_uses_server_totals_over_loaded_chats() {
        let mut session = Session::new(AccountKey("t-totals".into()), Arc::new(MemorySink::new()));
        with_totals(&mut session);
        // Default: messages, muted + archive (all-muted) included.
        assert_eq!(badge_count(&session, &BadgePrefs::default()), 44 + 30);
        let chats = BadgePrefs {
            count_messages: false,
            ..BadgePrefs::default()
        };
        assert_eq!(badge_count(&session, &chats), 20 + 9);
        let unmuted = BadgePrefs {
            include_muted: false,
            ..BadgePrefs::default()
        };
        assert_eq!(badge_count(&session, &unmuted), 13);
        let no_archive = BadgePrefs {
            include_archived: false,
            ..BadgePrefs::default()
        };
        assert_eq!(badge_count(&session, &no_archive), 44);
    }

    #[test]
    fn badge_count_totals_fall_back_per_list() {
        // Only the main totals arrived: the archive sums its loaded chats.
        let mut session = Session::new(AccountKey("t-partial".into()), Arc::new(MemorySink::new()));
        session.chats.insert(1, archived_chat(1, 4));
        session.unread_totals.main.messages = Some(UnreadPair {
            all: 10,
            unmuted: 7,
        });
        assert_eq!(badge_count(&session, &BadgePrefs::default()), 14);
    }

    #[test]
    fn badge_count_can_count_chats_instead_of_messages() {
        let session = session_with(&[3, 0, 7, -5]);
        let prefs = BadgePrefs {
            count_messages: false,
            ..BadgePrefs::default()
        };
        assert_eq!(badge_count(&session, &prefs), 2);
    }

    #[test]
    fn badge_count_saturates() {
        let session = session_with(&[i32::MAX, i32::MAX, i32::MAX]);
        assert_eq!(badge_count(&session, &BadgePrefs::default()), u32::MAX);
    }

    #[test]
    fn icon_is_64x64_rgba() {
        let (rgba, w, h) = render_tray_icon(7);
        assert_eq!((w, h), (ICON_SIZE, ICON_SIZE));
        assert_eq!(rgba.len(), (ICON_SIZE * ICON_SIZE * 4) as usize);
    }

    fn red_pixel_count(rgba: &[u8]) -> usize {
        let (chunks, _) = rgba.as_chunks::<4>();
        chunks
            .iter()
            .filter(|px| px[0] == 0xFF && px[1] == 0x3B && px[2] == 0x30)
            .count()
    }

    #[test]
    fn badge_red_only_when_unread() {
        let (rgba, _, _) = render_tray_icon(5);
        assert!(red_pixel_count(&rgba) > 100);
        let (rgba, _, _) = render_tray_icon(0);
        assert_eq!(red_pixel_count(&rgba), 0);
    }

    #[test]
    fn template_is_black_with_alpha_and_cuts_digits_out() {
        let (plain, _, _) = render_tray_template(0);
        let (chunks, _) = plain.as_chunks::<4>();
        assert!(chunks.iter().all(|p| p[0] == 0 && p[1] == 0 && p[2] == 0));
        assert!(chunks.iter().any(|p| p[3] == 0xFF), "glyph is drawn");
        let (badged, _, _) = render_tray_template(7);
        assert_ne!(plain, badged);
        let (chunks, _) = badged.as_chunks::<4>();
        assert!(chunks.iter().all(|p| p[0] == 0 && p[1] == 0 && p[2] == 0));
    }

    #[test]
    fn badge_caps_at_99_plus() {
        // Renders without panic; the capped badge differs from the 99 badge.
        let (a, _, _) = render_tray_icon(99);
        let (b, _, _) = render_tray_icon(150);
        assert_ne!(a, b);
        assert!(red_pixel_count(&b) > 100);
    }
}
