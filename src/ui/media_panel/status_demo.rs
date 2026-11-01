//! Screenshot demos: the emoji status selector and its "Set Status for..."
//! box, over an offline Premium account.
#![deny(clippy::indexing_slicing)]

use crate::ui::demo::seed_ready_custom_emoji_session;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::FileId;
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{StickerFormat, StickerItem};
use quill::telegram::{EmojiStatusItem, UpgradedGiftEmojiStatus};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

register_demos![
    DemoSpec::ready(
        "ready-emoji-status",
        seed_status_session,
        "screenshot demo — emoji status selector (injected statuses, no live Telegram)"
    )
    .setup(|app, window, cx| app.open_status_selector(point(px(8.), px(52.)), window, cx)),
    DemoSpec::ready(
        "ready-emoji-status-until",
        seed_status_session,
        "screenshot demo — Set Status for... box (injected statuses, no live Telegram)"
    )
    .setup(|app, window, cx| app.open_status_time_box(5003, window, cx)),
    // The account alone, with the selector closed (the base of the UI tests).
    DemoSpec::ready(
        "ready-emoji-status-account",
        seed_status_session,
        "screenshot demo — Premium account with emoji statuses (injected, no live Telegram)"
    ),
];

const FACES: [&str; 12] = [
    "😀", "😎", "🥳", "🤩", "😴", "🤔", "😇", "🥰", "😜", "🤖", "👻", "🔥",
];

/// The demo sticker every status resolves to.
pub(super) const STATUS_FILE: i32 = 61;

pub(super) fn status_sticker(id: i64, emoji: &str) -> StickerItem {
    StickerItem {
        custom_emoji_id: Some(id),
        id,
        set_id: 0,
        emoji: emoji.to_string(),
        width: 512,
        height: 512,
        format: StickerFormat::Webp,
        file_id: FileId(STATUS_FILE),
        thumb_file_id: None,
        thumb_width: 0,
        thumb_height: 0,
        requires_premium: false,
        premium_animation: None,
    }
}

/// A Premium account with themed, recent and default statuses and two
/// collectibles, all resolved to the demo sticker.
pub(super) fn seed_status_session(sink: Arc<MemorySink>) -> Session {
    let mut session = seed_ready_custom_emoji_session(sink.clone());
    session.payments.premium_option = Some(true);
    session.my_user_id = Some(777);
    let seq = AtomicU64::new(session.last_seq);
    let dyn_sink: Arc<dyn DiagnosticSink> = sink;
    if let Some(owned) = copy_and_parse(
        r#"{"@type":"updateUser","user":{"id":777,"first_name":"Demo","last_name":"Viewer","is_premium":true,"emoji_status":null,"type":{"@type":"userTypeRegular"}}}"#,
        &seq,
        &dyn_sink,
    ) {
        session.apply(owned);
    }
    let ids: Vec<i64> = (5001..=5034).collect();
    let emoji = &mut session.stickers.emoji;
    emoji.custom_emoji_stickers.update(|list| {
        for (ix, id) in ids.iter().enumerate() {
            let face = FACES.get(ix % FACES.len()).copied().unwrap_or("😀");
            list.push(status_sticker(*id, face));
        }
    });
    emoji.themed_status_ids = (5001..=5010).collect();
    emoji.default_status_ids = (5011..=5026).collect();
    emoji.recent_statuses = [5003, 5030, 5031]
        .into_iter()
        .map(|id| EmojiStatusItem {
            custom_emoji_id: id,
            expiration_date: 0,
            gift: None,
        })
        .collect();
    emoji.upgraded_gift_statuses = [5033, 5034]
        .into_iter()
        .map(|id| EmojiStatusItem {
            custom_emoji_id: 0,
            expiration_date: 0,
            gift: Some(UpgradedGiftEmojiStatus {
                upgraded_gift_id: id,
                gift_title: "Plush Pepe".into(),
                gift_name: "PlushPepe-1".into(),
                model_custom_emoji_id: id,
                symbol_custom_emoji_id: id,
                backdrop_center_color: 0,
                backdrop_edge_color: 0,
                backdrop_symbol_color: 0,
                backdrop_text_color: 0,
            }),
        })
        .collect();
    session
}
