//! Screenshot demos of the emoji set style (parity:emoji-set-style): the
//! "Choose emoji set" box, and a chat whose messages are full of emoji.
//! With `QUILL_DEMO_EMOJI_PACKS=<builder output>` and
//! `QUILL_DEMO_EMOJI_SET=twemoji` they draw with that set
//! (`emoji_style_ui::demo_install`).
#![deny(clippy::indexing_slicing)]

use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::diagnostics::DiagnosticSink;
use quill::telegram::client::copy_and_parse;

register_demos![
    DemoSpec::chats(
        "ready-emoji-sets",
        "screenshot demo — the Choose emoji set box"
    )
    .setup(QuillApp::demo_emoji_sets),
    DemoSpec::chats(
        "ready-emoji-set-chat",
        "screenshot demo — emoji in messages, reactions and the chat list"
    )
    .setup(QuillApp::demo_emoji_set_chat),
];

/// Messages with plain emoji: mixed text, a big-emoji message, flags,
/// ZWJ families, skin tones and a keycap.
const MESSAGES: [(i64, bool, &str); 4] = [
    (
        120,
        false,
        "Weekend plans: hike \u{26f0}\u{fe0f} then pizza \u{1f355} and a movie \u{1f37f} \u{1f389}",
    ),
    (121, true, "\u{1f602}\u{1f44d}"),
    (
        122,
        false,
        "Trip list \u{1f1ef}\u{1f1f5} \u{1f1e7}\u{1f1f7} \u{1f1e8}\u{1f1e6}, family \u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}, waves \u{1f44b}\u{1f3fd} \u{1f44b}\u{1f3ff}, step 1\u{fe0f}\u{20e3}",
    ),
    (
        123,
        true,
        "Sounds great \u{2764}\u{fe0f} see you there \u{1f60e}",
    ),
];

impl QuillApp {
    fn demo_emoji_sets(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_emoji_set_style(cx);
    }

    fn demo_emoji_set_chat(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            for (id, outgoing, text) in MESSAGES {
                let message = serde_json::json!({
                    "@type": "updateNewMessage",
                    "message": {
                        "id": id,
                        "chat_id": 11,
                        "is_outgoing": outgoing,
                        "date": 1_790_640_000 + id,
                        "content": {
                            "@type": "messageText",
                            "text": {"@type": "formattedText", "text": text, "entities": []}
                        }
                    }
                });
                if let Some(owned) = copy_and_parse(&message.to_string(), &self.demo_ui.seq, &sink)
                {
                    session.apply(owned);
                }
            }
        }
        cx.notify();
    }
}
