//! Connect driver: animated emoji taps (tdesktop `EmojiInteractions`).
use super::*;
use crate::emoji_animations::search_emoji_animations;
use crate::emoji_interaction::{click_animated_emoji_message, watching_animations_action};
use crate::ids::{ChatId, MessageId, RequestId};
use crate::state::{RequestPurpose, StickersPurpose};
use crate::telegram::envelope::ChatKind;

impl<S: JsonSender> ConnectDriver<S> {
    /// A big emoji was tapped: `clickAnimatedEmojiMessage`. tdesktop plays
    /// emoji interactions in one-to-one chats only; TDLib answers with the
    /// effect sticker, or a 404 when the usual animation is enough (the
    /// answer lands in `Session::stickers.interactions`).
    pub fn click_animated_emoji(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() || message_id.0 <= 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let private = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| matches!(chat.kind, ChatKind::Private { .. }));
        if !private {
            return Ok(None);
        }
        let extra = self.session.request(
            RequestPurpose::Stickers(StickersPurpose::ClickAnimatedEmoji {
                chat_id: chat_id.0,
                message_id: message_id.0,
            }),
            Some(chat_id),
        );
        let json = click_animated_emoji_message(extra, chat_id, message_id);
        self.send_json_request(extra, &json).map(Some)
    }

    /// The window shows `shown` of `chat_id` while it is read: the incoming
    /// lone emoji there that are unread play an interaction effect, once a
    /// session, in a one-to-one chat (`Session::autoplay_emoji_effects`).
    /// Loads the effects pack when it is due (`searchStickerSet`
    /// "EmojiAnimations"). Never `clickAnimatedEmojiMessage`: that tells the
    /// partner the emoji was tapped. Returns the pack request, when one
    /// went out.
    pub fn autoplay_emoji_effects(
        &mut self,
        chat_id: ChatId,
        shown: &[MessageId],
        paused: bool,
        now_ms: u64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let private = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| matches!(chat.kind, ChatKind::Private { .. }));
        if !private {
            return Ok(None);
        }
        let request = if self.session.stickers.animations.load_due(now_ms) {
            let extra = self.session.request(RequestPurpose::EmojiAnimations, None);
            let sent = self.send_json_request(extra, &search_emoji_animations(extra));
            if sent.is_ok() {
                self.session.stickers.animations.loading(now_ms);
            }
            sent.map(Some)
        } else {
            Ok(None)
        };
        self.session
            .autoplay_emoji_effects(chat_id, shown, paused, rand::random());
        request
    }

    /// The partner's effect started playing: tell them with
    /// `chatActionWatchingAnimations`, at most once a second per emoji.
    pub fn send_watching_animations(
        &mut self,
        chat_id: ChatId,
        emoji: &str,
        now_ms: u64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() || emoji.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self
            .session
            .stickers
            .interactions
            .may_send_seen(chat_id, emoji, now_ms)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::SendChatAction, Some(chat_id));
        let json = watching_animations_action(extra, chat_id, emoji);
        self.send_json_request(extra, &json).map(Some)
    }
}
