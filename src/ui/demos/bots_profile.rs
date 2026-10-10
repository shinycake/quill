//! Screenshot demos: bots profile.

use crate::ui::app::QuillApp;
use crate::ui::bots::{
    apply_ready_bot_chat, apply_ready_bot_command_menu, apply_ready_bot_keyboard,
    apply_ready_bot_profile, apply_ready_rich_message,
};
use crate::ui::profile::apply_ready_profile_edit;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::telegram::envelope::UsernameCheckResult;
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
    // Bot command menu demo (injected, no live Telegram): like
    // `ReadyBotChat`, plus a cached `getCommands` response (global
    // scope) so the `/` command menu renders open above the composer
    // with the bot-specific and "Global" sections (Phase 3.3).
    DemoSpec::chats(
        "ready-bot-command-menu",
        "screenshot demo — bot chat with / command menu"
    )
    .setup(QuillApp::demo_ready_bot_command_menu),
    // Inline keyboard demo (injected, no live Telegram): like
    // `ReadyBotChat`, but the bot message carries a
    // `replyMarkupInlineKeyboard` with URL / callback / switchInline /
    // copy-text / unknown (disabled) buttons (Phase 3.2).
    DemoSpec::chats(
        "ready-bot-keyboard",
        "screenshot demo — B1 bot keyboards: inline buttons, custom keyboard, force reply"
    )
    .setup(QuillApp::demo_ready_bot_keyboard),
    // Bot profile actions demo (injected, no live Telegram): like
    // `ReadyBotChat`, plus an armed `bot_start_params` entry (START
    // button), `botInfo` with a menu button and a privacy-policy URL,
    // and a loaded `getBotSimilarBots` answer (Slice B2).
    DemoSpec::chats(
        "ready-bot-profile",
        "screenshot demo — B2 bot profile actions"
    )
    .setup(QuillApp::demo_ready_bot_profile),
    // Inline-mode demo (injected, no live Telegram): like
    // `ReadyBotChat`, but the Demo Bot is an inline bot (`@gif`) with
    // an injected resolved slot and a loaded results page, so the
    // `@bot` inline-results dropdown renders open above the composer
    // (bots slice).
    DemoSpec::chats(
        "ready-inline-results",
        "screenshot demo — inline-mode results dropdown"
    )
    .setup(QuillApp::demo_ready_inline_results),
    // Slice A5: profile management (injected, no live Telegram) — the
    // "Edit profile" dialog open on the current user (id 777) with a
    // seeded name, bio, usernames and profile-photo id.
    DemoSpec::chats(
        "ready-profile-edit",
        "screenshot demo — edit profile dialog (injected, no live Telegram)"
    )
    .setup(|app, window, cx| app.demo_profile_edit(ProfileEditDemo::ProfileEdit, window, cx)),
    // Slice msg-richtext-ai-tools: the same rich editor with a short
    // draft so the AI ghost buttons (✨ Fix / Rewrite / Create / Fix rich /
    // Rewrite rich) sit in frame. Injected Ready session, no live Telegram.
    DemoSpec::chats(
        "ready-rich-ai-tools",
        "screenshot demo — rich editor AI tools"
    )
    .setup(QuillApp::demo_ready_rich_ai_tools),
    // M2: rich editor demo (injected, no live Telegram) — the demo bot
    // chat with the composer in rich mode (markup text, block buttons,
    // live block preview).
    DemoSpec::chats("ready-rich-editor", "screenshot demo — rich editor")
        .setup(QuillApp::demo_ready_rich_editor),
    // M2: rich message demo (injected, no live Telegram) — the demo bot
    // chat with an injected `messageRichMessage` (headings, styled
    // paragraphs, list, collapsible, inline document, table, divider,
    // `pageBlockButtonRow` with URL + callback buttons) plus a message
    // whose `ephemeral_content` overrides the regular content.
    DemoSpec::chats(
        "ready-rich-message",
        "screenshot demo — rich message blocks"
    )
    .setup(QuillApp::demo_ready_rich_message),
    // Slice msg-richtext-premium-gate: multi-line composer so the ⛶ Rich
    // editor button is visible, status note shows the non-Premium refusal
    // ("Rich messages require Telegram Premium"). Editor stays closed.
    // Injected Ready session, no live Telegram.
    DemoSpec::chats(
        "ready-rich-premium-gate",
        "Rich messages require Telegram Premium"
    )
    .setup(QuillApp::demo_ready_rich_premium_gate),
    // Slice A5: like `ReadyProfileEdit`, but the username field holds a
    // freshly-checked value with a seeded `checkChatUsernameResultOk`
    // verdict — intended for a taller capture
    // (`QUILL_DEMO_WINDOW_SIZE`) so the username section is visible.
    DemoSpec::chats(
        "ready-username",
        "screenshot demo — edit profile dialog (injected, no live Telegram)"
    )
    .setup(|app, window, cx| app.demo_profile_edit(ProfileEditDemo::Username, window, cx)),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum ProfileEditDemo {
    ProfileEdit,
    Username,
}

