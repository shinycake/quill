//! Quill's own rejoin of a group call that needs one (split out of
//! `call_types.rs` for the file-size limit).
use super::*;

/// Quill's own rejoin of a group call that TDLib (or a stream feeder)
/// says needs one, and the leave that ends it. One value, so "a rejoin is
/// due", "the banner shows" and "the user left" cannot drift apart: the
/// driver sends a rejoin only from `Wanted`, a repeated `need_rejoin`
/// while one is `InFlight` changes nothing, and a call the user left
/// never rejoins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GroupCallRejoin {
    /// Joined, or never dropped.
    #[default]
    Idle,
    /// A rejoin is due; `attempts` were already spent on this drop.
    Wanted { attempts: usize },
    /// Attempt number `attempt` is being made: its offer is being built,
    /// or its join request (`request`) waits for TDLib.
    InFlight { request: RequestId, attempt: usize },
    /// Every attempt failed; only the manual Rejoin is left.
    GaveUp,
    /// The user left (or ended) the call with `leaveGroupCall` and TDLib
    /// has not said it is no longer joined: a late joined update must not
    /// take the slot back, and no rejoin is due. Any in-flight rejoin
    /// ends here.
    Left,
}

impl GroupCallRejoin {
    /// The user left the call.
    pub fn leave(&mut self) {
        *self = GroupCallRejoin::Left;
    }

    /// The call was left and TDLib said so, or the user joins it again:
    /// joined updates count once more.
    pub fn leave_over(&mut self) {
        if *self == GroupCallRejoin::Left {
            *self = GroupCallRejoin::Idle;
        }
    }

    /// The user left the call and TDLib has not confirmed it yet.
    pub fn is_left(&self) -> bool {
        *self == GroupCallRejoin::Left
    }

    /// TDLib's `need_rejoin` (or a stream's own rejoin): a drop starts
    /// one rejoin series, and a series already under way goes on.
    pub fn want(&mut self) {
        if *self == GroupCallRejoin::Idle {
            *self = GroupCallRejoin::Wanted { attempts: 0 };
        }
    }

    /// The attempt the driver may start now, if any: from `Wanted` with
    /// attempts left.
    pub fn next_attempt(&self) -> Option<usize> {
        match *self {
            GroupCallRejoin::Wanted { attempts } if attempts < MAX_GROUP_CALL_REJOIN_ATTEMPTS => {
                Some(attempts + 1)
            }
            _ => None,
        }
    }

    pub fn in_flight(&self) -> Option<RequestId> {
        match *self {
            GroupCallRejoin::InFlight { request, .. } => Some(request),
            _ => None,
        }
    }

    /// Due or under way, with attempts left.
    pub fn is_rejoining(&self) -> bool {
        match *self {
            GroupCallRejoin::Wanted { .. } => self.next_attempt().is_some(),
            GroupCallRejoin::InFlight { .. } => true,
            GroupCallRejoin::Idle | GroupCallRejoin::GaveUp | GroupCallRejoin::Left => false,
        }
    }

    /// The attempt behind `request` failed: the next one is due, or the
    /// series gave up. Whether `request` was this call's rejoin.
    pub fn attempt_failed(&mut self, request: RequestId) -> bool {
        match *self {
            GroupCallRejoin::InFlight {
                request: mine,
                attempt,
            } if mine == request => {
                *self = if attempt >= MAX_GROUP_CALL_REJOIN_ATTEMPTS {
                    GroupCallRejoin::GaveUp
                } else {
                    GroupCallRejoin::Wanted { attempts: attempt }
                };
                true
            }
            _ => false,
        }
    }

    /// TDLib answered the join behind `request`: the rejoin succeeded and
    /// the attempts start over. Any other answer (a first join, a stale
    /// rejoin) leaves the state alone.
    pub fn attempt_joined(&mut self, request: RequestId) {
        if self.in_flight() == Some(request) {
            *self = GroupCallRejoin::Idle;
        }
    }

    /// TDLib reports the call joined again after it was not: a join went
    /// through, so whatever was due or spent is over. A joined update that
    /// follows a joined one says nothing about a rejoin Quill started on
    /// its own (a stream's part fetch) and is not this.
    pub fn rejoined(&mut self) {
        *self = GroupCallRejoin::Idle;
    }

    /// The attempt behind `request` never reached TDLib (the send
    /// failed): it is not counted.
    pub fn attempt_unsent(&mut self, request: RequestId) {
        if let GroupCallRejoin::InFlight {
            request: mine,
            attempt,
        } = *self
            && mine == request
        {
            *self = GroupCallRejoin::Wanted {
                attempts: attempt.saturating_sub(1),
            };
        }
    }
}
