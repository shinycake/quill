//! Screenshot demos: bots profile.

use crate::ui::app::QuillApp;
use crate::ui::bots::{apply_ready_bot_chat, apply_ready_bot_keyboard};
use crate::ui::profile::apply_ready_profile_edit;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
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
];

impl QuillApp {
    fn demo_ready_bot_chat(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_bot_chat(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.connection.status_note = "screenshot demo — bot chat with info panel".into();
    }

    fn demo_ready_bot_keyboard(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_bot_keyboard(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.connection.status_note =
            "screenshot demo — bot keyboards: inline buttons, custom keyboard, force reply".into();
    }

    fn demo_profile_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_profile_edit(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.open_edit_profile_dialog(window, cx);
        self.connection.status_note = "screenshot demo — edit profile dialog".into();
    }
}
