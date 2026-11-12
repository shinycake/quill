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
use super::reply_options_ui::ReplyPanel;
use gpui_kit::*;
use quill::composer::{ComposerEdit, ComposerReplyTo};
use quill::ids::{ChatId, MessageId, TopicId};

/// Which info cards are open in a chat. A thread's card opens over a
/// topic's and the topic's is back when the thread closes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum InfoCard {
    #[default]
    Closed,
    /// Under the open forum topic's strip.
    Topic,
    /// Under the open reply thread's root bar.
    Thread,
    /// Both: the topic's card stays under the thread's.
    Both,
}

impl InfoCard {
    pub(crate) fn topic(self) -> bool {
        matches!(self, Self::Topic | Self::Both)
    }

    pub(crate) fn thread(self) -> bool {
        matches!(self, Self::Thread | Self::Both)
    }

    fn of(topic: bool, thread: bool) -> Self {
        match (topic, thread) {
            (false, false) => Self::Closed,
            (true, false) => Self::Topic,
            (false, true) => Self::Thread,
            (true, true) => Self::Both,
        }
    }

    /// The thread view closed (or another opened): its card goes, a
    /// topic's stays.
    pub(super) fn close_thread(&mut self) {
        *self = Self::of(self.topic(), false);
    }

    /// The topic's card opens (its menu's Info).
    pub(super) fn open_topic(&mut self) {
        *self = Self::of(true, self.thread());
    }

    /// A tap on the topic card's button.
    pub(super) fn toggle_topic(&mut self) {
        *self = Self::of(!self.topic(), self.thread());
    }

    /// A tap on the thread card's button.
    pub(super) fn toggle_thread(&mut self) {
        *self = Self::of(self.topic(), !self.thread());
    }
}

/// The composer while a message is edited: the edit, the normal draft and
/// reply stashed for it (`DraftType::Normal`), and the "Cancel editing?" box
/// (`DialogKind::CancelEditConfirm`). One value, so the box cannot outlive
/// the edit.

#[derive(Debug)]
pub(super) struct EditMode {
    /// Told apart from every other edit: a picker or an answer started for
    /// an edit applies only while this edit is still the one in place.
    pub(super) id: EditId,
    pub(super) stage: EditStage,
    pub(super) edit: ComposerEdit,
    pub(super) stashed_draft: String,
    /// Reply that belonged to the stashed draft, restored with it on leaving.
    pub(super) stashed_reply: Option<ComposerReplyTo>,
    /// B5: "Send as a document" for the replacement file while editing
    /// media (tdesktop `EditCaptionBox::_asFile`). The edit's own choice:
    /// another edit starts without it.
    pub(super) replace_as_file: bool,
}

/// One edit, told apart from every other (an edit cancelled and one started
/// on the same message are two).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EditId(u64);

impl EditId {
    fn next() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self(NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }
}

/// Where an edit stands: being typed, asking "Cancel editing?", or sent and
/// waiting for TDLib's answer. The edit leaves edit mode on the answer
/// (`ok`); an error returns it to `Editing` with the text still in the
/// field. A sent edit cannot be cancelled: the answer decides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EditStage {
    Editing,
    /// The "Cancel editing?" box is open over the edit.
    ConfirmingCancel,
    Sending(quill::ids::RequestId),
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

/// The reply bar: the message the composer replies to, and the panel open
/// for it (quote picker or chat chooser). One value, so the panel goes with
/// the reply however the reply goes (sent, cancelled, replaced by a reply to
/// another message, the chat view replaced).
#[derive(Debug, Clone)]
struct ReplyBar {
    reply: ComposerReplyTo,
    panel: ReplyPanel,
}

/// One view's state for the chat it shows.
/// Where the keyboard and the screen reader rest in the open chat's history.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum A11yFocus {
    #[default]
    None,
    /// The "Unread messages" bar.
    Bar,
    Row(MessageId),
}

impl A11yFocus {
    /// The focused message row, if a row has it.
    pub(crate) fn row(self) -> Option<MessageId> {
        match self {
            Self::Row(id) => Some(id),
            _ => None,
        }
    }

    pub(crate) fn is_bar(self) -> bool {
        self == Self::Bar
    }

