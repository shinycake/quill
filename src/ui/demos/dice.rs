//! Screenshot demo: dice messages in history (injected, no live Telegram):
//! a landed basketball, a landed slot machine and a die still rolling. The
//! animations are `dice-*.tgs` from the demo media folder when present, else
//! the demo sticker in every role.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::demo::{demo_file_json, demo_media_allowlist};
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::telegram::client::copy_and_parse;
use std::sync::atomic::Ordering;

register_demos![
    DemoSpec::chats("ready-dice", "screenshot demo — dice messages in history")
        .setup(QuillApp::demo_dice)
        .timing(400, 4000)
];

/// The first message id the demo uses; files take ids from 7_000.
const FIRST_MESSAGE: i64 = 9_101;

impl QuillApp {
    fn demo_dice(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
            self.demo_ui.sink.clone();
        let root = demo_media_allowlist();
        let mut next_file = 7_000;
        let mut sticker = |role: &str| {
            let named = root.join(format!("dice-{role}.tgs"));
            let path = if named.exists() {
                named
            } else {
                root.join("demo-sticker.tgs")
            };
            next_file += 1;
            let file = demo_file_json(next_file, &path.to_string_lossy(), true);
            format!(
                r#"{{"@type":"sticker","id":"{next_file}","set_id":"0","width":512,"height":512,"emoji":"🎲","format":{{"@type":"stickerFormatTgs"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":null,"sticker":{file}}}"#
            )
        };
        let regular =
            |sticker: String| format!(r#"{{"@type":"diceStickersRegular","sticker":{sticker}}}"#);
        let mut slots = |prefix: &str| {
            format!(
                r#"{{"@type":"diceStickersSlotMachine","background":{},"lever":{},"left_reel":{},"center_reel":{},"right_reel":{}}}"#,
                sticker(&format!("{prefix}background")),
                sticker(&format!("{prefix}lever")),
                sticker(&format!("{prefix}left")),
                sticker(&format!("{prefix}center")),
                sticker(&format!("{prefix}right")),
            )
        };
        let slot_start = slots("slot-start-");
        let slot_end = slots("slot-");
        let basketball_start = regular(sticker("basketball-start"));
        let basketball = regular(sticker("basketball"));
        let die_start = regular(sticker("die-start"));
        let messages = [
            ("🏀", 4, basketball_start, basketball),
            ("🎰", 10, slot_start, slot_end),
            ("🎲", 0, die_start, "null".to_string()),
        ];
        for (offset, (emoji, value, initial, last)) in (0..).zip(messages) {
            let id = FIRST_MESSAGE + offset;
            let json = format!(
                r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":11,"date":1760000000,"is_outgoing":true,"content":{{"@type":"messageDice","initial_state":{initial},"final_state":{last},"emoji":"{emoji}","value":{value},"success_animation_frame_number":0}}}}}}"#
            );
            if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &sink) {
                session.apply(owned);
            }
        }
        if let Some(history) = session.histories.get_mut(&11) {
            history
                .messages
                .retain(|id, _| (FIRST_MESSAGE..FIRST_MESSAGE + 3).contains(id));
        }
        cx.notify();
    }
}
