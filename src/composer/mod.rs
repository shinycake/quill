//! Composer send policy. IME composition must never send.

use crate::ids::{ChatId, MessageId, ViewGeneration};
use crate::local_path::{is_explicit_send_path, pick_send_path};
use crate::telegram::envelope::{BotCommand, MessageContent};
use crate::telegram::requests::SelfDestructSend;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

mod actions;
mod attachments;
mod commands;
mod edit;
mod markup;
mod reply;
mod send;
mod shortcuts;

pub use actions::{DeleteConfirm, ForwardDraft, cancel_forward_draft};
pub use attachments::{
    AttachmentKind, ComposerAttachment, clipboard_image_attachment, media_kind_for,
};
pub use commands::{
    CommandMenuItem, command_menu_trigger, complete_mention, filter_command_menu_items,
    inline_query_trigger, insert_bot_command_text, insert_switch_inline_text, mention_trigger,
    merge_command_menu_items, strip_command_menu_trigger,
};
pub use edit::{
    ComposerEdit, ComposerEditKind, EDIT_MEDIA_ALBUM_ERROR, EDIT_MEDIA_INVALID_FILE, EditMediaKind,
    EditMediaReplacement, EditableMedia, LinkPreviewChoice, MediaEdit, begin_edit_draft,
    begin_edit_keeping_reply, cancel_edit_draft, cancel_edit_keeping_reply,
};
pub use markup::{
    ComposerEntity, FormatAction, FormatKind, apply_format_markup, clear_format_markup,
    custom_emoji_markup, entities_to_markup, find_urls, mention_user_id, parse_format_markup,
};
pub use reply::{ComposerReplyTo, QuoteSelection, cancel_reply_draft, quote_position};
pub use send::{
    ComposerScheduling, EnterEvent, PreviewMediaSize, SendKeyMode, SendOptions,
    enter_event_from_kit, send_started_note, send_text_on_enter, should_send_on_enter,
};
pub use shortcuts::{
    COMPOSER_SHORTCUTS, ComposerShortcut, ComposerShortcutSpec, LinkChord, composer_shortcut_for,
    link_chord_target, normalize_link_url,
};

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

    /// The replied-to message id for a send into this snapshot's chat:
    /// same-chat replies and replies aimed here from another chat.
    pub fn send_reply_to(&self) -> Option<MessageId> {
        self.send_reply().map(|reply| reply.message_id)
    }

    /// Slice G1: the full reply (message id plus validated partial
    /// quote) for the send builders.
    pub fn send_reply(&self) -> Option<crate::telegram::SendReply> {
        self.reply_to
            .as_ref()
            .and_then(|reply| reply.send_target(ChatId(self.chat_id)))
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

    #[test]
    fn snapshot_freezes_destination() {
        let snap = ComposerSnapshot::capture(ChatId(7), ViewGeneration(3), "  hi  ");
        assert_eq!(snap.chat_id(), ChatId(7));
        assert_eq!(snap.view_generation, 3);
        assert!(!snap.is_empty());
        assert!(ComposerSnapshot::capture(ChatId(1), ViewGeneration(1), "   ").is_empty());
    }

    #[test]
    fn snapshot_reply_is_same_chat_unless_aimed_there() {
        let same = ComposerSnapshot::capture(ChatId(11), ViewGeneration(1), "hi").with_reply(Some(
            ComposerReplyTo::new(ChatId(11), MessageId(101), "orig"),
        ));
        assert_eq!(same.send_reply_to(), Some(MessageId(101)));
        assert_eq!(same.send_reply().unwrap().source_chat, None);
        // A reply from chat 12 that was never aimed at chat 11 is dropped.
        let other = same.clone().with_reply(Some(ComposerReplyTo::new(
            ChatId(12),
            MessageId(40),
            "other chat",
        )));
        assert_eq!(other.send_reply_to(), None);
        // Aimed at chat 11, it goes out as a reply to chat 12's message.
        let aimed = same.with_reply(Some(
            ComposerReplyTo::with_quote(
                ChatId(12),
                MessageId(40),
                "other chat",
                QuoteSelection {
                    text: "part".into(),
                    position: 3,
                },
            )
            .into_chat(ChatId(11)),
        ));
        let reply = aimed.send_reply().unwrap();
        assert_eq!(reply.message_id, MessageId(40));
        assert_eq!(reply.source_chat, Some(ChatId(12)));
        assert_eq!(reply.quote, Some(("part".to_string(), 3)));
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
