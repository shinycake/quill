//! Composer send policy. IME composition must never send.

use crate::ids::{ChatId, MessageId, ViewGeneration};
use crate::local_path::{is_explicit_send_path, pick_send_path};
use crate::telegram::envelope::{BotCommand, MessageContent};
use crate::telegram::requests::SelfDestructSend;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnterEvent {
    /// True while an IME composition is marked (CJK/Hangul/etc.).
    pub composing: bool,
    /// Shift+Enter inserts a newline in chat-style inputs.
    pub shift: bool,
    /// Platform secondary modifier (Ctrl/Cmd).
    pub secondary: bool,
}

/// Which keystroke sends a chat message (parity:settings-enter-send,
/// parity:settings-ctrlenter-send). Telegram Desktop calls this
/// "Send with Enter".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SendKeyMode {
    /// Plain Enter sends; Shift+Enter inserts a newline.
    #[default]
    Enter,
    /// Plain Enter inserts a newline; Ctrl/Cmd+Enter sends.
    CtrlEnter,
}

/// Whether the keystroke described by `event` sends the message under
/// `mode`. IME composition never sends, in either mode.
pub fn should_send_on_enter(event: EnterEvent, mode: SendKeyMode) -> bool {
    if event.composing {
        return false;
    }
    match mode {
        SendKeyMode::Enter => !event.shift && !event.secondary,
        SendKeyMode::CtrlEnter => event.secondary && !event.shift,
    }
}

/// Text to actually send for a `PressEnter` that passed
/// `should_send_on_enter`. In CtrlEnter mode kit inserts the newline
/// before emitting the event (the composer runs with
/// `submit_on_enter(false)`), so strip that single trailing newline —
/// otherwise Ctrl+Enter sends a trailing blank line.
pub fn send_text_on_enter(text: String, mode: SendKeyMode) -> String {
    match mode {
        SendKeyMode::CtrlEnter => text.strip_suffix('\n').unwrap_or(&text).to_string(),
        SendKeyMode::Enter => text,
    }
}

/// Map Kit `InputEvent::PressEnter` plus the IME mark from
/// `EntityInputHandler::marked_text_range`.
///
/// gpui-base 0.6.1 `InputEvent::PressEnter { secondary, shift }` has **no**
/// composing field. `InputBaseState::enter` always emits `PressEnter` and does
/// not consult `ime_marked_range` (Escape does). Callers must read the mark.
pub fn enter_event_from_kit(
    shift: bool,
    secondary: bool,
    marked_text_range: Option<std::ops::Range<usize>>,
) -> EnterEvent {
    EnterEvent {
        composing: marked_text_range.is_some(),
        shift,
        secondary,
    }
}

/// How the user chose to send a local file.
/// Video is `inputMessageVideo`, not a document (tdesktop Photo/Video vs File).
/// A video note is `inputMessageVideoNote` and is not mixed into an album.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentKind {
    Photo,
    Document,
    Video,
    VideoNote,
}

/// A local file the user explicitly attached. Path is canonical at pick time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposerAttachment {
    pub path: PathBuf,
    pub kind: AttachmentKind,
    pub file_name: String,
}

impl ComposerAttachment {
    /// Validate `candidate` as a user-picked send path. Never call with paths
    /// taken from untrusted TDLib JSON.
    pub fn pick(candidate: &Path, kind: AttachmentKind) -> Option<Self> {
        let path = pick_send_path(candidate)?;
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();
        Some(Self {
            path,
            kind,
            file_name,
        })
    }

    /// Add a picked file. A document replaces the list (documents are not mixed
    /// into a photo/video album). Photos and videos accumulate up to 10.
    pub fn push_attachment(list: &mut Vec<Self>, next: Self) {
        match next.kind {
            AttachmentKind::Document | AttachmentKind::VideoNote => {
                list.clear();
                list.push(next);
            }
            AttachmentKind::Photo | AttachmentKind::Video => {
                if list
                    .iter()
                    .any(|att| !matches!(att.kind, AttachmentKind::Photo | AttachmentKind::Video))
                {
                    list.clear();
                }
                if list.len() >= crate::album::ALBUM_MAX_ITEMS {
                    return;
                }
                list.push(next);
            }
        }
    }

    /// Path string safe to embed in `inputFileLocal` (matches the pick).
    pub fn send_path_str(&self) -> Option<String> {
        if !is_explicit_send_path(&self.path, &self.path) {
            return None;
        }
        Some(self.path.to_string_lossy().into_owned())
    }
}

/// Message the composer is quoting (tdesktop `FieldHeader::replyToMessage`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposerReplyTo {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub preview: String,
    /// Slice G1: partial-message quote (`inputTextQuote`, schema 1.8.67
    /// line 3056) — `text` is a verbatim substring of the original
    /// message and `position` its UTF-16 code-unit offset.
    pub quote: Option<QuoteSelection>,
}

/// Slice G1: a quoted part of the replied-to message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuoteSelection {
    pub text: String,
    pub position: i32,
}

impl ComposerReplyTo {
    pub fn new(chat_id: ChatId, message_id: MessageId, preview: impl Into<String>) -> Self {
        Self {
            chat_id,
            message_id,
            preview: preview.into(),
            quote: None,
        }
    }

    /// Slice G1: reply carrying a validated partial quote.
    pub fn with_quote(
        chat_id: ChatId,
        message_id: MessageId,
        preview: impl Into<String>,
        quote: QuoteSelection,
    ) -> Self {
        Self {
            chat_id,
            message_id,
            preview: preview.into(),
            quote: Some(quote),
        }
    }

    /// Slice G1: draft/send-pipeline view of this reply — the replied-to
    /// message id plus the optional validated partial quote, mirroring
    /// `telegram::SendReply`. `None` when this reply targets another chat.
    pub fn send_reply(&self, chat_id: ChatId) -> Option<crate::telegram::SendReply> {
        (self.chat_id == chat_id).then(|| crate::telegram::SendReply {
            message_id: self.message_id,
            quote: self
                .quote
                .as_ref()
                .map(|quote| (quote.text.clone(), quote.position)),
        })
    }
}

/// Slice G1: find `quote` in `full_text` and return its UTF-16 code-unit
/// offset, as `inputTextQuote.position` requires (schema 1.8.67, line
/// 3056). `None` when the quote is empty or not a verbatim substring —
/// the quote dialog only submits validated quotes, so this is a guard,
/// not the validation itself.
pub fn quote_position(full_text: &str, quote: &str) -> Option<i32> {
    if quote.is_empty() {
        return None;
    }
    let byte_offset = full_text.find(quote)?;
    Some(full_text[..byte_offset].encode_utf16().count() as i32)
}

/// tdesktop `FieldHeader` Escape / `replyCancelled`: drop the reply header
/// and keep the typed field. `ComposeControls::clear` wipes text on send,
/// not on cancel.
pub fn cancel_reply_draft(
    reply: Option<ComposerReplyTo>,
    text: String,
) -> (Option<ComposerReplyTo>, String) {
    let _ = reply;
    (None, text)
}

/// Which TDLib edit constructor an own-message edit uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposerEditKind {
    /// `editMessageText` + `inputMessageText`.
    Text,
    /// `editMessageCaption` for outgoing photo/document captions.
    Caption,
}

/// Composer edit mode (tdesktop `FieldHeader::editMessage` / `_editMsgId`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposerEdit {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub original_text: String,
    pub kind: ComposerEditKind,
    /// M1: the target is a scheduled send (`session.scheduled_messages`),
    /// not a history message. `edit_snapshot` validates against the
    /// scheduled list in that case.
    pub scheduled: bool,
    /// MED4: caption position for caption edits
    /// (`editMessageCaption.show_caption_above_media`, schema:12338).
    pub caption_above: bool,
}

