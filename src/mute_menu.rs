//! The Mute panel's custom duration. tdesktop `menu/menu_mute.cpp`:
//! the menu lists "Mute for 1 hour / 8 hours / 2 days / Custom...",
//! "Disable sound" (or "Enable sound") and "Mute forever" / "Unmute".
//! Custom opens `MuteBox`, whose `Ui::ChooseTimeWidget` is three number
//! fields (days, hours, minutes); the box starts at 8 hours and its button
//! reads "Unmute" while the total is zero.
#![deny(clippy::indexing_slicing)]

const MINUTE: u32 = 60;
const HOUR: u32 = 60 * MINUTE;
const DAY: u32 = 24 * HOUR;

/// `kMuteDurSecondsDefault`: the box starts on 8 hours.
const DEFAULT_SECS: u32 = 8 * HOUR;

/// `ChooseTimeWidget` field limits (`setMaxValue`): days 0-31, hours
/// 0-23, minutes 0-59.
pub const MAX_DAYS: u32 = 31;
pub const MAX_HOURS: u32 = 23;
pub const MAX_MINUTES: u32 = 59;

/// A custom mute duration being edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CustomMute {
    pub days: u32,
    pub hours: u32,
    pub minutes: u32,
}

impl Default for CustomMute {
    fn default() -> Self {
        Self::from_secs(DEFAULT_SECS)
    }
}

impl CustomMute {
    /// Split `secs` into the three fields (`ChooseTimeWidget`'s start
    /// value); the minutes drop any seconds.
    pub fn from_secs(secs: u32) -> Self {
        Self {
            days: secs / DAY,
            hours: secs % DAY / HOUR,
            minutes: secs % HOUR / MINUTE,
        }
    }

    /// Clamp each field to its limit, as the fields do while typing.
    pub fn clamped(self) -> Self {
        Self {
            days: self.days.min(MAX_DAYS),
            hours: self.hours.min(MAX_HOURS),
            minutes: self.minutes.min(MAX_MINUTES),
        }
    }

    /// The `mute_for` value to send; zero means "Unmute".
    pub fn mute_for(self) -> i32 {
        let secs = self
            .clamped()
            .days
            .saturating_mul(DAY)
            .saturating_add(self.hours.min(MAX_HOURS) * HOUR)
            .saturating_add(self.minutes.min(MAX_MINUTES) * MINUTE);
        i32::try_from(secs).unwrap_or(i32::MAX)
    }
}

/// "Disable sound" / "Enable sound" item: toggles the chat between its
/// default sound and no sound (`sound_id` 0), as tdesktop's mute menu does.
pub fn sound_toggle_label(sound_disabled: bool) -> &'static str {
    if sound_disabled {
        "Enable sound"
    } else {
        "Disable sound"
    }
}

/// "1 hour", "8 hours", "2 days": `Ui::FormatMuteFor`.
pub fn mute_period_text(seconds: i32) -> String {
    let day = 24 * 3600;
    if seconds % day == 0 {
        crate::lang::trn(crate::lang::keys::DAYS, seconds / day)
    } else {
        crate::lang::trn(crate::lang::keys::HOURS, seconds / 3600)
    }
}

/// The `(use_default_sound, sound_id)` the toggle applies.
pub fn sound_toggle_target(sound_disabled: bool) -> (bool, i64) {
    if sound_disabled {
        (true, 0)
    } else {
        (false, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_eight_hours() {
        let c = CustomMute::default();
        assert_eq!(
            c,
            CustomMute {
                days: 0,
                hours: 8,
                minutes: 0
            }
        );
        assert_eq!(c.mute_for(), 8 * 3600);
    }

    #[test]
    fn fields_add_up_and_clamp_like_choose_time() {
        assert_eq!(
            CustomMute {
                days: 2,
                hours: 3,
                minutes: 30
            }
            .mute_for(),
            2 * 86_400 + 3 * 3600 + 30 * 60
        );
        assert_eq!(
            CustomMute {
                days: 99,
                hours: 99,
                minutes: 99
            }
            .mute_for(),
            31 * 86_400 + 23 * 3600 + 59 * 60
        );
    }

    #[test]
    fn zero_means_unmute() {
        assert_eq!(
            CustomMute {
                days: 0,
                hours: 0,
                minutes: 0
            }
            .mute_for(),
            0
        );
    }

    #[test]
    fn from_secs_splits_the_fields() {
        assert_eq!(
            CustomMute::from_secs(90_061),
            CustomMute {
                days: 1,
                hours: 1,
                minutes: 1
            }
        );
    }

    #[test]
    fn sound_toggle() {
        assert_eq!(sound_toggle_label(false), "Disable sound");
        assert_eq!(sound_toggle_target(false), (false, 0));
        assert_eq!(sound_toggle_label(true), "Enable sound");
        assert_eq!(sound_toggle_target(true), (true, 0));
    }
}
