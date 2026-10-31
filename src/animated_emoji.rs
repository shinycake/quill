//! Messages that are only emoji, drawn big (tdesktop
//! `Element::refreshMedia`, `Sticker`, `CustomEmoji` and `LargeEmoji`).
//!
//! - One emoji TDLib has an animated sticker for (`messageAnimatedEmoji`)
//!   shows as that sticker, 112 pt, with its skin tone. It plays once the
//!   first time it is shown, then rests on its first frame; a tap plays it
//!   again with the interaction effect.
//! - One, two or three custom emoji (counted on the longest line, or the
//!   number of lines when that is more) show as stickers of 112, 78 or
//!   58 pt each, looping like other stickers.
//! - Other messages of one to three emoji keep the large glyphs (36 pt).
//!
//! [`EmojiPlays`] is the "already played" set of the open chat (tdesktop
//! `_animatedStickersPlayed`), cleared when the chat is left.
#![deny(clippy::indexing_slicing)]

use crate::bubble_layout::LARGE_EMOJI_SIZE;
use crate::telegram::envelope::TextContent;
use crate::text::TextEntityKind;
use std::collections::{HashMap, HashSet};

/// A message: chat id and message id.
pub type MessageKey = (i64, i64);

/// tdesktop `Sticker::EmojiSize` (`maxAnimatedEmojiSize`): a lone emoji's
/// sticker, and the base of the custom emoji sizes.
pub const STICKER_SIDE: i32 = 112;

/// Every size an emoji-only message is drawn at, in points: the lone
/// animated emoji, then custom emoji by the message's dimension (tdesktop
/// `CustomEmoji` `SizesInfo`: 1, 0.7 and 0.52 of the sticker size, rounded).
const SIDES: [(EmojiSizeKey, i32); 5] = [
    (EmojiSizeKey::Sticker, STICKER_SIDE),
    (EmojiSizeKey::Custom(1), STICKER_SIDE),
    (EmojiSizeKey::Custom(2), 78),
    (EmojiSizeKey::Custom(3), 58),
    (EmojiSizeKey::Glyphs, LARGE_EMOJI_SIZE),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum EmojiSizeKey {
    Sticker,
    Custom(usize),
    Glyphs,
}

fn side_of(key: EmojiSizeKey) -> Option<i32> {
    SIDES
        .iter()
        .find(|(entry, _)| *entry == key)
        .map(|(_, side)| *side)
}

/// How a message of only emoji is drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmojiMedia {
    /// One emoji as its animated sticker (`TextContent::animated`).
    Sticker,
    /// Custom emoji by line (ids), at most three on a line and three lines.
    CustomEmoji { lines: Vec<Vec<i64>> },
    /// One to three emoji as large glyphs in the text.
    Glyphs,
}

impl EmojiMedia {
    /// The side of one emoji, in points.
    pub fn side(&self) -> i32 {
        let key = match self {
            Self::Sticker => EmojiSizeKey::Sticker,
            Self::CustomEmoji { lines } => EmojiSizeKey::Custom(dimension(lines)),
            Self::Glyphs => EmojiSizeKey::Glyphs,
        };
        side_of(key).unwrap_or(LARGE_EMOJI_SIZE)
    }

    /// One emoji alone, which a tap plays an effect for.
    pub fn is_single(&self, text: &TextContent) -> bool {
        match self {
            Self::Sticker => true,
            Self::CustomEmoji { lines } => lines.len() == 1 && dimension(lines) == 1,
            Self::Glyphs => {
                text.entities.is_empty()
                    && crate::emoji_catalog::big_emoji_count(&text.text) == Some(1)
            }
        }
    }
}

/// tdesktop's dimension of a custom emoji message: its lines, or the emoji
/// on its longest line when that is more.
fn dimension(lines: &[Vec<i64>]) -> usize {
    let longest = lines.iter().map(Vec::len).max().unwrap_or(0);
    longest.max(lines.len())
}

/// How `text` is drawn when it is only emoji and big emoji are on
/// (Settings → Chat → "Large emoji"); `None` draws it as a text bubble.
pub fn emoji_media(text: &TextContent, big_emoji: bool) -> Option<EmojiMedia> {
    if !big_emoji || text.link_preview.is_some() {
        return None;
    }
    if let Some(lines) = custom_emoji_lines(text)
        && side_of(EmojiSizeKey::Custom(dimension(&lines))).is_some()
    {
        return Some(EmojiMedia::CustomEmoji { lines });
    }
    if text.animated.is_some()
        && text.entities.is_empty()
        && crate::emoji_catalog::big_emoji_count(&text.text) == Some(1)
    {
        return Some(EmojiMedia::Sticker);
    }
    crate::emoji_catalog::big_emoji_count_with_entities(&text.text, &text.entities)
        .map(|_| EmojiMedia::Glyphs)
}