impl ComposerEdit {
    /// Own outgoing, already-sent messages only. Incoming / pending / unsupported
    /// stay out of this slice (schema `messageProperties.can_be_edited` is the
    /// live gate; official clients call `getMessageProperties`).
    pub fn from_own_content(
        chat_id: ChatId,
        message_id: MessageId,
        is_outgoing: bool,
        pending: bool,
        content: &MessageContent,
    ) -> Option<Self> {
        if !is_outgoing || pending {
            return None;
        }
        let (kind, original_text) = match content {
            MessageContent::Text(text) => (ComposerEditKind::Text, text.text.clone()),
            MessageContent::Photo(photo) => (ComposerEditKind::Caption, photo.caption.clone()),
            MessageContent::Document(doc) => (ComposerEditKind::Caption, doc.caption.clone()),
            MessageContent::VoiceNote(note) => (ComposerEditKind::Caption, note.caption.clone()),
            MessageContent::Animation(animation) => {
                (ComposerEditKind::Caption, animation.caption.clone())
            }
            MessageContent::Video(video) => (ComposerEditKind::Caption, video.caption.clone()),
            MessageContent::Audio(audio) => (ComposerEditKind::Caption, audio.caption.clone()),
            MessageContent::VideoNote(_)
            | MessageContent::Sticker(_)
            | MessageContent::Poll(_)
            | MessageContent::Location(_)
            | MessageContent::Venue(_)
            | MessageContent::Contact(_)
            | MessageContent::Dice(_)
            // Phase B4: timer-change service rows are not editable.
            | MessageContent::ChatTtlChanged { .. }
            // Phase C2f: group-call invitations are not editable.
            | MessageContent::GroupCallInvitation { .. }
            // Phase C2i: call entries are not editable.
            | MessageContent::Call { .. }
            // Phase S1: screenshot-taken service rows are not editable.
            | MessageContent::ScreenshotTaken
            // Slice C2k: community service rows are not editable.
            | MessageContent::ChatAddedToCommunity { .. }
            | MessageContent::ChatRemovedFromCommunity
            // Slice G9: community service rows are not editable.
            | MessageContent::ChatJoinFromCommunity { .. }
            // M2: rich messages are edited in the rich editor, not here.
            | MessageContent::RichMessage(_)
            // B1: games are not editable.
            | MessageContent::Game(_)
            // Slice P1: invoices and payment notices are not editable.
            | MessageContent::Invoice(_)
            | MessageContent::PaymentSuccessful(_)
            | MessageContent::PaymentReceived(_)
            | MessageContent::Unsupported { .. } => return None,
        };
        Some(Self {
            chat_id,
            message_id,
            original_text,
            kind,
            scheduled: false,
            // MED4: preserve the message's caption position on edit
            // (`editMessageCaption.show_caption_above_media`,
            // schema:12338). The composer toggle can flip it.
            caption_above: match content {
                MessageContent::Photo(photo) => photo.show_caption_above_media,
                MessageContent::Video(video) => video.show_caption_above_media,
                MessageContent::Animation(animation) => animation.show_caption_above_media,
                _ => false,
            },
        })
    }
}

/// Enter edit: stash the current field as the normal draft (tdesktop
/// `DraftType::Normal`) and load the message text into the field.
pub fn begin_edit_draft(
    current_text: String,
    edit: ComposerEdit,
) -> (ComposerEdit, String, String) {
    let field = edit.original_text.clone();
    (edit, field, current_text)
}

/// tdesktop `ComposeControls::cancelEditMessage`: clear the edit header,
/// then `applyDraft()` restores the **normal** draft — not the typed edit.
pub fn cancel_edit_draft(
    edit: Option<ComposerEdit>,
    saved_draft: String,
) -> (Option<ComposerEdit>, String) {
    let _ = edit;
    (None, saved_draft)
}

/// Enter edit without dropping the normal draft's reply. Flush `saved_reply`
/// with the stashed text first; cancel/finish restores both.
pub fn begin_edit_keeping_reply(
    current_text: String,
    edit: ComposerEdit,
    reply: Option<ComposerReplyTo>,
) -> (ComposerEdit, String, String, Option<ComposerReplyTo>) {
    let (edit, field, saved) = begin_edit_draft(current_text, edit);
    (edit, field, saved, reply)
}

/// Cancel edit: normal draft text and its reply come back together.
pub fn cancel_edit_keeping_reply(
    saved_draft: String,
    saved_reply: Option<ComposerReplyTo>,
) -> (String, Option<ComposerReplyTo>) {
    (saved_draft, saved_reply)
}

/// Pending delete confirm (tdesktop `DeleteMessagesBox` / Unigram
/// `DeleteMessagesPopup`). `revoke` maps to `deleteMessages.revoke`
/// (schema 1.8.67 line 12282): true deletes for everyone, false only for
/// the current user. The revoke toggle is only offered for own outgoing
/// messages (`can_revoke`); incoming deletes are always for-me.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteConfirm {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub revoke: bool,
    pub can_revoke: bool,
}

impl DeleteConfirm {
    pub fn own(
        chat_id: ChatId,
        message_id: MessageId,
        is_outgoing: bool,
        pending: bool,
    ) -> Option<Self> {
        if is_outgoing && !pending {
            Some(Self {
                chat_id,
                message_id,
                revoke: true,
                can_revoke: true,
            })
        } else {
            None
        }
    }

    /// M1: any already-sent message (incoming included) can be deleted
    /// for the current user (`revoke: false`); the for-everyone toggle
    /// stays hidden.
    pub fn for_message(
        chat_id: ChatId,
        message_id: MessageId,
        is_outgoing: bool,
        pending: bool,
    ) -> Option<Self> {
        if pending || message_id.0 <= 0 {
            return None;
        }
        Some(Self {
            chat_id,
            message_id,
            revoke: is_outgoing,
            can_revoke: is_outgoing,
        })
    }
}

/// Messages queued for `forwardMessages` (tdesktop `Data::ForwardDraft` /
/// `ShowForwardMessagesBox`). Ids stay strictly increasing (schema).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardDraft {
    pub from_chat_id: ChatId,
    pub message_ids: Vec<MessageId>,
    /// M1: `forwardMessages.send_copy` — drop the "Forwarded from"
    /// attribution (TGX "Hide sender name"; schema 1.8.67 line 12237).
    pub send_copy: bool,
    /// M1: `forwardMessages.remove_caption` — strip captions on the copies
    /// (ignored by TDLib unless `send_copy` is true).
    pub remove_caption: bool,
}

impl ForwardDraft {
    /// Already-sent history only. Pending / local ids cannot be forwarded.
    pub fn from_message(chat_id: ChatId, message_id: MessageId, pending: bool) -> Option<Self> {
        if pending || message_id.0 <= 0 {
            return None;
        }
        Some(Self {
            from_chat_id: chat_id,
            message_ids: vec![message_id],
            send_copy: false,
            remove_caption: false,
        })
    }

    pub fn contains(&self, message_id: MessageId) -> bool {
        self.message_ids.contains(&message_id)
    }

    pub fn is_empty(&self) -> bool {
        self.message_ids.is_empty()
    }

    /// Toggle a same-chat id. Cross-chat selection is not official history
    /// multi-select — start a new draft instead.
    pub fn toggle(&mut self, chat_id: ChatId, message_id: MessageId, pending: bool) {
        if pending || message_id.0 <= 0 {
            return;
        }
        if chat_id != self.from_chat_id {
            self.from_chat_id = chat_id;
            self.message_ids = vec![message_id];
            return;
        }
        if let Some(index) = self.message_ids.iter().position(|id| *id == message_id) {
            self.message_ids.remove(index);
        } else {
            self.message_ids.push(message_id);
            self.message_ids.sort_by_key(|id| id.0);
        }
    }

    pub fn count(&self) -> usize {
        self.message_ids.len()
    }
}

/// tdesktop ShareBox / `ShowForwardMessagesBox` Escape: leave the picker
/// (and optional selection) without sending.
pub fn cancel_forward_draft(draft: Option<ForwardDraft>) -> (Option<ForwardDraft>, bool) {
    let _ = draft;
    (None, false)
}

/// M1: `messageSendOptions` choices for a send (TDLib 1.8.67,
/// `schema/td_api.tl:5934` —
/// `messageSendOptions suggested_post_info disable_notification
/// from_background protect_content allow_paid_broadcast
/// paid_message_star_count update_order_of_installed_sticker_sets
/// scheduling_state effect_id sending_id only_preview`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SendOptions {
    /// `disable_notification` — silent send.
    pub disable_notification: bool,
    pub scheduling: ComposerScheduling,
    /// `linkPreviewOptions.is_disabled` — the composer preview toggle.
    /// Secret chats force this on the driver side regardless.
    pub link_preview_disabled: bool,
    /// MED4b: `linkPreviewOptions.show_above_text` (TDLib 1.8.67,
    /// `schema/td_api.tl:2236`) — preview above the message text instead
    /// of below. Ignored in secret chats.
    pub link_preview_above_text: bool,
    /// MED4b: `linkPreviewOptions.force_small_media` /
    /// `force_large_media` (TDLib 1.8.67, `schema/td_api.tl:2234-2235`) —
    /// TGX's large/small toggle cycles these. Ignored in secret chats or
    /// when the URL isn't explicitly specified (the send path sets `url`
    /// whenever this isn't `Auto`).
    pub link_preview_media: PreviewMediaSize,
    /// M1 fix-up: the driver sets this when the target chat is a secret
    /// chat. `textEntityTypeBlockQuote` is not supported in secret chats
    /// (schema 1.8.67), so `send_text` strips blockquote entities instead
    /// of letting TDLib drop them.
    pub is_secret: bool,
    /// S15: `messageSendOptions.update_order_of_installed_sticker_sets`
    /// (TDLib 1.8.67, `schema/td_api.tl:5934`) — pass true when the user
    /// explicitly chose a sticker from an installed set so TDLib moves
    /// that set to the front of the installed order.
    pub update_order_of_installed_sticker_sets: bool,
}

