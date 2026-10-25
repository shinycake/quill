//! Screenshot demo: a Premium account in a Russian-language channel, for
//! the chat translate bar (click it to show the seeded English
//! translations) and the header menu's Translate. `QUILL_DEMO_TRANSLATE`
//! is `hidden` for a chat whose bar the user hid (the ⋯ menu then offers
//! Translate). Injected, no live Telegram.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::diagnostics::DiagnosticSink;
use quill::ids::ChatId;
use quill::state::Translation;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;

register_demos![
    DemoSpec::chats(
        "ready-chat-translate",
        "screenshot demo — translate an entire chat (Premium)"
    )
    .setup(QuillApp::demo_chat_translate)
];

const CHANNEL: i64 = 13;

/// `(original, translation)` for each channel post, oldest first.
const POSTS: [(&str, &str); 8] = [
    (
        "Доброе утро! Сегодня в городе солнечно и тепло.",
        "Good morning! It is sunny and warm in the city today.",
    ),
    (
        "Новая выставка открывается в музее в эту субботу.",
        "A new exhibition opens at the museum this Saturday.",
    ),
    (
        "Метро продлевает часы работы на время праздников.",
        "The metro extends its hours for the holidays.",
    ),
    (
        "В парке пройдёт бесплатный концерт классической музыки.",
        "A free classical music concert will take place in the park.",
    ),
    (
        "Синоптики обещают снег к концу недели.",
        "Forecasters expect snow by the end of the week.",
    ),
    (
        "Библиотека запускает курсы программирования для детей.",
        "The library is launching programming courses for children.",
    ),
    (
        "На набережной открылось новое кафе с видом на реку.",
        "A new café with a river view has opened on the embankment.",
    ),
    (
        "Спасибо, что читаете наш канал каждый день!",
        "Thank you for reading our channel every day!",
    ),
];

impl QuillApp {
    fn demo_chat_translate(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let hidden = std::env::var("QUILL_DEMO_TRANSLATE").is_ok_and(|mode| mode == "hidden");
        let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        session.payments.premium_option = Some(true);
        self.demo_ui
            .seq
            .store(session.last_seq, std::sync::atomic::Ordering::SeqCst);
        let member = format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{CHANNEL},"is_channel":true,"status":{{"@type":"chatMemberStatusMember"}},"member_count":18400}}}}"#
        );
        if let Some(owned) = copy_and_parse(&member, &self.demo_ui.seq, &dyn_sink) {
            session.apply(owned);
        }
        for (ix, (text, translation)) in (0i64..).zip(POSTS) {
            let id = 201 + ix;
            let text = serde_json::to_string(text).unwrap_or_default();
            let json = format!(
                r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{CHANNEL},"is_outgoing":false,"is_channel_post":true,"date":{},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{text},"entities":[]}}}}}}}}"#,
                1_790_640_000 + ix * 600
            );
            if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &dyn_sink) {
                session.apply(owned);
            }
            session.messages.translate.messages.insert(
                (CHANNEL, id, "en".to_string()),
                Translation::Done {
                    text: translation.to_string(),
                    entities: Vec::new(),
                },
            );
        }
        session.open_chat(ChatId(CHANNEL));
        let translate = &mut session.messages.translate;
        translate.translatable_chats.insert(CHANNEL);
        translate.revision += 1;
        if hidden {
            self.settings.translate.prefs.set_bar_hidden(CHANNEL, true);
        }
        cx.notify();
    }
}
