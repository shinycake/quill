//! The state of the chat shown, as one value.
//!
//! What a view has open *for its chat* (the reply and edit bars, the rich
//! editor, the "send as" list, the `/` and inline menus, the delete and
//! stop-poll boxes, a link's copy menu) sits in [`ChatUi`], which carries
//! the chat it was made for. When the view shows another chat the value
//! is replaced whole ([`QuillApp::enter_chat_ui`]), so nothing opened for
//! one chat can act on the next, and no list of fields to clear has to be
//! kept. It is the chat level of `OnScreen` (the account level): each chat
//! window has its own in its `WindowSlot`. Design:
//! docs/decisions/account-scoped-ui-state.md, sections 6.6 and 9.
//!
//! A new field that names one chat's messages or holds a box opened from
//! the chat goes here, not in `ComposerUi` or `MessageUi`
//! (`scripts/check-screen-state.py` keeps a list of the moved names).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::*;
use quill::composer::{ComposerEdit, ComposerReplyTo, DeleteConfirm};
use quill::ids::{ChatId, MessageId};

/// The composer while a message is edited: the edit, the normal draft and
/// reply stashed for it (`DraftType::Normal`), and the "Cancel editing?" box
/// (`DialogKind::CancelEditConfirm`). One value, so the box cannot outlive
/// the edit.
#[derive(Debug)]
pub(super) struct EditMode {
    pub(super) edit: ComposerEdit,
    pub(super) stashed_draft: String,
    /// Reply that belonged to the stashed draft, restored with it on leaving.
    pub(super) stashed_reply: Option<ComposerReplyTo>,
    pub(super) confirm_cancel: bool,
}

/// One view's state for the chat it shows.
#[derive(Default)]
pub(crate) struct ChatUi {
    /// The chat this value was made for; `None` before any chat showed.
    /// Only [`QuillApp::enter_chat_ui`] sets it.
    chat: Option<ChatId>,
    /// M2: the rich editor is open — the composer textarea is interpreted
    /// as block markup (`quill::rich::markup_to_blocks`) and sends via
    /// `inputMessageRichMessage`. Opened via the ⛶ button (visible after
    /// 3+ lines, per the anniversary post).
    pub(super) rich_editor_open: bool,
    /// The composer's "send as" identity list is open.
    pub(super) send_as_open: bool,
    /// The copy menu of a phone number, card number or date.
    pub(super) link_popup: Option<super::entity_links::LinkPopup>,
    /// Phase 3.3: `/` command menu state. Open while the composer text
    /// ends with a `/`-led token and the open bot chat has commands;
    /// `command_menu_selected` is the highlighted row (Up/Down/Enter).
    pub(super) command_menu_open: bool,
    pub(super) command_menu_selected: usize,
    /// Bots slice: `@botname query` inline-mode results dropdown above
    /// the composer. `inline_results_selected` is the highlighted row
    /// (Up/Down/Enter). `inline_query_armed` is the `(username, query)` a
    /// debounce timer is currently armed for — `poll_live` re-runs the
    /// inline progress check after every batch of session updates, and the
    /// armed identity keeps it from spawning a duplicate timer per poll.
    pub(super) inline_results_open: bool,
    pub(super) inline_results_selected: usize,
    pub(super) inline_query_armed: Option<(String, String)>,
    /// Delete confirm (tdesktop `DeleteMessagesBox` / Unigram popup).
    pub(super) pending_delete: Option<DeleteConfirm>,
    /// B4: pending "Stop poll" confirm (`stopPoll`, schema 1.8.67 line
    /// 12953) — (chat, message, is_quiz) for the warning copy. TGX
    /// `StopPollWarn` / `StopQuizWarn`: nobody can vote afterwards and
    /// the action can't be undone.
    pub(super) pending_stop_poll: Option<(ChatId, MessageId, bool)>,
    /// Last row clicked in selection mode: the Shift+click range anchor.
    pub(super) selection_anchor: Option<MessageId>,
    /// The row keyboard selection acts on (Ctrl+Space, Up / Down).
    pub(super) selection_focus: Option<MessageId>,
    /// Scheduled messages ticked in the scheduled-messages dialog (send
    /// now, reschedule or delete together).
    pub(super) scheduled_selected: Vec<MessageId>,
    /// Same-chat reply draft (tdesktop `FieldHeader::replyToMessage`).
    /// Private on purpose: a sender must take the reply through
    /// `QuillApp::reply_for_send` so a reply to a deleted message is dropped.
    pending_reply: Option<ComposerReplyTo>,
    /// Editing an own message (tdesktop `FieldHeader::editMessage`), with
    /// the normal draft stashed beside it. `None` is plain composing. Read
    /// and change it only through the methods below so every way out of
    /// editing is one assignment.
    edit_mode: Option<EditMode>,
}

impl ChatUi {
    /// A fresh value for `chat`.
    pub(super) fn for_chat(chat: Option<ChatId>) -> Self {
        Self {
            chat,
            ..Self::default()
        }
    }

    /// The chat this value was made for.
    #[cfg(test)]
    pub(super) fn chat(&self) -> Option<ChatId> {
        self.chat
    }

