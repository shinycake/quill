//! Live stories: the rules the viewer shares with the driver, ported from
//! Telegram Desktop (`calls_group_messages.cpp`,
//! `calls_group_stars_coloring.cpp`, `media_stories_controller.cpp`,
//! `history_view_compose_controls.cpp`).
//!
//! - Paid comment levels (tdesktop "colorings"): what a comment of N Stars
//!   gets (pin time, text and emoji limits, colors). TDLib sends them with
//!   `updateGroupCallMessageLevels`; until then the table tdesktop falls
//!   back to is used.
//! - Which comments stay: at most 100, and an older one leaves only once
//!   its pin time is over (`checkDestroying`).
//! - The comments toggle state (`CommentsHas` / `CommentsState`).
//! - The pending Star reaction: taps add up for 5.5 seconds after the last
//!   one (`kPaidAccumulatePeriod`), then they are committed.
#![deny(clippy::indexing_slicing)]

use crate::telegram::envelope::{GroupCallMessageLevel, MessageSender, ParsedGroupCallMessage};

/// tdesktop `kMaxShownVideoStreamMessages`.
pub const MAX_SHOWN_COMMENTS: usize = 100;
/// tdesktop `kMaxTopPaidShown`: crowns for the first three donors.
pub const MAX_TOP_SHOWN: usize = 3;
/// The Stars box starts on this many (tdesktop `kDefaultStars`).
pub const DEFAULT_STARS: i64 = 10;
/// The cap while `paid_group_call_message_star_count_max` is unknown
/// (tdesktop `kMaxStarsFallback`).
pub const FALLBACK_MAX_STARS: i64 = 10_000;
/// A left click on the Stars button sends one Star.
pub const QUICK_STARS: i64 = 1;

/// tdesktop's fallback levels (`AppConfig::groupCallColorings`), lowest
/// first: bgLight, bgDark, from Stars, pin seconds, characters, emoji.
pub fn fallback_levels() -> Vec<GroupCallMessageLevel> {
    [
        (0x955CDB, 0x49079B, 0, 30, 30, 0),
        (0x955CDB, 0x49079B, 10, 60, 60, 1),
        (0x46A3EB, 0x00508E, 50, 120, 80, 2),
        (0x40A920, 0x176200, 100, 300, 110, 3),
        (0xE29A09, 0x9A3E00, 250, 600, 150, 4),
        (0xED771E, 0x9B3100, 500, 900, 200, 7),
        (0xE14542, 0x8B0503, 2_000, 1800, 280, 10),
        (0x596473, 0x252C36, 10_000, 3600, 400, 20),
    ]
    .into_iter()
    .map(
        |(light, dark, stars, pin, characters, emoji)| GroupCallMessageLevel {
            min_star_count: stars,
            pin_duration: pin,
            max_text_length: characters,
            max_custom_emoji_count: emoji,
            first_color: light,
            second_color: dark,
            background_color: dark,
        },
    )
    .collect()
}

/// TDLib's levels (highest first) as tdesktop keeps them: lowest first,
/// with a level for 0 Stars. An empty list falls back to tdesktop's table.
pub fn normalize_levels(mut levels: Vec<GroupCallMessageLevel>) -> Vec<GroupCallMessageLevel> {
    if levels.is_empty() {
        return fallback_levels();
    }
    levels.sort_by_key(|level| level.min_star_count);
    if levels.first().is_some_and(|level| level.min_star_count > 0) {
        levels.insert(0, FREE_LEVEL);
    }
    levels
}

/// The level a comment of `stars` gets (tdesktop `StarsColoringForCount`):
/// the last one it reaches.
pub fn level_for_stars(levels: &[GroupCallMessageLevel], stars: i64) -> GroupCallMessageLevel {
    let pick = |levels: &[GroupCallMessageLevel]| {
        levels
            .iter()
            .take_while(|level| level.min_star_count <= stars)
            .last()
            .or_else(|| levels.first())
            .copied()
    };
    pick(levels)
        .or_else(|| pick(&fallback_levels()))
        .unwrap_or(FREE_LEVEL)
}

/// tdesktop's level for free comments.
const FREE_LEVEL: GroupCallMessageLevel = GroupCallMessageLevel {
    min_star_count: 0,
    pin_duration: 30,
    max_text_length: 30,
    max_custom_emoji_count: 0,
    first_color: 0x955CDB,
    second_color: 0x49079B,
    background_color: 0x49079B,
};

/// Emoji in `text`, each flag, skin tone or ZWJ sequence counted once.
fn emoji_count(text: &str) -> i32 {
    use unicode_segmentation::UnicodeSegmentation;
    let count = text
        .graphemes(true)
        .filter(|grapheme| crate::emoji_catalog::big_emoji_count(grapheme) == Some(1))
        .count();
    i32::try_from(count).unwrap_or(i32::MAX)
}

/// The fewest Stars that let `text` through (tdesktop
/// `StarsRequiredForMessage`): the first level whose character and emoji
/// limits it fits. Length is counted in UTF-16 units, as Qt does.
pub fn stars_required(levels: &[GroupCallMessageLevel], text: &str) -> i64 {
    let length = i32::try_from(text.encode_utf16().count()).unwrap_or(i32::MAX);
    let emoji = emoji_count(text);
    levels
        .iter()
        .find(|level| emoji <= level.max_custom_emoji_count && length <= level.max_text_length)
        .map(|level| level.min_star_count)
        .unwrap_or_else(|| {
            levels
                .last()
                .map_or(0, |level| level.min_star_count.saturating_add(1))
        })
}

