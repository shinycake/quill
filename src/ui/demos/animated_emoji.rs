//! Screenshot demo: messages of only emoji drawn as stickers (injected, no
//! live Telegram). An incoming lone emoji (`messageAnimatedEmoji`, the demo
//! hand), the same emoji sent with a skin tone, and messages of two and
//! three custom emoji. `QUILL_DEMO_EMOJI_TONE=<0-6>` picks the outgoing
//! one's `fitzpatrick_type` (4 by default).
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::demo::{demo_file_json, demo_media_allowlist};
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::ids::FileId;
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{StickerFormat, StickerItem};
use std::sync::atomic::Ordering;

register_demos![
    DemoSpec::chats(
        "ready-animated-emoji",
        "screenshot demo — lone emoji and custom emoji drawn as stickers"
    )
    .setup(QuillApp::demo_animated_emoji)
    .timing(900, 3000)
];

const CHAT: i64 = 11;
/// The first message id; files take ids from 7_300.
const FIRST_MESSAGE: i64 = 9_301;
const HAND_FILE: i32 = 7_301;
const CUSTOM_FILE: i32 = 7_302;

/// A `sticker` for `file`, a custom emoji's when `custom_emoji_id` is set.
fn sticker_json(file: i32, path: &str, emoji: &str, custom_emoji_id: Option<i64>) -> String {
    let file_json = demo_file_json(file, path, true);
    let full_type = match custom_emoji_id {
        Some(id) => format!(
            r#"{{"@type":"stickerFullTypeCustomEmoji","custom_emoji_id":"{id}","needs_repainting":false}}"#
        ),
        None => r#"{"@type":"stickerFullTypeRegular","premium_animation":null}"#.to_string(),
    };
    format!(
        r#"{{"@type":"sticker","id":"{file}","set_id":"0","width":512,"height":512,"emoji":"{emoji}","format":{{"@type":"stickerFormatTgs"}},"full_type":{full_type},"thumbnail":null,"sticker":{file_json}}}"#
    )
}

/// `messageAnimatedEmoji` for the demo hand in skin tone `fitz`.
pub(super) fn hand_message_json(id: i64, outgoing: bool, emoji: &str, fitz: i32) -> String {
    let path = demo_media_allowlist().join("demo-emoji-fitz.tgs");
    let sticker = sticker_json(HAND_FILE, &path.to_string_lossy(), "👋", None);
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{CHAT},"date":1760000000,"is_outgoing":{outgoing},"content":{{"@type":"messageAnimatedEmoji","emoji":"{emoji}","animated_emoji":{{"@type":"animatedEmoji","sticker":{sticker},"sticker_width":512,"sticker_height":512,"fitzpatrick_type":{fitz},"sound":null,"is_regular_emoji":true}}}}}}}}"#
    )
}

/// A text message of custom emoji `ids`, each over "⭐".
fn custom_message_json(id: i64, ids: &[i64]) -> String {
    let text = "⭐".repeat(ids.len());
    let entities: Vec<String> = (0..)
        .zip(ids)
        .map(|(index, emoji)| {
            format!(
                r#"{{"@type":"textEntity","offset":{index},"length":1,"type":{{"@type":"textEntityTypeCustomEmoji","custom_emoji_id":"{emoji}"}}}}"#
            )
        })
        .collect();
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{CHAT},"date":1760000000,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[{}]}}}}}}}}"#,
        entities.join(",")
    )
}

/// The demo's custom emoji: one animated sticker under three ids.
const CUSTOM_IDS: [i64; 3] = [8_801, 8_802, 8_803];

fn custom_sticker(id: i64) -> StickerItem {
    StickerItem {
        custom_emoji_id: Some(id),
        id,
        set_id: 0,
        emoji: "⭐".into(),
        width: 512,
        height: 512,
        format: StickerFormat::Tgs,
        file_id: FileId(CUSTOM_FILE),
        thumb_file_id: None,
        thumb_width: 0,
        thumb_height: 0,
        requires_premium: false,
        premium_animation: None,
    }
}

impl QuillApp {
    fn demo_animated_emoji(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
            self.demo_ui.sink.clone();
        let tone = std::env::var("QUILL_DEMO_EMOJI_TONE")
            .ok()
            .and_then(|tone| tone.parse::<i32>().ok())
            .unwrap_or(4);
        let custom_path = demo_media_allowlist().join("demo-sticker-60.tgs");
        let [a, b, c] = CUSTOM_IDS;
        let updates = [
            format!(
                r#"{{"@type":"updateFile","file":{}}}"#,
                demo_file_json(CUSTOM_FILE, &custom_path.to_string_lossy(), true)
            ),
            hand_message_json(FIRST_MESSAGE, false, "👋", 0),
            hand_message_json(FIRST_MESSAGE + 1, true, "👋🏽", tone),
            custom_message_json(FIRST_MESSAGE + 2, &[a, b]),
            custom_message_json(FIRST_MESSAGE + 3, &[a, b, c]),
        ];
        for json in updates {
            if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &sink) {
                session.apply(owned);
            }
        }
        session.stickers.emoji.custom_emoji_stickers.update(|list| {
            list.extend(CUSTOM_IDS.map(custom_sticker));
        });
        if let Some(history) = session.histories.get_mut(&CHAT) {
            history
                .messages
                .retain(|id, _| (FIRST_MESSAGE..FIRST_MESSAGE + 4).contains(id));
        }
        cx.notify();
    }
}
