//! The Edit box's manage rows (tdesktop `Controller::fillManageSection`,
//! `boxes/peers/edit_peer_info_box.cpp`): the value beside a row, the
//! auto-translate boost lock, the signatures pair and the history
//! visibility texts. Pure, so the rules are tested without a window.
#![deny(clippy::indexing_slicing)]

use crate::lang::{Key, keys};
use crate::telegram::envelope::{ChatAvailableReactions, ParsedBoostFeatures, ReactionType};

/// The Reactions row's value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReactionsValue {
    /// Every reaction (`lng_manage_peer_reactions_on`).
    All,
    /// None (`lng_manage_peer_reactions_off`).
    Off,
    /// How many are allowed.
    Count(usize),
}

impl ReactionsValue {
    /// tdesktop counts the chosen emoji; the paid reaction alone counts as
    /// one.
    pub fn of(setting: &ChatAvailableReactions) -> Self {
        match setting {
            ChatAvailableReactions::All { .. } => Self::All,
            ChatAvailableReactions::Some { reactions, .. } => {
                let some = reactions
                    .iter()
                    .filter(|reaction| **reaction != ReactionType::Paid)
                    .count();
                if some > 0 {
                    Self::Count(some)
                } else if reactions.contains(&ReactionType::Paid) {
                    Self::Count(1)
                } else {
                    Self::Off
                }
            }
        }
    }
}

/// tdesktop `ToPositiveNumberString`: a row's count shows only above zero.
pub fn positive_count(count: i32) -> Option<String> {
    (count > 0).then(|| count.to_string())
}

/// The first boost level whose features enable auto-translation
/// (`chatBoostLevelFeatures.can_enable_automatic_translation`); `None`
/// until TDLib sent the level table.
pub fn auto_translate_level(features: Option<&ParsedBoostFeatures>) -> Option<i32> {
    features?
        .levels
        .iter()
        .find(|level| level.can_enable_automatic_translation)
        .map(|level| level.level)
}

/// The auto-translate switch of a channel (tdesktop
/// `fillAutoTranslateButton`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoTranslate {
    /// The switch as the box shows it.
    pub on: bool,
    /// The channel's boost level (`chatBoostStatus.level`).
    pub level: Option<i32>,
    /// The level auto-translation needs.
    pub required: Option<i32>,
}

/// What a click on the auto-translate switch does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoTranslateClick {
    /// Flip the switch.
    Toggle(bool),
    /// The level is too low: show the boost box for this level.
    AskBoost(i32),
    /// The level or the requirement isn't known yet: ask TDLib, change
    /// nothing.
    Check,
}

impl AutoTranslate {
    /// Locked while the level is below the requirement, or either is
    /// unknown.
    pub fn locked(&self) -> bool {
        match (self.level, self.required) {
            (Some(level), Some(required)) => level < required,
            _ => true,
        }
    }

    /// A locked switch still turns off; turning it on asks for boosts.
    pub fn click(&self) -> AutoTranslateClick {
        if self.on || !self.locked() {
            return AutoTranslateClick::Toggle(!self.on);
        }
        match self.required {
            Some(required) if self.level.is_some() => AutoTranslateClick::AskBoost(required),
            _ => AutoTranslateClick::Check,
        }
    }
}

/// "Sign messages" and "Show authors' profiles" (tdesktop
/// `fillSignaturesButton`): profiles exist only while signing is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Signatures {
    pub sign: bool,
    pub profiles: bool,
}

impl Signatures {
    pub fn new(sign: bool, profiles: bool) -> Self {
        Self {
            sign,
            profiles: sign && profiles,
        }
    }

    pub fn toggle_sign(self) -> Self {
        Self::new(!self.sign, false)
    }

    pub fn toggle_profiles(self) -> Self {
        Self::new(self.sign, !self.profiles)
    }

    /// The "Show authors' profiles" row is there only while signing.
    pub fn shows_profiles(self) -> bool {
        self.sign
    }

    /// The divider under the pair.
    pub fn about(self) -> Key {
        if self.sign {
            keys::EDIT_SIGN_PROFILES_ABOUT
        } else {
            keys::EDIT_SIGN_MESSAGES_ABOUT
        }
    }
}

/// The value of the "Chat history for new members" row.
pub fn history_value(visible: bool) -> Key {
    if visible {
        keys::MANAGE_HISTORY_VISIBILITY_SHOWN
    } else {
        keys::MANAGE_HISTORY_VISIBILITY_HIDDEN
    }
}

/// The text under "Hidden" in the history box: a basic group keeps the
/// last 100 messages visible (`lng_manage_history_visibility_hidden_legacy`).
pub fn history_hidden_about(basic_group: bool) -> Key {
    if basic_group {
        keys::MANAGE_HISTORY_VISIBILITY_HIDDEN_LEGACY
    } else {
        keys::MANAGE_HISTORY_VISIBILITY_HIDDEN_ABOUT
    }
}

/// tdesktop `AddReactionsText`: the channel's custom emoji reactions
/// above what its level allows need a higher level. Returns the count of
/// custom reactions when the level text shows.
pub fn custom_reactions_over_level(
    setting: Option<&ChatAvailableReactions>,
    allowed: Option<i32>,
) -> Option<usize> {
    let Some(ChatAvailableReactions::Some { reactions, .. }) = setting else {
        return None;
    };
    let custom = reactions
        .iter()
        .filter(|reaction| matches!(reaction, ReactionType::CustomEmoji { .. }))
        .count();
    let allowed = usize::try_from(allowed?.max(0)).unwrap_or(0);
    (custom > allowed).then_some(custom)
}

/// How many custom emoji reactions a channel at `level` may use (the
/// level table's `custom_emoji_reaction_count` for the highest level not
/// above it).
pub fn custom_reactions_allowed(features: Option<&ParsedBoostFeatures>, level: i32) -> Option<i32> {
    let features = features?;
    Some(
        features
            .levels
            .iter()
            .filter(|row| row.level <= level)
            .map(|row| row.custom_emoji_reaction_count)
            .max()
            .unwrap_or(0),
    )
}

#[cfg(test)]
#[path = "group_manage_tests.rs"]
mod tests;