/// When a paid comment stops being pinned (tdesktop `PinFinishDate`); 0
/// for a free comment or one not confirmed yet.
pub fn pin_finish(levels: &[GroupCallMessageLevel], date: i32, stars: i64) -> i64 {
    if date == 0 || stars == 0 {
        return 0;
    }
    i64::from(date).saturating_add(i64::from(level_for_stars(levels, stars).pin_duration))
}

/// Live story comments cannot hold line feeds (TDLib `sendGroupCallMessage`).
pub fn flatten_comment(text: &str) -> String {
    text.trim()
        .chars()
        .map(|ch| if ch == '\n' || ch == '\r' { ' ' } else { ch })
        .collect()
}

/// tdesktop `checkDestroying` for a video stream: past 100 comments, the
/// oldest ones leave, but only once their pin time is over.
pub fn retain_comments(
    messages: &mut Vec<ParsedGroupCallMessage>,
    levels: &[GroupCallMessageLevel],
    now: i64,
) {
    let mut over = messages.len().saturating_sub(MAX_SHOWN_COMMENTS);
    if over == 0 {
        return;
    }
    messages.retain(|message| {
        if over == 0 {
            return true;
        }
        over -= 1;
        let expired = message.date != 0
            && pin_finish(levels, message.date, message.paid_message_star_count) <= now;
        !expired
    });
}

/// Whether the comments toggle has anything to say (tdesktop `CommentsHas`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum CommentsHas {
    #[default]
    None,
    AllRead,
    WithUnread,
}

/// The comments toggle button (tdesktop `CommentsState`): hidden while
/// there are no comments, a dot while new ones arrived hidden.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CommentsState {
    #[default]
    Empty,
    Shown,
    Hidden,
    WithNew,
}

/// tdesktop `Controller::updateVideoStream` / `setCommentsShownToggles`.
#[derive(Debug, Clone, Default)]
pub struct CommentsTracker {
    state: CommentsState,
    has: CommentsHas,
    last_id: i32,
    last_read_id: i32,
}

impl CommentsTracker {
    pub fn state(&self) -> CommentsState {
        self.state
    }

    /// Whether the list is drawn.
    pub fn shown(&self) -> bool {
        matches!(self.state, CommentsState::Shown | CommentsState::Empty)
    }

    /// The comments changed. `me` is who your comments are sent as: your
    /// own never count as new.
    pub fn on_messages(&mut self, messages: &[ParsedGroupCallMessage], me: Option<MessageSender>) {
        let has = if self.shown() {
            if let Some(last) = messages.iter().rev().find(|m| m.message_id > 0) {
                self.last_id = last.message_id;
                self.last_read_id = last.message_id;
            }
            if messages.is_empty() {
                CommentsHas::None
            } else {
                CommentsHas::AllRead
            }
        } else {
            let mut unread = false;
            if let Some(last) = messages.iter().rev().find(|m| Some(m.sender_id) != me) {
                self.last_id = last.message_id;
                unread = last.message_id > self.last_read_id;
            }
            if messages.is_empty() {
                CommentsHas::None
            } else if unread {
                CommentsHas::WithUnread
            } else {
                CommentsHas::AllRead
            }
        };
        self.set_has(has);
    }

    fn set_has(&mut self, has: CommentsHas) {
        if has == self.has {
            return;
        }
        self.has = has;
        self.state = match has {
            CommentsHas::None => CommentsState::Empty,
            CommentsHas::WithUnread => CommentsState::WithNew,
            CommentsHas::AllRead => match self.state {
                CommentsState::Shown | CommentsState::Empty => CommentsState::Shown,
                _ => CommentsState::Hidden,
            },
        };
    }

    /// The toggle button was clicked.
    pub fn toggle(&mut self) {
        if self.state != CommentsState::Shown {
            self.last_read_id = self.last_id;
            if self.has == CommentsHas::WithUnread {
                self.has = CommentsHas::AllRead;
            }
        }
        self.state = if self.state == CommentsState::Shown {
            CommentsState::Hidden
        } else {
            CommentsState::Shown
        };
    }

    /// A pinned comment was clicked: show the list again.
    pub fn show_from_pinned(&mut self) {
        if self.state != CommentsState::Empty {
            self.state = CommentsState::Shown;
        }
    }
}

/// Stars tapped on a live story and not yet committed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingLiveReaction {
    pub stars: i64,
    /// Unix milliseconds.
    pub commit_at_ms: u64,
}

impl PendingLiveReaction {
    /// Adds a tap: the stars add up and the timer starts over.
    pub fn add(pending: Option<Self>, stars: i64, now_ms: u64) -> Self {
        Self {
            stars: pending.map_or(0, |p| p.stars).saturating_add(stars),
            commit_at_ms: now_ms.saturating_add(crate::paid_reaction::ACCUMULATE_MS),
        }
    }

    pub fn due(&self, now_ms: u64) -> bool {
        self.commit_at_ms <= now_ms
    }

    /// Whole seconds on the Undo countdown, never below 1.
    pub fn seconds_left(&self, now_ms: u64) -> u64 {
        self.commit_at_ms
            .saturating_sub(now_ms)
            .div_ceil(1000)
            .max(1)
    }

    /// 0.0 when just tapped, 1.0 when due: the Undo ring.
    pub fn elapsed_fraction(&self, now_ms: u64) -> f32 {
        let left = self.commit_at_ms.saturating_sub(now_ms) as f32;
        (1.0 - left / crate::paid_reaction::ACCUMULATE_MS as f32).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests;
