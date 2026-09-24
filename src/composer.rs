//! Composer send policy. IME composition must never send.

use crate::ids::{ChatId, MessageId, ViewGeneration};
use crate::local_path::{is_explicit_send_path, pick_send_path};
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

/// Message the composer is editing (tdesktop `FieldHeader::editMessage`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposerEdit {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub preview: String,
}

impl ComposerEdit {
    pub fn new(chat_id: ChatId, message_id: MessageId, preview: impl Into<String>) -> Self {
        Self {
            chat_id,
            message_id,
            preview: preview.into(),
        }
    }
}

/// tdesktop `ComposeControls::cancelEditMessage` / `FieldHeader::editCancelled`:
/// drop the edit header and restore the pre-edit compose draft. The field is
/// filled with the message text while editing; cancel does not keep that.
pub fn cancel_edit_draft(
    edit: Option<ComposerEdit>,
    _current: String,
    restore: String,
) -> (Option<ComposerEdit>, String) {
    let _ = edit;
    (None, restore)
}

/// Snapshot of a send attempt: destination is frozen at submit time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposerSnapshot {
    pub chat_id: i64,
    pub view_generation: u64,
    pub text: String,
    pub attachment: Option<ComposerAttachment>,
    pub reply_to: Option<ComposerReplyTo>,
    pub edit: Option<ComposerEdit>,
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
            edit: None,
        }
    }

    pub fn with_reply(mut self, reply_to: Option<ComposerReplyTo>) -> Self {
        self.reply_to = reply_to;
        self
    }

    pub fn with_edit(mut self, edit: Option<ComposerEdit>) -> Self {
        self.edit = edit;
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

    /// Same-chat `editMessageText.message_id` only.
    pub fn send_edit(&self) -> Option<MessageId> {
        self.edit.as_ref().and_then(|edit| {
            if edit.chat_id.0 == self.chat_id {
                Some(edit.message_id)
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
    fn cancel_edit_restores_stashed_draft() {
        let edit = ComposerEdit::new(ChatId(11), MessageId(102), "original");
        let (cleared, text) =
            cancel_edit_draft(Some(edit), "in-progress edit".into(), "stash".into());
        assert_eq!(cleared, None);
        assert_eq!(text, "stash");
    }

    #[test]
    fn snapshot_edit_is_same_chat_only() {
        let same = ComposerSnapshot::capture(ChatId(11), ViewGeneration(1), "hi")
            .with_edit(Some(ComposerEdit::new(ChatId(11), MessageId(102), "orig")));
        assert_eq!(same.send_edit(), Some(MessageId(102)));
        let other = same.with_edit(Some(ComposerEdit::new(
            ChatId(12),
            MessageId(40),
            "other chat",
        )));
        assert_eq!(other.send_edit(), None);
    }
}
