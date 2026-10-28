//! Editing the caption of a media message that is still being sent.
//!
//! Telegram Desktop lets Up in an empty field open the caption box of the
//! last message while it is still local (`HistoryWidget::keyPressEvent`:
//! `last->isLocal()` and `media()->allowsEdit()`), and the new caption goes
//! out with the upload. TDLib cannot edit a message before it has a server
//! id, so the new caption is kept with the pending message, shown at once,
//! and sent as an edit when `updateMessageSendSucceeded` gives the message
//! its real id.

use super::{ComposerEdit, ComposerEditKind};
use crate::ids::{ChatId, MessageId};
use crate::telegram::envelope::MessageContent;

/// A caption chosen for a message that has not been sent yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeferredCaption {
    pub chat_id: ChatId,
    /// The temporary id of the pending message.
    pub message_id: MessageId,
    pub text: String,
    pub caption_above: bool,
}

/// Whether `content` is media whose caption can be edited (tdesktop
/// `Media::allowsEditCaption`: photos and files, not video notes and not
/// stickers).
pub fn has_editable_caption(content: &MessageContent) -> bool {
    matches!(
        content,
        MessageContent::Photo(_)
            | MessageContent::Document(_)
            | MessageContent::VoiceNote(_)
            | MessageContent::Animation(_)
            | MessageContent::Video(_)
            | MessageContent::Audio(_)
    )
}

/// Show `caption` on a pending media row.
pub fn set_caption(content: &mut MessageContent, caption: &str) {
    let caption = caption.to_string();
    match content {
        MessageContent::Photo(media) => {
            media.caption = caption;
            media.caption_entities.clear();
        }
        MessageContent::Document(media) => {
            media.caption = caption;
            media.caption_entities.clear();
        }
        MessageContent::VoiceNote(media) => {
            media.caption = caption;
            media.caption_entities.clear();
        }
        MessageContent::Animation(media) => {
            media.caption = caption;
            media.caption_entities.clear();
        }
        MessageContent::Video(media) => {
            media.caption = caption;
            media.caption_entities.clear();
        }
        MessageContent::Audio(media) => {
            media.caption = caption;
            media.caption_entities.clear();
        }
        _ => {}
    }
}

impl ComposerEdit {
    /// The caption edit for the pending media message `message_id`; `None`
    /// for text and for media without an editable caption. Unlike
    /// [`ComposerEdit::from_own_content`] it takes a message that is still
    /// being sent, and offers no file replacement.
    pub fn from_pending_media(
        chat_id: ChatId,
        message_id: MessageId,
        content: &MessageContent,
    ) -> Option<Self> {
        if !has_editable_caption(content) {
            return None;
        }
        let mut edit = Self::from_own_content(chat_id, message_id, true, false, content)?;
        edit.kind = ComposerEditKind::Caption;
        edit.media_edit = Default::default();
        Some(edit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telegram::envelope::TextContent;

    #[test]
    fn only_media_with_a_caption_opens_for_editing() {
        let text = MessageContent::Text(TextContent {
            text: "hi".into(),
            entities: Vec::new(),
            link_preview: None,
            animated: None,
        });
        assert!(ComposerEdit::from_pending_media(ChatId(1), MessageId(2), &text).is_none());
        assert!(!has_editable_caption(&text));
    }
}
