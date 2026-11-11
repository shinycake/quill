//! A 1:1 call's hang-up while its `discardCall` is in flight.
use crate::ids::RequestId;
use crate::state::{PendingRequest, RequestPurpose, Session};
use crate::telegram::envelope::CallState;

/// The `discardCall` that put the active call into `HangingUp`. The
/// request owns the phase: TDLib's terminal update ends the call, and an
/// error for `request` puts the call back in `resume`. The call engine
/// keeps the call until TDLib took the hang-up, so a failed one goes on
/// with its sound.
#[derive(Debug, Clone, PartialEq)]
pub struct HangUp {
    pub request: RequestId,
    pub call_id: i32,
    /// The phase to go back to if the hang-up fails. Non-terminal
    /// updates that land meanwhile are stored here, so the panel keeps
    /// showing "Hanging up".
    pub resume: CallState,
    /// TDLib answered `ok`: the call cannot come back, and the engine
    /// has not been told yet (`ConnectDriver::pump_call_engine` tells it).
    pub engine_hang_up_due: bool,
    /// What the call was before the hang-up went out; a failed hang-up
    /// goes back to that.
    from: HangUpFrom,
}

/// The `CallProgress` a hang-up replaced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HangUpFrom {
    Unanswered,
    /// `CallProgress::Answered`.
    Answered,
}

/// How far the active 1:1 call got on this side, one value: ringing (or
/// not ours yet), answered here, or being hung up. TDLib's `updateCall`
/// stays the source of truth for the call's own phase
/// (`ActiveCall::state`); this is what *we* did to it. Leaves: the call's
/// terminal update (`Idle`), a failed `acceptCall` (back to `Idle`), a failed
/// `discardCall` (back to `Answered` or `Idle`).
#[derive(Debug, Clone, PartialEq, Default)]
pub enum CallProgress {
    #[default]
    Idle,
    /// `acceptCall` went out for this incoming call. Its state stays
    /// `Pending` until TDLib's `updateCall`, but it no longer rings: a
    /// close hangs it up.
    Answered { call_id: i32 },
    /// The `discardCall` in flight, which owns the `HangingUp` phase.
    HangingUp(HangUp),
}

impl CallProgress {
    pub fn answered_call(&self) -> Option<i32> {
        match self {
            Self::Answered { call_id } => Some(*call_id),
            _ => None,
        }
    }

    pub fn hang_up(&self) -> Option<&HangUp> {
        match self {
            Self::HangingUp(hang_up) => Some(hang_up),
            _ => None,
        }
    }

    pub fn hang_up_mut(&mut self) -> Option<&mut HangUp> {
        match self {
            Self::HangingUp(hang_up) => Some(hang_up),
            _ => None,
        }
    }

    /// `acceptCall` went out for `call_id`.
    pub fn answer(&mut self, call_id: i32) {
        if !matches!(self, Self::HangingUp(_)) {
            *self = Self::Answered { call_id };
        }
    }

    /// A failed `acceptCall`: the call keeps ringing.
    pub fn answer_failed(&mut self, call_id: i32) {
        if self.answered_call() == Some(call_id) {
            *self = Self::Idle;
        }
    }

    /// The `discardCall` for `hang_up.call_id` went out.
    pub fn hang_up_sent(&mut self, mut hang_up: HangUp) {
        hang_up.from = if self.answered_call() == Some(hang_up.call_id) {
            HangUpFrom::Answered
        } else {
            HangUpFrom::Unanswered
        };
        *self = Self::HangingUp(hang_up);
    }

    /// The hang-up `request` failed: its record, with the progress back to
    /// what it was before.
    pub fn hang_up_failed(&mut self, request: Option<RequestId>) -> Option<HangUp> {
        let Self::HangingUp(hang_up) = self else {
            return None;
        };
        if Some(hang_up.request) != request {
            return None;
        }
        let hang_up = hang_up.clone();
        *self = match hang_up.from {
            HangUpFrom::Answered => Self::Answered {
                call_id: hang_up.call_id,
            },
            HangUpFrom::Unanswered => Self::Idle,
        };
        Some(hang_up)
    }

    /// The terminal update of `call_id` ended whatever we did to it.
    pub fn call_ended(&mut self, call_id: i32) {
        let ours = match self {
            Self::Idle => false,
            Self::Answered { call_id: id } => *id == call_id,
            Self::HangingUp(hang_up) => hang_up.call_id == call_id,
        };
        if ours {
            *self = Self::Idle;
        }
    }
}

impl HangUp {
    pub fn new(request: RequestId, call_id: i32, resume: CallState) -> Self {
        Self {
            request,
            call_id,
            resume,
            engine_hang_up_due: false,
            from: HangUpFrom::Unanswered,
        }
    }
}

impl Session {
    /// TDLib took the active call's `discardCall`: the engine may hang up.
    pub(crate) fn apply_hang_up_ok(&mut self, pending: Option<&PendingRequest>) {
        let Some(pending) = pending.filter(|p| p.purpose == RequestPurpose::DiscardCall) else {
            return;
        };
        if let Some(hang_up) = self
            .calls
            .progress
            .hang_up_mut()
            .filter(|hang_up| hang_up.request == pending.id)
        {
            hang_up.engine_hang_up_due = true;
        }
    }
}