/// MED4b: media-size half of `linkPreviewOptions` (TDLib 1.8.67,
/// `schema/td_api.tl:2234-2235`). Two bools on the wire but at most one
/// may be set — the enum makes the invalid both-true state
/// unrepresentable. TGX (`MessagesController.onRequestToggleLargeMedia`
/// → `LinkPreview.toggleLargeMedia`) flips these relative to the
/// preview's current effective size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreviewMediaSize {
    /// No force flags — TDLib uses the preview's own default.
    #[default]
    Auto,
    /// `force_small_media: true`.
    ForceSmall,
    /// `force_large_media: true`.
    ForceLarge,
}

impl PreviewMediaSize {
    /// TGX toggle semantics: flip relative to the currently effective
    /// size; never returns to `Auto`.
    pub fn toggle(self, effective_large: bool) -> Self {
        if effective_large {
            Self::ForceSmall
        } else {
            Self::ForceLarge
        }
    }

    /// Currently effective size given the preview's server default.
    pub fn effective_large(self, preview_show_large_media: bool) -> bool {
        match self {
            Self::Auto => preview_show_large_media,
            Self::ForceSmall => false,
            Self::ForceLarge => true,
        }
    }
}

/// M1: `MessageSchedulingState` for a send (TDLib 1.8.67,
/// `schema/td_api.tl:5902` `messageSchedulingStateSendAtDate
/// send_date repeat_period` / `:5905` `messageSchedulingStateSendWhenOnline`).
/// `repeat_period` is always 0 (premium-only, never surfaced).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComposerScheduling {
    #[default]
    None,
    /// Unix timestamp of the scheduled send.
    SendAtDate(i64),
    SendWhenOnline,
}

/// M1: composer text formatting. The composer stays plain text; formatting
/// is authored as lightweight markup (Telegram X `InputView` format menu /
/// tdesktop markdown behavior) and converted to TDLib `textEntities` on
/// the send path by `parse_format_markup`. Paired delimiters only;
/// unmatched delimiters stay literal; no nesting (documented, keeps the
/// parser a single pass):
/// `**bold**` `*italic*` (or `_italic_`) `__underline__` `~~strike~~`
/// `` `code` `` `||spoiler||` `[label](url)`; fenced ` ```lang? ` blocks;
/// `> ` line prefix for quotes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatKind {
    Bold,
    Italic,
    Underline,
    Strikethrough,
    Code,
    Pre,
    Spoiler,
    BlockQuote,
    TextUrl,
}

/// M1: one parsed entity. Offsets are UTF-16 code units — the units TDLib
/// `textEntity` uses (`src/text.rs` maps them back for rendering).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposerEntity {
    pub offset: i32,
    pub length: i32,
    pub kind: FormatKind,
    /// `TextUrl` target.
    pub url: String,
    /// `Pre` language; empty renders as plain `textEntityTypePre`.
    pub language: String,
}

/// M1: a formatting action the toolbar / shortcut applies to the composer
/// selection (byte range; empty = cursor).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatAction {
    Bold,
    Italic,
    Underline,
    Strikethrough,
    Code,
    Pre,
    Spoiler,
    BlockQuote,
    Link(String),
}

impl FormatAction {
    fn open_marker(&self) -> &'static str {
        match self {
            FormatAction::Bold => "**",
            FormatAction::Italic => "*",
            FormatAction::Underline => "__",
            FormatAction::Strikethrough => "~~",
            FormatAction::Code => "`",
            FormatAction::Pre => "```\n",
            FormatAction::Spoiler => "||",
            FormatAction::BlockQuote => "> ",
            FormatAction::Link(_) => "[",
        }
    }

    fn close_marker(&self) -> &'static str {
        match self {
            FormatAction::Bold => "**",
            FormatAction::Italic => "*",
            FormatAction::Underline => "__",
            FormatAction::Strikethrough => "~~",
            FormatAction::Code => "`",
            FormatAction::Pre => "\n```",
            FormatAction::Spoiler => "||",
            FormatAction::BlockQuote => "",
            FormatAction::Link(_) => "]()",
        }
    }
}

/// M1: parse composer markup into clean text + entities. Returns the text
/// with markers stripped and entities with UTF-16 offsets into it.
pub fn parse_format_markup(text: &str) -> (String, Vec<ComposerEntity>) {
    let mut parser = MarkupParser {
        text,
        out: String::with_capacity(text.len()),
        out16: 0,
        entities: Vec::new(),
    };
    parser.parse_top();
    (parser.out, parser.entities)
}

/// M1: apply a formatting action to `range` (UTF-8 byte range; snapped to
/// char boundaries). Returns the new text and the new selection: the
/// wrapped region for a selection, the cursor between markers when empty.
pub fn apply_format_markup(
    text: &str,
    range: std::ops::Range<usize>,
    action: &FormatAction,
) -> (String, std::ops::Range<usize>) {
    let (start, end) = snap_range(text, range);
    if *action == FormatAction::BlockQuote {
        return apply_block_quote(text, start, end);
    }
    let open = action.open_marker();
    let close = action.close_marker();
    if start == end {
        // Empty selection: insert the marker pair, cursor between them.
        // Link inserts `[` + `](url)` and lands the cursor in the URL slot.
        let (insert, cursor_off) = match action {
            FormatAction::Link(url) if url.is_empty() => ("[]()".to_string(), 3),
            FormatAction::Link(url) => (format!("[]({url})"), 3 + url.len()),
            _ => (format!("{open}{close}"), open.len()),
        };
        let mut new_text = String::with_capacity(text.len() + insert.len());
        new_text.push_str(&text[..start]);
        new_text.push_str(&insert);
        new_text.push_str(&text[start..]);
        let cursor = start + cursor_off;
        (new_text, cursor..cursor)
    } else {
        let selected = &text[start..end];
        let wrapped = match action {
            FormatAction::Link(url) => format!("[{selected}]({url})"),
            _ => format!("{open}{selected}{close}"),
        };
        let mut new_text = String::with_capacity(text.len() + wrapped.len());
        new_text.push_str(&text[..start]);
        new_text.push_str(&wrapped);
        new_text.push_str(&text[end..]);
        let new_start = start + open.len();
        let new_end = new_start + selected.len();
        (new_text, new_start..new_end)
    }
}

/// M1: strip all markup markers in `range` (whole text when empty),
/// keeping the inner text. `[label](url)` collapses to `label`.
pub fn clear_format_markup(text: &str, range: std::ops::Range<usize>) -> String {
    let (start, end) = snap_range(text, range);
    let (start, end) = if start == end {
        (0, text.len())
    } else {
        (start, end)
    };
    let mut result = String::with_capacity(text.len());
    result.push_str(&text[..start]);
    result.push_str(&strip_markup(&text[start..end]));
    result.push_str(&text[end..]);
    result
}

fn snap_range(text: &str, range: std::ops::Range<usize>) -> (usize, usize) {
    let len = text.len();
    let mut start = range.start.min(len);
    let mut end = range.end.min(len);
    while start > 0 && !text.is_char_boundary(start) {
        start -= 1;
    }
    while end < len && !text.is_char_boundary(end) {
        end += 1;
    }
    if start > end {
        std::mem::swap(&mut start, &mut end);
    }
    (start, end)
}

/// M1: `> ` prefix on every line intersecting the range.
fn apply_block_quote(text: &str, start: usize, end: usize) -> (String, std::ops::Range<usize>) {
    let line_start = text[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = text[end..]
        .find('\n')
        .map(|i| end + i)
        .unwrap_or(text.len());
    let mut new_text = String::with_capacity(text.len() + 8);
    new_text.push_str(&text[..line_start]);
    let mut added_total = 0;
    for line in text[line_start..line_end].split_inclusive('\n') {
        new_text.push_str("> ");
        added_total += 2;
        new_text.push_str(line);
    }
    new_text.push_str(&text[line_end..]);
    // Select the quoted lines (predictable; the user can keep typing).
    (new_text, line_start..line_end + added_total)
}

struct MarkupParser<'a> {
    text: &'a str,
    out: String,
    /// UTF-16 code-unit length of `out`.
    out16: i32,
    entities: Vec<ComposerEntity>,
}

