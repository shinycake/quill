//! Screenshot demo: the effect of a tap on a lone emoji (injected, no live
//! Telegram). The other party's lone emoji is tapped a moment after the
//! chat shows, and its effect (the demo sticker) plays over it, mirrored as
//! for a message on the left, at tdesktop's effect size (`emoji_effect`).
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::demo::{demo_file_json, demo_media_allowlist};
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::emoji_interaction::EmojiEffect;
use quill::ids::{ChatId, FileId, MessageId};
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
    .timing(1700, 3000)
];

const CHAT: i64 = 11;
const MESSAGE: i64 = 9_201;
const EFFECT_FILE: i32 = 7_201;
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
            format!(
                r#"{{"@type":"updateNewMessage","message":{{"id":{MESSAGE},"chat_id":{CHAT},"date":1760000000,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"🎉","entities":[]}}}}}}}}"#
            ),
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
                        emoji: "🎉".into(),
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
}
