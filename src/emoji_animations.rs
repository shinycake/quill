//! The interaction effects pack, and its effect playing by itself when an
//! incoming lone emoji is read (tdesktop `EmojiPack::refreshAnimations`
//! and `EmojiInteractions::startAutoplay`).
//!
//! tdesktop loads the pack with `inputStickerSetAnimatedEmojiAnimations`.
//! TDLib has no call for that input, but the same set answers to its short
//! name, "EmojiAnimations", through `searchStickerSet` (tdesktop's birthday
//! effect resolves it by that name too). Each sticker of the set lists, in
//! `stickerSet.emojis`, the emoji it animates and one or more index
//! emoticons ("1⃣" to "9⃣", "🔟"), so the pack maps an emoji to its effects
//! by index.
//!
//! The effect plays only here: nothing is sent to the chat partner, unlike
//! `clickAnimatedEmojiMessage`, which reports a tap.
#![deny(clippy::indexing_slicing)]

use crate::ids::{ChatId, MessageId, RequestId};
use crate::telegram::envelope::StickerItem;
use serde_json::json;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

/// The pack's short name.
pub const SET_NAME: &str = "EmojiAnimations";
/// tdesktop `kRefreshTimeout`: the pack is fetched again two hours after
/// the last answer or failure.
pub const REFRESH_MS: u64 = 7_200 * 1_000;
/// Messages remembered as played at most; past it the set starts over.
const MAX_WATCHED: usize = 4_096;
/// Messages kept while the pack loads for the first time.
const MAX_WAITING: usize = 8;
/// tdesktop `chooseInteractionEmoji`: these hearts play the red heart's
/// effects when they have none of their own.
const HEARTS: [&str; 8] = [
    "\u{1F49B}",
    "\u{1F499}",
    "\u{1F49A}",
    "\u{1F49C}",
    "\u{1F9E1}",
    "\u{1F5A4}",
    "\u{1F90E}",
    "\u{1F90D}",
];
const RED_HEART: &str = "\u{2764}";

/// `searchStickerSet name:string ignore_cache:Bool = StickerSet` for the
/// pack.
pub fn search_emoji_animations(extra: RequestId) -> String {
    json!({
        "@type": "searchStickerSet",
        "@extra": extra.as_extra(),
        "name": SET_NAME,
        "ignore_cache": false,
    })
    .to_string()
}

/// tdesktop `IndexFromEmoticon`: "1⃣" to "9⃣" are 0 to 8 and "🔟" is 9.
/// A digit counts when anything follows it (the keycap, with or without
/// the variation selector).
pub fn index_from_emoticon(emoticon: &str) -> Option<u8> {
    let mut chars = emoticon.chars();
    let first = chars.next()?;
    if first == '\u{1F51F}' {
        return Some(9);
    }
    if !('1'..='9').contains(&first) || chars.next().is_none() {
        return None;
    }
    first
        .to_digit(10)
        .and_then(|digit| digit.checked_sub(1))
        .and_then(|index| u8::try_from(index).ok())
}

/// The emoji without variation selectors, so "❤️" and "❤" are one key
/// (tdesktop looks both up as the same `EmojiPtr`).
fn emoji_key(emoji: &str) -> String {
    emoji
        .trim()
        .chars()
        .filter(|c| !matches!(c, '\u{FE0E}' | '\u{FE0F}'))
        .collect()
}

/// The emoji without its skin tone (tdesktop `EmojiPtr::original`).
fn without_skin_tone(key: &str) -> String {
    key.chars()
        .filter(|c| !('\u{1F3FB}'..='\u{1F3FF}').contains(c))
        .collect()
}

/// splitmix64, so nearby seeds pick unrelated effects.
fn spread(seed: u64) -> u64 {
    let mut x = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// Where the pack's request stands.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Load {
    #[default]
    Never,
    /// Asked at this time (ms).
    Loading { since_ms: u64 },
    /// Answered, or failed, for the request made at this time (ms).
    Done { asked_ms: u64 },
}

/// A lone emoji read before the pack first arrived: its effect plays when
/// the pack comes, if the chat is still on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiting {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub emoji: String,
}

/// The pack's effects by emoji and index, its load state, and the messages
/// whose effect played this session (tdesktop's per-message
/// `EmojiInteractionWatched` flag).
#[derive(Debug, Default)]
pub struct EmojiAnimations {
    by_emoji: HashMap<String, BTreeMap<u8, StickerItem>>,
    /// A set has arrived at least once.
    ready: bool,
    load: Load,
    watched: HashSet<(i64, i64)>,
    waiting: VecDeque<Waiting>,
}

