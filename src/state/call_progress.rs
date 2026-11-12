//! How far a live 1:1 call got, with TDLib's last word on it.
use crate::telegram::envelope::{CallState, ReadyParams};
use std::time::Instant;

/// What `callStateReady` brought, and when.
#[derive(Debug, Clone)]
pub struct CallReady {
    pub params: ReadyParams,
    pub since: Instant,
}

/// Where a live call stands. The Ready record (its key parameters, and the
/// instant the call-duration clock starts) exists only once the call was
/// Ready, and is kept through a later hang-up state, so a state and its
/// record cannot disagree.
#[derive(Debug, Clone)]
pub enum CallProgress {
    /// TDLib's state before `callStateReady` (ringing, exchanging keys).
    Signaling(CallState),
    /// `callStateReady` arrived; `state` is TDLib's latest state since
    /// (Ready, or a hang-up it reported).
    Ready { ready: CallReady, state: CallState },
}

impl CallProgress {
    /// A call first seen in `state`.
    pub fn first(state: CallState, now: Instant) -> Self {
        match &state {
            CallState::Ready(params) => Self::Ready {
                ready: CallReady {
                    params: params.clone(),
                    since: now,
                },
                state,
            },
            _ => Self::Signaling(state),
        }
    }

    /// TDLib's last state of the call.
    pub fn state(&self) -> &CallState {
        match self {
            Self::Signaling(state) | Self::Ready { state, .. } => state,
        }
    }

    /// The Ready record, once the call was Ready.
    pub fn ready(&self) -> Option<&CallReady> {
        match self {
            Self::Signaling(_) => None,
            Self::Ready { ready, .. } => Some(ready),
        }
    }

    /// An `updateCall` that is not terminal: the first Ready starts the
    /// clock, a repeat only refreshes the parameters, and a later state
    /// keeps the record.
    pub fn update(&mut self, state: CallState, now: Instant) {
        match self {
            Self::Ready { ready, state: held } => {
                if let CallState::Ready(params) = &state {
                    ready.params = params.clone();
                }
                *held = state;
            }
            Self::Signaling(_) if matches!(state, CallState::Ready(_)) => {
                *self = Self::first(state, now);
            }
            Self::Signaling(held) => *held = state,
        }
    }
}
