//! The "Show more" paging of a group call's participant list.
use crate::ids::RequestId;
use crate::state::{PendingRequest, RequestPurpose, Session};

use super::CallsPurpose;

/// Where the next `loadGroupCallParticipants` stands. One request at a time:
/// only its own answer or error leaves `Loading`, and the call itself going
/// away takes the stage with it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ParticipantPaging {
    #[default]
    Idle,
    /// Asked as this request; TDLib's `ok` ends it (the participants come
    /// as `updateGroupCallParticipants`).
    Loading(RequestId),
    /// The last ask was refused; the button asks again.
    Failed,
}

impl Session {
    /// The tracked call's paging request `pending` was answered or refused:
    /// the stage leaves `Loading` when it is the one it waits for.
    /// `refused` lands in `Failed`.
    pub(crate) fn settle_group_paging(&mut self, pending: Option<&PendingRequest>, refused: bool) {
        let Some(pending) = pending else { return };
        let RequestPurpose::Calls(CallsPurpose::LoadGroupCallParticipants { group_call_id }) =
            pending.purpose
        else {
            return;
        };
        if let Some(call) = self.calls.active_group_call.as_mut()
            && call.id == group_call_id
            && call.paging == ParticipantPaging::Loading(pending.id)
        {
            call.paging = if refused {
                ParticipantPaging::Failed
            } else {
                ParticipantPaging::Idle
            };
        }
    }
}
