//! The clocks behind the sign-in countdowns, each one value that its
//! owner drops: the resend and email-reset countdown of the auth step on
//! screen, the flood countdown of the last auth error with its banned
//! box, and the one-second tick that redraws them.
#![deny(clippy::indexing_slicing)]

use gpui_kit::Task;
use quill::state::AuthRequestError;
use quill::telegram::envelope::{AuthorizationState, CodeDelivery, EmailResetState};
use std::time::Instant;

fn seconds(since: Instant) -> i64 {
    i64::try_from(since.elapsed().as_secs()).unwrap_or(0)
}

/// The countdown of the code step or the email-code step: it starts when
/// the step (or its delivery / reset state) first shows and ends with it.
#[derive(Debug, Default)]
pub(super) enum StepClock {
    #[default]
    None,
    Code {
        delivery: CodeDelivery,
        since: Instant,
    },
    Email {
        reset: EmailResetState,
        since: Instant,
    },
}

impl StepClock {
    /// Follow the auth step: a new step, or the same one with a new
    /// delivery, restarts the clock; any other step ends it.
    pub(super) fn follow(&mut self, auth: &AuthorizationState) {
        match auth {
            AuthorizationState::WaitCode { delivery, .. } => {
                if !matches!(self, Self::Code { delivery: seen, .. } if seen == delivery) {
                    *self = Self::Code {
                        delivery: delivery.clone(),
                        since: Instant::now(),
                    };
                }
            }
            AuthorizationState::WaitEmailCode { reset, .. } => {
                if !matches!(self, Self::Email { reset: seen, .. } if seen == reset) {
                    *self = Self::Email {
                        reset: *reset,
                        since: Instant::now(),
                    };
                }
            }
            _ => *self = Self::None,
        }
    }

    /// A new code was asked for: the next `follow` starts the countdown
    /// again.
    pub(super) fn restart_code(&mut self) {
        if matches!(self, Self::Code { .. }) {
            *self = Self::None;
        }
    }

    pub(super) fn code_elapsed(&self) -> i64 {
        match self {
            Self::Code { since, .. } => seconds(*since),
            _ => 0,
        }
    }

    pub(super) fn email_elapsed(&self) -> i64 {
        match self {
            Self::Email { since, .. } => seconds(*since),
            _ => 0,
        }
    }
}

/// The banned-number box of an error: shown until the user says OK.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Banner {
    Showing,
    Dismissed,
}

/// The last auth error on screen, with when it first showed (the flood
/// countdown runs from there) and its box.
#[derive(Debug, Default)]
pub(super) enum ErrorClock {
    #[default]
    None,
    Seen {
        err: AuthRequestError,
        since: Instant,
        banner: Banner,
    },
}

impl ErrorClock {
    /// The error on screen now (or none) and the seconds since it first
    /// showed. A different error starts over, box included.
    pub(super) fn observe(
        &mut self,
        now: Option<AuthRequestError>,
    ) -> Option<(AuthRequestError, i64)> {
        let Some(err) = now else {
            *self = Self::None;
            return None;
        };
        match self {
            Self::Seen {
                err: seen, since, ..
            } if *seen == err => Some((err, seconds(*since))),
            _ => {
                *self = Self::Seen {
                    err,
                    since: Instant::now(),
                    banner: Banner::Showing,
                };
                Some((err, 0))
            }
        }
    }

    /// A new number was sent: the old error is over.
    pub(super) fn clear(&mut self) {
        *self = Self::None;
    }

    pub(super) fn banner_dismissed(&self) -> bool {
        matches!(
            self,
            Self::Seen {
                banner: Banner::Dismissed,
                ..
            }
        )
    }

    pub(super) fn dismiss_banner(&mut self) {
        if let Self::Seen { banner, .. } = self {
            *banner = Banner::Dismissed;
        }
    }
}

/// The one-second redraw while a countdown runs. Holding the task is the
/// armed state: it is dropped when it fires, or with the screen.
#[derive(Default)]
pub(super) enum Ticker {
    #[default]
    Idle,
    Armed {
        /// Held so that dropping the ticker cancels the tick.
        _task: Task<()>,
    },
}

impl Ticker {
    pub(super) fn is_armed(&self) -> bool {
        matches!(self, Self::Armed { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flood() -> AuthRequestError {
        AuthRequestError {
            purpose: quill::state::RequestPurpose::SetPhoneNumber,
            class: quill::telegram::envelope::ErrorClass::PhoneBanned,
            flood_wait_secs: None,
        }
    }

    #[test]
    fn a_new_error_shows_its_banner_again() {
        let mut clock = ErrorClock::default();
        assert_eq!(clock.observe(Some(flood())).map(|(_, s)| s), Some(0));
        clock.dismiss_banner();
        assert!(clock.banner_dismissed());
        // The same error keeps the answer.
        assert!(clock.observe(Some(flood())).is_some());
        assert!(clock.banner_dismissed());
        // Gone, then back: the box returns.
        assert!(clock.observe(None).is_none());
        assert!(!clock.banner_dismissed());
        assert!(clock.observe(Some(flood())).is_some());
        assert!(!clock.banner_dismissed());
    }

    #[test]
    fn leaving_the_code_step_ends_its_clock() {
        let mut clock = StepClock::default();
        clock.follow(&AuthorizationState::WaitPhoneNumber);
        assert!(matches!(clock, StepClock::None));
        clock.restart_code();
        assert!(matches!(clock, StepClock::None));
    }
}