    /// Selection let go of its row: the unread bar keeps its focus.
    pub(crate) fn clear_row(&mut self) {
        if matches!(self, Self::Row(_)) {
            *self = Self::None;
        }
    }
}

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
    /// M2: the rich editor (opened via the ⛶ button, visible after 3+
    /// lines, per the anniversary post) and the rich draft's "discard" or
    /// "Remove Formatting" box over the composer: its buttons act on this
    /// chat's draft.
    pub(super) rich: super::rich_draft::RichComposer,
    /// The composer's "send as" identity list is open.
    pub(super) send_as_open: bool,
    /// Messages of this chat whose fact-check block is expanded (tdesktop
    /// `Factcheck::expanded`, held by the view's own message element): each
    /// window has its own, and a chat change starts them collapsed.
    pub(super) fact_check_expanded: std::collections::HashSet<i64>,
    /// The copy menu of a phone number, card number or date.
    pub(super) link_popup: Option<super::entity_links::LinkPopup>,
    /// The address tooltip of the link under the pointer. It goes with the
    /// chat: another chat under a resting pointer shows none.
    pub(super) link_tooltip: Option<super::entity_links::LinkTooltip>,
    /// A topic menu action whose note waits for its answer. It goes with
    /// the chat, so another chat never shows it.
    pub(super) topic_action: super::forum_extras::TopicAction,
    /// The bot command list above the composer: closed, opened by a typed
    /// `/` token, or opened by the bot's Menu button.
    pub(super) command_menu: super::composer::CommandMenu,
    /// Bots slice: `@botname query` inline-mode results dropdown above the
    /// composer: closed, or open with its highlighted row and what its
    /// query waits for (the 100ms debounce timer of TGX, or the session).
    pub(super) inline: super::inline_stage::InlineMode,
    /// "Save as HTML" with "Ask where to save" on: the folder picker of
    /// this chat's selection, while it is open.
    pub(super) html_save: super::selection_html::HtmlSave,
    /// The banner for the last `forwardMessages` answer of this chat's
    /// forward (Dismiss or Esc ends it). The chat's own: another chat starts
    /// without it, and a window that does not show the chat draws none.
    pub(super) forward_result: Option<quill::state::ForwardResult>,
    /// "Save Selected": waiting for the folder the user picks. The picker's
    /// task lives in the stage, so a chat change drops it with the selection.
    pub(super) save_selection: super::selection_mode::SaveSelection,
    /// What the selected topic tab was last scrolled into view for. Per
    /// chat and topic: another chat's strip with the same index, count and
    /// width starts at the old scroll offset until it is revealed.
    pub(super) tabs_revealed: std::cell::Cell<Option<super::folder_tabs_fit::RevealKey>>,
    /// The 1 s tick task of this chat's self-destruct countdown badges
    /// (phase B3). Held by the chat's value, so the chat changing drops it
    /// and a return to the chat starts one task, not two.
    pub(super) self_destruct_tick: Option<(ChatId, gpui_kit::Task<()>)>,
    /// Cross-fade timeline of the round Send / Record / Save button. A
    /// chat starts with none, so its button begins settled instead of
    /// fading from the chat before.
    pub(super) send_morph: std::cell::Cell<Option<quill::send_button::SendMorph>>,
    /// Phase S2: pending inline-bot warning for a `SwitchInline` press (or
    /// a typed trigger) in this secret chat (TGX `SecretChatContextBotAlert`);
    /// the stashed query is inserted on Confirm. It lives with the chat
    /// that asked, so another chat never confirms it.
    pub(super) bot_alert: super::inline_mode::BotAlert,
    /// A bot's Restart whose unblock is out (`bottom_action`).
    pub(super) restart: super::bottom_action::BotRestart,
    /// What the last rich send took out of the field, until TDLib answers.
    pub(super) sent_text: super::composer::SentText,
    /// The draft the last AI composer action was asked about.
    pub(super) ai_ask: super::composer::AiAsk,
    /// The composer's link dialog (Cmd/Ctrl+K on a selection). It holds a
    /// byte range of this chat's draft, so a chat change replaces it.
    pub(super) link_dialog: Option<super::composer_shortcuts::ComposerLinkDialog>,
    /// The composer's date box (Formatting > Date).
    pub(super) date_dialog: Option<super::composer_date::ComposerDateDialog>,
    /// The "Code Language" box for the fenced block under the caret.
    pub(super) code_language: Option<super::composer_shortcuts::CodeLanguageDialog>,
    /// B4: pending "Stop poll" confirm (`stopPoll`, schema 1.8.67 line
    /// 12953) — (chat, message, is_quiz) for the warning copy. TGX
    /// `StopPollWarn` / `StopQuizWarn`: nobody can vote afterwards and
    /// the action can't be undone.
    pub(super) pending_stop_poll: Option<(ChatId, MessageId, bool)>,
    /// What keyboard selection and the screen reader's focus rest on in
    /// this chat: a row (Ctrl+Space, Up / Down) or the unread bar. One
    /// value of the chat, so another chat starts with none.
    pub(super) focus: A11yFocus,
    /// M1: the scheduled-messages dialog (view/delete) with the load of
    /// its list and the messages ticked in it (send now, reschedule or
    /// delete together). Here so that a chat change closes it: its rows act
    /// on the chat shown.
    pub(super) scheduled_box: super::scheduled_box::ScheduledBox,
    /// M1: the swipe-to-reply press of the gesture under way; it names this
    /// chat's rows, so another chat starts with none.
    pub(super) swipe_reply: super::conversation::swipe_reply::SwipeReply,
    /// Scheduled deletes sent from this chat and not yet answered.
    pub(super) scheduled_deletes: Vec<super::composer::ScheduledDelete>,
    /// M1: the schedule picker popup above the composer: `Some` while it is
    /// open, with the date+time picker it holds (created when the popup
    /// opens, see `open_schedule_picker`). Its Reschedule target is a message
    /// id of this chat, so the picker goes with the chat. The share box's
    /// schedule step is a picker of its own target in the same slot.
    pub(super) schedule_picker: Option<super::scheduled::SchedulePicker>,
    /// Phase 4.2: poll creation dialog (open above the composer). Here so
    /// that a chat change closes it: Create posts to the chat shown.
    pub(super) poll_dialog: Option<super::PollDialog>,
    /// B15: the checklist composer / "Add Tasks" box. A new checklist goes
    /// to the chat shown, so the box goes with the chat.
    pub(super) checklist_dialog: Option<super::ChecklistDialog>,
    /// The attach menu's contact chooser. Here so that a chat change closes
    /// it: the contact goes to the chat this value was made for.
    pub(super) contact_dialog: Option<super::ShareContentDialog>,
    /// The info card under the open forum topic's strip or the open reply
    /// thread's root bar: one at a time, and gone with the chat.
    pub(super) info_card: InfoCard,
    /// Text a chat open handed over without a window at hand, for this
    /// chat's composer (`QuillApp::apply_compose_after_open`).
    pub(super) compose_after_open: Option<String>,
    /// A Send in a bot's "All" view waiting for its new thread.
    pub(super) bot_thread: super::composer::bot_thread::BotThreadStart,
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
    /// Hide the schedule popup above the composer. The share box's schedule
    /// step is not that popup and stays.
    pub(super) fn hide_schedule_popup(&mut self) {
        if self
            .schedule_picker
            .as_ref()
            .is_some_and(|picker| picker.target != super::scheduled::ScheduleTarget::Share)
        {
            self.schedule_picker = None;
        }
    }

    /// The popup above the composer is showing.
    pub(super) fn schedule_popup_shown(&self) -> bool {
        self.schedule_picker
            .as_ref()
            .is_some_and(|picker| picker.target != super::scheduled::ScheduleTarget::Share)
    }

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

    /// The chat this value was made for (what a box opened over it acts on).
    pub(super) fn made_for(&self) -> Option<ChatId> {
        self.chat
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

    /// A new quote for the same message keeps its panel open; a reply to
    /// another message, or none, closes it.
    pub(super) fn set_reply(&mut self, reply: Option<ComposerReplyTo>) {
        let panel = |next: &ComposerReplyTo| match &self.pending_reply {
            Some(bar)
                if (bar.reply.chat_id, bar.reply.message_id) == (next.chat_id, next.message_id) =>
            {
                bar.panel
            }
            _ => ReplyPanel::None,
        };
        self.pending_reply = reply.map(|reply| ReplyBar {
            panel: panel(&reply),
            reply,
        });
    }

    pub(super) fn take_reply(&mut self) -> Option<ComposerReplyTo> {
        self.pending_reply.take().map(|bar| bar.reply)
    }

    /// The panel open over the reply shown (`None` without a reply).
    pub(super) fn reply_panel(&self) -> ReplyPanel {
        self.pending_reply
            .as_ref()
            .map_or(ReplyPanel::None, |bar| bar.panel)
    }

    /// Open `panel` for the reply shown, in place of the one open (one
    /// value: at most one panel). Nothing without a reply.
    pub(super) fn show_reply_panel(&mut self, panel: ReplyPanel) {
        if let Some(bar) = self.pending_reply.as_mut() {
            bar.panel = panel;
        }
    }

    /// The forward box opens: a chat chooser open over the reply closes.
    pub(super) fn close_reply_chooser(&mut self) {
        if matches!(self.reply_panel(), ReplyPanel::Chooser(_)) {
            self.show_reply_panel(ReplyPanel::None);
        }
    }

    /// The chat chooser's first draw asks: true once per opening, so its
    /// search field takes focus after the dialog has mounted.
    pub(super) fn take_chooser_first_draw(&mut self) -> bool {
        match self.pending_reply.as_mut().map(|bar| &mut bar.panel) {
            Some(ReplyPanel::Chooser(mount)) => mount.take_first_draw(),
            _ => false,
        }
    }

    pub(super) fn is_editing(&self) -> bool {
        self.edit_mode.is_some()
    }

    /// The edit in place, told apart from any other.
    pub(super) fn edit_id(&self) -> Option<EditId> {
        self.edit_mode.as_ref().map(|mode| mode.id)
    }

    /// The `editMessage` request the edit waits on, if it was sent.
    pub(super) fn edit_sending(&self) -> Option<quill::ids::RequestId> {
        match self.edit_mode.as_ref()?.stage {
            EditStage::Sending(id) => Some(id),
            EditStage::Editing | EditStage::ConfirmingCancel => None,
        }
    }

    /// The edit was sent as `request`.
    pub(super) fn mark_edit_sending(&mut self, request: quill::ids::RequestId) {
        if let Some(mode) = self.edit_mode.as_mut() {
            mode.stage = EditStage::Sending(request);
        }
    }

    /// The send failed: the edit goes on, its text still in the field.
    pub(super) fn back_to_editing(&mut self) {
        if let Some(mode) = self.edit_mode.as_mut()
            && matches!(mode.stage, EditStage::Sending(_))
        {
            mode.stage = EditStage::Editing;
        }
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
            id: EditId::next(),
            stage: EditStage::Editing,
            edit,
            stashed_draft,
            stashed_reply,
            replace_as_file: false,
        });
    }

    /// "Send as a document" ticked for the replacement of the edit in place.
    pub(super) fn edit_replace_as_file(&self) -> bool {
        self.edit_mode
            .as_ref()
            .is_some_and(|mode| mode.replace_as_file)
    }

    pub(super) fn set_edit_replace_as_file(&mut self, on: bool) {
        if let Some(mode) = self.edit_mode.as_mut() {
            mode.replace_as_file = on;
        }
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
            .is_some_and(|mode| mode.stage == EditStage::ConfirmingCancel)
    }

    /// Opens or closes the box. Opening is ignored without an edit and for
    /// an edit already sent (the answer decides); closing leaves any other
    /// stage as it is.
    pub(super) fn set_cancel_edit_confirm(&mut self, open: bool) {
        let Some(mode) = self.edit_mode.as_mut() else {
            return;
        };
        mode.stage = match (open, mode.stage) {
            (true, EditStage::Editing) => EditStage::ConfirmingCancel,
            (false, EditStage::ConfirmingCancel) => EditStage::Editing,
            (_, stage) => stage,
        };
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
        // The forward bar follows the topic too, so it is checked for the
        // chat chosen again as well; the rest ends only with a chat change.
        self.close_ttl_picker_of_left_chat(Some(chat));
        self.dismiss_forward_for_view(chat);
        if self.chat_ui.chat == Some(chat) {
            return;
        }
        self.end_leftovers_of_left_chat(chat, cx);
        let mut next = ChatUi::for_topic(chat, self.open_topic_id(chat));
        let old = &mut self.screen.chat_ui;
        next.pending_reply = old
            .pending_reply
            .take()
            .filter(|bar| bar.reply.belongs_to(chat));
        // An edit of another chat's message does not come along; one that
        // was already sent stops waiting for its answer.
        let dropped = match old.edit_mode.take() {
            Some(mode) if mode.edit.chat_id == chat => {
                next.edit_mode = Some(mode);
                None
            }
            other => other.and_then(|mode| match mode.stage {
                EditStage::Sending(request) => Some(request),
                _ => None,
            }),
        };
        if let Some(request) = dropped
            && let Some(live) = self.live.as_mut()
        {
            live.driver.session.acks.forget(request);
        }
        self.replace_chat_ui(next);
    }

    /// What other owners hold for the chat the view leaves for `chat`, ended
    /// in one place for both ways a chat changes (`enter_chat_ui` and the
    /// frame's `sync_chat_ui`): the auto-delete picker and forward selection
    /// of the chat left, a recording started in this window, and the profile
    /// modal.
    fn end_leftovers_of_left_chat(&mut self, chat: ChatId, cx: &mut Context<Self>) {
        self.close_ttl_picker_of_left_chat(Some(chat));
        self.dismiss_forward_for_view(chat);
        self.cancel_recording_here(cx);
        self.profile_modal_leaves_chat(cx);
    }

    /// The one writer of the value in place: the visit it ends takes with
    /// it a round video or GIF parked to play during it, so it neither
    /// starts with sound when the chat shows again nor holds the slot.
    fn replace_chat_ui(&mut self, next: ChatUi) {
        let mut old = std::mem::replace(&mut *self.screen.chat_ui, next);
        // The send menu (and its effect card) was opened over the old
        // value's composer; its popup and subscriptions go with it.
        self.composer_ui.send_menu.open = None;
        if let Some(request) = old.edit_sending()
            && let Some(live) = self.live.as_mut()
        {
            live.driver.session.acks.forget(request);
        }
        // A rich send still waiting goes on waiting off the view, so a
        // refusal after the chat changed still gives the text back.
        self.leave_sent_text(std::mem::take(&mut old.sent_text));
        if let Some(request) = old
            .checklist_dialog
            .as_ref()
            .and_then(|dialog| dialog.send.request())
            && let Some(live) = self.live.as_mut()
        {
            live.driver.session.acks.forget(request);
        }
        if let Some(request) = old.scheduled_box.loading()
            && let Some(live) = self.live.as_mut()
        {
            live.driver.session.acks.forget(request);
        }
        if let super::bottom_action::BotRestart::Unblocking { request, .. } = old.restart
            && let Some(live) = self.live.as_mut()
        {
            live.driver.session.acks.forget(request);
        }
        for delete in &old.scheduled_deletes {
            if let Some(live) = self.live.as_mut() {
                live.driver.session.acks.forget(delete.request);
            }
        }
        self.drop_play_intent_of_visit(old.visit);
    }

    /// The view in place leaves its chat for `next` (`None`: no chat):
    /// an auto-delete picker open for the chat left closes with it.
    pub(super) fn close_ttl_picker_of_left_chat(&mut self, next: Option<ChatId>) {
        let left = self.chat_ui.chat.filter(|chat| Some(*chat) != next);
        if left.is_some() && self.notify.ttl_picker == left {
            self.close_ttl_picker();
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
    pub(super) fn sync_chat_ui(&mut self, cx: &mut Context<Self>) {
        let Some(chat) = self.session().and_then(|session| session.open_chat_id()) else {
            // The reducer closed the chat (removed, left, kicked) with no
            // goodbye of the view's own: what was open for it goes, or it
            // came back with the chat the next time it opens.
            if self.session().is_some() && self.chat_ui.chat.is_some() {
                self.close_ttl_picker_of_left_chat(None);
                self.leave_chat_ui();
            }
            return;
        };
        let topic = self.open_topic_id(chat);
        match self.chat_ui.chat {
            None => {
                self.screen.chat_ui.chat = Some(chat);
                self.screen.chat_ui.topic = topic;
            }
            Some(made_for) if made_for != chat => {
                self.end_leftovers_of_left_chat(chat, cx);
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
                    self.chat_ui.is_editing() || self.chat_ui.rich.editor_open();
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
