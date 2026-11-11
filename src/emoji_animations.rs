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

/// The pack's effects by emoji, then by index.
type Effects = HashMap<String, BTreeMap<u8, StickerItem>>;

/// A lone emoji read before the pack first arrived: its effect plays when
/// the pack comes, if the chat is still on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiting {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub emoji: String,
}

/// Where the pack stands. The effects of the last set stay while it is
/// fetched again or the fetch failed; messages read before the first set
/// wait only in the stages that still expect it.
#[derive(Debug)]
enum Pack {
    /// Never asked for.
    Never { waiting: VecDeque<Waiting> },
    /// Asked at `since_ms`. The answer makes it `Ready`, the error `Failed`
    /// (which drops `waiting`).
    Loading {
        since_ms: u64,
        waiting: VecDeque<Waiting>,
        shown: Option<Effects>,
    },
    /// A set arrived. `asked_ms` is the request it answered; none when the
    /// set came without one.
    Ready {
        asked_ms: Option<u64>,
        effects: Effects,
    },
    /// The request made at `asked_ms` failed; asked again after the
    /// refresh time.
    Failed {
        asked_ms: u64,
        shown: Option<Effects>,
    },
}

impl Default for Pack {
    fn default() -> Self {
        Self::Never {
            waiting: VecDeque::new(),
        }
    }
}

/// The pack's effects by emoji and index, its load state, and the messages
/// whose effect played this session (tdesktop's per-message
/// `EmojiInteractionWatched` flag).
#[derive(Debug, Default)]
pub struct EmojiAnimations {
    pack: Pack,
    watched: HashSet<(i64, i64)>,
}

impl EmojiAnimations {
    /// The pack built from a `stickerSet`: `emojis` lists each sticker's
    /// emoticons in the order of `stickers` (tdesktop `applyAnimationsSet`
    /// with `collectAnimationsIndices`). Returns the messages that waited
    /// for it.
    pub fn apply_set(&mut self, stickers: &[StickerItem], emojis: &[Vec<String>]) -> Vec<Waiting> {
        let mut by_emoji: Effects = HashMap::new();
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
                let list = by_emoji.entry(key).or_default();
                for index in &indices {
                    list.entry(*index).or_insert_with(|| sticker.clone());
                }
            }
        }
        let (asked_ms, waiting) = match std::mem::take(&mut self.pack) {
            Pack::Never { waiting } => (None, waiting),
            Pack::Loading {
                since_ms, waiting, ..
            } => (Some(since_ms), waiting),
            Pack::Ready { asked_ms, .. } => (asked_ms, VecDeque::new()),
            Pack::Failed { asked_ms, .. } => (Some(asked_ms), VecDeque::new()),
        };
        self.pack = Pack::Ready {
            asked_ms,
            effects: by_emoji,
        };
        waiting.into_iter().collect()
    }

    /// The request failed: try again after the refresh time, and drop the
    /// messages that waited for it (they are unwatched again).
    pub fn failed(&mut self) {
        let (asked_ms, shown, waiting) = match std::mem::take(&mut self.pack) {
            Pack::Loading {
                since_ms,
                waiting,
                shown,
            } => (Some(since_ms), shown, waiting),
            Pack::Never { waiting } => (None, None, waiting),
            other => {
                self.pack = other;
                return;
            }
        };
        self.pack = match asked_ms {
            Some(asked_ms) => Pack::Failed { asked_ms, shown },
            None => Pack::default(),
        };
        self.unwatch(waiting);
    }

    /// Let `waiting` messages count as unwatched again, so a later read
    /// plays their effect once the pack has arrived.
    fn unwatch(&mut self, waiting: impl IntoIterator<Item = Waiting>) {
        for waiting in waiting {
            self.watched
                .remove(&(waiting.chat_id.0, waiting.message_id.0));
        }
    }

    /// Whether the pack should be asked for at `now_ms`: never asked, or
    /// two hours after the last answer (a lost request counts as one).
    pub fn load_due(&self, now_ms: u64) -> bool {
        let elapsed = |at: u64| now_ms.saturating_sub(at) >= REFRESH_MS;
        match &self.pack {
            Pack::Never { .. } => true,
            Pack::Loading { since_ms: at, .. } | Pack::Failed { asked_ms: at, .. } => elapsed(*at),
            Pack::Ready { asked_ms, .. } => asked_ms.is_none_or(elapsed),
        }
    }

    /// The request went out at `now_ms`.
    pub fn loading(&mut self, now_ms: u64) {
        let (waiting, shown) = match std::mem::take(&mut self.pack) {
            Pack::Never { waiting } => (waiting, None),
            Pack::Loading { waiting, shown, .. } => (waiting, shown),
            Pack::Ready { effects, .. } => (VecDeque::new(), Some(effects)),
            Pack::Failed { shown, .. } => (VecDeque::new(), shown),
        };
        self.pack = Pack::Loading {
            since_ms: now_ms,
            waiting,
            shown,
        };
    }

    /// The effects of the last set that arrived.
    fn effects(&self) -> Option<&Effects> {
        match &self.pack {
            Pack::Never { .. } => None,
            Pack::Ready { effects, .. } => Some(effects),
            Pack::Loading { shown, .. } | Pack::Failed { shown, .. } => shown.as_ref(),
        }
    }

    /// A set has arrived at least once.
    pub fn is_ready(&self) -> bool {
        self.effects().is_some()
    }

    /// tdesktop `chooseInteractionEmoji` + `animationsForEmoji`: the
    /// effects for `emoji`, else for it without its skin tone, else the red
    /// heart's for another heart.
    pub fn animations_for(&self, emoji: &str) -> Option<&BTreeMap<u8, StickerItem>> {
        let key = emoji_key(emoji);
        let effects = self.effects()?;
        let found = |key: &str| effects.get(key).filter(|list| !list.is_empty());
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

    /// Keep a message read before the pack first arrived. Where no set is
    /// expected (it failed and is not asked again yet) the message counts as
    /// unwatched again instead.
    pub fn wait(&mut self, waiting: Waiting) {
        let (Pack::Never { waiting: queue } | Pack::Loading { waiting: queue, .. }) =
            &mut self.pack
        else {
            self.unwatch([waiting]);
            return;
        };
        let dropped = if queue.len() >= MAX_WAITING {
            queue.pop_front()
        } else {
            None
        };
        queue.push_back(waiting);
        self.unwatch(dropped);
    }
}

#[cfg(test)]
#[path = "emoji_animations_tests.rs"]
mod tests;
