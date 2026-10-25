//! Message effects: `updateAvailableMessageEffects` and `getMessageEffect`
//! answers land in `Session::stickers.effects`.
use crate::message_effect_watch::EffectArrival;
use crate::state::*;
use crate::telegram::envelope::{MessageSendState, ParsedMessage, StickersPayload};

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

    /// A message reached the session: note whether it has an effect left
    /// to play (tdesktop `applyInitialEffectWatched`). `pending`: the row
    /// is this client's send in flight.
    pub(crate) fn note_message_effect(&mut self, message: &ParsedMessage, pending: bool) {
        if message.extras.effect_id == 0 {
            return;
        }
        let arrival = if message.is_outgoing {
            if pending || message.send_state != MessageSendState::Sent {
                EffectArrival::SentHere
            } else {
                EffectArrival::SentElsewhere
            }
        } else {
            // A chat whose read marker is unknown counts as read, like
            // tdesktop with no `inboxReadTillId`.
            let unread = self
                .chats
                .get(&message.chat_id.0)
                .is_some_and(|chat| message.id > chat.last_read_inbox_message_id);
            EffectArrival::Incoming { unread }
        };
        self.stickers
            .effects
            .watch
            .note((message.chat_id.0, message.id.0), arrival);
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
