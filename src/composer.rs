//! Composer send policy. IME composition must never send.

use crate::ids::{ChatId, MessageId, ViewGeneration};
use crate::local_path::{is_explicit_send_path, pick_send_path};
use crate::telegram::envelope::MessageContent;
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

/// How the user chose to send a local file.
/// Video is `inputMessageVideo`, not a document (tdesktop Photo/Video vs File).
/// A video note is `inputMessageVideoNote` and is not mixed into an album.
/// Audio is `inputMessageAudio` and is not mixed into a photo/video album.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentKind {
    Photo,
    Document,
    Video,
    VideoNote,
    Audio,
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
            AttachmentKind::Document | AttachmentKind::VideoNote | AttachmentKind::Audio => {
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
            | MessageContent::Unsupported { .. } => return None,
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
        let note = root.join("round.mp4");
        fs::write(&note, [1]).unwrap();
        let video_note = ComposerAttachment::pick(&note, AttachmentKind::VideoNote).unwrap();
        let mut list = vec![att];
        ComposerAttachment::push_attachment(&mut list, video_note);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].kind, AttachmentKind::VideoNote);
        let track = root.join("night.mp3");
        fs::write(&track, [1]).unwrap();
        let audio = ComposerAttachment::pick(&track, AttachmentKind::Audio).unwrap();
        ComposerAttachment::push_attachment(&mut list, audio);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].kind, AttachmentKind::Audio);
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
}