/// The custom emoji of `text` by line, when it holds nothing else but
/// line breaks (lib_ui `String::toOnlyCustomEmoji`).
fn custom_emoji_lines(text: &TextContent) -> Option<Vec<Vec<i64>>> {
    if text.entities.is_empty() {
        return None;
    }
    let mut spans: Vec<_> = text.entities.iter().collect();
    spans.sort_by_key(|entity| entity.utf8_start);
    let mut lines: Vec<Vec<i64>> = vec![Vec::new()];
    let mut at = 0;
    let breaks = |gap: &str, lines: &mut Vec<Vec<i64>>| -> Option<()> {
        for ch in gap.chars() {
            match ch {
                '\n' => lines.push(Vec::new()),
                '\r' => {}
                _ => return None,
            }
        }
        Some(())
    };
    for entity in spans {
        let TextEntityKind::CustomEmoji { custom_emoji_id } = entity.kind else {
            return None;
        };
        if entity.utf8_start < at || entity.utf8_end <= entity.utf8_start {
            return None;
        }
        breaks(text.text.get(at..entity.utf8_start)?, &mut lines)?;
        lines.last_mut()?.push(custom_emoji_id);
        at = entity.utf8_end;
    }
    breaks(text.text.get(at..)?, &mut lines)?;
    lines.retain(|line| !line.is_empty());
    (!lines.is_empty()).then_some(lines)
}

/// Messages remembered at most; past it the set starts over, and a
/// forgotten message at worst plays once more.
const MAX_PLAYED: usize = 4096;

/// What a lone animated emoji's view does now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmojiPlay {
    /// Play once from the start on the head of this generation (a tap
    /// starts a new one), then rest on the first frame.
    Play { generation: u32 },
    /// It played and its head is gone: show the first frame.
    Rest,
}

/// The lone animated emoji played in the open chat (tdesktop
/// `HistoryInner::_animatedStickersPlayed`) and the replays taps asked for.
#[derive(Debug, Default)]
pub struct EmojiPlays {
    played: HashSet<MessageKey>,
    /// Taps (and the other side's taps) since the chat opened.
    generations: HashMap<MessageKey, u32>,
    /// Replays asked for and not started yet.
    replays: HashSet<MessageKey>,
}

impl EmojiPlays {
    /// The play of `key` its view is on: 0, plus one for each replay.
    pub fn generation(&self, key: MessageKey) -> u32 {
        self.generations.get(&key).copied().unwrap_or(0)
    }

    /// What the view of `key` does; `head_alive`: the head of its current
    /// generation is still known to the player (it may be mid-play).
    pub fn play(&self, key: MessageKey, head_alive: bool) -> EmojiPlay {
        let generation = self.generation(key);
        if head_alive || !self.played.contains(&key) || self.replays.contains(&key) {
            EmojiPlay::Play { generation }
        } else {
            EmojiPlay::Rest
        }
    }

    /// The view of `key` showed its first frame: it counts as played.
    pub fn started(&mut self, key: MessageKey) {
        self.replays.remove(&key);
        if self.played.len() >= MAX_PLAYED && !self.played.contains(&key) {
            self.played.clear();
        }
        self.played.insert(key);
    }

    /// Whether `key` played since the chat opened.
    pub fn has_played(&self, key: MessageKey) -> bool {
        self.played.contains(&key)
    }

    /// A tap, or an interaction effect starting on it: play `key` again
    /// from the start.
    pub fn replay(&mut self, key: MessageKey) {
        if self.generations.len() >= MAX_PLAYED && !self.generations.contains_key(&key) {
            self.generations.clear();
        }
        let generation = self.generations.entry(key).or_insert(0);
        *generation = generation.wrapping_add(1);
        self.replays.insert(key);
    }

    /// `chat` was left: its emoji play again next time it shows (tdesktop
    /// `stickerClearLoopPlayed` when its `HistoryInner` goes). Those of
    /// chats other windows show stay as they are.
    pub fn leave_chat(&mut self, chat: i64) {
        self.played.retain(|key| key.0 != chat);
        self.generations.retain(|key, _| key.0 != chat);
        self.replays.retain(|key| key.0 != chat);
    }

    /// Every chat was left (another account, the loop setting changed):
    /// everything plays again.
    pub fn clear(&mut self) {
        self.played.clear();
        self.generations.clear();
        self.replays.clear();
    }
}

#[cfg(test)]
mod tests;
