//! The Mute submenu's custom duration. tdesktop `menu/menu_mute.cpp`:
//! the menu lists "Mute for 1 hour / 8 hours / 2 days / Custom...",
//! "Disable sound" (or "Enable sound") and "Mute forever" / "Unmute".
//! Custom opens a time picker (`Ui::ChooseTimeWidget`) of days and hours
//! that is applied as `mute_for` seconds.

use crate::telegram::envelope::MUTE_FOREVER;

const HOUR: i64 = 3600;
const DAY: i64 = 24 * HOUR;
/// tdesktop clamps the custom duration below a year; anything longer is
/// "Forever".
const MAX_CUSTOM_SECS: i64 = 366 * DAY;

/// A custom mute duration being edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CustomMute {
    pub days: u32,
    pub hours: u32,
}

impl Default for CustomMute {
    /// `ChooseTimeWidget` starts on one day.
    fn default() -> Self {
        Self { days: 1, hours: 0 }
    }
}

impl CustomMute {
    pub fn total_secs(self) -> i64 {
        i64::from(self.days) * DAY + i64::from(self.hours) * HOUR
    }

    /// The `mute_for` value to send: at least one hour, and a duration
    /// past a year becomes [`MUTE_FOREVER`] (what TDLib treats as
    /// "forever" is anything above 366 days).
    pub fn mute_for(self) -> i32 {
        let secs = self.total_secs().max(HOUR);
        if secs > MAX_CUSTOM_SECS {
            MUTE_FOREVER
        } else {
            secs as i32
        }
    }

    pub fn step_days(self, delta: i32) -> Self {
        let days = (i64::from(self.days) + i64::from(delta)).clamp(0, 366) as u32;
        Self { days, ..self }.floor_one_hour()
    }

    pub fn step_hours(self, delta: i32) -> Self {
        // Hours wrap into days like a clock face would not: carry instead.
        let mut total = self.total_secs() / HOUR + i64::from(delta);
        total = total.clamp(0, 366 * 24);
        Self {
            days: (total / 24) as u32,
            hours: (total % 24) as u32,
        }
        .floor_one_hour()
    }

    fn floor_one_hour(self) -> Self {
        if self.total_secs() < HOUR {
            Self { days: 0, hours: 1 }
        } else {
            self
        }
    }

    /// "1 day", "2 days 3 hours", "5 hours".
    pub fn label(self) -> String {
        let plural = |n: u32, unit: &str| {
            if n == 1 {
                format!("1 {unit}")
            } else {
                format!("{n} {unit}s")
            }
        };
        match (self.days, self.hours) {
            (0, h) => plural(h, "hour"),
            (d, 0) => plural(d, "day"),
            (d, h) => format!("{} {}", plural(d, "day"), plural(h, "hour")),
        }
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
    fn default_is_one_day() {
        let c = CustomMute::default();
        assert_eq!(c.mute_for(), 86_400);
        assert_eq!(c.label(), "1 day");
    }

    #[test]
    fn mute_for_has_a_one_hour_floor_and_forever_ceiling() {
        assert_eq!(CustomMute { days: 0, hours: 0 }.mute_for(), 3600);
        assert_eq!(
            CustomMute { days: 2, hours: 3 }.mute_for(),
            2 * 86_400 + 3 * 3600
        );
        assert_eq!(
            CustomMute {
                days: 366,
                hours: 0
            }
            .mute_for(),
            366 * 86_400
        );
        assert_eq!(
            CustomMute {
                days: 367,
                hours: 0
            }
            .mute_for(),
            MUTE_FOREVER
        );
    }

    #[test]
    fn stepping_carries_hours_into_days_and_never_goes_below_an_hour() {
        let c = CustomMute { days: 1, hours: 23 };
        assert_eq!(c.step_hours(1), CustomMute { days: 2, hours: 0 });
        assert_eq!(
            CustomMute { days: 1, hours: 0 }.step_hours(-1),
            CustomMute { days: 0, hours: 23 }
        );
        assert_eq!(
            CustomMute { days: 0, hours: 1 }.step_hours(-1),
            CustomMute { days: 0, hours: 1 }
        );
        assert_eq!(
            CustomMute { days: 1, hours: 0 }.step_days(-5),
            CustomMute { days: 0, hours: 1 }
        );
        assert_eq!(
            CustomMute {
                days: 366,
                hours: 0
            }
            .step_days(5)
            .days,
            366
        );
    }

    #[test]
    fn labels() {
        assert_eq!(CustomMute { days: 2, hours: 3 }.label(), "2 days 3 hours");
        assert_eq!(CustomMute { days: 0, hours: 5 }.label(), "5 hours");
        assert_eq!(CustomMute { days: 0, hours: 1 }.label(), "1 hour");
    }

    #[test]
    fn sound_toggle() {
        assert_eq!(sound_toggle_label(false), "Disable sound");
        assert_eq!(sound_toggle_target(false), (false, 0));
        assert_eq!(sound_toggle_label(true), "Enable sound");
        assert_eq!(sound_toggle_target(true), (true, 0));
    }
}
