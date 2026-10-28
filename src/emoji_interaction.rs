//! Animated emoji taps (tdesktop `ChatHelpers::EmojiInteractions`).
//!
//! tdesktop picks the effect sticker from its own emoji pack and sends
//! `sendMessageEmojiInteraction` for the taps. TDLib does both behind two
//! calls: `clickAnimatedEmojiMessage`
//! returns the effect sticker for a tap (or a 404 when the usual animation
//! is enough), and `updateAnimatedEmojiMessageClicked` brings the sticker
//! the other party's tap should play. When the sticker plays,
//! `chatActionWatchingAnimations` with the message's emoji tells the other
//! party it was seen (tdesktop `playStarted`, at most once a second per
//! emoji).
#![deny(clippy::indexing_slicing)]

use crate::ids::{ChatId, MessageId, RequestId};
use crate::telegram::envelope::StickerItem;
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};

/// tdesktop `kAccumulateSeenRequests`: one "watching" action per emoji and
/// chat in this many milliseconds.
pub const SEEN_INTERVAL_MS: u64 = 1000;
/// tdesktop `kMaxPlays`: effects playing at once.
pub const MAX_PLAYS: usize = 5;
/// Effects waiting to start, newest kept.
const MAX_PENDING: usize = 8;
/// Renamed messages remembered at most; past it the oldest names are
/// dropped and such a message at worst plays its emoji once more.
const MAX_ALIASES: usize = 1024;

/// `clickAnimatedEmojiMessage chat_id:int53 message_id:int53 = Sticker`.
pub fn click_animated_emoji_message(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
) -> String {
    json!({
        "@type": "clickAnimatedEmojiMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
    })
    .to_string()
}

/// `sendChatAction` with `chatActionWatchingAnimations emoji:string`.
pub fn watching_animations_action(extra: RequestId, chat_id: ChatId, emoji: &str) -> String {
    json!({
        "@type": "sendChatAction",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": Value::Null,
        "business_connection_id": "",
        "action": { "@type": "chatActionWatchingAnimations", "emoji": emoji },
    })
    .to_string()
}

/// An effect sticker to play over a message.
#[derive(Debug, Clone, PartialEq)]
pub struct EmojiEffect {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub sticker: StickerItem,
    /// The other party tapped (`updateAnimatedEmojiMessageClicked`): the
    /// partner is told it was watched.
    pub incoming: bool,
}

/// Effects the session received and the window has not started yet.
#[derive(Debug, Default)]
pub struct EmojiInteractions {
    pending: VecDeque<EmojiEffect>,
    /// When "watching" was last sent for an emoji in a chat.
    seen_sent: Vec<(ChatId, String, u64)>,
    /// Messages sent from here that got their final id: the key their
    /// lone emoji plays under, so it keeps playing across the rename
    /// (tdesktop hands the player to the new view, `stickerTakePlayer`).
    aliases: HashMap<(i64, i64), (i64, i64)>,
}

impl EmojiInteractions {
    pub fn push(&mut self, effect: EmojiEffect) {
        if effect.sticker.file_id.0 == 0 {
            return;
        }
        if self.pending.len() >= MAX_PENDING {
            self.pending.pop_front();
        }
        self.pending.push_back(effect);
    }

    /// Hand the waiting effects to the window.
    pub fn take_pending(&mut self) -> Vec<EmojiEffect> {
        self.pending.drain(..).collect()
    }

    /// A message sent from here got its final id (or failed under a new
    /// one): its lone emoji keeps its play.
    pub fn renamed(&mut self, old: (i64, i64), new: (i64, i64)) {
        if old == new {
            return;
        }
        let root = self.play_key(old);
        if self.aliases.len() >= MAX_ALIASES {
            self.aliases.clear();
        }
        self.aliases.remove(&old);
        self.aliases.insert(new, root);
    }