    /// The reply the bar shows. Display and bookkeeping only: anything that
    /// sends reads `QuillApp::reply_for_send`, which drops a deleted target.
    pub(super) fn reply(&self) -> Option<&ComposerReplyTo> {
        self.pending_reply.as_ref()
    }

    pub(super) fn has_reply(&self) -> bool {
        self.pending_reply.is_some()
    }

    pub(super) fn set_reply(&mut self, reply: Option<ComposerReplyTo>) {
        self.pending_reply = reply;
    }

    pub(super) fn take_reply(&mut self) -> Option<ComposerReplyTo> {
        self.pending_reply.take()
    }

    pub(super) fn is_editing(&self) -> bool {
        self.edit_mode.is_some()
    }

    pub(super) fn pending_edit(&self) -> Option<&ComposerEdit> {
        self.edit_mode.as_ref().map(|mode| &mode.edit)
    }

    pub(super) fn pending_edit_mut(&mut self) -> Option<&mut ComposerEdit> {
        self.edit_mode.as_mut().map(|mode| &mut mode.edit)
    }

    /// Enter edit mode with the normal draft and reply set aside.
    pub(super) fn enter_edit(
        &mut self,
        edit: ComposerEdit,
        stashed_draft: String,
        stashed_reply: Option<ComposerReplyTo>,
    ) {
        self.edit_mode = Some(EditMode {
            edit,
            stashed_draft,
            stashed_reply,
            confirm_cancel: false,
        });
    }

    /// Leave edit mode (and with it the cancel box), handing back what was
    /// stashed. `None` when nothing was being edited.
    pub(super) fn leave_edit(&mut self) -> Option<EditMode> {
        self.edit_mode.take()
    }

    pub(super) fn stashed_draft(&self) -> Option<&str> {
        self.edit_mode
            .as_ref()
            .map(|mode| mode.stashed_draft.as_str())
    }

    pub(super) fn stashed_reply(&self) -> Option<&ComposerReplyTo> {
        self.edit_mode
            .as_ref()
            .and_then(|mode| mode.stashed_reply.as_ref())
    }

    /// A sticker, GIF or voice send used the stashed reply.
    pub(super) fn drop_stashed_reply(&mut self) {
        if let Some(mode) = self.edit_mode.as_mut() {
            mode.stashed_reply = None;
        }
    }

    /// Whether the "Cancel editing?" box is open. Only an edit has one.
    pub(super) fn cancel_edit_confirm_open(&self) -> bool {
        self.edit_mode
            .as_ref()
            .is_some_and(|mode| mode.confirm_cancel)
    }

    /// Opens or closes the box; opening is ignored without an edit.
    pub(super) fn set_cancel_edit_confirm(&mut self, open: bool) {
        if let Some(mode) = self.edit_mode.as_mut() {
            mode.confirm_cancel = open;
        }
    }
}

/// Nothing to say: the chat's goodbyes (draft flush, deselect, recording)
/// are `HistoryUi`'s and `RecordingUi`'s.
impl super::on_screen::Farewell for ChatUi {}

impl QuillApp {
    /// The view in place now shows `chat`: what was open for another chat
    /// goes, as one value. A reply aimed at `chat` from the chat before
    /// ("Reply in Another Chat") and an edit of one of `chat`'s messages
    /// come along; nothing else does. A no-op while the value is already
    /// `chat`'s, so choosing the open chat again keeps its bars.
    ///
    /// What other owners still hold for the chat left is ended here too:
    /// the forward selection (`ShareUi`), the auto-delete picker
    /// (`NotifyUi`, not per window) and a recording (`RecordingUi`, which
    /// owes TDLib and disk a cancel).
    pub(super) fn enter_chat_ui(&mut self, chat: ChatId, cx: &mut Context<Self>) {
        // Phase B4: the TTL picker belongs to the previous chat.
        self.notify.ttl_picker_open = false;
        self.dismiss_forward_for_chat(chat);
        self.cancel_recording_here(cx);
        if self.chat_ui.chat == Some(chat) {
            return;
        }
        let mut next = ChatUi::for_chat(Some(chat));
        let old = std::mem::take(&mut *self.screen.chat_ui);
        next.pending_reply = old.pending_reply.filter(|reply| reply.belongs_to(chat));
        next.edit_mode = old.edit_mode.filter(|mode| mode.edit.chat_id == chat);
        *self.screen.chat_ui = next;
    }

    /// The view in place shows no chat any more (`leave_open_chat`).
    pub(super) fn leave_chat_ui(&mut self) {
        *self.screen.chat_ui = ChatUi::for_chat(None);
    }

    /// Frame upkeep: the value follows the chat the view shows, however
    /// that chat changed (an upgrade to a supergroup, a link, another
    /// device). A value made for no chat is adopted by the first chat it
    /// sees; one made for another chat is replaced.
    pub(super) fn sync_chat_ui(&mut self) {
        let shown = self.session().and_then(|session| session.open_chat);
        match (self.chat_ui.chat, shown) {
            (None, Some(chat)) => self.screen.chat_ui.chat = Some(chat),
            (Some(made_for), Some(chat)) if made_for != chat => {
                *self.screen.chat_ui = ChatUi::for_chat(Some(chat));
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "chat_ui_tests.rs"]
mod tests;
