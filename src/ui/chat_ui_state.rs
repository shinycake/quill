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
use quill::ids::{ChatId, MessageId, TopicId};

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

/// One value of [`ChatUi`], told apart from every other: each one made
/// gets a new visit. Something parked for the chat shown (a click to play
/// a round video before its file landed) records it, and applies only
/// while the same value is in place: a chat change, closing the chat
/// (Esc) and coming back, or another topic replaced the value, so the
/// click was for a visit that is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ChatVisit(u64);

impl Default for ChatVisit {
    fn default() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self(NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }
}

/// The reply bar: the message the composer replies to, and its quote
/// picker. One value, so the picker goes with the reply however the reply
/// goes (sent, cancelled, replaced by a reply to another message).
#[derive(Debug, Clone)]
struct ReplyBar {
    reply: ComposerReplyTo,
    quote_picker: QuotePicker,
}

/// "Update Quote" opened the picker for the part of the message to quote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QuotePicker {
    Closed,
    Open,
}

/// One view's state for the chat it shows.
#[derive(Default)]
pub(crate) struct ChatUi {
    /// Which value this is; new with each one made.
    visit: ChatVisit,
    /// The chat this value was made for; `None` before any chat showed.
    /// Only [`QuillApp::enter_chat_ui`] sets it.
    chat: Option<ChatId>,
    /// The conversation of `chat` it was made for: a forum topic, thread,
    /// direct-messages conversation or Saved sublist
    /// (`Session::open_topic_id`; with `chat`, the view's
    /// `ConversationKey`). A forum's topics share the chat id, so without
    /// it an edit opened in one topic stayed in the next topic's composer.
    topic: TopicId,
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
    /// The trigger's query waits for the session: the bot is being
    /// resolved, or an older query of this chat and bot is in flight.
    /// `progress_inline_mode` sends it when updates land, in whichever
    /// window this value is in.
    pub(super) inline_query_waiting: bool,
    /// The rich draft's "discard" or "Remove Formatting" box: its buttons
    /// act on this chat's draft.
    pub(super) rich_draft_box: super::rich_draft::RichDraftBox,
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
    /// M1: the scheduled-messages dialog (view/delete). Here so that a
    /// chat change closes it: its rows act on the chat shown.
    pub(super) scheduled_dialog_open: bool,
    /// M1: the schedule picker popup above the composer.
    pub(super) schedule_popup_open: bool,
    /// The date+time picker behind the schedule popup (created when the
    /// popup opens, see `open_schedule_picker`). Its Reschedule target is
    /// a message id of this chat, so the picker goes with the chat.
    pub(super) schedule_picker: Option<super::scheduled::SchedulePicker>,
    /// Phase 4.2: poll creation dialog (open above the composer). Here so
    /// that a chat change closes it: Create posts to the chat shown.
    pub(super) poll_dialog: Option<super::PollDialog>,
    /// B15: the checklist composer / "Add Tasks" box. A new checklist goes
    /// to the chat shown, so the box goes with the chat.
    pub(super) checklist_dialog: Option<super::ChecklistDialog>,
    /// The info card under the open forum topic's strip.
    pub(super) topic_info_open: bool,
    /// The info card under the open reply thread's root bar.
    pub(super) thread_info_open: bool,
    /// Text a chat open handed over without a window at hand, for this
    /// chat's composer (`QuillApp::apply_compose_after_open`).
    pub(super) compose_after_open: Option<String>,
    /// Same-chat reply draft (tdesktop `FieldHeader::replyToMessage`).
    /// Private on purpose: a sender must take the reply through
    /// `QuillApp::reply_for_send` so a reply to a deleted message is dropped.
    pending_reply: Option<ReplyBar>,
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

    /// A fresh value for `topic` of `chat`.
    fn for_topic(chat: ChatId, topic: TopicId) -> Self {
        Self {
            chat: Some(chat),
            topic,
            ..Self::default()
        }
    }

    /// Which value this is (see [`ChatVisit`]).
    pub(super) fn visit(&self) -> ChatVisit {
        self.visit
    }

    /// The chat this value was made for.
    #[cfg(test)]
    pub(super) fn chat(&self) -> Option<ChatId> {
        self.chat
    }

    /// The reply the bar shows. Display and bookkeeping only: anything that
    /// sends reads `QuillApp::reply_for_send`, which drops a deleted target.
    pub(super) fn reply(&self) -> Option<&ComposerReplyTo> {
        self.pending_reply.as_ref().map(|bar| &bar.reply)
    }

    pub(super) fn has_reply(&self) -> bool {
        self.pending_reply.is_some()
    }

    /// A new quote for the same message keeps its picker open; a reply to
    /// another message, or none, closes it.
    pub(super) fn set_reply(&mut self, reply: Option<ComposerReplyTo>) {
        let picker = |next: &ComposerReplyTo| match &self.pending_reply {
            Some(bar)
                if (bar.reply.chat_id, bar.reply.message_id) == (next.chat_id, next.message_id) =>
            {
                bar.quote_picker
            }
            _ => QuotePicker::Closed,
        };
        self.pending_reply = reply.map(|reply| ReplyBar {
            quote_picker: picker(&reply),
            reply,
        });
    }

    pub(super) fn take_reply(&mut self) -> Option<ComposerReplyTo> {
        self.pending_reply.take().map(|bar| bar.reply)
    }

