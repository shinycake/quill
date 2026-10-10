//! Composer state: menus, attachments, send options, reply and edit drafts.

use super::*;
use quill::composer::ComposerAttachment;
use quill::composer::ComposerEdit;
use quill::composer::ComposerReplyTo;
use quill::composer::ComposerScheduling;
use quill::composer::PreviewMediaSize;
use quill::ids::ChatId;
use quill::ids::MessageId;
use quill::telegram::requests::SelfDestructSend;

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
    /// Local files the user explicitly attached (canonical paths via `pick`).
    /// One item sends with `sendMessage`. Two or more photos/videos send with
    /// `sendMessageAlbum`.
    pub(super) pending_attachments: Vec<ComposerAttachment>,
    /// Phase B3: the composer's self-destruct choice for photo/video
    /// sends (`inputMessagePhoto`/`inputMessageVideo`
    /// `self_destruct_type`, schema 1.8.67 lines 6117/6128 — private
    /// chats only). Cycles Off → 5s → 30s → 1m → View once via the
    /// picker button; captured into `ComposerSnapshot` at submit time.
    pub(super) self_destruct: Option<SelfDestructSend>,
    /// MED4: caption-above-media toggle for photo/video sends
    /// (`show_caption_above_media`, schema 1.8.67 lines 6117/6128).
    /// Captured into `ComposerSnapshot` at submit time; reset after send.
    pub(super) caption_above: bool,
    /// M1: silent-send toggle (`messageSendOptions.disable_notification`,
    /// schema 1.8.67 line 5934). Persists across sends until toggled.
    pub(super) silent: bool,
    /// The chat whose `default_disable_notification` the user turned off
    /// for the composer ("Send with sound" in a chat that sends silently).
    pub(super) loud_chat: Option<i64>,
    /// M1: link-preview toggle (`linkPreviewOptions.is_disabled`, schema
    /// 1.8.67 line 2237). Persists across sends; secret chats force it on.
    pub(super) preview_disabled: bool,
    /// MED4b: `linkPreviewOptions.show_above_text` (schema:2236, TGX
    /// `onRequestToggleShowAbove`). Persists across sends like the
    /// disable toggle; hidden while the preview is off.
    pub(super) preview_above: bool,
    /// MED4b: `linkPreviewOptions.force_small_media` /
    /// `force_large_media` (schema:2234-2235, TGX
    /// `onRequestToggleLargeMedia`). Persists across sends; the size
    /// button only renders when the prefetched preview actually offers
    /// large media.
    pub(super) preview_media: PreviewMediaSize,
    /// B5: which detected link drives the preview (tdesktop "choose
    /// link"); an index into the composer text's URLs, clamped.
    pub(super) preview_link: usize,
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
    /// M1: scheduling choice (`messageSchedulingState*`, schema 1.8.67
    /// lines 5902/5905). Reset to `None` after each successful send.
    pub(super) scheduling: ComposerScheduling,
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
    /// Same-chat reply draft (tdesktop `FieldHeader::replyToMessage`).
    pub(super) pending_reply: Option<ComposerReplyTo>,
    /// Chat whose draft should be cleared after `updateMessageSendSucceeded`
    /// if the composer is still empty.
    pub(super) clear_draft_on_success: Option<ChatId>,
    /// Own-message edit (tdesktop `FieldHeader::editMessage`).
    pub(super) pending_edit: Option<ComposerEdit>,
    /// Normal composer draft stashed while editing (`DraftType::Normal`).
    pub(super) saved_edit_draft: String,
    /// Reply that belonged to that normal draft. Restored with the text on cancel.
    pub(super) saved_edit_reply: Option<ComposerReplyTo>,
    /// The composer's link dialog (Cmd/Ctrl+K on a selection).
    pub(super) link_dialog: Option<super::composer_shortcuts::ComposerLinkDialog>,
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
}

impl ComposerUi {
    pub(super) fn new(pending_attachments: Vec<ComposerAttachment>) -> Self {
        Self {
            command_menu_open: false,
            command_menu_selected: 0,
            mention_selected: 0,
            suggest: super::composer_suggest::SuggestUi::load(),
            inline_results_open: false,
            inline_results_selected: 0,
            inline_query_token: 0,
            inline_query_armed: None,
            pending_attachments,
            self_destruct: None,
            caption_above: false,
            silent: false,
            loud_chat: None,
            preview_disabled: false,
            preview_above: false,
            preview_media: PreviewMediaSize::Auto,
            preview_link: 0,
            edit_replace_as_file: false,
            preview_token: 0,
            prev_text: String::new(),
            markdown_revert: None,
            scheduling: ComposerScheduling::None,
            schedule_popup_open: false,
            schedule_picker: None,
            scheduled_dialog_open: false,
            scheduled_selected: Vec::new(),
            rich_editor_open: false,
            pending_reply: None,
            clear_draft_on_success: None,
            pending_edit: None,
            saved_edit_draft: String::new(),
            saved_edit_reply: None,
            link_dialog: None,
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
        }
    }
}