    /// The key a message's lone emoji plays under: the id it was sent
    /// with, for a message sent from here.
    pub fn play_key(&self, key: (i64, i64)) -> (i64, i64) {
        self.aliases.get(&key).copied().unwrap_or(key)
    }

    /// Whether "watching" for this emoji may go out now (and note it).
    pub fn may_send_seen(&mut self, chat_id: ChatId, emoji: &str, now_ms: u64) -> bool {
        self.seen_sent
            .retain(|(_, _, at)| now_ms.saturating_sub(*at) < SEEN_INTERVAL_MS);
        if self
            .seen_sent
            .iter()
            .any(|(chat, sent, _)| *chat == chat_id && sent == emoji)
        {
            return false;
        }
        self.seen_sent.push((chat_id, emoji.to_string(), now_ms));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::FileId;
    use crate::telegram::envelope::StickerFormat;

    fn effect(file: i32, incoming: bool) -> EmojiEffect {
        EmojiEffect {
            chat_id: ChatId(7),
            message_id: MessageId(9),
            sticker: StickerItem {
                custom_emoji_id: None,
                id: 1,
                set_id: 2,
                emoji: "\u{1F44D}".into(),
                width: 512,
                height: 512,
                format: StickerFormat::Tgs,
                file_id: FileId(file),
                thumb_file_id: None,
                thumb_width: 0,
                thumb_height: 0,
                requires_premium: false,
                premium_animation: None,
            },
            incoming,
        }
    }

    #[test]
    fn click_request_json() {
        let v: Value = serde_json::from_str(&click_animated_emoji_message(
            RequestId(3),
            ChatId(-100),
            MessageId(4_194_304),
        ))
        .unwrap();
        assert_eq!(v["@type"], "clickAnimatedEmojiMessage");
        assert_eq!(v["chat_id"], -100);
        assert_eq!(v["message_id"], 4_194_304);
        assert_eq!(v["@extra"], RequestId(3).as_extra());
    }

    #[test]
    fn watching_action_json() {
        let v: Value = serde_json::from_str(&watching_animations_action(
            RequestId(4),
            ChatId(5),
            "\u{2764}",
        ))
        .unwrap();
        assert_eq!(v["@type"], "sendChatAction");
        assert_eq!(v["action"]["@type"], "chatActionWatchingAnimations");
        assert_eq!(v["action"]["emoji"], "\u{2764}");
        assert!(v["topic_id"].is_null());
    }

    #[test]
    fn pending_is_bounded_and_skips_empty_files() {
        let mut state = EmojiInteractions::default();
        state.push(effect(0, false));
        assert!(state.take_pending().is_empty());
        for file in 1..=12 {
            state.push(effect(file, file % 2 == 0));
        }
        let taken = state.take_pending();
        assert_eq!(taken.len(), MAX_PENDING);
        assert_eq!(taken[0].sticker.file_id, FileId(5));
        assert!(state.take_pending().is_empty());
    }

    #[test]
    fn a_renamed_message_keeps_the_key_it_was_sent_with() {
        let mut state = EmojiInteractions::default();
        assert_eq!(state.play_key((1, 5)), (1, 5));
        state.renamed((1, 5), (1, 90));
        assert_eq!(state.play_key((1, 90)), (1, 5));
        // Failed, then resent under another id: still the first one.
        state.renamed((1, 90), (1, 91));
        assert_eq!(state.play_key((1, 91)), (1, 5));
        assert_eq!(state.play_key((1, 90)), (1, 90));
    }

    #[test]
    fn seen_is_throttled_per_chat_and_emoji() {
        let mut state = EmojiInteractions::default();
        assert!(state.may_send_seen(ChatId(1), "a", 0));
        assert!(!state.may_send_seen(ChatId(1), "a", 500));
        assert!(state.may_send_seen(ChatId(1), "b", 500));
        assert!(state.may_send_seen(ChatId(2), "a", 500));
        assert!(state.may_send_seen(ChatId(1), "a", 1000));
    }
}
