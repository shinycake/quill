//! A 1:1 call's hang-up while its `discardCall` is in flight.
use crate::ids::RequestId;
use crate::telegram::envelope::CallState;

/// The `discardCall` that put the active call into `HangingUp`. The
/// request owns the phase: TDLib's terminal update ends the call, and an
/// error for `request` puts the call back in `resume`.
#[derive(Debug, Clone, PartialEq)]
pub struct HangUp {
    pub request: RequestId,
    pub call_id: i32,
    /// The phase to go back to if the hang-up fails. Non-terminal
    /// updates that land meanwhile are stored here, so the panel keeps
    /// showing "Hanging up".
    pub resume: CallState,
}
