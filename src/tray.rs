//! System tray icon with an unread-count badge (parity:platform-tray-icon).
//!
//! The icon is drawn programmatically as 64x64 RGBA (no asset files): a blue
//! rounded square with a white paper-plane glyph, plus a red pill badge with
//! the unread count (capped at "99+") whenever any chat is unread. The badge
//! sums every chat's `unread_count` except archived chats. Muted chats ARE
//! included — that is Telegram Desktop's actual default: its Notifications
//! toggle "Include muted chats" (`lng_settings_include_muted`) is ON by
//! default (`_includeMutedCounter = true`, tdesktop
//! `Telegram/SourceFiles/core/core_settings.h`), i.e. excluding muted is the
//! opt-out, not the opt-in. (Telegram X's launcher badge excludes muted by
//! default, but this is a desktop tray icon — Telegram Desktop is the
//! platform-appropriate reference.) Archived chats are excluded, matching
//! both clients (TD badges the main chats list; TGX's `BADGE_FLAG_ARCHIVED`
//! is off by default). The sum saturates instead of overflowing.
//!
//! The OS tray itself is managed behind `#[cfg(feature = "ui")]` with the
//! `tray-icon` crate. [`sync_tray`] is called from a 1s timer in `main.rs`;
//! it re-renders only when the count changes and silently no-ops when the OS
//! has no system tray (a tray appearing later is picked up on the next sync —
//! the handle is re-created until it succeeds).

use crate::state::Session;

/// Icon edge length in pixels.
pub const ICON_SIZE: u32 = 64;

/// Total unread messages across non-archived chats. Negative per-chat counts
/// (shouldn't happen) are ignored; muted chats are included (Telegram
/// Desktop's default — `_includeMutedCounter = true`); the sum saturates.
pub fn total_unread(session: &Session) -> u32 {
    session
        .chats
        .values()
        .filter(|chat| !chat.in_archive)
        .map(|chat| chat.unread_count.max(0) as u32)
        .fold(0u32, u32::saturating_add)
}

/// Rendered icon: raw RGBA bytes plus `(width, height)`.
pub fn render_tray_icon(unread: u32) -> (Vec<u8>, u32, u32) {
    let mut px = Pixels::new(ICON_SIZE);
    // Blue rounded-square base.
    px.rounded_rect(0, 0, ICON_SIZE, ICON_SIZE, 14, [0x22, 0x9E, 0xD9, 0xFF]);
    // White paper-plane glyph with a subtle fold.
    px.triangle((44, 15), (15, 31), (27, 47), [0xFF, 0xFF, 0xFF, 0xFF]);
    px.triangle((44, 15), (27, 47), (31, 35), [0xD6, 0xEC, 0xF7, 0xFF]);
    if unread > 0 {
        draw_badge(&mut px, unread);
    }
    (px.buf, ICON_SIZE, ICON_SIZE)
}

/// 3x5 bitmap glyphs for the badge digits, rows top-to-bottom, bit 2 = left.
fn glyph(ch: char) -> Option<[u8; 5]> {
    Some(match ch {
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        '3' => [0b111, 0b001, 0b111, 0b001, 0b111],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        '6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b001, 0b010, 0b010],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        '+' => [0b000, 0b010, 0b111, 0b010, 0b000],
        _ => return None,
    })
}

fn draw_badge(px: &mut Pixels, unread: u32) {
    let text = if unread > 99 {
        "99+".to_string()
    } else {
        unread.to_string()
    };
    const SCALE: u32 = 3;
    const ADVANCE: u32 = 3 * SCALE + 3; // glyph width + tracking
    let text_w = text.len() as u32 * ADVANCE - 3;
    let pill_w = text_w + 16;
    let pill_h = 30;
    // Top-right, clamped so the pill's right edge never leaves the icon.
    let cx: u32 = 47;
    let x0 = cx
        .saturating_sub(pill_w / 2)
        .min(ICON_SIZE.saturating_sub(pill_w));
    let cy: u32 = 17;
    let y0 = cy.saturating_sub(pill_h / 2);
    px.rounded_rect(x0, y0, pill_w, pill_h, pill_h / 2, [0xFF, 0x3B, 0x30, 0xFF]);
    let gx = x0 + (pill_w - text_w) / 2;
    let gy = y0 + (pill_h - 5 * SCALE) / 2;
    for (i, ch) in text.chars().enumerate() {
        draw_glyph(px, ch, gx + i as u32 * ADVANCE, gy, SCALE);
    }
}

