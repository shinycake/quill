//! Screenshot demos: UI that follows the history. The reply bar and the
//! message selection store ids; these show them after the message was
//! edited or deleted.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::ChatListFilter;
use crate::ui::app::QuillApp;
use crate::ui::chat_list::apply_ready_chat_list_3;
use crate::ui::composer::apply_ready_reply;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::composer::{ComposerEdit, ComposerReplyTo, ForwardDraft};
use quill::ids::{ChatId, MessageId};
use quill::telegram::envelope::MessageContent;
use std::sync::atomic::Ordering;

const CHAT: ChatId = ChatId(11);
const REPLIED: MessageId = MessageId(101);

register_demos![
    // The reply bar after the replied-to message was edited.
    DemoSpec::chats(
        "ready-reply-edited",
        "screenshot demo — reply bar follows an edit of the message"
    )
    .setup(QuillApp::demo_ready_reply_edited),
    // The reply bar after the replied-to message was deleted: gone.
    DemoSpec::chats(
        "ready-reply-deleted",
        "screenshot demo — reply bar cancelled by deleting the message"
    )
    .setup(QuillApp::demo_ready_reply_deleted),
    // Two selected messages.
    DemoSpec::chats("ready-selection", "screenshot demo — two selected messages")
        .setup(QuillApp::demo_ready_selection),
    // Two selected messages, one deleted: the count and Forward say 1.
    DemoSpec::chats(
        "ready-selection-deleted",
        "screenshot demo — a selected message was deleted"
    )
    .setup(QuillApp::demo_ready_selection_deleted),
    // Chats 11 and 12 checked, then the Unread category: a read chat is no
    // longer listed and no longer counted.
    DemoSpec::chats(
        "ready-chats-select-filtered",
        "screenshot demo — a checked chat left the Unread category"
    )
    .setup(QuillApp::demo_ready_chats_select_filtered),
    DemoSpec::chats(
        "ready-chats-select-all",
        "screenshot demo — two checked chats on the All category"
    )
    .setup(QuillApp::demo_ready_chats_select_all),
];

impl QuillApp {
    /// Delete a message the way TDLib reports it. The update counter first
    /// catches up with the session, or the reducer drops it as stale.
    pub(in crate::ui) fn demo_delete_message(&mut self, chat_id: ChatId, message_id: MessageId) {
        if let Some(session) = self.demo_session.as_ref() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        }
        self.apply_demo_delete(chat_id, message_id);
    }

    fn demo_reply_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer.update(cx, |input, cx| {
            input.set_value("sounds good", window, cx);
            input.focus(window, cx);
        });
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_reply(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.composer_ui.pending_reply = Some(ComposerReplyTo::new(CHAT, REPLIED));
    }

    fn demo_ready_reply_edited(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_reply_bar(window, cx);
        let edit = ComposerEdit::from_own_content(
            CHAT,
            REPLIED,
            true,
            false,
            &MessageContent::Text("x".into()),
        );
        if let Some(edit) = edit {
            self.apply_demo_edit(&edit, "Moved to 6 pm, same place.");
        }
    }

    fn demo_ready_reply_deleted(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_reply_bar(window, cx);
        self.demo_delete_message(CHAT, REPLIED);
    }

    fn demo_select_two(&mut self) {
        if let Some(mut draft) = ForwardDraft::from_message(CHAT, MessageId(101), false) {
            draft.toggle(CHAT, MessageId(102), false);
            self.share.pending_forward = Some(draft);
        }
    }

    fn demo_ready_selection(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.demo_select_two();
    }

    fn demo_ready_selection_deleted(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.demo_select_two();
        self.demo_delete_message(CHAT, MessageId(102));
    }

    fn demo_chats_checked(&mut self) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_chat_list_3(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.chat_list.selected.insert(11);
        self.chat_list.selected.insert(12);
    }

    fn demo_ready_chats_select_all(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.demo_chats_checked();
    }

    fn demo_ready_chats_select_filtered(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.demo_chats_checked();
        self.set_chat_list_view(ChatListFilter::Unread, None);
    }
}
