//! Composer send policy. IME composition must never send.

use crate::ids::{ChatId, ViewGeneration};
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

/// Snapshot of a send attempt: destination is frozen at submit time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposerSnapshot {
    pub chat_id: i64,
    pub view_generation: u64,
    pub text: String,
    pub attachment: Option<ComposerAttachment>,
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
        }
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
}
