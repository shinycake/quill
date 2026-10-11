//! Giveaways: the form's draft and its checks, and the JSON TDLib wants
//! (tdesktop `info/channel_statistics/boosts/create_giveaway_box.cpp`).
//! Pure, so it is tested without a window.
//!
//! Quill starts prepaid giveaways (`launchPrepaidGiveaway`). Paying for a
//! new one needs the card checkout, which Quill only has for invoice
//! messages, so a new giveaway stops at the price.

mod about;

pub use about::{AboutText, GiveawayTextContext, about_text};

use crate::telegram::envelope::{
    GiveawayPrize, ParsedPrepaidGiveaway, PremiumGiveawayOption, StarGiveawayOption,
};
use serde_json::{Value, json};

/// Longest additional prize text (tdesktop `kAdditionalPrizeLengthMax`).
pub const PRIZE_DESCRIPTION_MAX: usize = 128;
/// Other chats users must join (tdesktop `giveawayAddPeersMax` fallback).
pub const ADDITIONAL_CHATS_MAX: usize = 10;
/// Longest wait before winners are picked (tdesktop `giveawayPeriodMax`
/// fallback), in seconds.
pub const PERIOD_MAX_SECS: i32 = 7 * 24 * 3600;
/// Shortest wait before winners are picked (`giveawayParameters` doc).
pub const PERIOD_MIN_SECS: i32 = 60;
/// Days ahead the date starts at (tdesktop `ThreeDaysAfterToday`).
pub const DEFAULT_DAYS_AHEAD: i32 = 3;
/// Boosts per Premium prize (tdesktop `giveaway_boosts_per_premium`
/// fallback).
pub const BOOSTS_PER_PREMIUM: i32 = 4;

/// What the giveaway hands out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GiveawayKind {
    Premium,
    Stars,
}

/// Why the form can't be submitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GiveawayFormError {
    /// No prize option is picked (the options did not load).
    NoOption,
    TooManyChats,
    PrizeTooLong,
    DateTooSoon,
    DateTooLate,
}

impl GiveawayFormError {
    pub fn text(self) -> String {
        match self {
            Self::NoOption => "Choose how many prizes to give away.".into(),
            Self::TooManyChats => {
                format!("You can add up to {ADDITIONAL_CHATS_MAX} groups and channels.")
            }
            Self::PrizeTooLong => {
                format!("The additional prize can be up to {PRIZE_DESCRIPTION_MAX} characters.")
            }
            Self::DateTooSoon => "Pick a date that is at least a minute away.".into(),
            Self::DateTooLate => format!(
                "Winners must be picked within {} days.",
                PERIOD_MAX_SECS / 86_400
            ),
        }
    }
}

/// The giveaway form's fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GiveawayDraft {
    /// The channel or supergroup the giveaway boosts.
    pub boosted_chat_id: i64,
    pub kind: GiveawayKind,
    /// A giveaway already paid for; fixes the prize and the winner count.
    pub prepaid: Option<ParsedPrepaidGiveaway>,
    /// Number of Premium prizes (one of the offered winner counts).
    pub premium_winners: i32,
    /// Months of Premium per prize.
    pub premium_months: i32,
    /// Index into the Stars options.
    pub star_option: usize,
    /// Winners for the chosen Stars option.
    pub star_winners: i32,
    pub additional_chat_ids: Vec<i64>,
    pub only_new_members: bool,
    pub has_public_winners: bool,
    pub prize_description: String,
    /// When winners are picked, Unix seconds.
    pub selection_date: i32,
}

impl GiveawayDraft {
    /// A fresh form for `boosted_chat_id`. `prepaid` is the giveaway being
    /// set up, if the user picked one from the list.
    pub fn new(boosted_chat_id: i64, prepaid: Option<ParsedPrepaidGiveaway>, now: i32) -> Self {
        let kind = match prepaid.as_ref().map(|p| p.prize) {
            Some(GiveawayPrize::Stars { .. }) => GiveawayKind::Stars,
            _ => GiveawayKind::Premium,
        };
        Self {
            boosted_chat_id,
            kind,
            prepaid,
            premium_winners: 0,
            premium_months: 0,
            star_option: 0,
            star_winners: 0,
            additional_chat_ids: Vec::new(),
            only_new_members: false,
            has_public_winners: true,
            prize_description: String::new(),
            selection_date: now.saturating_add(DEFAULT_DAYS_AHEAD * 86_400),
        }
    }

    /// Whole days from `now` to the selection date, rounded up.
    pub fn days_ahead(&self, now: i32) -> i32 {
        let secs = i64::from(self.selection_date) - i64::from(now);
        ((secs + 86_399) / 86_400).clamp(0, i64::from(i32::MAX)) as i32
    }

    /// Put the date `days` days after `now`, within the allowed range.
    pub fn set_days_ahead(&mut self, now: i32, days: i32) {
        let days = days.clamp(1, PERIOD_MAX_SECS / 86_400);
        self.selection_date = now.saturating_add(days * 86_400);
    }

    /// Add or remove an extra chat; refuses a chat past the limit.
    pub fn toggle_chat(&mut self, chat_id: i64) -> Result<(), GiveawayFormError> {
        if let Some(index) = self
            .additional_chat_ids
            .iter()
            .position(|id| *id == chat_id)
        {
            self.additional_chat_ids.remove(index);
            return Ok(());
        }
        if self.additional_chat_ids.len() >= ADDITIONAL_CHATS_MAX {
            return Err(GiveawayFormError::TooManyChats);
        }
        self.additional_chat_ids.push(chat_id);
        Ok(())
    }