impl<'a> MarkupParser<'a> {
    fn parse_top(&mut self) {
        let bytes = self.text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let line_start = i == 0 || bytes[i - 1] == b'\n';
            if line_start && self.text[i..].starts_with("> ") {
                let content_start = i + 2;
                let line_end = self.text[content_start..]
                    .find('\n')
                    .map(|k| content_start + k)
                    .unwrap_or(self.text.len());
                let inner = &self.text[content_start..line_end];
                let entity_start = self.out16;
                self.push_str(inner);
                let entity_len = self.out16 - entity_start;
                if entity_len > 0 {
                    self.entities.push(ComposerEntity {
                        offset: entity_start,
                        length: entity_len,
                        kind: FormatKind::BlockQuote,
                        url: String::new(),
                        language: String::new(),
                    });
                }
                i = line_end;
                continue;
            }
            if let Some(consumed) = self.try_inline(i) {
                i = consumed;
                continue;
            }
            if let Some(consumed) = self.try_link(i) {
                i = consumed;
                continue;
            }
            let ch = self.text[i..].chars().next().expect("char boundary");
            self.push_char(ch);
            i += ch.len_utf8();
        }
    }

    /// Try a paired delimiter at byte offset `i`. Returns the offset just
    /// past the closing delimiter on success. Check order matters: longer
    /// delimiters first (`**` before `*`, `__` before `_`, ` ``` ` before
    /// `` ` ``).
    fn try_inline(&mut self, i: usize) -> Option<usize> {
        let rest = &self.text[i..];
        // Fenced code block first (may span lines, optional language).
        if let Some(after) = rest.strip_prefix("```") {
            return self.try_fenced(i, after);
        }
        for (open, kind) in [
            ("**", FormatKind::Bold),
            ("__", FormatKind::Underline),
            ("~~", FormatKind::Strikethrough),
            ("||", FormatKind::Spoiler),
            ("`", FormatKind::Code),
            ("*", FormatKind::Italic),
            ("_", FormatKind::Italic),
        ] {
            if let Some(after_open) = rest.strip_prefix(open) {
                // A lone `*`/`_` that continues a `**`/`__` pair never opens
                // italic: an unmatched `**bold` must not re-parse the second
                // `*` as an italic opener (unmatched delimiters stay literal).
                if open.len() == 1 && i > 0 && self.text.as_bytes()[i - 1] == open.as_bytes()[0] {
                    continue;
                }
                // Inline spans stay on one line (Telegram entities do).
                let line_end = after_open.find('\n').unwrap_or(after_open.len());
                let searchable = &after_open[..line_end];
                if let Some(close_rel) = searchable.find(open) {
                    let inner = &searchable[..close_rel];
                    if !inner.is_empty() {
                        let entity_start = self.out16;
                        self.push_str(inner);
                        self.entities.push(ComposerEntity {
                            offset: entity_start,
                            length: self.out16 - entity_start,
                            kind,
                            url: String::new(),
                            language: String::new(),
                        });
                        return Some(i + open.len() + close_rel + open.len());
                    }
                }
                return None;
            }
        }
        None
    }

    fn try_fenced(&mut self, i: usize, after: &str) -> Option<usize> {
        // Language = first line after the fence (may be empty).
        let first_nl = after.find('\n')?;
        let language = after[..first_nl].trim().to_string();
        let body_start = first_nl + 1;
        let close_rel = after[body_start..].find("```")?;
        let inner = &after[body_start..body_start + close_rel];
        // A fenced block with only whitespace is not formatting.
        if inner.trim().is_empty() {
            return None;
        }
        // The newline before the closing fence is block syntax, not content.
        let inner = inner
            .strip_suffix("\r\n")
            .or_else(|| inner.strip_suffix('\n'))
            .unwrap_or(inner);
        let entity_start = self.out16;
        self.push_str(inner);
        self.entities.push(ComposerEntity {
            offset: entity_start,
            length: self.out16 - entity_start,
            kind: FormatKind::Pre,
            url: String::new(),
            language,
        });
        Some(i + 3 + body_start + close_rel + 3)
    }

    /// `[label](url)` — label stays plain (no nesting).
    fn try_link(&mut self, i: usize) -> Option<usize> {
        let rest = &self.text[i..];
        let after_bracket = rest.strip_prefix('[')?;
        let close_bracket = after_bracket.find("](")?;
        let after_paren = &after_bracket[close_bracket + 2..];
        // URL stays on one line.
        let line_end = after_paren.find('\n').unwrap_or(after_paren.len());
        let close_paren = after_paren[..line_end].find(')')?;
        let label = &after_bracket[..close_bracket];
        let url = &after_paren[..close_paren];
        if label.is_empty() || url.is_empty() {
            return None;
        }
        let entity_start = self.out16;
        self.push_str(label);
        self.entities.push(ComposerEntity {
            offset: entity_start,
            length: self.out16 - entity_start,
            kind: FormatKind::TextUrl,
            url: url.to_string(),
            language: String::new(),
        });
        Some(i + 1 + close_bracket + 2 + close_paren + 1)
    }

    fn push_str(&mut self, s: &str) {
        for ch in s.chars() {
            self.push_char(ch);
        }
    }

    fn push_char(&mut self, ch: char) {
        self.out.push(ch);
        self.out16 += ch.len_utf16() as i32;
    }
}

