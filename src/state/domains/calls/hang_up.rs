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
}

impl HangUp {
    pub fn new(request: RequestId, call_id: i32, resume: CallState) -> Self {
        Self {
            request,
            call_id,
            resume,
            engine_hang_up_due: false,
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
            .hang_up
            .as_mut()
            .filter(|hang_up| hang_up.request == pending.id)
        {
            hang_up.engine_hang_up_due = true;
        }
    }
}