    /// Checks everything the form controls. `now` is Unix seconds.
    pub fn validate(&self, now: i32) -> Result<(), GiveawayFormError> {
        if self.prepaid.is_none() {
            let chosen = match self.kind {
                GiveawayKind::Premium => self.premium_winners > 0 && self.premium_months > 0,
                GiveawayKind::Stars => self.star_winners > 0,
            };
            if !chosen {
                return Err(GiveawayFormError::NoOption);
            }
        }
        if self.additional_chat_ids.len() > ADDITIONAL_CHATS_MAX {
            return Err(GiveawayFormError::TooManyChats);
        }
        if self.prize_description.chars().count() > PRIZE_DESCRIPTION_MAX {
            return Err(GiveawayFormError::PrizeTooLong);
        }
        let wait = i64::from(self.selection_date) - i64::from(now);
        if wait < i64::from(PERIOD_MIN_SECS) {
            return Err(GiveawayFormError::DateTooSoon);
        }
        if wait > i64::from(PERIOD_MAX_SECS) {
            return Err(GiveawayFormError::DateTooLate);
        }
        Ok(())
    }

    /// `giveawayParameters` (schema `td_api.tl:5130`). Countries stay
    /// empty: every country can take part.
    pub fn parameters_json(&self) -> Value {
        json!({
            "@type": "giveawayParameters",
            "boosted_chat_id": self.boosted_chat_id,
            "additional_chat_ids": self.additional_chat_ids,
            "winners_selection_date": self.selection_date,
            "only_new_members": self.only_new_members,
            "has_public_winners": self.has_public_winners,
            "country_codes": Vec::<String>::new(),
            "prize_description": self.prize_description.trim(),
        })
    }

    /// Giveaway id, winners and Stars to pass to `launchPrepaidGiveaway`;
    /// `None` when this is not a prepaid giveaway.
    pub fn prepaid_launch(&self) -> Option<(i64, i32, i64)> {
        let prepaid = self.prepaid.as_ref()?;
        let stars = match prepaid.prize {
            GiveawayPrize::Stars { star_count } => star_count,
            GiveawayPrize::Premium { .. } => 0,
        };
        Some((prepaid.id, prepaid.winner_count, stars))
    }

    /// Boosts the chat gets: from the prepaid giveaway, else from the
    /// chosen option.
    pub fn boost_count(&self, stars: &[StarGiveawayOption]) -> i32 {
        if let Some(prepaid) = &self.prepaid {
            return if prepaid.boost_count > 0 {
                prepaid.boost_count
            } else {
                prepaid.winner_count.saturating_mul(BOOSTS_PER_PREMIUM)
            };
        }
        match self.kind {
            GiveawayKind::Premium => self.premium_winners.saturating_mul(BOOSTS_PER_PREMIUM),
            GiveawayKind::Stars => stars
                .get(self.star_option)
                .map_or(0, |option| option.yearly_boost_count),
        }
    }
}

/// Distinct winner counts offered for Premium prizes, ascending.
pub fn premium_winner_counts(options: &[PremiumGiveawayOption]) -> Vec<i32> {
    let mut counts: Vec<i32> = options.iter().map(|o| o.winner_count).collect();
    counts.sort_unstable();
    counts.dedup();
    counts
}

/// The options for `winners` prizes, shortest subscription first.
pub fn premium_options_for(
    options: &[PremiumGiveawayOption],
    winners: i32,
) -> Vec<&PremiumGiveawayOption> {
    let mut found: Vec<&PremiumGiveawayOption> = options
        .iter()
        .filter(|o| o.winner_count == winners)
        .collect();
    found.sort_by_key(|o| o.month_count);
    found
}

/// Keeps the draft's Premium winners and months when still offered, else
/// takes the first offered ones.
pub fn settle_premium(draft: &mut GiveawayDraft, options: &[PremiumGiveawayOption]) {
    let counts = premium_winner_counts(options);
    if !counts.contains(&draft.premium_winners) {
        draft.premium_winners = counts.first().copied().unwrap_or(0);
    }
    let months = premium_options_for(options, draft.premium_winners);
    if !months.iter().any(|o| o.month_count == draft.premium_months) {
        draft.premium_months = months.first().map_or(0, |o| o.month_count);
    }
}

/// Keeps the Stars option and winners when still offered, else takes the
/// default option and its first winner count.
pub fn settle_stars(draft: &mut GiveawayDraft, options: &[StarGiveawayOption]) {
    // Nothing picked yet (no winners): start on the default option.
    if draft.star_winners == 0 || draft.star_option >= options.len() {
        draft.star_option = options
            .iter()
            .position(|o| o.is_default)
            .unwrap_or(0)
            .min(options.len().saturating_sub(1));
    }
    let winners = options
        .get(draft.star_option)
        .map(|o| o.winner_counts.as_slice())
        .unwrap_or_default();
    if !winners.contains(&draft.star_winners) {
        draft.star_winners = winners.first().copied().unwrap_or(0);
    }
}

#[cfg(test)]
mod tests;