/// M1: strip markup without producing entities (clear-formatting).
fn strip_markup(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let bytes = text.as_bytes();
    while i < bytes.len() {
        let line_start = i == 0 || bytes[i - 1] == b'\n';
        if line_start && text[i..].starts_with("> ") {
            i += 2;
            continue;
        }
        let rest = &text[i..];
        if let Some(after) = rest.strip_prefix("```")
            && let Some(first_nl) = after.find('\n')
        {
            let body_start = first_nl + 1;
            if let Some(close_rel) = after[body_start..].find("```") {
                out.push_str(&strip_markup(&after[body_start..body_start + close_rel]));
                i += 3 + body_start + close_rel + 3;
                continue;
            }
        }
        let mut stripped = false;
        for open in ["**", "__", "~~", "||", "`", "*", "_"] {
            if let Some(after_open) = rest.strip_prefix(open) {
                let line_end = after_open.find('\n').unwrap_or(after_open.len());
                if let Some(close_rel) = after_open[..line_end].find(open) {
                    out.push_str(&strip_markup(&after_open[..close_rel]));
                    i += open.len() + close_rel + open.len();
                    stripped = true;
                    break;
                }
            }
        }
        if stripped {
            continue;
        }
        // `[label](url)` → `label`.
        if let Some(after_bracket) = rest.strip_prefix('[')
            && let Some(close_bracket) = after_bracket.find("](")
        {
            let after_paren = &after_bracket[close_bracket + 2..];
            let line_end = after_paren.find('\n').unwrap_or(after_paren.len());
            if let Some(close_paren) = after_paren[..line_end].find(')') {
                out.push_str(&strip_markup(&after_bracket[..close_bracket]));
                i += 1 + close_bracket + 2 + close_paren + 1;
                continue;
            }
        }
        let ch = text[i..].chars().next().expect("char boundary");
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// tdesktop `ComposeControls::kSaveDraftTimeout` — quiet period before a local draft write.
pub const DRAFT_SAVE_TIMEOUT_MS: u64 = 1_000;
/// tdesktop `kSaveDraftAnywayTimeout` — keep resetting the 1s timer only inside this window.
pub const DRAFT_SAVE_ANYWAY_MS: u64 = 5_000;

/// Clock for tdesktop `saveDraft(delayed)`: first edit arms 1s; further edits
/// reset that 1s until 5s from the first edit, then the write happens immediately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DraftSaveClock {
    pub started_ms: Option<u64>,
}

impl DraftSaveClock {
    pub fn idle() -> Self {
        Self { started_ms: None }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftSaveStep {
    /// Arm or reset the quiet timer. `delay_ms` is `kSaveDraftTimeout`.
    Wait { delay_ms: u64, started_ms: u64 },
    /// `writeDrafts` — quiet window elapsed, the 5s cap fired, or the caller forced a flush.
    Write,
}

/// `delayed == false` is Unigram `SaveDraft(force: true)` on leaving the chat
/// (and tdesktop's non-delayed `saveDraft`). `delayed == true` is tdesktop's
/// field-changed path.
pub fn schedule_draft_save(clock: DraftSaveClock, now_ms: u64, delayed: bool) -> DraftSaveStep {
    if !delayed {
        return DraftSaveStep::Write;
    }
    match clock.started_ms {
        None => DraftSaveStep::Wait {
            delay_ms: DRAFT_SAVE_TIMEOUT_MS,
            started_ms: now_ms,
        },
        Some(started) if now_ms.saturating_sub(started) < DRAFT_SAVE_ANYWAY_MS => {
            DraftSaveStep::Wait {
                delay_ms: DRAFT_SAVE_TIMEOUT_MS,
                started_ms: started,
            }
        }
        Some(_) => DraftSaveStep::Write,
    }
}

/// Text TDLib should store. Unigram saves when the field is non-whitespace or a reply is set.
pub fn draft_text_to_store(text: &str, has_reply: bool) -> Option<&str> {
    if text.trim().is_empty() && !has_reply {
        None
    } else {
        Some(text)
    }
}

/// Phase 3.1: composer text after tapping a bot command. Empty field → the
/// bare `/command`; otherwise appended after a space (or directly when the
/// field already ends in whitespace).
pub fn insert_bot_command_text(current: &str, command: &str) -> String {
    let insertion = format!("/{command}");
    if current.trim().is_empty() {
        insertion
    } else if current.ends_with(char::is_whitespace) {
        format!("{current}{insertion}")
    } else {
        format!("{current} {insertion}")
    }
}

/// Phase 3.3: one row in the composer `/` command menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandMenuItem {
    /// Command name without the leading `/`.
    pub command: String,
    pub description: String,
    /// True when the row comes from `getCommands` (global/default scope)
    /// rather than the bot's `botInfo`; rendered in the "Global" section
    /// below the bot-specific commands.
    pub global: bool,
    /// True when the command is ephemeral (schema 1.8.67 `botCommand`,
    /// line 826) — the row shows the ephemeral icon; the result is only
    /// visible to the sender.
    pub is_ephemeral: bool,
}

/// Phase 3.3: `/` command-menu trigger. Returns the filter prefix typed
/// after the `/` when the composer text ends with a `/`-led token at a
/// word boundary — start of text or right after whitespace — e.g.
/// `Some("")` for a bare `/`, `Some("st")` for `/st`. Returns `None` for
/// mid-word slashes (`a/b`, `http://…`), so the menu never opens inside
/// words or URLs. The trailing-token convention matches the input's lack
/// of an exposed cursor offset (documented in DECISIONS).
pub fn command_menu_trigger(text: &str) -> Option<&str> {
    let token = text.split(char::is_whitespace).next_back()?;
    let prefix = token.strip_prefix('/')?;
    // `/` alone or `/` + command chars; a second `/` (`/a/b`, `//`) is not
    // a command token.
    if prefix.contains('/') {
        return None;
    }
    Some(prefix)
}

/// Phase 3.3: composer text with the trailing `/`-token removed, for menu
/// picks. The menu replaces the partial token the user typed, so `/st` +
/// pick `start` → `/start`, not `/st /start`. Returns `None` when there is
/// no trigger token (the caller should not pick).
pub fn strip_command_menu_trigger(text: &str) -> Option<&str> {
    let prefix = command_menu_trigger(text)?;
    let token_len = prefix.len() + 1; // leading `/`
    text.get(..text.len() - token_len)
}

/// Bots slice: `@botname query` inline-mode trigger. Returns
/// `(username, query)` when the composer text starts with an `@`-led
/// token — TGX `InlineSearchContext` only runs inline lookup from the
/// message-composer start (`startIndex == 0`), never mid-text or in
/// captions. `@bot` → `("bot", "")`; `@bot cats` → `("bot", "cats")`.
/// `None` for a non-leading `@` (`hi @bot`), a bare `@`, or an empty
/// token. Usernames are ASCII alphanumeric + underscore; the query is
/// everything after the token's first whitespace run.
pub fn inline_query_trigger(text: &str) -> Option<(&str, &str)> {
    let rest = text.strip_prefix('@')?;
    let username_len = rest
        .char_indices()
        .take_while(|(_, c)| c.is_ascii_alphanumeric() || *c == '_')
        .map(|(i, c)| i + c.len_utf8())
        .last()?;
    let query = rest[username_len..].trim_start();
    Some((&rest[..username_len], query))
}

/// Phase 3.3: merge `botInfo.commands` (bot-specific) with `getCommands`
/// results (global/default scope) into menu rows. Bot-specific rows come
/// first; global rows follow, skipping command names already listed, so a
/// command defined in both scopes appears once (the bot-specific
/// description wins).
pub fn merge_command_menu_items(
    specific: &[BotCommand],
    global: &[BotCommand],
) -> Vec<CommandMenuItem> {
    let mut items: Vec<CommandMenuItem> = specific
        .iter()
        .map(|command| CommandMenuItem {
            command: command.command.clone(),
            description: command.description.clone(),
            global: false,
            is_ephemeral: command.is_ephemeral,
        })
        .collect();
    for command in global {
        if items.iter().any(|item| item.command == command.command) {
            continue;
        }
        items.push(CommandMenuItem {
            command: command.command.clone(),
            description: command.description.clone(),
            global: true,
            is_ephemeral: command.is_ephemeral,
        });
    }
    items
}

/// Phase 3.3: filter menu rows by the typed prefix (case-insensitive; bot
/// commands are lowercase `a-z0-9_` but users may type capitals). An empty
/// prefix matches everything.
pub fn filter_command_menu_items<'a>(
    items: &'a [CommandMenuItem],
    prefix: &str,
) -> Vec<&'a CommandMenuItem> {
    let prefix = prefix.to_lowercase();
    items
        .iter()
        .filter(|item| item.command.to_lowercase().starts_with(&prefix))
        .collect()
}

/// Phase 3.2: composer text after tapping a `switchInline` keyboard button.
/// The button's query is inserted — bare when the field is empty, otherwise
/// appended after a space (or directly when the field already ends in
/// whitespace). `targetChatChosen` / `targetChatInternalLink` (no chat
/// picker in this slice) use the current chat, same as `targetChatCurrent`.
pub fn insert_switch_inline_text(current: &str, query: &str) -> String {
    if current.trim().is_empty() {
        query.to_string()
    } else if current.ends_with(char::is_whitespace) {
        format!("{current}{query}")
    } else {
        format!("{current} {query}")
    }
}

/// Snapshot of a send attempt: destination is frozen at submit time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposerSnapshot {
    pub chat_id: i64,
    pub view_generation: u64,
    pub text: String,
    pub attachment: Option<ComposerAttachment>,
    /// 2–10 photos and/or videos. Empty when `attachment` is the single send.
    pub album: Vec<ComposerAttachment>,
    pub reply_to: Option<ComposerReplyTo>,
    /// Phase B3: self-destruct choice for photo/video sends
    /// (`inputMessagePhoto`/`inputMessageVideo` `self_destruct_type`,
    /// TDLib 1.8.67 lines 6117/6128 — "private chats only"). Set from the
    /// composer's timer picker; the driver strips it for non-private chats.
    pub self_destruct: Option<SelfDestructSend>,
    /// M1: silent / scheduled / when-online / link-preview send options.
    pub send_options: SendOptions,
    /// MED4: `show_caption_above_media` for photo/video sends (TDLib
    /// 1.8.67, `schema/td_api.tl:6117/6128`). Toggled from the composer;
    /// the driver also enforces it on album items (schema: all album
    /// contents must share the same value).
    pub caption_above_media: bool,
}

impl ComposerSnapshot {
    /// Freeze destination + text at submit time (chat switches must not redirect).
    pub fn capture(
        chat_id: ChatId,
        view_generation: ViewGeneration,
        text: impl Into<String>,
    ) -> Self {
        Self::capture_with_attachment(chat_id, view_generation, text, None)
    }

    pub fn capture_with_attachment(
        chat_id: ChatId,
        view_generation: ViewGeneration,
        text: impl Into<String>,
        attachment: Option<ComposerAttachment>,
    ) -> Self {
        Self {
            chat_id: chat_id.0,
            view_generation: view_generation.0,
            text: text.into(),
            attachment,
            album: Vec::new(),
            reply_to: None,
            self_destruct: None,
            send_options: SendOptions::default(),
            caption_above_media: false,
        }
    }

