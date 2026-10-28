//! Screenshot demos: the effect of a lone emoji (injected, no live
//! Telegram), played over the other party's lone animated emoji (the demo
//! hand, drawn as its 112 pt sticker), mirrored as for a message on the
//! left, at tdesktop's effect size (`emoji_effect`). The effect is the demo
//! sticker.
//!
//! - `ready-emoji-tap-effect`: the emoji is tapped a moment after the chat
//!   shows.
//! - `ready-emoji-read-effect`: the emoji is unread and the effects pack
//!   (`emoji_animations`) is loaded; the effect starts by itself when the
//!   history reports the row as read, the path a live session takes
//!   (`report_visible_history`).
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::demo::{demo_file_json, demo_media_allowlist};
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::emoji_interaction::EmojiEffect;
use quill::ids::{ChatId, FileId, MessageId};
use quill::state::RequestPurpose;
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{StickerFormat, StickerItem};
use std::sync::atomic::Ordering;
use std::time::Duration;

register_demos![
    DemoSpec::chats(
        "ready-emoji-tap-effect",
        "screenshot demo — a lone emoji's tap effect"
    )
    .setup(QuillApp::demo_emoji_tap)
    .timing(1700, 3000),
    DemoSpec::chats(
        "ready-emoji-read-effect",
        "screenshot demo — an unread lone emoji plays its effect when read"
    )
    .setup(QuillApp::demo_emoji_read)
    .timing(1500, 3000)
];

const CHAT: i64 = 11;
const MESSAGE: i64 = 9_201;
const EFFECT_FILE: i32 = 7_201;
/// The unread hand of `ready-emoji-read-effect` (the chat is read up to
/// message 100).
const UNREAD_MESSAGE: i64 = 9_202;
/// The tap comes once the message has been painted (an effect is only
/// played over a message on screen).
const TAP_AFTER: Duration = Duration::from_millis(700);

impl QuillApp {
    fn demo_emoji_tap(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
            self.demo_ui.sink.clone();
        let path = demo_media_allowlist().join("demo-sticker.tgs");
        let file = demo_file_json(EFFECT_FILE, &path.to_string_lossy(), true);
        let updates = [
            format!(r#"{{"@type":"updateFile","file":{file}}}"#),
            super::animated_emoji::hand_message_json(MESSAGE, false, "👋", 0),
        ];
        for json in updates {
            if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &sink) {
                session.apply(owned);
            }
        }
        if let Some(history) = session.histories.get_mut(&CHAT) {
            history.messages.retain(|id, _| *id == MESSAGE);
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(TAP_AFTER).await;
            let _ = this.update(cx, |this, cx| {
                let Some(session) = this.demo_session.as_mut() else {
                    return;
                };
                session.stickers.interactions.push(EmojiEffect {
                    chat_id: ChatId(CHAT),
                    message_id: MessageId(MESSAGE),
                    sticker: StickerItem {
                        custom_emoji_id: None,
                        id: i64::from(EFFECT_FILE),
                        set_id: 0,
                        emoji: "👋".into(),
                        width: 512,
                        height: 512,
                        format: StickerFormat::Tgs,
                        file_id: FileId(EFFECT_FILE),
                        thumb_file_id: None,
                        thumb_width: 0,
                        thumb_height: 0,
                        requires_premium: false,
                        premium_animation: None,
                    },
                    incoming: true,
                });
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn demo_emoji_read(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
            self.demo_ui.sink.clone();
        let path = demo_media_allowlist().join("demo-sticker.tgs");
        let file = demo_file_json(EFFECT_FILE, &path.to_string_lossy(), true);
        // The pack as `searchStickerSet` "EmojiAnimations" answers it: the
        // demo sticker is the hand's effect number one.
        let extra = session.request(RequestPurpose::EmojiAnimations, None);
        let pack = format!(
            r#"{{"@type":"stickerSet","@extra":"{}","id":"1","title":"Animated Emoji Interactions","name":"EmojiAnimations","thumbnail":null,"thumbnail_outline":null,"is_owned":false,"is_installed":false,"is_archived":false,"is_official":true,"sticker_type":{{"@type":"stickerTypeRegular"}},"needs_repainting":false,"is_allowed_as_chat_emoji_status":false,"is_viewed":true,"stickers":[{{"@type":"sticker","id":"{EFFECT_FILE}","set_id":"1","width":512,"height":512,"emoji":"👋","format":{{"@type":"stickerFormatTgs"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":null,"sticker":{file}}}],"emojis":[{{"@type":"emojis","emojis":["👋","1⃣"]}}]}}"#,
            extra.as_extra()
        );
        let updates = [
            pack,
            super::animated_emoji::hand_message_json(UNREAD_MESSAGE, false, "👋", 0),
        ];
        for json in updates {
            if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &sink) {
                session.apply(owned);
            }
        }
        if let Some(history) = session.histories.get_mut(&CHAT) {
            history.messages.retain(|id, _| *id == UNREAD_MESSAGE);
        }
        cx.notify();
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "emoji_read_tests.rs"]
mod tests;
