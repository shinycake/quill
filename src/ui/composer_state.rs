//! Composer state: menus, dialogs, reply and edit drafts. The next send's
//! attachments and options are per chat, in `composer_sends.rs`.
#![deny(clippy::indexing_slicing)]

use super::*;
use quill::composer::ComposerEdit;
use quill::composer::ComposerReplyTo;
use quill::composer::DraftTopic;
use quill::ids::ChatId;
use quill::ids::MessageId;
use quill::telegram::envelope::ChatDraft;

/// A draft to clear once a send is acknowledged. `sent` is the composer
/// markup that send took: when set, the stored draft goes only while it
/// still equals it, so text kept or typed since survives a late ack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DraftClear {
    pub(super) chat_id: ChatId,
    pub(super) sent: Option<String>,
}

impl DraftClear {
    pub(super) fn whole_composer(chat_id: ChatId) -> Self {
        Self {
            chat_id,
            sent: None,
        }
    }

    pub(super) fn of_text(chat_id: ChatId, sent: &str) -> Self {
        Self {
            chat_id,
            sent: Some(sent.to_string()),
        }
    }

    /// Whether the ack for `chat_id` may clear its draft, given the
    /// draft text stored now.
    pub(super) fn matches(&self, chat_id: ChatId, stored: Option<&str>) -> bool {
        self.chat_id == chat_id
            && self
                .sent
                .as_deref()
                .is_none_or(|sent| stored.is_none_or(|stored| stored == sent))
    }
}

pub(crate) struct ComposerUi {
    /// Phase 3.3: `/` command menu state. Open while the composer text
    /// ends with a `/`-led token and the open bot chat has commands;
    /// `command_menu_selected` is the highlighted row (Up/Down/Enter).
    pub(super) command_menu_open: bool,
    pub(super) command_menu_selected: usize,
    /// Highlighted row of the composer's `@` suggestions.
    pub(super) mention_selected: usize,
    /// Composer `#hashtag` / `:emoji` autocomplete popup.
    pub(super) suggest: super::composer_suggest::SuggestUi,
    pub(super) emoji_words: super::emoji_words_ui::EmojiWordsUi,
    /// Bots slice: `@botname query` inline-mode results dropdown above
    /// the composer. `inline_results_selected` is the highlighted row
    /// (Up/Down/Enter); `inline_query_token` debounces the
    /// `getInlineQueryResults` dispatch (100ms quiet window, TGX).
    /// `inline_query_armed` is the `(username, query)` a debounce timer
    /// is currently armed for — `poll_live` re-runs the inline progress
    /// check after every batch of session updates, and the armed identity
    /// keeps it from spawning a duplicate timer per poll.
    pub(super) inline_results_open: bool,
    pub(super) inline_results_selected: usize,
    pub(super) inline_query_token: u64,
    pub(super) inline_query_armed: Option<(String, String)>,
    /// Scroll position of the inline results panel (`inline_grid.rs`).
    pub(super) inline_scroll: gpui_kit::ScrollHandle,
    /// The chat an inline results button left, to come back to
    /// (`inline_return.rs`).
    pub(super) inline_return: Option<super::inline_return::InlineReturn>,
    /// B5: "Send as a document" for the replacement file while editing
    /// media (tdesktop `EditCaptionBox::_asFile`).
    pub(super) edit_replace_as_file: bool,
    /// MED4b: debounce token for the `getLinkPreview` prefetch — each
    /// keystroke bumps it so only the latest quiet window fires (schema:
    /// "Do not call this function too often"; TGX rate-limits 400ms).
    pub(super) preview_token: u64,
    /// The composer text before the last keystroke, for "Replace emoji
    /// automatically" (it only reacts to one typed character).
    pub(super) prev_text: String,
    /// A typed markdown replacement Backspace can still take back.
    pub(super) markdown_revert: Option<super::composer_field::MarkdownRevert>,
    /// "Silent send" shortcut: the next send alone goes without sound.
    pub(super) send_silent_once: bool,
    /// "Send with Effect": the next send alone carries this effect
    /// (`messageSendOptions.effect_id`); zero for none.
    pub(super) send_effect_once: i64,
    /// The send button's options menu and its effect picker
    /// (`send_effects`).
    pub(super) send_menu: super::send_effects::SendMenuUi,
    /// M1: the schedule picker popup above the composer.
    pub(super) schedule_popup_open: bool,
    /// The date+time picker behind the schedule popup (created when the
    /// popup opens, see `open_schedule_picker`).
    pub(super) schedule_picker: Option<super::scheduled::SchedulePicker>,
    /// M1: the scheduled-messages dialog (view/delete).
    pub(super) scheduled_dialog_open: bool,
    /// Scheduled messages ticked in the dialog (send now, reschedule or
    /// delete together).
    pub(super) scheduled_selected: Vec<MessageId>,
    /// M2: the rich editor is open — the composer textarea is interpreted
    /// as block markup (`quill::rich::markup_to_blocks`) and sends via
    /// `inputMessageRichMessage`. Opened via the ⛶ button (visible after
    /// 3+ lines, per the anniversary post).
    pub(super) rich_editor_open: bool,
    /// A rich draft's discard and Premium boxes and the rich paste notice
    /// (`rich_draft`).
    pub(super) rich_draft: super::rich_draft::RichDraftUi,
    /// Same-chat reply draft (tdesktop `FieldHeader::replyToMessage`).
    /// Private on purpose: a sender must take the reply through
    /// `QuillApp::reply_for_send` so a reply to a deleted message is dropped.
    pending_reply: Option<ComposerReplyTo>,
    /// Chat whose draft should be cleared after `updateMessageSendSucceeded`
    /// if the composer is still empty, and the text that send took.
    pub(super) clear_draft_on_success: Option<DraftClear>,
    /// A topic or thread switch asked for the new slot's draft to be put
    /// in the composer on the next frame (`restore_draft_if_pending`).
    pub(super) draft_restore_pending: bool,
    /// The session's remote draft revision this composer last checked
    /// (`QuillApp::adopt_remote_draft`).
    pub(super) remote_draft_seen: u64,
    /// The slot this composer last showed and its stored draft then. Only
    /// a change to that draft replaces the field: text a link put in the
    /// composer survives drafts arriving for other chats.
    pub(super) shown_draft: Option<(ChatId, DraftTopic, Option<ChatDraft>)>,
    /// Own-message edit (tdesktop `FieldHeader::editMessage`).
    pub(super) pending_edit: Option<ComposerEdit>,
    /// The "Cancel editing?" box is open (`DialogKind::CancelEditConfirm`).
    pub(super) cancel_edit_confirm: bool,
    /// What the compose field's placeholder was last built from
    /// (`sync_composer_placeholder`): the facts and the language.
    pub(super) placeholder_shown: Option<(quill::composer::PlaceholderFacts, usize)>,
    /// Normal composer draft stashed while editing (`DraftType::Normal`).
    pub(super) saved_edit_draft: String,
    /// Reply that belonged to that normal draft. Restored with the text on cancel.
    pub(super) saved_edit_reply: Option<ComposerReplyTo>,
    /// The composer's link dialog (Cmd/Ctrl+K on a selection).
    pub(super) link_dialog: Option<super::composer_shortcuts::ComposerLinkDialog>,
    /// The composer's date box (Formatting > Date).
    pub(super) date_dialog: Option<super::composer_date::ComposerDateDialog>,
    /// The "Code Language" box for the fenced block under the caret.
    pub(super) code_language: Option<super::composer_shortcuts::CodeLanguageDialog>,
    /// MED1: composer "group media" override for 2+ attachments; `None`
    /// follows `media_prefs.default_grouping()`.
    pub(super) group_media: Option<bool>,
    /// Cross-fade timeline of the round Send / Record / Save button.
    pub(super) send_morph: std::cell::Cell<Option<quill::send_button::SendMorph>>,
    /// Phase S2: pending inline-bot warning for a `SwitchInline` press in
    /// a secret chat (TGX `SecretChatContextBotAlert`) — the stashed
    /// query is inserted on Confirm.
    pub(super) pending_inline_bot_alert: Option<String>,
    /// Phase S2: the inline-bot warning has been confirmed once this
    /// session (TGX `TUTORIAL_INLINE_SEARCH_SECRECY`, which persists;
    /// Quill keeps it per session — documented divergence).
    pub(super) inline_bot_alert_shown: bool,
    /// Files being dragged over the conversation, and what they hold
    /// (`drop_zones`).
    pub(super) drop_paths: Vec<std::path::PathBuf>,
    pub(super) drop_state: Option<quill::drop_modes::DragState>,
    /// A demo's fixed drop-zone state (no real drag in a screenshot).
    pub(super) drop_preview: Option<quill::drop_modes::DragState>,
    /// The composer's "send as" identity list is open.
    pub(super) send_as_open: bool,
    /// Phase A1: the open chat whose slow-mode countdown is ticking
    /// (`Some` exactly while the 1s tick task runs). Mirrors `voice_tick`.
    pub(super) slow_mode_tick_chat: Option<ChatId>,
    /// Phase 4.2: poll creation dialog (open above the composer).
    pub(super) poll_dialog: Option<PollDialog>,
    /// B15: the checklist composer / "Add Tasks" box.
    pub(super) checklist_dialog: Option<ChecklistDialog>,
    /// The attachment tray's file checks, price box and video editor.
    pub(super) send_box: super::send_box_state::SendBoxUi,
}

