#![deny(clippy::indexing_slicing)]

use crate::lang::{keys, tr, tr_with, trn};
use crate::state::StoryStealthMode;
use crate::telegram::envelope::OptionValue;

/// Seconds of views stealth mode hides before it is turned on.
pub const PAST_PERIOD_OPTION: &str = "story_stealth_mode_past_period";
/// Seconds of views stealth mode hides after it is turned on.
pub const FUTURE_PERIOD_OPTION: &str = "story_stealth_mode_future_period";

/// The periods tdesktop's strings name.
const DEFAULT_PAST: i64 = 5 * 60;
const DEFAULT_FUTURE: i64 = 25 * 60;

/// How far back and ahead stealth mode hides the account's views, in
/// seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StealthPeriods {
    pub past: i64,
    pub future: i64,
}

impl Default for StealthPeriods {
    fn default() -> Self {
        Self {
            past: DEFAULT_PAST,
            future: DEFAULT_FUTURE,
        }
    }
}

impl StealthPeriods {
    pub fn apply_option(&mut self, name: &str, value: &OptionValue) {
        let OptionValue::Integer(seconds) = value else {
            return;
        };
        if *seconds <= 0 {
            return;
        }
        match name {
            PAST_PERIOD_OPTION => self.past = *seconds,
            FUTURE_PERIOD_OPTION => self.future = *seconds,
            _ => {}
        }
    }

    pub fn past_about(&self) -> String {
        if self.past == DEFAULT_PAST {
            tr(keys::STEALTH_MODE_PAST_ABOUT).to_string()
        } else {
            tr_with(
                keys::Q_STEALTH_PAST_ABOUT,
                &[("duration", &duration_text(self.past))],
            )
        }
    }

    pub fn next_about(&self) -> String {
        if self.future == DEFAULT_FUTURE {
            tr(keys::STEALTH_MODE_NEXT_ABOUT).to_string()
        } else {
            tr_with(
                keys::Q_STEALTH_NEXT_ABOUT,
                &[("duration", &duration_text(self.future))],
            )
        }
    }

    fn enabled_tip(&self) -> String {
        if *self == Self::default() {
            tr(keys::STEALTH_MODE_ENABLED_TIP).to_string()
        } else {
            tr_with(
                keys::Q_STEALTH_ENABLED_TIP,
                &[
                    ("past", &duration_text(self.past)),
                    ("future", &duration_text(self.future)),
                ],
            )
        }
    }
}

fn duration_text(seconds: i64) -> String {
    if seconds >= 3600 && seconds % 3600 == 0 {
        trn(keys::HOURS, seconds / 3600)
    } else if seconds >= 60 && seconds % 60 == 0 {
        trn(keys::MINUTES, seconds / 60)
    } else {
        trn(keys::SECONDS, seconds)
    }
}

/// tdesktop `TimeLeftText`: `1:02:03`, `4:05`, `0:09`.
pub fn time_left_text(left: i64) -> String {
    let left = left.max(0);
    let (hours, minutes, seconds) = (left / 3600, (left % 3600) / 60, left % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else if minutes > 0 {
        format!("{minutes}:{seconds:02}")
    } else {
        format!("0:{seconds:02}")
    }
}

pub fn about_text(premium: bool) -> &'static str {
    if premium {
        tr(keys::STEALTH_MODE_ABOUT)
    } else {
        tr(keys::STEALTH_MODE_UNLOCK_ABOUT)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StealthButton {
    /// Not Premium: "Unlock Stealth Mode" with a lock.
    Unlock,
    Enable,
    /// Opened from a story tile: turning it on also opens the stories.
    EnableAndOpen,
    /// Cooling down: "Available in 4:59", dimmed.
    Cooldown {
        left: i64,
    },
}

impl StealthButton {
    pub fn of(mode: StoryStealthMode, now: i64, premium: bool, opens_stories: bool) -> Self {
        if !premium {
            Self::Unlock
        } else if i64::from(mode.cooldown_until_date) <= now {
            if opens_stories {
                Self::EnableAndOpen
            } else {
                Self::Enable
            }
        } else {
            Self::Cooldown {
                left: (i64::from(mode.cooldown_until_date) - now).max(1),
            }
        }
    }

    pub fn label(&self) -> String {
        match self {
            Self::Unlock => tr(keys::STEALTH_MODE_UNLOCK).to_string(),
            Self::Enable => tr(keys::STEALTH_MODE_ENABLE).to_string(),
            Self::EnableAndOpen => tr(keys::STEALTH_MODE_ENABLE_AND_OPEN).to_string(),
            Self::Cooldown { left } => tr_with(
                keys::STEALTH_MODE_COOLDOWN_IN,
                &[("left", &time_left_text(*left))],
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StealthPress {
    /// Already on: the "Stealth Mode On" toast, and the box closes.
    EnabledToast,
    /// Not Premium: the Premium preview of upgraded stories; the box stays.
    PremiumPreview,
    /// Cooling down: the cooldown toast, and the box closes.
    CooldownToast,
    /// Send `activateStoryStealthMode`.
    Activate,
    /// An activation is already on its way.
    Wait,
}

pub fn press(mode: StoryStealthMode, now: i64, premium: bool, requested: bool) -> StealthPress {
    if mode.is_active(now) {
        StealthPress::EnabledToast
    } else if !premium {
        StealthPress::PremiumPreview
    } else if i64::from(mode.cooldown_until_date) > now {
        StealthPress::CooldownToast
    } else if requested {
        StealthPress::Wait
    } else {
        StealthPress::Activate
    }
}

/// Seconds stealth mode stays on, or `None` when it is off: an entry point
/// then shows the "You are in Stealth Mode" toast instead of the box
/// (tdesktop `SetupStealthMode`).
pub fn active_left(mode: StoryStealthMode, now: i64) -> Option<i64> {
    let left = i64::from(mode.active_until_date) - now;
    (left > 0).then_some(left)
}

/// A Stealth Mode menu entry; `locked` puts a lock on its icon for an
/// account that cannot turn stealth mode on (tdesktop `menuIconStealthLocked`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StealthEntry {
    pub locked: bool,
}

impl StealthEntry {
    pub fn of(premium: bool, mode: StoryStealthMode, now: i64) -> Self {
        Self {
            locked: !premium && !mode.is_active(now),
        }
    }
}

/// One of the box's toasts: an optional bold title over text with
/// `**bold**` marks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StealthToast {
    pub title: Option<&'static str>,
    pub text: String,
}

impl StealthToast {
    pub fn activated(periods: StealthPeriods) -> Self {
        Self {
            title: Some(tr(keys::STEALTH_MODE_ENABLED_TIP_TITLE)),
            text: periods.enabled_tip(),
        }
    }

    pub fn already(left: i64) -> Self {
        Self {
            title: Some(tr(keys::STEALTH_MODE_ALREADY_TITLE)),
            text: tr_with(
                keys::STEALTH_MODE_ALREADY_ABOUT,
                &[("left", &time_left_text(left))],
            ),
        }
    }

    pub fn cooldown() -> Self {
        Self {
            title: None,
            text: tr(keys::STEALTH_MODE_COOLDOWN_TIP).to_string(),
        }
    }

    /// The text without its `**` marks, for a plain status note.
    pub fn plain_text(&self) -> String {
        self.text.replace("**", "")
    }
}

#[cfg(test)]
#[path = "story_stealth_tests.rs"]
mod tests;
