//! Start-in-tray: the window stays hidden only once a tray icon is up.
//!
//! No tray host must never leave the only window inaccessible. The tray
//! registers off-thread, so after launch the tray loop asks each pass what
//! came of it: the tray is up (stay hidden), no host answered or the wait
//! ran out (reveal the window). The stage lives in that loop, so it ends
//! with the window that owns it.
#![deny(clippy::indexing_slicing)]

use std::time::{Duration, Instant};

/// How often the loop looks again while the tray registers.
pub const LOOK_AGAIN: Duration = Duration::from_millis(100);

/// Where the start-in-tray check stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupWatch {
    /// Nothing to decide: the window started shown, or the check ended.
    Idle,
    /// Waiting for the tray host since this instant.
    Watching { since: Instant },
}

/// What one look at the tray decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupStep {
    /// Nothing to do.
    Nothing,
    /// Still registering: look again soon.
    Waiting,
    /// No tray: show the window.
    Reveal,
}

impl StartupWatch {
    /// The check for a launch that `start_in_tray` hid the window for.
    pub fn new(start_in_tray: bool, now: Instant) -> Self {
        if start_in_tray {
            Self::Watching { since: now }
        } else {
            Self::Idle
        }
    }

    /// Looks at the tray at `now`. `Reveal` and a tray that is up end the
    /// check.
    pub fn step(&mut self, available: bool, registering: bool, now: Instant) -> StartupStep {
        let Self::Watching { since } = *self else {
            return StartupStep::Nothing;
        };
        let waited =
            u64::try_from(now.saturating_duration_since(since).as_millis()).unwrap_or(u64::MAX);
        match crate::tray::tray_startup_reveal(available, registering, waited) {
            None => StartupStep::Waiting,
            Some(reveal) => {
                *self = Self::Idle;
                if reveal {
                    StartupStep::Reveal
                } else {
                    StartupStep::Nothing
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tray::TRAY_STARTUP_WAIT_MS;

    #[test]
    fn a_shown_window_has_nothing_to_decide() {
        let now = Instant::now();
        let mut watch = StartupWatch::new(false, now);
        assert_eq!(watch.step(false, false, now), StartupStep::Nothing);
    }

    #[test]
    fn the_window_stays_hidden_once_the_tray_is_up() {
        let start = Instant::now();
        let mut watch = StartupWatch::new(true, start);
        assert_eq!(watch.step(false, true, start), StartupStep::Waiting);
        assert_eq!(watch.step(true, false, start), StartupStep::Nothing);
        assert_eq!(watch, StartupWatch::Idle, "the check ended");
        assert_eq!(
            watch.step(false, false, start),
            StartupStep::Nothing,
            "and does not reveal later"
        );
    }

    #[test]
    fn no_tray_host_reveals_the_window_at_once_or_when_the_wait_runs_out() {
        let start = Instant::now();
        let mut refused = StartupWatch::new(true, start);
        assert_eq!(refused.step(false, false, start), StartupStep::Reveal);
        assert_eq!(refused, StartupWatch::Idle);

        let mut slow = StartupWatch::new(true, start);
        let late = start + Duration::from_millis(TRAY_STARTUP_WAIT_MS - 1);
        assert_eq!(slow.step(false, true, late), StartupStep::Waiting);
        let over = start + Duration::from_millis(TRAY_STARTUP_WAIT_MS);
        assert_eq!(slow.step(false, true, over), StartupStep::Reveal);
    }
}
