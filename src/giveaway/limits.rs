//! The limits of the giveaway form that TDLib reports as options
//! (`giveaway_additional_chat_count_max`, `giveaway_country_count_max`),
//! with tdesktop's fallbacks until they arrive. tdesktop uses the same
//! "add peers" limit for extra chats and for members to award.
use super::{ADDITIONAL_CHATS_MAX, COUNTRIES_MAX};

/// Option names (`schema/td_api.tl`, `giveawayParameters`).
pub const CHATS_OPTION: &str = "giveaway_additional_chat_count_max";
pub const COUNTRIES_OPTION: &str = "giveaway_country_count_max";

/// How many extra chats or members and countries a giveaway may name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GiveawayLimits {
    /// Extra chats to join, and members to award.
    pub chats_max: usize,
    pub countries_max: usize,
}

impl Default for GiveawayLimits {
    fn default() -> Self {
        Self {
            chats_max: ADDITIONAL_CHATS_MAX,
            countries_max: COUNTRIES_MAX,
        }
    }
}

impl GiveawayLimits {
    /// Takes an `updateOption` value; other options and non-positive
    /// values are ignored.
    pub fn apply_option(&mut self, name: &str, value: i64) {
        let Ok(value) = usize::try_from(value) else {
            return;
        };
        if value == 0 {
            return;
        }
        match name {
            CHATS_OPTION => self.chats_max = value,
            COUNTRIES_OPTION => self.countries_max = value,
            _ => {}
        }
    }
}
