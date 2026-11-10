//! Screenshot demos: bots profile.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::bots::{apply_ready_bot_chat, apply_ready_bot_keyboard};
use crate::ui::profile::apply_ready_profile_edit;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use std::sync::atomic::Ordering;

register_demos![
    // Bot chat demo (injected, no live Telegram): private chat with a
    // `userTypeBot` user (id 21), opened with history plus a cached
    // `botInfo` (description + commands), so the bot panel renders under
    // the header and the composer is visible (Phase 3.1).
    DemoSpec::chats(
        "ready-bot-chat",
        "screenshot demo — bot chat with info panel"
    )
    .setup(QuillApp::demo_ready_bot_chat),
    // Inline keyboard demo (injected, no live Telegram): like
    // `ReadyBotChat`, but the bot message carries a
    // `replyMarkupInlineKeyboard` with URL / callback / switchInline /
    // copy-text / unknown (disabled) buttons (Phase 3.2).
    DemoSpec::chats(
        "ready-bot-keyboard",
        "screenshot demo — B1 bot keyboards: inline buttons, custom keyboard, force reply"
    )
    .setup(QuillApp::demo_ready_bot_keyboard),
    // Slice A5: profile management (injected, no live Telegram) — the
    // "Edit profile" dialog open on the current user (id 777) with a
    // seeded name, bio, usernames and profile-photo id.
    DemoSpec::chats(
        "ready-profile-edit",
        "screenshot demo — edit profile dialog (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_profile_edit),
    // The Contacts "Import" box with no file chosen; `QUILL_DEMO_VCF=<path>`
    // loads that file as if it had been chosen.
    DemoSpec::chats(
        "ready-contacts-import",
        "screenshot demo — Contacts import box (choose a .vcf file)"
    )
    .setup(QuillApp::demo_contacts_import),
    // The bot chat with a "Share location" keyboard as its newest message;
    // `QUILL_DEMO_LOCATION=found|denied|unavailable|locating` is what the
    // platform answers when the button is pressed.
    DemoSpec::chats(
        "ready-bot-location-request",
        "screenshot demo — bot keyboard with a Share location button"
    )
    .setup(QuillApp::demo_bot_location_request),
    // A bot chat with no messages yet: the "What can this bot do?" card
    // with the bot's description, its link and /commands clickable.
    DemoSpec::chats(
        "ready-bot-about",
        "screenshot demo — empty bot chat with the bot's description"
    )
    .setup(QuillApp::demo_bot_about),
];

impl QuillApp {
    fn demo_ready_bot_chat(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_bot_chat(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.set_note(StatusNote::silent(
            "screenshot demo — bot chat with info panel",
        ));
    }

    fn demo_ready_bot_keyboard(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_bot_keyboard(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.set_note(StatusNote::silent(
            "screenshot demo — bot keyboards: inline buttons, custom keyboard, force reply",
        ));
    }

    fn demo_bot_location_request(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        let keyboard = r#"{"@type":"updateNewMessage","message":{"id":307,"chat_id":21,"is_outgoing":false,"reply_markup":{"@type":"replyMarkupShowKeyboard","rows":[[{"@type":"keyboardButton","text":"Share location","type":{"@type":"keyboardButtonTypeRequestLocation"}}]],"is_persistent":false,"resize_keyboard":true,"one_time":true,"is_personal":false,"force_reply":false,"input_field_placeholder":""},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Where are you?","entities":[]}}}}"#;
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_bot_chat(session, &self.demo_ui.sink, &self.demo_ui.seq);
            let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
                self.demo_ui.sink.clone();
            if let Some(owned) =
                quill::telegram::client::copy_and_parse(keyboard, &self.demo_ui.seq, &sink)
            {
                session.apply(owned);
            }
        }
    }

    fn demo_bot_about(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        let clear = r#"{"@type":"updateDeleteMessages","chat_id":21,"message_ids":[301,302],"is_permanent":true,"from_cache":false}"#;
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        apply_ready_bot_chat(session, &self.demo_ui.sink, &self.demo_ui.seq);
        let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
            self.demo_ui.sink.clone();
        // The bot's full info again, now with a description photo.
        let extra = session.request(
            quill::state::RequestPurpose::GetUserFullInfo,
            Some(quill::ids::ChatId(21)),
        );
        let path = crate::ui::demo::demo_media_allowlist().join("showcase/p1.jpg");
        let file = crate::ui::demo::demo_file_json(9401, &path.to_string_lossy(), true);
        let full_info = format!(
            r#"{{"@type":"userFullInfo","@extra":"{}","bot_info":{{"@type":"botInfo","short_description":"A demo bot","description":"Demo Bot answers questions and shows how the info panel looks. It understands /start, /help and /ping.","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"x","photo":{file},"width":1100,"height":739,"progressive_sizes":[]}}]}},"commands":[]}}}}"#,
            extra.0,
        );
        for json in [clear, full_info.as_str()] {
            if let Some(owned) =
                quill::telegram::client::copy_and_parse(json, &self.demo_ui.seq, &sink)
            {
                session.apply(owned);
            }
        }
        // Offline there is no TDLib to find the entities; mark what it
        // would.
        if let Some(Some(info)) = session.bots.bot_info.get_mut(&21) {
            let text = info.description.trim().to_string();
            info.description_entities = ["/start", "/help", "/ping"]
                .into_iter()
                .filter_map(|command| {
                    let start = text.find(command)?;
                    Some(quill::text::TextEntity {
                        utf8_start: start,
                        utf8_end: start + command.len(),
                        kind: quill::text::TextEntityKind::BotCommand,
                    })
                })
                .collect();
        }
    }

    fn demo_contacts_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_import_contacts_dialog(window, cx);
    }

    fn demo_profile_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_profile_edit(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.open_edit_profile_dialog(window, cx);
        self.set_note(StatusNote::silent("screenshot demo — edit profile dialog"));
    }
}
