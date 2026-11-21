//! Screenshot demo: right-to-left and mixed messages with entities (injected, no
//! live Telegram), for the RTL sweep (docs/decisions/rtl-sweep.md). A link, a
//! mention, bold and strikethrough sit inside Hebrew and Arabic sentences, next to
//! digits, emoji and brackets, so a capture shows whether each styled word keeps
//! its own color and underline and whether every line reads the right way round.
#![deny(clippy::indexing_slicing)]

use super::animated_emoji::{CUSTOM_FILE, CUSTOM_IDS, custom_sticker};
use crate::ui::app::QuillApp;
use crate::ui::demo::{demo_file_json, demo_media_allowlist};
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::telegram::client::copy_and_parse;
use std::sync::atomic::Ordering;

register_demos![
    DemoSpec::chats(
        "ready-rtl-text",
        "screenshot demo — Hebrew, Arabic and mixed messages with links, mentions and styles"
    )
    .setup(QuillApp::demo_rtl_text)
    .timing(900, 2500)
];

const CHAT: i64 = 11;
const FIRST_MESSAGE: i64 = 9_401;

/// One styled stretch of a message: UTF-16 offset, length and the TDLib entity type.
struct Entity(usize, usize, &'static str);

/// The demo's messages: text, outgoing, entities.
const MESSAGES: &[(&str, bool, &[Entity])] = &[
    (
        "שלום קישור עולם",
        false,
        &[Entity(
            5,
            5,
            r#"{"@type":"textEntityTypeTextUrl","url":"https://example.com"}"#,
        )],
    ),
    (
        "ברוכים הבאים @dana לקבוצה שלנו",
        false,
        &[Entity(13, 5, r#"{"@type":"textEntityTypeMention"}"#)],
    ),
    (
        "مرحبا بالعالم غامق ١٢٣ والنهاية",
        false,
        &[Entity(14, 4, r#"{"@type":"textEntityTypeBold"}"#)],
    ),
    (
        "שלום 😀 עולם (בדיקה) [1] מחוק כאן",
        true,
        &[Entity(25, 4, r#"{"@type":"textEntityTypeStrikethrough"}"#)],
    ),
    ("Version 2.0 של האפליקציה", true, &[]),
    (
        "הודעה ארוכה שעוברת שורה כדי לבדוק שהשורות נשארות בסדר הנכון גם כשיש בה קישור ארוך בסוף https://example.com/path",
        false,
        &[Entity(87, 24, r#"{"@type":"textEntityTypeUrl"}"#)],
    ),
    // The chat-list preview of a Hebrew line with a custom emoji in it: the
    // pieces must read right to left (D3).
    (
        "שלום ⭐ לכולם",
        false,
        &[Entity(
            5,
            1,
            r#"{"@type":"textEntityTypeCustomEmoji","custom_emoji_id":"8801"}"#,
        )],
    ),
];

fn message_json(id: i64, text: &str, outgoing: bool, entities: &[Entity]) -> String {
    let entities: Vec<String> = entities
        .iter()
        .map(|Entity(offset, length, kind)| {
            format!(r#"{{"@type":"textEntity","offset":{offset},"length":{length},"type":{kind}}}"#)
        })
        .collect();
    format!(
        r#"{{"id":{id},"chat_id":{CHAT},"date":1760000000,"is_outgoing":{outgoing},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[{}]}}}}}}"#,
        entities.join(",")
    )
}

impl QuillApp {
    fn demo_rtl_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
            self.demo_ui.sink.clone();
        // The custom emoji's sticker file, shared with the animated-emoji demo.
        let path = demo_media_allowlist().join("demo-sticker-60.tgs");
        let file = format!(
            r#"{{"@type":"updateFile","file":{}}}"#,
            demo_file_json(CUSTOM_FILE, &path.to_string_lossy(), true)
        );
        if let Some(owned) = copy_and_parse(&file, &self.demo_ui.seq, &sink) {
            session.apply(owned);
        }
        session.stickers.emoji.custom_emoji_stickers.update(|list| {
            list.extend(CUSTOM_IDS.map(custom_sticker));
        });
        let mut last_message = None;
        for (id, (text, outgoing, entities)) in (FIRST_MESSAGE..).zip(MESSAGES) {
            let message = message_json(id, text, *outgoing, entities);
            let json = format!(r#"{{"@type":"updateNewMessage","message":{message}}}"#);
            if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &sink) {
                session.apply(owned);
            }
            last_message = Some(message);
        }
        // The chat-list row previews the last one (D3: a Hebrew line with a
        // custom emoji).
        if let Some(message) = last_message {
            let json = format!(
                r#"{{"@type":"updateChatLastMessage","chat_id":{CHAT},"last_message":{message},"positions":[{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"95","is_pinned":false}}]}}"#
            );
            if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &sink) {
                session.apply(owned);
            }
        }
        let last = FIRST_MESSAGE + MESSAGES.len() as i64;
        if let Some(history) = session.histories.get_mut(&CHAT) {
            history
                .messages
                .retain(|id, _| (FIRST_MESSAGE..last).contains(id));
        }
        cx.notify();
    }
}
