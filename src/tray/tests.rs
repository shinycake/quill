use super::*;

#[test]
fn startup_reveal_waits_then_decides() {
    assert_eq!(tray_startup_reveal(true, false, 0), Some(false));
    assert_eq!(tray_startup_reveal(false, true, 0), None);
    assert_eq!(tray_startup_reveal(false, true, 1499), None);
    assert_eq!(tray_startup_reveal(false, true, 1500), Some(true));
    assert_eq!(tray_startup_reveal(false, false, 10), Some(true));
}
#[test]
fn tray_menu_routes_only_its_own_actions() {
    assert_eq!(menu_action("quill-tray-open"), Some(TrayAction::Open));
    assert_eq!(menu_action("quill-tray-quit"), Some(TrayAction::Quit));
    assert_eq!(menu_action("quill-tray-lock"), Some(TrayAction::Lock));
    assert_eq!(
        menu_action("quill-tray-notifications"),
        Some(TrayAction::ToggleNotifications)
    );
    assert_eq!(
        menu_action("quill-tray-sounds"),
        Some(TrayAction::ToggleSounds)
    );
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
        my_restriction: None,
        my_rights_fetched: false,
        is_forum: None,
        photo_file_id: None,
        can_send_basic_messages: true,
        permissions: None,
        can_be_deleted_for_all_users: false,
        can_be_deleted_only_for_self: false,
        is_marked_as_unread: false,
        unread_mention_count: 0,
        unread_reaction_count: 0,
        unread_poll_vote_count: 0,
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
    session.chat_list.unread_totals.main = ListUnreadTotals {
        messages: Some(UnreadPair {
            all: 44,
            unmuted: 13,
        }),
        chats: Some(UnreadPair {
            all: 20,
            unmuted: 6,
        }),
    };
    session.chat_list.unread_totals.archive = ListUnreadTotals {
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
    session.chat_list.unread_totals.main.messages = Some(UnreadPair {
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

#[test]
fn tooltip_names_the_unread_count() {
    assert_eq!(tray_tooltip(0), "Quill");
    assert_eq!(tray_tooltip(3), "Quill \u{2014} 3 unread");
}

#[test]
fn argb32_pixmap_is_network_byte_order() {
    assert_eq!(
        rgba_to_argb32(&[0x11, 0x22, 0x33, 0x44, 0xAA, 0xBB, 0xCC, 0xDD]),
        vec![0x44, 0x11, 0x22, 0x33, 0xDD, 0xAA, 0xBB, 0xCC]
    );
    let (rgba, w, h) = render_tray_icon(0);
    assert_eq!(rgba_to_argb32(&rgba).len(), (w * h * 4) as usize);
}

#[test]
fn overlay_icon_is_empty_without_unread_and_centered_pill_with_unread() {
    let (empty, w, h) = render_overlay_icon(0);
    assert_eq!((w, h), (ICON_SIZE, ICON_SIZE));
    assert!(empty.as_chunks::<4>().0.iter().all(|p| p[3] == 0));

    let (rgba, _, _) = render_overlay_icon(7);
    let red = |p: &[u8; 4]| *p == BADGE_RED;
    assert!(rgba.as_chunks::<4>().0.iter().any(red));
    // The pill is vertically centered: no opaque pixel in the top or
    // bottom margin rows.
    let margin = ((ICON_SIZE - BADGE_HEIGHT) / 2) as usize;
    let row = (ICON_SIZE * 4) as usize;
    assert!(
        rgba[..margin * row]
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| p[3] == 0)
    );
    assert!(
        rgba[(ICON_SIZE as usize - margin) * row..]
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| p[3] == 0)
    );
    // The top-right badge of the tray icon starts at row 0, so the
    // overlay differs from it.
    assert_ne!(rgba, render_tray_icon(7).0);
}

#[test]
fn tray_switches_hide_without_a_tray() {
    // No tray host and the icon wanted: nothing to configure.
    assert_eq!(
        tray_setting_switches(false, true),
        TraySettingSwitches {
            show_tray_icon: false,
            start_in_tray: false,
            run_in_background: false
        }
    );
    // A live tray offers everything, on every OS.
    assert_eq!(
        tray_setting_switches(true, true),
        TraySettingSwitches {
            show_tray_icon: true,
            start_in_tray: true,
            run_in_background: true
        }
    );
    // The user turned the icon off: only the way back stays visible.
    assert_eq!(
        tray_setting_switches(false, false),
        TraySettingSwitches {
            show_tray_icon: true,
            start_in_tray: false,
            run_in_background: false
        }
    );
}

#[test]
fn closing_runs_in_the_background_only_with_a_tray() {
    use CloseOutcome::*;
    assert_eq!(close_outcome(false, true, true, true), Quit);
    assert_eq!(close_outcome(true, false, true, true), Quit);
    assert_eq!(close_outcome(true, false, false, true), Quit);
    assert_eq!(close_outcome(true, true, true, true), HideApp);
    assert_eq!(close_outcome(true, true, false, true), Hide);
    // Linux and Windows cannot hide a GPUI window: minimize it.
    assert_eq!(close_outcome(true, true, false, false), Minimize);
}
