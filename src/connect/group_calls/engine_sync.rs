//! What the call engine last applied for one group call's microphone and
//! camera, and whether the last change it refused waits out its backoff.
//!
//! A refused change is asked again after a growing wait (1 s, 2 s, 4 s, up
//! to 5 s) whoever looks (the periodic poll or a Telegram update: updates
//! of a live call arrive several times a second), not on every look, and at
//! once when the wanted value changes (a user action). The microphone is
//! retried so the engine follows what the screen says; the wait only keeps
//! a failing engine from being driven (and its error rewritten) on every
//! tick or update.
use std::time::{Duration, Instant};

const FIRST_WAIT: Duration = Duration::from_secs(1);
const MAX_WAIT: Duration = Duration::from_secs(5);

/// One setting of the engine: nothing applied yet, applied, or refused.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) enum EngineApply<T> {
    /// Nothing applied since the call joined.
    #[default]
    Unset,
    Applied(T),
    /// The engine refused `wanted`; it is asked again at `retry_at`, or at
    /// once for another value. `wait` is the wait that was applied.
    Refused {
        wanted: T,
        retry_at: Instant,
        wait: Duration,
    },
}

impl<T: PartialEq + Clone> EngineApply<T> {
    /// The engine holds `value` (it applied it, or took it for the connect).
    pub(crate) fn applied(&mut self, value: T) {
        *self = Self::Applied(value);
    }

    /// Whether `wanted` should be handed to the engine at `now`.
    pub(crate) fn due(&self, wanted: &T, now: Instant) -> bool {
        match self {
            Self::Unset => true,
            Self::Applied(held) => held != wanted,
            Self::Refused {
                wanted: refused,
                retry_at,
                ..
            } => refused != wanted || now >= *retry_at,
        }
    }

    /// The engine refused `wanted` at `now`: the wait doubles while the
    /// same value keeps being refused.
    pub(crate) fn refused(&mut self, wanted: T, now: Instant) {
        let wait = match self {
            Self::Refused {
                wanted: refused,
                wait,
                ..
            } if *refused == wanted => wait.saturating_mul(2).min(MAX_WAIT),
            _ => FIRST_WAIT,
        };
        *self = Self::Refused {
            wanted,
            retry_at: now.checked_add(wait).unwrap_or(now),
            wait,
        };
    }
}

/// The microphone and the camera of one group call.
#[derive(Debug, Clone, Default)]
pub(crate) struct GroupEngineSync {
    pub(crate) mute: EngineApply<bool>,
    /// The camera device (`None`: off).
    pub(crate) camera: EngineApply<Option<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refused_change_waits_out_its_backoff_and_a_new_value_goes_at_once() {
        let now = Instant::now();
        let poll = |after: Duration| now + after;
        let mut mute = EngineApply::<bool>::Unset;
        assert!(mute.due(&true, poll(Duration::ZERO)));
        mute.refused(true, now);
        assert!(
            !mute.due(&true, poll(Duration::ZERO)),
            "not on the next poll"
        );
        assert!(
            mute.due(&false, poll(Duration::ZERO)),
            "another value goes at once"
        );
        assert!(
            !mute.due(&true, now),
            "an update inside the backoff does not ask again"
        );
        assert!(mute.due(&true, poll(FIRST_WAIT)));
        mute.refused(true, now + FIRST_WAIT);
        assert!(!mute.due(&true, poll(FIRST_WAIT * 2)));
        assert!(mute.due(&true, poll(FIRST_WAIT * 3)));
    }

    #[test]
    fn the_wait_stops_growing_and_an_applied_value_is_not_asked_again() {
        let now = Instant::now();
        let mut mute = EngineApply::<bool>::Unset;
        for _ in 0..10 {
            mute.refused(true, now);
        }
        assert!(mute.due(&true, now + MAX_WAIT));
        assert!(!mute.due(&true, now + MAX_WAIT / 2));
        mute.applied(true);
        assert!(!mute.due(&true, now));
        assert!(mute.due(&false, now));
    }
}
