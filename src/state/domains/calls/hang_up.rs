//! A 1:1 call's hang-up while its `discardCall` is in flight.
use crate::ids::RequestId;
use crate::state::{CallsState, PendingRequest, RequestPurpose, Session};

/// The `discardCall` that put the active call into `HangingUp`. The
/// request owns the phase: TDLib's terminal update ends the call, and an
/// error for `request` ends the hang-up. The call engine keeps the call
/// until TDLib took the hang-up, so a failed one goes on with its sound.
#[derive(Debug, Clone, PartialEq)]
pub struct HangUp {
    pub request: RequestId,
    pub call_id: i32,
    /// Where the hang-up is: in flight, or TDLib took it and the engine is
    /// owed its hang-up (`ConnectDriver::pump_call_engine` settles that).
    stage: HangUpStage,
    /// What the call was before the hang-up went out; a failed hang-up
    /// goes back to that.
    from: HangUpFrom,
}

/// The stages of a hang-up. `Discarding` leaves on TDLib's `ok`
/// (`EngineOwed`) or on the request's error (the hang-up is dropped);
/// `EngineOwed` leaves when the driver pump tells the engine
/// (`EngineTold`), and the call's terminal update ends all three.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HangUpStage {
    Discarding,
    EngineOwed,
    EngineTold,
}

/// The [`CallAction`] a hang-up replaced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HangUpFrom {
    Unanswered,
    /// [`CallAction::Answered`], by this `acceptCall`.
    Answered(RequestId),
}

/// What this side did to the active 1:1 call, one value inside the call it
/// belongs to (so it ends with it): nothing yet (ringing, or not ours),
/// answered here, or being hung up. TDLib's `updateCall` stays the source of
/// truth for the call's own phase (`ActiveCall::state`); the call shows
/// `HangingUp` while this says so ([`ActiveCall::phase`]). Leaves: the
/// call's terminal update (the call goes), a failed `acceptCall` (back to
/// `Idle`), a failed `discardCall` (back to `Answered` or `Idle`).
#[derive(Debug, Clone, PartialEq, Default)]
pub enum CallAction {
    #[default]
    Idle,
    /// `acceptCall` went out for this incoming call. Its state stays
    /// `Pending` until TDLib's `updateCall`, but it no longer rings: a
    /// close hangs it up. It holds the accept's request, so only that
    /// request's error puts the call back to ringing.
    Answered(RequestId),
    /// The `discardCall` in flight, which owns the `HangingUp` phase.
    HangingUp(HangUp),
}

impl CallAction {
    pub fn is_answered(&self) -> bool {
        matches!(self, Self::Answered(_))
    }

    /// The `acceptCall` already out: a second click answers nothing new.
    pub fn accept_request(&self) -> Option<RequestId> {
        match self {
            Self::Answered(accept) => Some(*accept),
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

    /// `acceptCall` went out as `accept`.
    pub fn answer(&mut self, accept: RequestId) {
        if !matches!(self, Self::HangingUp(_)) {
            *self = Self::Answered(accept);
        }
    }

    /// A failed `acceptCall`: the call keeps ringing. Only the accept the
    /// action holds clears it, so a late error of an earlier one does not.
    pub fn answer_failed(&mut self, accept: RequestId) {
        if self.accept_request() == Some(accept) {
            *self = Self::Idle;
        }
    }

    /// The `discardCall` went out.
    pub fn hang_up_sent(&mut self, mut hang_up: HangUp) {
        hang_up.from = if let Some(accept) = self.accept_request() {
            HangUpFrom::Answered(accept)
        } else {
            HangUpFrom::Unanswered
        };
        *self = Self::HangingUp(hang_up);
    }

    /// The hang-up `request` failed: its record, with the action back to
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
            HangUpFrom::Answered(accept) => Self::Answered(accept),
            HangUpFrom::Unanswered => Self::Idle,
        };
        Some(hang_up)
    }
}

impl HangUp {
    /// TDLib answered `ok`: the call cannot come back, and the engine is
    /// owed its hang-up. Only a hang-up still in flight takes it.
    fn accepted(&mut self) {
        if self.stage == HangUpStage::Discarding {
            self.stage = HangUpStage::EngineOwed;
        }
    }

    /// The engine's hang-up, once: true the first time after TDLib took the
    /// hang-up, false after that.
    pub fn take_engine_hang_up(&mut self) -> bool {
        if self.stage != HangUpStage::EngineOwed {
            return false;
        }
        self.stage = HangUpStage::EngineTold;
        true
    }

    pub fn new(request: RequestId, call_id: i32) -> Self {
        Self {
            request,
            call_id,
            stage: HangUpStage::Discarding,
            from: HangUpFrom::Unanswered,
        }
    }
}

impl CallsState {
    /// The active call's action, when `call_id` is the active call.
    fn action_of_mut(&mut self, call_id: i32) -> Option<&mut CallAction> {
        self.active_call
            .as_mut()
            .filter(|call| call.id == call_id)
            .map(|call| &mut call.action)
    }

    /// `acceptCall` went out for `call_id` as `accept`.
    pub fn answer_call(&mut self, call_id: i32, accept: RequestId) {
        if let Some(action) = self.action_of_mut(call_id) {
            action.answer(accept);
        }
    }

    /// The `acceptCall` `accept` for `call_id` failed.
    pub fn answer_call_failed(&mut self, call_id: i32, accept: RequestId) {
        if let Some(action) = self.action_of_mut(call_id) {
            action.answer_failed(accept);
        }
    }

    /// The `acceptCall` already out for `call_id`, the active call.
    pub fn accept_request(&self, call_id: i32) -> Option<RequestId> {
        self.active_call
            .as_ref()
            .filter(|call| call.id == call_id)
            .and_then(|call| call.action.accept_request())
    }

    /// `call_id` is the active call and was answered here.
    pub fn answered(&self, call_id: i32) -> bool {
        self.active_call
            .as_ref()
            .is_some_and(|call| call.id == call_id && call.action.is_answered())
    }

    /// The active call's hang-up in flight.
    pub fn hang_up(&self) -> Option<&HangUp> {
        self.active_call.as_ref()?.action.hang_up()
    }

    pub fn hang_up_mut(&mut self) -> Option<&mut HangUp> {
        self.active_call.as_mut()?.action.hang_up_mut()
    }

    /// The `discardCall` for the active call went out.
    pub fn hang_up_sent(&mut self, hang_up: HangUp) {
        if let Some(action) = self.action_of_mut(hang_up.call_id) {
            action.hang_up_sent(hang_up);
        }
    }

    /// The hang-up `request` failed: its record, the call back in what it
    /// was.
    pub fn hang_up_failed(&mut self, request: Option<RequestId>) -> Option<HangUp> {
        self.active_call.as_mut()?.action.hang_up_failed(request)
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
            .hang_up_mut()
            .filter(|hang_up| hang_up.request == pending.id)
        {
            hang_up.accepted();
        }
    }
}
