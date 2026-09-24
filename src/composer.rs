//! Composer send policy. IME composition must never send.

use crate::ids::{ChatId, MessageId, ViewGeneration};
use crate::local_path::{is_explicit_send_path, pick_send_path};
use crate::telegram::envelope::{MessageContent, MessageReaction};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnterEvent {
    /// True while an IME composition is marked (CJK/Hangul/etc.).
    pub composing: bool,
    /// Shift+Enter inserts a newline in chat-style inputs.
    pub shift: bool,
    /// Platform secondary modifier (Ctrl/Cmd) — treated as "do not send".
    pub secondary: bool,
}

/// Enter sends only when the composition is finished and no modifiers apply.
pub fn should_send_on_enter(event: EnterEvent) -> bool {
    !event.composing && !event.shift && !event.secondary
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

/// How the user chose to send a local file (`inputMessagePhoto` vs document).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentKind {
    Photo,
    Document,
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
}

impl ComposerReplyTo {
    pub fn new(chat_id: ChatId, message_id: MessageId, preview: impl Into<String>) -> Self {
        Self {
            chat_id,
            message_id,
            preview: preview.into(),
        }
    }
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
            MessageContent::Text(text) => (ComposerEditKind::Text, text.clone()),
            MessageContent::Photo(photo) => (ComposerEditKind::Caption, photo.caption.clone()),
            MessageContent::Document(doc) => (ComposerEditKind::Caption, doc.caption.clone()),
            MessageContent::Unsupported { .. } => return None,
        };
        Some(Self {
            chat_id,
            message_id,
            original_text,
            kind,
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

/// Pending delete confirm (tdesktop `DeleteMessagesBox` / Unigram
/// `DeleteMessagesPopup`). Own outgoing only in this slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteConfirm {
    pub chat_id: ChatId,
    pub message_id: MessageId,
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
            })
        } else {
            None
        }
    }
}

/// Messages queued for `forwardMessages` (tdesktop `Data::ForwardDraft` /
/// `ShowForwardMessagesBox`). Ids stay strictly increasing (schema).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardDraft {
    pub from_chat_id: ChatId,
    pub message_ids: Vec<MessageId>,
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

/// Official default emoji reactions (Telegram launch set / tdesktop strip
/// fallback before recents exist). Heart is U+2764 `❤` as TDLib stores it,
/// not `❤️` with VS16. Unigram compact flyout shows up to 7 of
/// Top+Recent+Popular; this slice keeps the classic eight.
pub const DEFAULT_QUICK_REACTIONS: &[&str] = &["👍", "👎", "❤", "🔥", "🥰", "👏", "😁", "🤔"];

/// Per-message react picker (tdesktop hover `HistoryView::Reactions::Button`
/// / Unigram `ReactionsMenuFlyout`). Already-sent history only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReactionPicker {
    pub chat_id: ChatId,
    pub message_id: MessageId,
}

impl ReactionPicker {
    pub fn from_message(chat_id: ChatId, message_id: MessageId, pending: bool) -> Option<Self> {
        if !can_react_to(pending, message_id) {
            return None;
        }
        Some(Self {
            chat_id,
            message_id,
        })
    }
}

/// Pending / local ids cannot be reacted (same gate as forward).
pub fn can_react_to(pending: bool, message_id: MessageId) -> bool {
    !pending && message_id.0 > 0
}

/// Unigram `ReactionButton.OnClick`: chosen → `removeMessageReaction`, else
/// `addMessageReaction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReactionToggle {
    Add,
    Remove,
}

pub fn reaction_toggle_for(reactions: &[MessageReaction], emoji: &str) -> ReactionToggle {
    if reactions
        .iter()
        .any(|reaction| reaction.emoji == emoji && reaction.is_chosen)
    {
        ReactionToggle::Remove
    } else {
        ReactionToggle::Add
    }
}

/// Local list after the current user toggles an emoji. Matches the
/// `updateMessageInteractionInfo` TDLib would emit (demo / tests only).
pub fn toggle_emoji_reaction(list: &[MessageReaction], emoji: &str) -> Vec<MessageReaction> {
    let mut out = list.to_vec();
    if let Some(existing) = out.iter_mut().find(|reaction| reaction.emoji == emoji) {
        if existing.is_chosen {
            existing.total_count -= 1;
            existing.is_chosen = false;
            if existing.total_count <= 0 {
                out.retain(|reaction| reaction.emoji != emoji);
            }
        } else {
            existing.total_count += 1;
            existing.is_chosen = true;
        }
    } else {
        out.push(MessageReaction {
            emoji: emoji.to_string(),
            total_count: 1,
            is_chosen: true,
        });
    }
    out
}

