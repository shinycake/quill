//! Message effects: `updateAvailableMessageEffects` and `getMessageEffect`
//! answers land in `Session::stickers.effects`.
use crate::state::*;
use crate::telegram::envelope::StickersPayload;

impl Session {
    /// Apply the two payloads that carry message effects.
    pub(crate) fn apply_message_effect_payload(&mut self, payload: StickersPayload) {
        match payload {
            StickersPayload::MessageEffect { effect, files } => {
                self.remember_files(&files);
                self.stickers.effects.accept(*effect);
            }
            StickersPayload::AvailableMessageEffects {
                reaction_ids,
                sticker_ids,
            } => self
                .stickers
                .effects
                .set_available(reaction_ids, sticker_ids),
            _ => {}
        }
    }

    /// The effect ids on the open chat's messages that were never looked
    /// up (the badge needs each one's icon).
    pub fn message_effect_ids_in_view(&self) -> Vec<i64> {
        let Some(history) = self.open_chat.and_then(|chat| self.histories.get(&chat.0)) else {
            return Vec::new();
        };
        history
            .messages
            .values()
            .map(|message| message.extras.effect_id)
            .filter(|id| *id != 0)
            .collect()
    }

    /// Whether the chat can take a message sent with an effect: a private
    /// chat (tdesktop `_peer->isUser()`; TDLib applies effects to private
    /// chats only, and a secret chat is not one).
    pub fn chat_takes_message_effects(&self, chat_id: ChatId) -> bool {
        self.chats
            .get(&chat_id.0)
            .is_some_and(|chat| matches!(chat.kind, ChatKind::Private { .. }))
    }
}