fn draw_glyph(px: &mut Pixels, ch: char, x0: u32, y0: u32, scale: u32) {
    let rows = glyph(ch).unwrap_or([0; 5]);
    for (ry, row) in rows.iter().enumerate() {
        for rx in 0..3 {
            if (row >> (2 - rx)) & 1 == 1 {
                for dy in 0..scale {
                    for dx in 0..scale {
                        px.set(
                            x0 + rx * scale + dx,
                            y0 + ry as u32 * scale + dy,
                            [0xFF, 0xFF, 0xFF, 0xFF],
                        );
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

    fn triangle(&mut self, a: (u32, u32), b: (u32, u32), c: (u32, u32), col: [u8; 4]) {
        let (ax, ay) = (a.0 as i64, a.1 as i64);
        let (bx, by) = (b.0 as i64, b.1 as i64);
        let (cx, cy) = (c.0 as i64, c.1 as i64);
        let sign = |px: i64, py: i64, x1: i64, y1: i64, x2: i64, y2: i64| {
            (px - x2) * (y1 - y2) - (x1 - x2) * (py - y2)
        };
        let minx = ax.min(bx).min(cx).max(0) as u32;
        let maxx = ax.max(bx).max(cx).min(self.size as i64 - 1) as u32;
        let miny = ay.min(by).min(cy).max(0) as u32;
        let maxy = ay.max(by).max(cy).min(self.size as i64 - 1) as u32;
        for y in miny..=maxy {
            for x in minx..=maxx {
                let (px, py) = (x as i64, y as i64);
                let d1 = sign(px, py, ax, ay, bx, by);
                let d2 = sign(px, py, bx, by, cx, cy);
                let d3 = sign(px, py, cx, cy, ax, ay);
                if (d1 >= 0 && d2 >= 0 && d3 >= 0) || (d1 <= 0 && d2 <= 0 && d3 <= 0) {
                    self.set(x, y, col);
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
        let (rgba, w, h) = render_tray_icon(0);
        let icon = tray_icon::Icon::from_rgba(rgba, w, h).ok()?;
        let tray = tray_icon::TrayIconBuilder::new()
            .with_tooltip("Quill")
            .with_icon(icon)
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
        let (rgba, w, h) = render_tray_icon(unread);
        if let Ok(icon) = tray_icon::Icon::from_rgba(rgba, w, h) {
            let _ = self.icon.set_icon(Some(icon));
        }
        let tooltip = if unread == 0 {
            "Quill".to_string()
        } else {
            format!("Quill — {unread} unread")
        };
        let _ = self.icon.set_tooltip(Some(tooltip));
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
    let unread = session.map(total_unread).unwrap_or(0);
    TRAY.with(|cell| {
        let mut slot = cell.borrow_mut();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::MemorySink;
    use crate::ids::{AccountKey, ChatId, MessageId};
    use crate::state::{ChatSummary, Session};
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
            typing_senders: Vec::new(),
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
    fn total_unread_sums_and_ignores_negative() {
        let session = session_with(&[3, 0, 7, -5]);
        assert_eq!(total_unread(&session), 10);
    }

    #[test]
    fn total_unread_includes_muted_chats() {
        // Muted chats count: Telegram Desktop's badge includes them by
        // default (`_includeMutedCounter = true`).
        let mut session = Session::new(
            AccountKey("tray-test-muted".into()),
            Arc::new(MemorySink::new()),
        );
        session.chats.insert(0, chat(0, 5));
        session.chats.insert(1, muted_chat(1, 9));
        assert_eq!(total_unread(&session), 14);
    }

    #[test]
    fn total_unread_excludes_archived_chats() {
        // Archived chats never count (both official clients exclude them
        // from the badge by default).
        let mut session = Session::new(
            AccountKey("tray-test-archived".into()),
            Arc::new(MemorySink::new()),
        );
        session.chats.insert(0, chat(0, 5));
        session.chats.insert(1, archived_chat(1, 9));
        assert_eq!(total_unread(&session), 5);
    }

    #[test]
    fn total_unread_saturates() {
        let session = session_with(&[i32::MAX, i32::MAX, i32::MAX]);
        assert_eq!(total_unread(&session), u32::MAX);
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
    fn badge_caps_at_99_plus() {
        // Renders without panic; the capped badge differs from the 99 badge.
        let (a, _, _) = render_tray_icon(99);
        let (b, _, _) = render_tray_icon(150);
        assert_ne!(a, b);
        assert!(red_pixel_count(&b) > 100);
    }
}