    /// "Update Quote": the picker opens for the reply shown.
    pub(super) fn open_quote_picker(&mut self) {
        if let Some(bar) = self.pending_reply.as_mut() {
            bar.quote_picker = QuotePicker::Open;
        }
    }

    pub(super) fn close_quote_picker(&mut self) {
        if let Some(bar) = self.pending_reply.as_mut() {
            bar.quote_picker = QuotePicker::Closed;
        }
    }

    pub(super) fn quote_picker_open(&self) -> bool {
        self.pending_reply
            .as_ref()
            .is_some_and(|bar| bar.quote_picker == QuotePicker::Open)
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
    /// the forward selection (`ShareUi`), the auto-delete picker of the
    /// chat left (`NotifyUi`, not per window: one open for a chat another
    /// window shows stays), and a recording (`RecordingUi`, which owes
    /// TDLib and disk a cancel). An inline clip parked to play was for the
    /// old value's visit, so it no longer starts (`ChatVisit`).
    pub(super) fn enter_chat_ui(&mut self, chat: ChatId, cx: &mut Context<Self>) {
        // Phase B4: the TTL picker belongs to the chat it was opened for.
        self.close_ttl_picker_of_left_chat(Some(chat));
        self.dismiss_forward_for_view(chat);
        self.cancel_recording_here(cx);
        if self.chat_ui.chat == Some(chat) {
            return;
        }
        self.profile_modal_leaves_chat(cx);
        let mut next = ChatUi::for_topic(chat, self.open_topic_id(chat));
        let old = &mut self.screen.chat_ui;
        next.pending_reply = old
            .pending_reply
            .take()
            .filter(|bar| bar.reply.belongs_to(chat));
        next.edit_mode = old
            .edit_mode
            .take()
            .filter(|mode| mode.edit.chat_id == chat);
        self.replace_chat_ui(next);
    }

    /// The one writer of the value in place: the visit it ends takes with
    /// it a round video or GIF parked to play during it, so it neither
    /// starts with sound when the chat shows again nor holds the slot.
    fn replace_chat_ui(&mut self, next: ChatUi) {
        let ended = std::mem::replace(&mut *self.screen.chat_ui, next).visit;
        self.drop_play_intent_of_visit(ended);
    }

    /// The view in place leaves its chat for `next` (`None`: no chat):
    /// an auto-delete picker open for the chat left closes with it.
    pub(super) fn close_ttl_picker_of_left_chat(&mut self, next: Option<ChatId>) {
        let left = self.chat_ui.chat.filter(|chat| Some(*chat) != next);
        if left.is_some() && self.notify.ttl_picker == left {
            self.notify.ttl_picker = None;
        }
    }

    /// The `ChatUi` in place was made for the chat and topic the view
    /// shows now. A click can land between a chat change and the frame
    /// that replaces the value (`sync_chat_ui`): a box of the old chat
    /// must not act on the new one then.
    pub(super) fn chat_ui_is_current(&self) -> bool {
        let Some(chat) = self.open_chat_id() else {
            return false;
        };
        self.chat_ui.chat == Some(chat) && self.chat_ui.topic == self.open_topic_id(chat)
    }

    /// The view in place shows no chat any more (`leave_open_chat`).
    pub(super) fn leave_chat_ui(&mut self) {
        self.replace_chat_ui(ChatUi::for_chat(None));
    }

    /// Frame upkeep: the value follows the chat and topic the view shows,
    /// however they changed (an upgrade to a supergroup, a link, another
    /// device, another forum topic or thread). A value made for no chat is
    /// adopted by the first chat it sees; one made for another chat, or
    /// another topic of the same chat, is replaced. On a topic change the
    /// reply comes along (the new topic's draft restore decides it, as for
    /// any slot change); an edit does not, since its message is the old
    /// topic's, and the slot's draft goes back into the field it held.
    pub(super) fn sync_chat_ui(&mut self) {
        let Some(chat) = self.session().and_then(|session| session.open_chat) else {
            return;
        };
        let topic = self.open_topic_id(chat);
        match self.chat_ui.chat {
            None => {
                self.screen.chat_ui.chat = Some(chat);
                self.screen.chat_ui.topic = topic;
            }
            Some(made_for) if made_for != chat => {
                self.replace_chat_ui(ChatUi::for_topic(chat, topic));
            }
            Some(_) if self.chat_ui.topic != topic => {
                let mut next = ChatUi::for_topic(chat, topic);
                next.pending_reply = self.screen.chat_ui.pending_reply.take();
                // The field still holds the edited message (or the rich
                // editor's markup), which the new value no longer marks:
                // put the slot's draft back on this frame, or Enter sent
                // the edit as a new message and the upkeep saved it as the
                // draft. A Saved sublist or a thread inside a forum topic
                // keeps the draft slot, so no switch path restores it.
                let field_is_not_the_draft =
                    self.chat_ui.is_editing() || self.chat_ui.rich_editor_open;
                self.replace_chat_ui(next);
                if field_is_not_the_draft {
                    self.request_draft_restore();
                }
                // The selection was the old conversation's messages.
                self.dismiss_forward_for_view(chat);
            }
            Some(_) => {}
        }
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "chat_ui_tests.rs"]
mod tests;