impl QuillApp {
    fn demo_ready_bot_chat(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_bot_chat(session, &self.demo_sink, &self.demo_seq);
        }
        self.status_note = "screenshot demo — bot chat with info panel".into();
    }

    fn demo_ready_bot_command_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_bot_command_menu(session, &self.demo_sink, &self.demo_seq);
        }
        self.composer.update(cx, |input, cx| {
            input.set_value("/", window, cx);
        });
        self.sync_command_menu(cx);
        self.status_note = "screenshot demo — bot chat with / command menu".into();
    }

    fn demo_ready_bot_keyboard(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_bot_keyboard(session, &self.demo_sink, &self.demo_seq);
        }
        self.status_note =
            "screenshot demo — bot keyboards: inline buttons, custom keyboard, force reply".into();
    }

    fn demo_ready_bot_profile(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_bot_profile(session, &self.demo_sink, &self.demo_seq);
        }
        self.status_note = "screenshot demo — B2 bot profile actions".into();
    }

    fn demo_ready_inline_results(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            crate::ui::inline_mode::apply_ready_inline_results(
                session,
                &self.demo_sink,
                &self.demo_seq,
            );
        }
        self.composer.update(cx, |input, cx| {
            input.set_value("@gif cats", window, cx);
        });
        self.sync_inline_mode(cx);
        self.status_note = "screenshot demo — @bot inline results".into();
    }

    fn demo_ready_rich_ai_tools(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_bot_chat(session, &self.demo_sink, &self.demo_seq);
        }
        self.composer.update(cx, |input, cx| {
            input.set_value(
                "Please fix this sentance and rewrite it as a short invite.",
                window,
                cx,
            );
        });
        self.composer_ui.rich_editor_open = true;
        self.status_note = "screenshot demo — rich editor AI tools: Fix · Rewrite · Create".into();
    }

    fn demo_ready_rich_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_bot_chat(session, &self.demo_sink, &self.demo_seq);
        }
        self.composer.update(cx, |input, cx| {
                input.set_value(
                    "# Club night\n\nPick **one**:\n\n- Live set\n- DJ set\n- [] Bring a friend\n\n>> Details\nDoors at 9pm, show at 10pm.\n\n---\nSee you there!",
                    window,
                    cx,
                );
            });
        self.composer_ui.rich_editor_open = true;
        self.status_note = "screenshot demo — rich editor".into();
    }

    fn demo_ready_rich_message(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_rich_message(session, &self.demo_sink, &self.demo_seq);
        }
        self.status_note = "screenshot demo — rich message blocks".into();
    }

    fn demo_ready_rich_premium_gate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_bot_chat(session, &self.demo_sink, &self.demo_seq);
        }
        // >3 lines so the ⛶ Rich editor button is visible; editor stays
        // closed and the status note shows the non-Premium refusal.
        self.composer.update(cx, |input, cx| {
            input.set_value(
                "Line one of a long draft\nLine two\nLine three\nLine four — tap Rich editor",
                window,
                cx,
            );
        });
        self.composer_ui.rich_editor_open = false;
        self.status_note = "Rich messages require Telegram Premium".into();
    }

    fn demo_profile_edit(
        &mut self,
        demo: ProfileEditDemo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_profile_edit(session, &self.demo_sink, &self.demo_seq);
        }
        self.open_edit_profile_dialog(window, cx);
        if matches!(demo, ProfileEditDemo::Username) {
            if let Some(session) = self.demo_session.as_mut() {
                session.username_check =
                    Some(("newhandle".to_string(), UsernameCheckResult::Available));
            }
            if let Some(dialog) = self.edit_profile_dialog.as_ref() {
                dialog.username_input.update(cx, |input, cx| {
                    input.set_value("newhandle", window, cx);
                });
            }
        }
        self.status_note = "screenshot demo — edit profile dialog".into();
    }
}
