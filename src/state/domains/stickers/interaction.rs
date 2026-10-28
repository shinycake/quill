//! Animated emoji taps: effect stickers from `clickAnimatedEmojiMessage`
//! and `updateAnimatedEmojiMessageClicked`; and the effect an incoming
//! lone emoji plays by itself when it is read (`emoji_animations`).
use crate::emoji_animations::Waiting;
use crate::emoji_interaction::EmojiEffect;
use crate::state::*;
use crate::telegram::envelope::{ChatKind, ParsedFile, StickerItem, StickersPayload};

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

    /// The interaction effects pack arrived: build its map, then play the
    /// effects of the emoji read while it loaded, where still on screen.
    pub(crate) fn accept_emoji_animations(
        &mut self,
        stickers: &[StickerItem],
        emojis: &[Vec<String>],
    ) {
        self.stickers.animations.apply_set(stickers, emojis);
        for waiting in self.stickers.animations.take_waiting() {
            if !self.chat_is_shown(waiting.chat_id) {
                continue;
            }
            let seed = waiting.message_id.0 as u64;
            self.queue_read_effect(waiting.chat_id, waiting.message_id, &waiting.emoji, seed);
        }
    }

    /// The history of `chat_id` shows `shown` while it is being read (the
    /// window is active): each incoming lone emoji there that is still
    /// unread plays one of its interaction effects, once a session, in a
    /// one-to-one chat (tdesktop `HistoryInner`'s `startInteractions` and
    /// `EmojiInteractions::startAutoplay`). `paused`: power saving keeps
    /// emoji in chats still. Nothing is sent to the chat partner. `seed`
    /// picks among the emoji's effects.
    pub fn autoplay_emoji_effects(
        &mut self,
        chat_id: ChatId,
        shown: &[MessageId],
        paused: bool,
        seed: u64,
    ) {
        if paused {
            return;
        }
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return;
        };
        if !matches!(chat.kind, ChatKind::Private { .. }) {
            return;
        }
        let read_till = chat.last_read_inbox_message_id.0;
        let big_emoji = self.settings.media_prefs.big_emoji;
        let Some(history) = self.histories.get(&chat_id.0) else {
            return;
        };
        let due: Vec<(MessageId, String)> = shown
            .iter()
            .filter_map(|id| history.messages.get(&id.0))
            .filter(|message| {
                !message.is_outgoing
                    && !message.pending
                    && !message.failed
                    && message.id.0 > 0
                    && message.id.0 > read_till
            })
            .filter_map(|message| {
                let MessageContent::Text(text) = &message.content else {
                    return None;
                };
                let media = crate::animated_emoji::emoji_media(text, big_emoji)?;
                media
                    .is_single(text)
                    .then(|| (message.id, text.text.trim().to_string()))
            })
            .collect();
        for (message_id, emoji) in due {
            let key = (chat_id.0, message_id.0);
            let animations = &mut self.stickers.animations;
            if !animations.is_ready() {
                if animations.mark_watched(key) {
                    animations.wait(Waiting {
                        chat_id,
                        message_id,
                        emoji,
                    });
                }
                continue;
            }
            if animations.animations_for(&emoji).is_none() || !animations.mark_watched(key) {
                continue;
            }
            let seed = seed ^ (message_id.0 as u64);
            self.queue_read_effect(chat_id, message_id, &emoji, seed);
        }
    }

    /// Queue one of `emoji`'s effects over the message for the window. It
    /// plays like the partner's tap would, but tells the partner nothing
    /// (`incoming` false).
    fn queue_read_effect(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        emoji: &str,
        seed: u64,
    ) {
        let Some(sticker) = self.stickers.animations.pick(emoji, seed) else {
            return;
        };
        self.stickers.interactions.push(EmojiEffect {
            chat_id,
            message_id,
            sticker,
            incoming: false,
        });
    }
}
