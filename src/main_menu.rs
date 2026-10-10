//! Main menu rules that need no UI (tdesktop `window/window_main_menu.cpp`).
//!
//! tdesktop's menu opens with My Profile, then New Group / New Channel,
//! Contacts, Calls, Saved Messages, Settings and a Night Mode switch.
//! The switch flips between the day and night theme in one step, which is
//! what `night_mode_toggle` decides from the theme in effect.

use crate::settings::{AutoNight, ThemeChoice};

/// What the Night Mode switch sets when it is flipped: the opposite of
/// the theme in effect, with the schedule turned off so the choice sticks
/// (tdesktop `ToggleNightMode` keeps the chosen palette the same way).
pub fn night_mode_toggle(dark_now: bool) -> (ThemeChoice, AutoNight) {
    (
        if dark_now {
            ThemeChoice::Light
        } else {
            ThemeChoice::Dark
        },
        AutoNight::Off,
    )
}

/// Whether the switch shows as on: a dark theme is in effect, whether it
/// was chosen or came from the schedule.
pub fn night_mode_on(applied_dark: Option<bool>, choice: ThemeChoice) -> bool {
    applied_dark.unwrap_or(matches!(
        choice,
        ThemeChoice::Dark | ThemeChoice::HighContrast
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_flips_and_pins_the_theme() {
        assert_eq!(
            night_mode_toggle(false),
            (ThemeChoice::Dark, AutoNight::Off)
        );
        assert_eq!(
            night_mode_toggle(true),
            (ThemeChoice::Light, AutoNight::Off)
        );
    }

    #[test]
    fn applied_theme_wins_over_the_stored_choice() {
        assert!(night_mode_on(Some(true), ThemeChoice::Light));
        assert!(!night_mode_on(Some(false), ThemeChoice::Dark));
        assert!(night_mode_on(None, ThemeChoice::HighContrast));
        assert!(!night_mode_on(None, ThemeChoice::Light));
    }
}
