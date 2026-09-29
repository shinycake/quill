//! Slice S15 (review fix-up): the S15 tests live here instead of the
//! waived files (`src/state.rs`, `src/telegram/envelope.rs`,
//! `src/telegram/requests.rs` must not grow with new tests). Proves
//! through the public reducer/request interfaces:
//! 1. `updateInstalledStickerSets` reorders the cached installed sets in
//!    place (regular → sticker panel, other types → emoji panel); sets
//!    missing from the update keep their relative order at the end; an
//!    empty cache is a no-op.
//! 2. `send_sticker` (panel-picked) passes
//!    `update_order_of_installed_sticker_sets: true` in messageSendOptions
//!    (schema 1.8.67:5934).

use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{AccountKey, ChatId, FileId, MessageId, RequestId};
use quill::state::{RequestPurpose, Session};
use quill::telegram::SendReply;
use quill::telegram::client::copy_and_parse;
use quill::telegram::requests::{StickerSend, send_sticker};
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

/// `updateInstalledStickerSets` reorders the cached installed sets in
/// place (regular → sticker panel, other types → emoji panel); sets
/// missing from the update keep their relative order at the end; an
/// empty cache is a no-op.
#[test]
fn s15_dynamic_set_order_applies_update() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let set_json = |id: i64, title: &str| {
        format!(
            r#"{{"@type":"stickerSetInfo","id":"{id}","title":"{title}","name":"{title}","thumbnail":null,"thumbnail_outline":null,"is_owned":false,"is_installed":true,"is_archived":false,"is_official":false,"sticker_type":{{"@type":"stickerTypeRegular"}},"needs_repainting":false,"is_allowed_as_chat_emoji_status":false,"is_viewed":true,"size":1,"covers":[]}}"#
        )
    };
    let extra = session.request(RequestPurpose::GetInstalledStickerSets, None);
    apply(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"stickerSets","total_count":3,"sets":[{},{},{}],"@extra":"{}"}}"#,
            set_json(77, "Alpha"),
            set_json(78, "Beta"),
            set_json(79, "Gamma"),
            extra.0
        ),
    );
    assert_eq!(
        session
            .stickers
            .sets
            .iter()
            .map(|s| s.id)
            .collect::<Vec<_>>(),
        vec![77, 78, 79]
    );

    // Usage-driven reorder: 79 used most recently, 77 next; 78 is
    // absent from the update and keeps its relative order at the end.
    apply(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateInstalledStickerSets","sticker_type":{"@type":"stickerTypeRegular"},"sticker_set_ids":["79","77"]}"#,
    );
    assert_eq!(
        session
            .stickers
            .sets
            .iter()
            .map(|s| s.id)
            .collect::<Vec<_>>(),
        vec![79, 77, 78]
    );

    // Non-regular types route to the emoji panel's installed sets.
    session.emoji.installed_sets = session.stickers.sets.clone();
    apply(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateInstalledStickerSets","sticker_type":{"@type":"stickerTypeCustomEmoji"},"sticker_set_ids":["78","79","77"]}"#,
    );
    assert_eq!(
        session
            .emoji
            .installed_sets
            .iter()
            .map(|s| s.id)
            .collect::<Vec<_>>(),
        vec![78, 79, 77]
    );
    // The sticker panel is untouched by the emoji-type update.
    assert_eq!(
        session
            .stickers
            .sets
            .iter()
            .map(|s| s.id)
            .collect::<Vec<_>>(),
        vec![79, 77, 78]
    );

    // Empty cache: no-op, no panic.
    session.stickers.sets.clear();
    apply(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateInstalledStickerSets","sticker_type":{"@type":"stickerTypeRegular"},"sticker_set_ids":["79"]}"#,
    );
    assert!(session.stickers.sets.is_empty());
}

/// Panel-picked stickers ask TDLib to move the used set to the front of
/// the installed order (schema 1.8.67:5934).
#[test]
fn s15_send_sticker_sets_update_order_flag() {
    let json = send_sticker(
        RequestId(13),
        ChatId(7),
        StickerSend {
            file_id: FileId(41),
            emoji: "😀",
            width: 512,
            height: 512,
            thumb: Some((FileId(42), 128, 128)),
            reply_to: Some(SendReply::plain(MessageId(101))),
            topic_id: None,
        },
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "sendMessage");
    assert_eq!(v["options"]["update_order_of_installed_sticker_sets"], true);
}
