//! The notification windows' fade hold and tick clock.
#![deny(clippy::indexing_slicing)]

use std::time::{Duration, Instant};

use super::{FRAME, INPUT_CHECK_MS, NotifyWindows};

/// Why the window has not started its slow fade. It stays up until the
/// user has touched the computer after it appeared (`notify_check_input`),
/// then lingers until `hide_at`; the pointer leaving the stack can bring
/// that time forward at any point. The time is never cleared while the
/// window is up; whether it fades now is derived from it and the hold
/// (`notify_settle_hiding`).
#[derive(Clone, Copy)]
pub(super) enum Hold {
    /// Nobody has touched the computer yet; `Some` once the pointer left.
    WaitingForInput { hide_at: Option<Instant> },
    /// The slow fade is due at this time.
    Lingering { hide_at: Instant },
}

impl Hold {
    pub(super) fn hide_at(self) -> Option<Instant> {
        match self {
            Self::WaitingForInput { hide_at } => hide_at,
            Self::Lingering { hide_at } => Some(hide_at),
        }
    }

    pub(super) fn waiting(self) -> bool {
        matches!(self, Self::WaitingForInput { .. })
    }

    /// The fade is due no later than `due`.
    pub(super) fn due_by(self, due: Instant) -> Self {
        let sooner = |at: Option<Instant>| at.map_or(due, |at| at.min(due));
        match self {
            Self::WaitingForInput { hide_at } => Self::WaitingForInput {
                hide_at: Some(sooner(hide_at)),
            },
            Self::Lingering { hide_at } => Self::Lingering {
                hide_at: sooner(Some(hide_at)),
            },
        }
    }

    /// The user is back: the linger starts, due no later than `due`.
    pub(super) fn input_back(self, due: Instant) -> Self {
        Self::Lingering {
            hide_at: self.hide_at().map_or(due, |at| at.min(due)),
        }
    }
}

/// The tick that moves the windows: stopped, or the task that runs it and
/// the instant it wakes at. The tick stops itself when nothing is up;
/// dropping the windows drops it.
pub(super) enum Clock {
    Stopped,
    Running {
        due: std::time::Instant,
        _task: gpui_kit::Task<()>,
    },
}

impl NotifyWindows {
    /// When the clock steps next once nothing animates; stops it when no
    /// popup waits for the user and no fade is due.
    pub(super) fn next_step(&mut self, now: Instant) -> Option<Duration> {
        let windows = self;
        // Held, nothing is due: the hold ends on an event, which settles.
        let deadline = (!windows.held())
            .then(|| {
                windows
                    .shown
                    .iter()
                    .filter(|s| !s.unlinked() && !s.popup.hiding())
                    .filter_map(|s| s.hold.hide_at())
                    .min()
            })
            .flatten()
            .map(|at| at.saturating_duration_since(now));
        let waiting = windows
            .shown
            .iter()
            .any(|s| s.hold.waiting() && !s.unlinked());
        let next = match (deadline, waiting) {
            (Some(deadline), true) => Some(deadline.min(Duration::from_millis(INPUT_CHECK_MS))),
            (Some(deadline), false) => Some(deadline),
            (None, true) => Some(Duration::from_millis(INPUT_CHECK_MS)),
            (None, false) => None,
        };
        match next {
            Some(delay) => Some(delay.max(FRAME)),
            None => {
                windows.clock = Clock::Stopped;
                None
            }
        }
    }
}
