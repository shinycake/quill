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
use quill::ids::{ChatId, MessageId, UserId};
use quill::state::MessageReactionsState;
use quill::telegram::envelope::ChatKind;
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
    // The message menu of a secret chat, as it opens: no strip, and only what
    // a secret chat allows.
    DemoSpec::chats(
        "ready-secret-chat-menu",
        "screenshot demo — message menu in a secret chat"
    )
    .setup(QuillApp::demo_ready_secret_chat_menu),
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
    // A long chat read up to the middle opens with the "Unread messages"
    // bar at the top (docs/decisions/unread-open-landing.md).
    DemoSpec::chats(
        "ready-unread-landing",
        "screenshot demo — a chat opens at its first unread message"
    )
    .setup(QuillApp::demo_ready_unread_landing),
];

/// Messages `UNREAD_FIRST..=UNREAD_LAST` follow demo chat A's own; the reader
/// read up to `UNREAD_READ`.
const UNREAD_FIRST: i64 = 1_001;
const UNREAD_READ: i64 = 1_100;
const UNREAD_LAST: i64 = 1_160;

const UNREAD_LINES: [&str; 5] = [
    "Are we still on for Thursday?",
    "I moved the call to the afternoon so everyone can make it, and I put the notes from last week in the shared folder.",
    "Sounds good.",
    "Can you send me the latest version when you get a chance? I want to read it through once more before we send it out.",
    "On it.",
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
        match edit {
            Some(edit) => {
                self.chat_ui
                    .enter_edit(edit, "unrelated draft stays".into(), None);
            }
            None => {
                self.chat_ui.leave_edit();
            }
        }
        self.chat_ui.pending_delete = DeleteConfirm::own(ChatId(11), MessageId(102), true, false);
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
        self.set_note(StatusNote::silent(
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
        self.set_note(StatusNote::silent(
            "screenshot demo — mute presets · muted icon · archive",
        ));
    }

    fn demo_ready_pin(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_pin(session, &self.demo_ui.sink, &self.demo_ui.seq);
            let _ = session.begin_chat_search_jump(MessageId(101));
        }
        self.set_note(StatusNote::silent(
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
        self.set_note(StatusNote::silent(
            "screenshot demo — react · unreact · chips",
        ));
    }

    fn demo_ready_secret_chat_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_ready_reactions(window, cx);
        self.message_ui.reactions_expanded = false;
        if let Some(session) = self.demo_session.as_mut() {
            if let Some(chat) = session.chats.get_mut(&11) {
                chat.kind = ChatKind::Secret {
                    secret_chat_id: 9,
                    user_id: UserId(11),
                };
            }
            session.stickers.message_reaction_options = MessageReactionsState::Loading {
                chat_id: ChatId(11),
                message_id: MessageId(102),
            };
        }
        self.message_ui.menu = Some(MessageMenuState {
            chat_id: ChatId(11),
            message_id: MessageId(102),
            position: point(px(420.), px(200.)),
        });
        // `QUILL_DEMO_SECRET_CLOSE=1`: the Close Secret Chat box instead.
        if std::env::var_os("QUILL_DEMO_SECRET_CLOSE").is_some() {
            self.message_ui.menu = None;
            self.open_close_secret_chat_confirm(ChatId(11), cx);
        }
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
            self.chat_ui.set_reply(Some(ComposerReplyTo::with_quote(
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

    fn demo_ready_unread_landing(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        let outgoing = |id: i64| id % 4 == 0;
        let message = |id: i64| {
            let line = UNREAD_LINES
                .get(usize::try_from(id).unwrap_or(0) % UNREAD_LINES.len())
                .copied()
                .unwrap_or_default();
            format!(
                r#"{{"id":{id},"chat_id":11,"is_outgoing":{},"date":{},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{} {line}","entities":[]}}}}}}"#,
                outgoing(id),
                1_790_640_000 + (id - UNREAD_FIRST) * 90,
                id - UNREAD_FIRST + 1
            )
        };
        let unread = (UNREAD_READ + 1..=UNREAD_LAST)
            .filter(|id| !outgoing(*id))
            .count();
        let mut jsons: Vec<String> = (UNREAD_FIRST..=UNREAD_LAST)
            .map(|id| {
                format!(
                    r#"{{"@type":"updateNewMessage","message":{}}}"#,
                    message(id)
                )
            })
            .collect();
        jsons.push(format!(
            r#"{{"@type":"updateChatLastMessage","chat_id":11,"last_message":{},"positions":[{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"30","is_pinned":true}}]}}"#,
            message(UNREAD_LAST)
        ));
        jsons.push(format!(
            r#"{{"@type":"updateChatReadInbox","chat_id":11,"last_read_inbox_message_id":{UNREAD_READ},"unread_count":{unread}}}"#
        ));
        // Opened again once the reads are in, so it lands at the bar.
        if let Some(session) = self.demo_session.as_mut() {
            session.open_chat(ChatId(12));
        }
        self.demo_apply_jsons(jsons);
        if let Some(session) = self.demo_session.as_mut() {
            session.open_chat(ChatId(11));
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
