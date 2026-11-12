//! The notification windows' fade hold and tick clock.
#![deny(clippy::indexing_slicing)]

use std::time::Instant;

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