    /// Freeze a 2–10 photo/video album. Caption is the composer text.
    pub fn capture_album(
        chat_id: ChatId,
        view_generation: ViewGeneration,
        text: impl Into<String>,
        album: Vec<ComposerAttachment>,
    ) -> Self {
        Self {
            chat_id: chat_id.0,
            view_generation: view_generation.0,
            text: text.into(),
            attachment: None,
            album,
            reply_to: None,
            self_destruct: None,
            send_options: SendOptions::default(),
            caption_above_media: false,
        }
    }

    pub fn with_reply(mut self, reply_to: Option<ComposerReplyTo>) -> Self {
        self.reply_to = reply_to;
        self
    }

    /// Phase B3: attach the composer's self-destruct choice (photo/video
    /// sends only; the driver enforces the private-chat gate).
    pub fn with_self_destruct(mut self, choice: Option<SelfDestructSend>) -> Self {
        self.self_destruct = choice;
        self
    }

    /// M1: attach the composer's send options (silent / scheduled /
    /// when-online / link-preview toggle).
    pub fn with_send_options(mut self, options: SendOptions) -> Self {
        self.send_options = options;
        self
    }

    /// MED4: caption-above-media toggle state from the composer.
    pub fn with_caption_above_media(mut self, above: bool) -> Self {
        self.caption_above_media = above;
        self
    }

    /// Same-chat `inputMessageReplyToMessage.message_id` only. Cross-chat
    /// `inputMessageReplyToExternalMessage` is out of this slice.
    pub fn send_reply_to(&self) -> Option<MessageId> {
        self.reply_to.as_ref().and_then(|reply| {
            if reply.chat_id.0 == self.chat_id {
                Some(reply.message_id)
            } else {
                None
            }
        })
    }

    /// Slice G1: the full reply (message id plus validated partial
    /// quote) for the send builders.
    pub fn send_reply(&self) -> Option<crate::telegram::SendReply> {
        self.reply_to.as_ref().and_then(|reply| {
            if reply.chat_id.0 == self.chat_id {
                Some(crate::telegram::SendReply {
                    message_id: reply.message_id,
                    quote: reply
                        .quote
                        .as_ref()
                        .map(|quote| (quote.text.clone(), quote.position)),
                })
            } else {
                None
            }
        })
    }

    pub fn chat_id(&self) -> ChatId {
        ChatId(self.chat_id)
    }

    pub fn is_media_album(&self) -> bool {
        let n = self.album.len();
        (2..=crate::album::ALBUM_MAX_ITEMS).contains(&n)
            && self
                .album
                .iter()
                .all(|att| matches!(att.kind, AttachmentKind::Photo | AttachmentKind::Video))
    }

    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty() && self.attachment.is_none() && self.album.is_empty()
    }

    pub fn caption(&self) -> &str {
        self.text.trim()
    }
}

