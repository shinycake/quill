//! Where a player is in its life: awake and maybe waiting for a picture,
//! failed with an error not yet reported, or asleep (its backend dropped).
//! One value, so a sleeping player waits for nothing and an error is
//! reported once whatever else happens.
#![deny(clippy::indexing_slicing)]

use super::PlayerError;
use std::time::Instant;

pub(super) enum Life {
    /// `awaiting`: a new picture is wanted since then (opened, seeked,
    /// stepped); the element keeps drawing until it comes, even paused.
    Awake { awaiting: Option<Instant> },
    /// The backend failed; `error` is kept until it is reported.
    Failed { error: Option<PlayerError> },
    /// The backend was dropped by `VideoPlayer::hibernate`; a pending
    /// error stays to be reported.
    Asleep { error: Option<PlayerError> },
}

impl Life {
    pub(super) fn awake(awaiting: Option<Instant>) -> Self {
        Self::Awake { awaiting }
    }

    pub(super) fn is_asleep(&self) -> bool {
        matches!(self, Self::Asleep { .. })
    }

    /// Since when a new picture is wanted, if it is.
    pub(super) fn awaiting(&self) -> Option<Instant> {
        match self {
            Self::Awake { awaiting } => *awaiting,
            Self::Failed { .. } | Self::Asleep { .. } => None,
        }
    }

    /// A new picture is wanted from `at` (an awake player only).
    pub(super) fn await_picture(&mut self, at: Instant) {
        if let Self::Awake { awaiting } = self {
            *awaiting = Some(at);
        }
    }

    /// The wanted picture came.
    pub(super) fn picture_came(&mut self) {
        if let Self::Awake { awaiting } = self {
            *awaiting = None;
        }
    }

    pub(super) fn sleep(&mut self) {
        *self = Self::Asleep {
            error: self.take_error(),
        };
    }

    /// The backend failed with `error`.
    pub(super) fn fail(&mut self, error: PlayerError) {
        match self {
            Self::Asleep { error: kept } => *kept = Some(error),
            Self::Awake { .. } | Self::Failed { .. } => {
                *self = Self::Failed { error: Some(error) };
            }
        }
    }

    /// The error, once.
    pub(super) fn take_error(&mut self) -> Option<PlayerError> {
        match self {
            Self::Failed { error } | Self::Asleep { error } => error.take(),
            Self::Awake { .. } => None,
        }
    }
}