impl ComposerUi {
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

    pub(super) fn new() -> Self {
        Self {
            command_menu_open: false,
            command_menu_selected: 0,
            mention_selected: 0,
            suggest: super::composer_suggest::SuggestUi::load(),
            emoji_words: Default::default(),
            inline_results_open: false,
            inline_results_selected: 0,
            inline_query_token: 0,
            inline_query_armed: None,
            inline_scroll: gpui_kit::ScrollHandle::new(),
            inline_return: None,
            edit_replace_as_file: false,
            preview_token: 0,
            prev_text: String::new(),
            markdown_revert: None,
            send_silent_once: false,
            send_effect_once: 0,
            send_menu: Default::default(),
            schedule_popup_open: false,
            schedule_picker: None,
            scheduled_dialog_open: false,
            scheduled_selected: Vec::new(),
            rich_editor_open: false,
            rich_draft: Default::default(),
            pending_reply: None,
            clear_draft_on_success: None,
            draft_restore_pending: false,
            remote_draft_seen: 0,
            shown_draft: None,
            pending_edit: None,
            cancel_edit_confirm: false,
            placeholder_shown: None,
            saved_edit_draft: String::new(),
            saved_edit_reply: None,
            link_dialog: None,
            date_dialog: None,
            code_language: None,
            group_media: None,
            send_morph: Default::default(),
            pending_inline_bot_alert: None,
            inline_bot_alert_shown: false,
            drop_paths: Vec::new(),
            drop_state: None,
            drop_preview: None,
            send_as_open: false,
            slow_mode_tick_chat: None,
            poll_dialog: None,
            checklist_dialog: None,
            send_box: super::send_box_state::SendBoxUi::new(),
        }
    }
}