/// tdesktop / Unigram Esc: close the reaction strip first, leave history.
pub fn cancel_reaction_picker(picker: Option<ReactionPicker>) -> Option<ReactionPicker> {
    let _ = picker;
    None
}

/// Snapshot of a send attempt: destination is frozen at submit time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposerSnapshot {
    pub chat_id: i64,
    pub view_generation: u64,
    pub text: String,
    pub attachment: Option<ComposerAttachment>,
    pub reply_to: Option<ComposerReplyTo>,
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
            reply_to: None,
        }
    }

    pub fn with_reply(mut self, reply_to: Option<ComposerReplyTo>) -> Self {
        self.reply_to = reply_to;
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

    pub fn chat_id(&self) -> ChatId {
        ChatId(self.chat_id)
    }

    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty() && self.attachment.is_none()
    }

    pub fn caption(&self) -> &str {
        self.text.trim()
    }
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
    fn ime_enter_does_not_send() {
        assert!(!should_send_on_enter(EnterEvent {
            composing: true,
            shift: false,
            secondary: false,
        }));
    }

    #[test]
    fn plain_enter_sends() {
        assert!(should_send_on_enter(EnterEvent {
            composing: false,
            shift: false,
            secondary: false,
        }));
    }

    #[test]
    fn shift_enter_is_newline() {
        assert!(!should_send_on_enter(EnterEvent {
            composing: false,
            shift: true,
            secondary: false,
        }));
    }

    #[test]
    fn secondary_enter_does_not_send() {
        assert!(!should_send_on_enter(EnterEvent {
            composing: false,
            shift: false,
            secondary: true,
        }));
    }

    #[test]
    fn kit_marked_text_range_is_the_composing_signal() {
        // Kit PressEnter has no composing field; a live IME mark must suppress send.
        let composing = enter_event_from_kit(false, false, Some(0..2));
        assert!(composing.composing);
        assert!(!should_send_on_enter(composing));

        let idle = enter_event_from_kit(false, false, None);
        assert!(!idle.composing);
        assert!(should_send_on_enter(idle));
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
    fn reaction_toggle_adds_then_removes_chosen() {
        assert!(ReactionPicker::from_message(ChatId(11), MessageId(-1), true).is_none());
        assert!(ReactionPicker::from_message(ChatId(11), MessageId(0), false).is_none());
        let picker = ReactionPicker::from_message(ChatId(11), MessageId(101), false).unwrap();
        assert_eq!(picker.message_id, MessageId(101));
        assert_eq!(reaction_toggle_for(&[], "👍"), ReactionToggle::Add);
        let added = toggle_emoji_reaction(&[], "👍");
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].emoji, "👍");
        assert_eq!(added[0].total_count, 1);
        assert!(added[0].is_chosen);
        assert_eq!(reaction_toggle_for(&added, "👍"), ReactionToggle::Remove);
        let others = vec![MessageReaction {
            emoji: "🔥".into(),
            total_count: 2,
            is_chosen: false,
        }];
        let with_own = toggle_emoji_reaction(&others, "👍");
        assert_eq!(with_own.len(), 2);
        assert!(with_own.iter().any(|r| r.emoji == "👍" && r.is_chosen));
        let after_remove = toggle_emoji_reaction(&added, "👍");
        assert!(after_remove.is_empty());
        let shared = vec![MessageReaction {
            emoji: "❤".into(),
            total_count: 3,
            is_chosen: true,
        }];
        let decremented = toggle_emoji_reaction(&shared, "❤");
        assert_eq!(decremented.len(), 1);
        assert_eq!(decremented[0].total_count, 2);
        assert!(!decremented[0].is_chosen);
        assert_eq!(cancel_reaction_picker(Some(picker)), None);
        assert_eq!(DEFAULT_QUICK_REACTIONS.len(), 8);
        assert!(DEFAULT_QUICK_REACTIONS.contains(&"❤"));
        assert!(!DEFAULT_QUICK_REACTIONS.contains(&"❤️"));
    }
}
