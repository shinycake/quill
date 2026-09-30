//! Slice `parity:chatlist-list-style`: chat-list style settings —
//! two/three preview lines, media icons, formatted preview text. Proves
//! through the public interfaces (settings load/save, the pure
//! `chatlist_style` helpers, and the Session reducer over parsed JSON):
//! 1. the three prefs round-trip and default to current behavior
//!    (2 lines, no icons, plain text); out-of-range line counts clamp.
//! 2. the virtual-list row height reflects the line count and tag strip.
//! 3. `updateChatLastMessage` captures the media icon, the formatted
//!    entities, and the sender name next to `last_preview`.
//! 4. the pure sender-name rule and entity clipping.

use quill::chatlist_style::{
    ChatListRowStyle, ChatPreviewStyle, PREVIEW_LINES_DEFAULT, chat_row_height_px,
    clamp_preview_lines, clip_entities, preview_sender_name, preview_style,
};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::AccountKey;
use quill::settings::{
    AccountPaths, AppearancePrefs, load_appearance_prefs, save_appearance_prefs,
};
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::MessageContent;
use quill::text::{TextEntity, TextEntityKind};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn session() -> (Session, Arc<MemorySink>) {
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    (Session::new(AccountKey::primary(), dyn_sink), sink)
}

fn apply(session: &mut Session, seq: &AtomicU64, sink: &Arc<MemorySink>, json: &str) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let owned = copy_and_parse(json, seq, &dyn_sink).unwrap();
    session.apply(owned);
}

fn prefs_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "quill-chatlist-style-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn chat_list_style_defaults_to_current_behavior() {
    let defaults = AppearancePrefs::default();
    assert_eq!(defaults.preview_lines, PREVIEW_LINES_DEFAULT);
    assert_eq!(defaults.preview_lines, 2);
    assert!(!defaults.chat_list_media_icons);
    assert!(!defaults.chat_list_rich_preview);
}

