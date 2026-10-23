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
];

impl QuillApp {
    fn demo_ready_bot_chat(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_bot_chat(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.connection.set_note(StatusNote::silent(
            "screenshot demo — bot chat with info panel",
        ));
    }

    fn demo_ready_bot_keyboard(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_bot_keyboard(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.connection.set_note(StatusNote::silent(
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

    fn demo_contacts_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_import_contacts_dialog(window, cx);
    }

    fn demo_profile_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_profile_edit(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.open_edit_profile_dialog(window, cx);
        self.connection
            .set_note(StatusNote::silent("screenshot demo — edit profile dialog"));
    }
}