/// MED4: find `http(s)://` URLs in composer text (TDLib's `getLinkPreview`
/// takes the raw text, but the composer preview chip needs to know when
/// to appear at all). Stdlib scan — no URL parser dependency; trailing
/// punctuation (`,.;:!?)]'"`) is trimmed the way clients do.
pub fn find_urls(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter_map(|token| {
            let lower = token.to_lowercase();
            let url = if lower.starts_with("http://") || lower.starts_with("https://") {
                token
            } else {
                return None;
            };
            let trimmed =
                url.trim_end_matches([',', '.', ';', ':', '!', '?', ')', ']', '\'', '"', '…']);
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn scratch(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "quill-composer-{}-{}-{}",
            label,
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn bot_command_insert_text() {
        // Phase 3.1: tapping a command chip composes the new composer text.
        assert_eq!(insert_bot_command_text("", "start"), "/start");
        assert_eq!(insert_bot_command_text("   ", "start"), "/start");
        assert_eq!(insert_bot_command_text("hello", "start"), "hello /start");
        assert_eq!(insert_bot_command_text("hello ", "start"), "hello /start");
        assert_eq!(insert_bot_command_text("/help", "start"), "/help /start");
    }

    #[test]
    fn switch_inline_insert_text() {
        // Phase 3.2: tapping a switchInline button inserts the query.
        assert_eq!(insert_switch_inline_text("", "pic"), "pic");
        assert_eq!(insert_switch_inline_text("   ", "pic"), "pic");
        assert_eq!(insert_switch_inline_text("hello", "pic"), "hello pic");
        assert_eq!(insert_switch_inline_text("hello ", "pic"), "hello pic");
    }

    #[test]
    fn ime_enter_does_not_send() {
        assert!(!should_send_on_enter(
            EnterEvent {
                composing: true,
                shift: false,
                secondary: false,
            },
            SendKeyMode::Enter,
        ));
    }

    #[test]
    fn plain_enter_sends() {
        assert!(should_send_on_enter(
            EnterEvent {
                composing: false,
                shift: false,
                secondary: false,
            },
            SendKeyMode::Enter,
        ));
    }

    #[test]
    fn shift_enter_is_newline() {
        assert!(!should_send_on_enter(
            EnterEvent {
                composing: false,
                shift: true,
                secondary: false,
            },
            SendKeyMode::Enter,
        ));
    }

    #[test]
    fn secondary_enter_does_not_send() {
        assert!(!should_send_on_enter(
            EnterEvent {
                composing: false,
                shift: false,
                secondary: true,
            },
            SendKeyMode::Enter,
        ));
    }

    #[test]
    fn kit_marked_text_range_is_the_composing_signal() {
        // Kit PressEnter has no composing field; a live IME mark must suppress send.
        let composing = enter_event_from_kit(false, false, Some(0..2));
        assert!(composing.composing);
        assert!(!should_send_on_enter(composing, SendKeyMode::Enter));

        let idle = enter_event_from_kit(false, false, None);
        assert!(!idle.composing);
        assert!(should_send_on_enter(idle, SendKeyMode::Enter));
    }

    #[test]
    fn snapshot_freezes_destination() {
        let snap = ComposerSnapshot::capture(ChatId(7), ViewGeneration(3), "  hi  ");
        assert_eq!(snap.chat_id(), ChatId(7));
        assert_eq!(snap.view_generation, 3);
        assert!(!snap.is_empty());
        assert!(ComposerSnapshot::capture(ChatId(1), ViewGeneration(1), "   ").is_empty());
    }

    #[test]
    fn attachment_pick_and_caption_only_send() {
        let root = scratch("attach");
        let photo = root.join("shot.png");
        fs::write(&photo, [9, 9]).unwrap();
        let att = ComposerAttachment::pick(&photo, AttachmentKind::Photo).unwrap();
        assert_eq!(att.file_name, "shot.png");
        assert!(att.send_path_str().is_some());
        let empty_text = ComposerSnapshot::capture_with_attachment(
            ChatId(1),
            ViewGeneration(1),
            "   ",
            Some(att.clone()),
        );
        assert!(!empty_text.is_empty());
        assert_eq!(empty_text.caption(), "");
        assert!(
            ComposerAttachment::pick(&root.join("nope.png"), AttachmentKind::Document).is_none()
        );
        let note = root.join("round.mp4");
        fs::write(&note, [1]).unwrap();
        let video_note = ComposerAttachment::pick(&note, AttachmentKind::VideoNote).unwrap();
        let mut list = vec![att];
        ComposerAttachment::push_attachment(&mut list, video_note);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].kind, AttachmentKind::VideoNote);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn cancel_reply_keeps_typed_text() {
        let reply = ComposerReplyTo::new(ChatId(11), MessageId(101), "Hello from injected JSON.");
        let (cleared, text) = cancel_reply_draft(Some(reply), "keep this draft".into());
        assert_eq!(cleared, None);
        assert_eq!(text, "keep this draft");
    }

    #[test]
    fn snapshot_reply_is_same_chat_only() {
        let same = ComposerSnapshot::capture(ChatId(11), ViewGeneration(1), "hi").with_reply(Some(
            ComposerReplyTo::new(ChatId(11), MessageId(101), "orig"),
        ));
        assert_eq!(same.send_reply_to(), Some(MessageId(101)));
        let other = same.with_reply(Some(ComposerReplyTo::new(
            ChatId(12),
            MessageId(40),
            "other chat",
        )));
        assert_eq!(other.send_reply_to(), None);
    }

    #[test]
    fn own_outgoing_text_can_enter_edit_incoming_cannot() {
        let outgoing = ComposerEdit::from_own_content(
            ChatId(11),
            MessageId(102),
            true,
            false,
            &MessageContent::Text("Reply from the session reducer.".into()),
        )
        .unwrap();
        assert_eq!(outgoing.kind, ComposerEditKind::Text);
        assert_eq!(outgoing.original_text, "Reply from the session reducer.");
        assert!(
            ComposerEdit::from_own_content(
                ChatId(11),
                MessageId(101),
                false,
                false,
                &MessageContent::Text("incoming".into()),
            )
            .is_none()
        );
        assert!(
            ComposerEdit::from_own_content(
                ChatId(11),
                MessageId(-1),
                true,
                true,
                &MessageContent::Text("pending".into()),
            )
            .is_none()
        );
    }

    #[test]
    fn cancel_edit_restores_unrelated_draft() {
        let edit = ComposerEdit::from_own_content(
            ChatId(11),
            MessageId(102),
            true,
            false,
            &MessageContent::Text("original outgoing".into()),
        )
        .unwrap();
        let (edit, field, saved) = begin_edit_draft("keep this draft".into(), edit);
        assert_eq!(field, "original outgoing");
        assert_eq!(saved, "keep this draft");
        let (cleared, restored) = cancel_edit_draft(Some(edit), saved);
        assert_eq!(cleared, None);
        assert_eq!(restored, "keep this draft");
    }

    #[test]
    fn edit_keeps_reply_on_the_normal_draft() {
        let edit = ComposerEdit::from_own_content(
            ChatId(11),
            MessageId(102),
            true,
            false,
            &MessageContent::Text("original outgoing".into()),
        )
        .unwrap();
        let reply = ComposerReplyTo::new(ChatId(11), MessageId(101), "quoted");
        let (edit, field, saved, stashed) =
            begin_edit_keeping_reply("keep this draft".into(), edit, Some(reply.clone()));
        assert_eq!(field, "original outgoing");
        assert_eq!(saved, "keep this draft");
        assert_eq!(stashed, Some(reply.clone()));
        let _ = edit;
        let (restored, reply_back) = cancel_edit_keeping_reply(saved, stashed);
        assert_eq!(restored, "keep this draft");
        assert_eq!(reply_back, Some(reply));
    }

    #[test]
    fn delete_confirm_is_own_outgoing_only() {
        assert!(DeleteConfirm::own(ChatId(11), MessageId(102), true, false).is_some());
        assert!(DeleteConfirm::own(ChatId(11), MessageId(101), false, false).is_none());
        assert!(DeleteConfirm::own(ChatId(11), MessageId(-5), true, true).is_none());
    }

    #[test]
    fn forward_draft_sorts_and_rejects_pending() {
        assert!(ForwardDraft::from_message(ChatId(11), MessageId(-1), true).is_none());
        assert!(ForwardDraft::from_message(ChatId(11), MessageId(0), false).is_none());
        let mut draft = ForwardDraft::from_message(ChatId(11), MessageId(102), false).unwrap();
        draft.toggle(ChatId(11), MessageId(101), false);
        assert_eq!(draft.message_ids, vec![MessageId(101), MessageId(102)]);
        draft.toggle(ChatId(11), MessageId(102), false);
        assert_eq!(draft.message_ids, vec![MessageId(101)]);
        draft.toggle(ChatId(12), MessageId(40), false);
        assert_eq!(draft.from_chat_id, ChatId(12));
        assert_eq!(draft.message_ids, vec![MessageId(40)]);
        let (cleared, picker) = cancel_forward_draft(Some(draft));
        assert_eq!(cleared, None);
        assert!(!picker);
    }

    #[test]
    fn draft_save_matches_tdesktop_quiet_and_anyway_windows() {
        let idle = DraftSaveClock::idle();
        assert_eq!(
            schedule_draft_save(idle, 10_000, true),
            DraftSaveStep::Wait {
                delay_ms: DRAFT_SAVE_TIMEOUT_MS,
                started_ms: 10_000,
            }
        );
        let armed = DraftSaveClock {
            started_ms: Some(10_000),
        };
        assert_eq!(
            schedule_draft_save(armed, 11_500, true),
            DraftSaveStep::Wait {
                delay_ms: DRAFT_SAVE_TIMEOUT_MS,
                started_ms: 10_000,
            }
        );
        assert_eq!(
            schedule_draft_save(armed, 15_000, true),
            DraftSaveStep::Write
        );
        assert_eq!(
            schedule_draft_save(armed, 10_100, false),
            DraftSaveStep::Write
        );
        assert_eq!(draft_text_to_store("  ", false), None);
        assert_eq!(draft_text_to_store("  ", true), Some("  "));
        assert_eq!(draft_text_to_store("hi", false), Some("hi"));
    }

    // Phase 3.3: `/` command-menu trigger / merge / filter.
    fn bot_command(command: &str, description: &str) -> BotCommand {
        BotCommand {
            command: command.into(),
            description: description.into(),
            is_ephemeral: false,
        }
    }

    #[test]
    fn command_menu_trigger_matches_trailing_slash_token() {
        assert_eq!(command_menu_trigger("/"), Some(""));
        assert_eq!(command_menu_trigger("/st"), Some("st"));
        assert_eq!(command_menu_trigger("hello /st"), Some("st"));
        assert_eq!(command_menu_trigger("hi\n/he"), Some("he"));
        assert_eq!(command_menu_trigger("/ST"), Some("ST"));
    }

    #[test]
    fn command_menu_trigger_rejects_mid_word_slashes() {
        assert_eq!(command_menu_trigger(""), None);
        assert_eq!(command_menu_trigger("hello"), None);
        assert_eq!(command_menu_trigger("a/b"), None);
        assert_eq!(command_menu_trigger("hello a/b"), None);
        assert_eq!(command_menu_trigger("http://x"), None);
        assert_eq!(command_menu_trigger("/a/b"), None);
        assert_eq!(command_menu_trigger("//"), None);
        // Trailing whitespace ends the token: the menu closes once the
        // user commits the token with a space.
        assert_eq!(command_menu_trigger("/start "), None);
        assert_eq!(command_menu_trigger("hello /st "), None);
    }

    #[test]
    fn inline_query_trigger_parses_leading_at_token() {
        assert_eq!(inline_query_trigger("@bot"), Some(("bot", "")));
        assert_eq!(inline_query_trigger("@bot "), Some(("bot", "")));
        assert_eq!(inline_query_trigger("@bot cats"), Some(("bot", "cats")));
        assert_eq!(
            inline_query_trigger("@gif_bot_1 cute cats"),
            Some(("gif_bot_1", "cute cats"))
        );
        // Token ends at the first non-username char; the rest is query.
        assert_eq!(inline_query_trigger("@bot!"), Some(("bot", "!")));
    }

    #[test]
    fn inline_query_trigger_rejects_non_leading_at() {
        assert_eq!(inline_query_trigger(""), None);
        assert_eq!(inline_query_trigger("@"), None);
        assert_eq!(inline_query_trigger("hi @bot"), None);
        assert_eq!(inline_query_trigger(" @bot"), None);
        assert_eq!(inline_query_trigger("@@bot"), None);
    }

    #[test]
    fn strip_command_menu_trigger_removes_trailing_token() {
        assert_eq!(strip_command_menu_trigger("/"), Some(""));
        assert_eq!(strip_command_menu_trigger("/st"), Some(""));
        assert_eq!(strip_command_menu_trigger("hello /st"), Some("hello "));
        assert_eq!(strip_command_menu_trigger("hi\n/he"), Some("hi\n"));
        assert_eq!(strip_command_menu_trigger("a/b"), None);
        assert_eq!(strip_command_menu_trigger("/start "), None);
    }

    #[test]
    fn merge_command_menu_items_prefers_specific_descriptions() {
        let specific = vec![bot_command("start", "Start the bot")];
        let global = vec![
            bot_command("start", "Global start"),
            bot_command("settings", "Tweak the bot"),
        ];
        let items = merge_command_menu_items(&specific, &global);
        assert_eq!(items.len(), 2);
        // Bot-specific rows first; the duplicate keeps the bot-specific
        // description.
        assert_eq!(
            items[0],
            CommandMenuItem {
                command: "start".into(),
                description: "Start the bot".into(),
                global: false,
                is_ephemeral: false,
            }
        );
        assert_eq!(
            items[1],
            CommandMenuItem {
                command: "settings".into(),
                description: "Tweak the bot".into(),
                global: true,
                is_ephemeral: false,
            }
        );
    }

    #[test]
    fn merge_command_menu_items_empty_sides() {
        assert!(merge_command_menu_items(&[], &[]).is_empty());
        let items = merge_command_menu_items(&[bot_command("start", "")], &[]);
        assert_eq!(items.len(), 1);
        assert!(!items[0].global);
    }

    #[test]
    fn merge_command_menu_items_keeps_ephemeral_flag() {
        // The `is_ephemeral` flag is what the menu icon reads — it must
        // survive the merge from both the bot-specific and the global
        // side.
        let mut secret = bot_command("secret", "Only you see this");
        secret.is_ephemeral = true;
        let mut gsettings = bot_command("gsettings", "Global ephemeral");
        gsettings.is_ephemeral = true;
        let items = merge_command_menu_items(&[secret, bot_command("start", "")], &[gsettings]);
        let secret_item = items.iter().find(|i| i.command == "secret").unwrap();
        assert!(secret_item.is_ephemeral);
        assert!(!secret_item.global);
        let global_item = items.iter().find(|i| i.command == "gsettings").unwrap();
        assert!(global_item.is_ephemeral);
        assert!(global_item.global);
        assert!(
            !items
                .iter()
                .find(|i| i.command == "start")
                .unwrap()
                .is_ephemeral
        );
    }

    #[test]
    fn filter_command_menu_items_matches_prefix_case_insensitively() {
        let items = vec![
            CommandMenuItem {
                command: "start".into(),
                description: String::new(),
                global: false,
                is_ephemeral: false,
            },
            CommandMenuItem {
                command: "settings".into(),
                description: String::new(),
                global: true,
                is_ephemeral: false,
            },
            CommandMenuItem {
                command: "help".into(),
                description: String::new(),
                global: false,
                is_ephemeral: false,
            },
        ];
        let all: Vec<&str> = filter_command_menu_items(&items, "")
            .iter()
            .map(|item| item.command.as_str())
            .collect();
        assert_eq!(all, vec!["start", "settings", "help"]);
        let st: Vec<&str> = filter_command_menu_items(&items, "st")
            .iter()
            .map(|item| item.command.as_str())
            .collect();
        assert_eq!(st, vec!["start"]);
        // Capitalized input still matches lowercase commands.
        let caps: Vec<&str> = filter_command_menu_items(&items, "ST")
            .iter()
            .map(|item| item.command.as_str())
            .collect();
        assert_eq!(caps, vec!["start"]);
        assert!(filter_command_menu_items(&items, "zzz").is_empty());
    }

    // M1: the markup parser emits TDLib UTF-16 offsets directly. One test
    // per entity kind, plus the documented edge cases (unmatched stays
    // literal, no nesting).
    #[test]
    fn markup_parses_every_entity_kind() {
        let cases: &[(&str, &str, FormatKind, i32, i32)] = &[
            ("**bold**", "bold", FormatKind::Bold, 0, 4),
            ("*italic*", "italic", FormatKind::Italic, 0, 6),
            ("_italic_", "italic", FormatKind::Italic, 0, 6),
            ("__under__", "under", FormatKind::Underline, 0, 5),
            ("~~strike~~", "strike", FormatKind::Strikethrough, 0, 6),
            ("`code`", "code", FormatKind::Code, 0, 4),
            ("||spoiler||", "spoiler", FormatKind::Spoiler, 0, 7),
            (
                "[label](https://example.com)",
                "label",
                FormatKind::TextUrl,
                0,
                5,
            ),
        ];
        for (input, clean, kind, offset, length) in cases {
            let (text, entities) = parse_format_markup(input);
            assert_eq!(&text, clean, "clean text for {input}");
            assert_eq!(entities.len(), 1, "entity count for {input}");
            assert_eq!(entities[0].kind, *kind);
            assert_eq!(entities[0].offset, *offset);
            assert_eq!(entities[0].length, *length);
        }
        // Pre without language and with language.
        let (text, entities) = parse_format_markup("```\nlet x = 1;\n```");
        assert_eq!(text, "let x = 1;");
        assert_eq!(entities[0].kind, FormatKind::Pre);
        assert!(entities[0].language.is_empty());
        let (text, entities) = parse_format_markup("```rust\nlet x = 1;\n```");
        assert_eq!(text, "let x = 1;");
        assert_eq!(entities[0].kind, FormatKind::Pre);
        assert_eq!(entities[0].language, "rust");
        // Block quote is a `> ` line prefix.
        let (text, entities) = parse_format_markup("> quoted");
        assert_eq!(text, "quoted");
        assert_eq!(entities[0].kind, FormatKind::BlockQuote);
        assert_eq!((entities[0].offset, entities[0].length), (0, 6));
        // URL lands on the entity.
        let (_, entities) = parse_format_markup("[t](https://t.me/x)");
        assert_eq!(entities[0].url, "https://t.me/x");
    }

    #[test]
    fn markup_unmatched_delimiters_stay_literal() {
        let (text, entities) = parse_format_markup("a **bold and *half");
        assert_eq!(text, "a **bold and *half");
        assert!(entities.is_empty());
    }

    #[test]
    fn markup_does_not_nest() {
        // Documented single-pass behavior: the inner marker pair is literal.
        let (text, entities) = parse_format_markup("**a *b* c**");
        assert_eq!(text, "a *b* c");
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].kind, FormatKind::Bold);
    }

    #[test]
    fn markup_offsets_are_utf16_with_emoji() {
        // 😀 is one char but two UTF-16 code units; TDLib counts those.
        let (text, entities) = parse_format_markup("😀 **bold**");
        assert_eq!(text, "😀 bold");
        assert_eq!(entities.len(), 1);
        assert_eq!((entities[0].offset, entities[0].length), (3, 4));
        // Emoji inside the formatted span shifts the length too.
        let (text, entities) = parse_format_markup("**a😀b**");
        assert_eq!(text, "a😀b");
        assert_eq!(entities.len(), 1);
        assert_eq!((entities[0].offset, entities[0].length), (0, 4));
        // BMP text after a surrogate pair keeps counting in code units.
        let (text, entities) = parse_format_markup("😀😀 *it*");
        assert_eq!(text, "😀😀 it");
        assert_eq!((entities[0].offset, entities[0].length), (5, 2));
    }

    #[test]
    fn apply_format_wraps_selection_and_places_cursor() {
        // Selection: wrap, selection covers the inner text only.
        let (text, sel) = apply_format_markup("hello world", 6..11, &FormatAction::Bold);
        assert_eq!(text, "hello **world**");
        assert_eq!(&text[sel], "world");
        // Empty selection: marker pair inserted, cursor between markers.
        let (text, sel) = apply_format_markup("hi", 2..2, &FormatAction::Italic);
        assert_eq!(text, "hi**");
        assert_eq!(sel, 3..3);
        // Link with empty selection lands the cursor in the URL slot.
        let (text, sel) = apply_format_markup("hi", 2..2, &FormatAction::Link(String::new()));
        assert_eq!(text, "hi[]()");
        assert_eq!(sel, 5..5);
        // Block quote prefixes each selected line.
        let (text, _) = apply_format_markup("a\nb", 0..3, &FormatAction::BlockQuote);
        assert_eq!(text, "> a\n> b");
    }

    #[test]
    fn clear_format_strips_markers_keeps_text() {
        assert_eq!(
            clear_format_markup("**bold** and `code`", 0..0),
            "bold and code"
        );
        assert_eq!(clear_format_markup("[label](https://x)", 0..0), "label");
        // A selection only strips inside itself.
        assert_eq!(clear_format_markup("**a** **b**", 0..5), "a **b**");
    }

    /// MED4: composer URL detection for the preview chip.
    #[test]
    fn find_urls_detects_http_links() {
        assert!(find_urls("no links here").is_empty());
        assert_eq!(
            find_urls("see https://example.com/a, and http://x.org."),
            vec!["https://example.com/a", "http://x.org"]
        );
        // Bare "www." is not a URL for the chip (TDLib may still preview
        // it server-side; the chip only tracks explicit schemes).
        assert!(find_urls("see www.example.com").is_empty());
        assert_eq!(
            find_urls("https://EXAMPLE.com/Path"),
            vec!["https://EXAMPLE.com/Path"]
        );
    }
    /// Slice G1: `quote_position` — UTF-16 code-unit offsets for
    /// `inputTextQuote.position`.
    #[test]
    fn quote_position_finds_substring_utf16_offset() {
        assert_eq!(quote_position("hello world", "world"), Some(6));
        assert_eq!(quote_position("hello world", "hello"), Some(0));
        // Non-BMP characters count as 2 UTF-16 code units.
        assert_eq!(quote_position("a😀b", "b"), Some(3));
        assert_eq!(quote_position("hello", ""), None);
        assert_eq!(quote_position("hello", "xyz"), None);
        // First occurrence wins.
        assert_eq!(quote_position("aa aa", "aa"), Some(0));
    }
}