impl EmojiAnimations {
    /// The pack built from a `stickerSet`: `emojis` lists each sticker's
    /// emoticons in the order of `stickers` (tdesktop `applyAnimationsSet`
    /// with `collectAnimationsIndices`).
    pub fn apply_set(&mut self, stickers: &[StickerItem], emojis: &[Vec<String>]) {
        self.by_emoji.clear();
        for (sticker, emoticons) in stickers.iter().zip(emojis) {
            if sticker.file_id.0 == 0 {
                continue;
            }
            let indices: Vec<u8> = emoticons
                .iter()
                .filter_map(|emoticon| index_from_emoticon(emoticon))
                .collect();
            if indices.is_empty() {
                continue;
            }
            let mut keys: Vec<String> = emoticons
                .iter()
                .filter(|emoticon| index_from_emoticon(emoticon).is_none())
                .chain(std::iter::once(&sticker.emoji))
                .map(|emoji| emoji_key(emoji))
                .filter(|key| !key.is_empty() && index_from_emoticon(key).is_none())
                .collect();
            keys.sort();
            keys.dedup();
            for key in keys {
                let list = self.by_emoji.entry(key).or_default();
                for index in &indices {
                    list.entry(*index).or_insert_with(|| sticker.clone());
                }
            }
        }
        self.ready = true;
        if let Load::Loading { since_ms } = self.load {
            self.load = Load::Done { asked_ms: since_ms };
        }
    }

    /// The request failed: try again after the refresh time, and drop the
    /// messages that waited for it.
    pub fn failed(&mut self) {
        if let Load::Loading { since_ms } = self.load {
            self.load = Load::Done { asked_ms: since_ms };
        }
        self.waiting.clear();
    }

    /// Whether the pack should be asked for at `now_ms`: never asked, or
    /// two hours after the last answer (a lost request counts as one).
    pub fn load_due(&self, now_ms: u64) -> bool {
        match self.load {
            Load::Never => true,
            Load::Loading { since_ms: at } | Load::Done { asked_ms: at } => {
                now_ms.saturating_sub(at) >= REFRESH_MS
            }
        }
    }

    /// The request went out at `now_ms`.
    pub fn loading(&mut self, now_ms: u64) {
        self.load = Load::Loading { since_ms: now_ms };
    }

    /// A set has arrived at least once.
    pub fn is_ready(&self) -> bool {
        self.ready
    }

    /// tdesktop `chooseInteractionEmoji` + `animationsForEmoji`: the
    /// effects for `emoji`, else for it without its skin tone, else the red
    /// heart's for another heart.
    pub fn animations_for(&self, emoji: &str) -> Option<&BTreeMap<u8, StickerItem>> {
        let key = emoji_key(emoji);
        let found = |key: &str| self.by_emoji.get(key).filter(|list| !list.is_empty());
        if let Some(list) = found(&key) {
            return Some(list);
        }
        let original = without_skin_tone(&key);
        if original != key
            && let Some(list) = found(&original)
        {
            return Some(list);
        }
        if HEARTS.contains(&key.as_str()) {
            return found(RED_HEART);
        }
        None
    }

    /// One of `emoji`'s effects, picked by `seed` (tdesktop picks one at
    /// random).
    pub fn pick(&self, emoji: &str, seed: u64) -> Option<StickerItem> {
        let list = self.animations_for(emoji)?;
        let count = u64::try_from(list.len()).ok().filter(|count| *count > 0)?;
        let nth = usize::try_from(spread(seed) % count).ok()?;
        list.values().nth(nth).cloned()
    }

    /// Note that `key`'s effect plays; false when it already did this
    /// session (tdesktop `markEmojiInteractionWatched`).
    pub fn mark_watched(&mut self, key: (i64, i64)) -> bool {
        if self.watched.contains(&key) {
            return false;
        }
        if self.watched.len() >= MAX_WATCHED {
            self.watched.clear();
        }
        self.watched.insert(key);
        true
    }

    /// Keep a message read before the pack first arrived.
    pub fn wait(&mut self, waiting: Waiting) {
        if self.waiting.len() >= MAX_WAITING {
            self.waiting.pop_front();
        }
        self.waiting.push_back(waiting);
    }

    /// The messages that waited for the pack.
    pub fn take_waiting(&mut self) -> Vec<Waiting> {
        self.waiting.drain(..).collect()
    }
}

#[cfg(test)]
#[path = "emoji_animations_tests.rs"]
mod tests;
