//! Screenshot demos: messages.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::chat_list::{apply_ready_mute_archive, apply_ready_pin};
use crate::ui::composer::apply_ready_reply;
use crate::ui::forward::apply_ready_forward;
use crate::ui::reactions::apply_ready_reactions;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::search_ui::{apply_ready_search, apply_ready_search_in_chat};
use crate::ui::status_note::StatusNote;
use crate::ui::*;
use gpui_kit::*;
use quill::composer::{ComposerEdit, ComposerReplyTo, DeleteConfirm, ForwardDraft};
use quill::ids::{ChatId, MessageId};
use std::sync::atomic::Ordering;

register_demos![
    // Own-message edit mode + delete confirm (injected, no live Telegram).
    DemoSpec::chats(
        "ready-edit-delete",
        "screenshot demo — edit + delete own messages (injected)"
    )
    .setup(QuillApp::demo_ready_edit_delete),
    // Forward select + dest picker + success (injected, no live Telegram).
    DemoSpec::chats(
        "ready-forward",
        "screenshot demo — forward message(s) (injected forwardMessages)"
    )
    .setup(QuillApp::demo_ready_forward),
    // Mute presets + muted icon + archive section (injected, no live Telegram).
    DemoSpec::chats(
        "ready-mute-archive",
        "screenshot demo — mute / archive (injected notification + chat list updates)"
    )
    .setup(QuillApp::demo_ready_mute_archive),
    // Pin / unpin + pinned banner (injected, no live Telegram).
    DemoSpec::chats(
        "ready-pin",
        "screenshot demo — pin / unpin (injected updateMessageIsPinned)"
    )
    .setup(QuillApp::demo_ready_pin),
    // Emoji react / unreact + chips (injected, no live Telegram).
    DemoSpec::chats(
        "ready-reactions",
        "screenshot demo — emoji reactions (injected interaction_info)"
    )
    .setup(QuillApp::demo_ready_reactions),
    // Reply-to-message: composer quote + history quote strip.
    DemoSpec::chats(
        "ready-reply",
        "screenshot demo — reply to message (injected reply_to)"
    )
    .setup(QuillApp::demo_ready_reply),
    // Sidebar search over injected recents / `searchChats` / `searchMessages`.
    DemoSpec::chats(
        "ready-search",
        "screenshot demo — sidebar search (injected searchChats / searchMessages)"
    )
    .setup(QuillApp::demo_ready_search),
    // In-chat search (`searchChatMessages`) + jump-to-message.
    DemoSpec::chats(
        "ready-search-in-chat",
        "screenshot demo — in-chat search (injected searchChatMessages)"
    )
    .setup(QuillApp::demo_ready_search_in_chat),
];

impl QuillApp {
    fn demo_ready_edit_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let edit = ComposerEdit::from_own_content(
            ChatId(11),
            MessageId(102),
            true,
            false,
            &quill::telegram::envelope::MessageContent::Text(
                "Reply from the session reducer.".into(),
            ),
        );
        self.composer.update(cx, |input, cx| {
            input.set_value("Reply from the session reducer.", window, cx);
            input.focus(window, cx);
        });
        self.composer_ui.pending_edit = edit;
        self.composer_ui.saved_edit_draft = "unrelated draft stays".into();
        self.message_ui.pending_delete =
            DeleteConfirm::own(ChatId(11), MessageId(102), true, false);
    }

    fn demo_ready_forward(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_forward(session, &self.demo_ui.sink, &self.demo_ui.seq);
            self.share.forward_result = session.messages.last_forward.clone();
        }
        if let Some(mut draft) = ForwardDraft::from_message(ChatId(11), MessageId(101), false) {
            draft.toggle(ChatId(11), MessageId(102), false);
            self.share.pending_forward = Some(draft);
        }
        self.share.forward_picker_open = true;
        self.share.search_input.update(cx, |input, cx| {
            input.set_value("Demo chat B", window, cx);
            input.focus(window, cx);
        });
        self.connection.set_note(StatusNote::silent(
            "screenshot demo — select → pick dest → forwarded",
        ));
    }

    fn demo_ready_mute_archive(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_mute_archive(session, &self.demo_ui.sink, &self.demo_ui.seq);
            for scope in quill::telegram::envelope::NotificationSettingsScope::ALL {
                session
                    .settings
                    .scope_notification_settings
                    .entry(scope)
                    .or_default();
            }
        }
        self.open_mute_menu(cx);
        self.connection.set_note(StatusNote::silent(
            "screenshot demo — mute presets · muted icon · archive",
        ));
    }

    fn demo_ready_pin(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_pin(session, &self.demo_ui.sink, &self.demo_ui.seq);
            let _ = session.begin_chat_search_jump(MessageId(101));
        }
        self.connection.set_note(StatusNote::silent(
            "screenshot demo — pin · unpin · pinned bar",
        ));
    }

    fn demo_ready_reactions(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_reactions(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        // The message menu with its reaction strip, expanded.
        if let Some(session) = self.demo_session.as_mut() {
            super::reaction_fixtures::seed_reaction_options(session, ChatId(11), MessageId(101));
        }
        self.message_ui.menu = Some(MessageMenuState {
            chat_id: ChatId(11),
            message_id: MessageId(101),
            position: point(px(420.), px(200.)),
        });
        self.message_ui.reactions_expanded = true;
        self.connection.set_note(StatusNote::silent(
            "screenshot demo — react · unreact · chips",
        ));
    }

    fn demo_ready_reply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer.update(cx, |input, cx| {
            input.set_value("sounds good", window, cx);
            input.focus(window, cx);
        });
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_reply(session, &self.demo_ui.sink, &self.demo_ui.seq);
            // The quoted part keeps its formatting in the bar and the strip.
            let entity = |start, end, kind| quill::text::TextEntity {
                utf8_start: start,
                utf8_end: end,
                kind,
            };
            let mut quote = quill::composer::QuoteSelection::plain("Hello from injected JSON.", 0);
            quote.entities = vec![
                entity(0, 5, quill::text::TextEntityKind::Bold),
                entity(11, 19, quill::text::TextEntityKind::Italic),
                entity(20, 24, quill::text::TextEntityKind::Strikethrough),
            ];
            self.composer_ui.set_reply(Some(ComposerReplyTo::with_quote(
                ChatId(11),
                MessageId(101),
                quote,
            )));
        }
    }

    fn demo_ready_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_ui.input.update(cx, |input, cx| {
            input.set_value("hello", window, cx);
            input.focus(window, cx);
        });
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_search(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
    }

    fn demo_ready_search_in_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_ui.chat_input.update(cx, |input, cx| {
            input.set_value("hello", window, cx);
            input.focus(window, cx);
        });
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_search_in_chat(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
    }
}