#[test]
fn chat_list_style_prefs_roundtrip() {
    let dir = prefs_dir("roundtrip");
    let paths = AccountPaths::for_root(&dir, &AccountKey::primary());
    let prefs = AppearancePrefs {
        preview_lines: 3,
        chat_list_media_icons: true,
        chat_list_rich_preview: true,
        ..AppearancePrefs::default()
    };
    save_appearance_prefs(&paths, &prefs).unwrap();
    assert_eq!(load_appearance_prefs(&paths), prefs);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn chat_list_style_preview_lines_clamp_on_load() {
    let dir = prefs_dir("clamp");
    let paths = AccountPaths::for_root(&dir, &AccountKey::primary());
    std::fs::create_dir_all(&paths.root).unwrap();
    for (raw, expected) in [("0", 2), ("1", 2), ("2", 2), ("3", 3), ("9", 3)] {
        std::fs::write(
            paths.root.join("appearance_prefs.json"),
            format!(r#"{{"preview_lines":{raw}}}"#),
        )
        .unwrap();
        assert_eq!(
            load_appearance_prefs(&paths).preview_lines,
            expected,
            "raw preview_lines={raw}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn row_style_clamps_preview_lines() {
    assert_eq!(ChatListRowStyle::new(0, true, true).preview_lines, 2);
    assert_eq!(ChatListRowStyle::new(3, false, false).preview_lines, 3);
    assert_eq!(ChatListRowStyle::new(9, true, true).preview_lines, 3);
    assert_eq!(clamp_preview_lines(0), 2);
    assert_eq!(clamp_preview_lines(3), 3);
}

#[test]
fn chat_row_height_reflects_lines_and_tags() {
    // 56px base (avatar 40 + py_2), 80px with the folder-tag strip; the
    // third line adds one text_xs line (16px).
    assert_eq!(chat_row_height_px(false, 2), 56.0);
    assert_eq!(chat_row_height_px(true, 2), 80.0);
    assert_eq!(chat_row_height_px(false, 3), 72.0);
    assert_eq!(chat_row_height_px(true, 3), 96.0);
    // Out-of-range line counts clamp instead of producing odd heights.
    assert_eq!(chat_row_height_px(false, 9), 72.0);
    assert_eq!(chat_row_height_px(false, 0), 56.0);
}

#[test]
fn preview_sender_name_rule() {
    assert_eq!(preview_sender_name(true, None, "Group"), "You");
    assert_eq!(
        preview_sender_name(false, Some("Admin"), "Channel"),
        "Admin"
    );
    assert_eq!(preview_sender_name(false, None, "Alice"), "Alice");
    // An empty signature falls back to the chat title (never blank).
    assert_eq!(preview_sender_name(false, Some("  "), "Alice"), "Alice");
}

#[test]
fn clip_entities_keeps_and_truncates_in_range() {
    let entities = vec![
        TextEntity {
            utf8_start: 0,
            utf8_end: 5,
            kind: TextEntityKind::Bold,
        },
        TextEntity {
            utf8_start: 70,
            utf8_end: 90,
            kind: TextEntityKind::Italic,
        },
        TextEntity {
            utf8_start: 95,
            utf8_end: 100,
            kind: TextEntityKind::Bold,
        },
        TextEntity {
            utf8_start: 10,
            utf8_end: 10,
            kind: TextEntityKind::Bold,
        },
    ];
    assert_eq!(
        clip_entities(&entities, 80),
        vec![
            TextEntity {
                utf8_start: 0,
                utf8_end: 5,
                kind: TextEntityKind::Bold,
            },
            TextEntity {
                utf8_start: 70,
                utf8_end: 80,
                kind: TextEntityKind::Italic,
            },
        ]
    );
}

#[test]
fn preview_style_for_text_keeps_icon_empty_and_entities() {
    let content = MessageContent::Text(quill::telegram::envelope::TextContent {
        text: "hello bold world".to_string(),
        entities: vec![TextEntity {
            utf8_start: 6,
            utf8_end: 10,
            kind: TextEntityKind::Bold,
        }],
        link_preview: None,
    });
    let style = preview_style(&content, "hello bold world");
    assert_eq!(
        style,
        ChatPreviewStyle {
            icon: None,
            entities: vec![TextEntity {
                utf8_start: 6,
                utf8_end: 10,
                kind: TextEntityKind::Bold,
            }],
        }
    );
    // Non-text content carries no entities.
    let style = preview_style(&MessageContent::ScreenshotTaken, "Took a screenshot");
    assert_eq!(style, ChatPreviewStyle::default());
}

#[test]
fn last_message_text_captures_entities_and_sender() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Group","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
    );
    apply(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatLastMessage","chat_id":7,"last_message":{"id":44,"chat_id":7,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hello bold world","entities":[{"@type":"textEntity","offset":6,"length":4,"type":{"@type":"textEntityTypeBold"}}]}}},"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"8","is_pinned":false}]}"#,
    );
    let chat = session.chats.get(&7).unwrap();
    assert_eq!(chat.last_preview, "hello bold world");
    assert_eq!(chat.last_preview_sender, "You");
    assert_eq!(chat.last_preview_style.icon, None);
    assert_eq!(
        chat.last_preview_style.entities,
        vec![TextEntity {
            utf8_start: 6,
            utf8_end: 10,
            kind: TextEntityKind::Bold,
        }]
    );
}

#[test]
fn last_message_photo_captures_icon_and_signature_sender() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":8,"title":"Channel","type":{"@type":"chatTypePrivate","user_id":8},"unread_count":0}}"#,
    );
    apply(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateChatLastMessage","chat_id":8,"last_message":{"id":3,"chat_id":8,"is_outgoing":false,"author_signature":"Admin","content":{"@type":"messagePhoto","photo":{"@type":"photo","has_stickers":false,"sizes":[{"@type":"photoSize","type":"m","photo":{"@type":"file","id":1,"size":24,"expected_size":24,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"r","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":24}},"width":320,"height":240,"progressive_sizes":[]}]},"caption":{"@type":"formattedText","text":"Nice pic","entities":[{"@type":"textEntity","offset":0,"length":4,"type":{"@type":"textEntityTypeBold"}}]},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}},"positions":[{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"8","is_pinned":false}]}"#,
    );
    let chat = session.chats.get(&8).unwrap();
    assert_eq!(chat.last_preview, "Nice pic");
    assert_eq!(chat.last_preview_style.icon, Some("🖼"));
    assert_eq!(
        chat.last_preview_style.entities,
        vec![TextEntity {
            utf8_start: 0,
            utf8_end: 4,
            kind: TextEntityKind::Bold,
        }]
    );
    assert_eq!(chat.last_preview_sender, "Admin");
}
