//! Animated emoji taps: effect stickers from `clickAnimatedEmojiMessage`
//! and `updateAnimatedEmojiMessageClicked`.
use crate::emoji_interaction::EmojiEffect;
use crate::state::*;
use crate::telegram::envelope::{ParsedFile, StickerItem, StickersPayload};

impl Session {
    /// Queue the effect of a tap for the window, when its chat is open.
    pub(crate) fn accept_emoji_effect(
        &mut self,
        chat_id: i64,
        message_id: i64,
        sticker: Option<StickerItem>,
        files: &[ParsedFile],
        incoming: bool,
    ) {
        self.remember_files(files);
        let Some(sticker) = sticker else {
            return;
        };
        if chat_id == 0 || message_id == 0 || self.open_chat != Some(ChatId(chat_id)) {
            return;
        }
        self.stickers.interactions.push(EmojiEffect {
            chat_id: ChatId(chat_id),
            message_id: MessageId(message_id),
            sticker,
            incoming,
        });
    }

    /// The two payloads that carry an effect sticker.
    pub(crate) fn apply_emoji_effect_payload(
        &mut self,
        payload: StickersPayload,
        pending: Option<&PendingRequest>,
    ) {
        match payload {
            StickersPayload::EmojiEffectSticker { sticker, files } => {
                if let Some(RequestPurpose::Stickers(StickersPurpose::ClickAnimatedEmoji {
                    chat_id,
                    message_id,
                })) = pending.map(|p| p.purpose)
                {
                    self.accept_emoji_effect(chat_id, message_id, sticker, &files, false);
                }
            }
            StickersPayload::AnimatedEmojiMessageClicked {
                chat_id,
                message_id,
                sticker,
                files,
            } => self.accept_emoji_effect(chat_id, message_id, sticker, &files, true),
            _ => {}
        }
    }
}
