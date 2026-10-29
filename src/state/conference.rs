//! Conference calls in the session: end-to-end encrypted group calls that
//! aren't bound to a chat (tdesktop `GroupCall::conference()`,
//! docs/decisions/tgcalls-conference-calls.md). A conference is reached by
//! accepting a `messageGroupCall` invitation, by its link, or by creating
//! one, which is what "Add people" does to a 1:1 call. Until the join
//! answer names the call, the join waits in [`CallsState::pending_conference`];
//! afterwards the tracked group call carries a [`ConferenceCall`].
use super::*;
use crate::telegram::envelope::CallDiscardReason;

/// How TDLib reaches a conference call that isn't joined (`InputGroupCall`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConferenceInput {
    /// `inputGroupCallLink`.
    Link(String),
    /// `inputGroupCallMessage`: the `messageGroupCall` invitation.
    Message { chat_id: i64, message_id: i64 },
}

/// A 1:1 call taken into a new conference call (tdesktop
/// `StartConferenceInfo` with `migrating`): once the conference has its
/// link, the 1:1 call ends with it (`discardCall.invite_link`, the other
/// side follows the link) and the people picked are invited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConferenceUpgrade {
    pub call_id: i32,
    /// The other side of the 1:1 call.
    pub peer_user_id: i64,
    pub is_video: bool,
    /// The users picked in the invite box.
    pub invite: Vec<i64>,
    /// "Share link" was picked instead: the link is offered once known.
    pub share_link: bool,
}

/// Why a conference call is being joined.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConferenceOrigin {
    /// Accepting a `messageGroupCall` invitation (tdesktop
    /// `acceptConferenceInvite`).
    Invitation { chat_id: i64, message_id: i64 },
    /// A call link: opened, or the one the other side of a 1:1 call
    /// ended it with (`callDiscardReasonUpgradeToGroupCall`, tdesktop
    /// `finishByMigration`).
    Link { link: String },
    /// Created here (`createGroupCall`), from "Add people" in a 1:1 call.
    Create { upgrade: Option<ConferenceUpgrade> },
}

impl ConferenceOrigin {
    /// What a rejoin uses before the call's own link is known.
    fn input(&self) -> Option<ConferenceInput> {
        match self {
            ConferenceOrigin::Invitation {
                chat_id,
                message_id,
            } => Some(ConferenceInput::Message {
                chat_id: *chat_id,
                message_id: *message_id,
            }),
            ConferenceOrigin::Link { link } => Some(ConferenceInput::Link(link.clone())),
            ConferenceOrigin::Create { .. } => None,
        }
    }
}

/// A conference join or create in flight (tdesktop `_startingGroupCall`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingConference {
    pub request: RequestId,
    pub origin: ConferenceOrigin,
    pub muted: bool,
    /// The 1:1 call the other side took into this conference: its end
    /// screen, shown if the join fails (tdesktop `finishByMigration`
    /// fails the 1:1 call).
    pub follows: Option<CallSummary>,
}

/// The other side took the 1:1 call into a conference call: its link,
/// for the driver to join with, the microphone state to keep, and the
/// 1:1 call's end screen, held back unless following the link fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallUpgrade {
    pub invite_link: String,
    pub muted: bool,
    pub ended: CallSummary,
}

/// The end line of a 1:1 call whose conference could not be followed
/// (tdesktop `State::Failed`, `lng_call_status_failed`).
pub const CONFERENCE_FOLLOW_FAILED: &str = "Failed to connect";

/// What a tracked conference call keeps beside its `groupCall` fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConferenceCall {
    /// How it was reached, for a rejoin before its link is known.
    pub input: Option<ConferenceInput>,
    /// The 1:1 call still to end and the people still to invite.
    pub upgrade: Option<ConferenceUpgrade>,
    /// The 1:1 call this conference took over: its end shows no end
    /// screen (tdesktop moves the call panel into the conference).
    pub migrated_from: Option<i32>,
    /// The call's link is on screen to share ("Share link" in the invite
    /// box, tdesktop `migrationShowShareLink`).
    pub link_open: bool,
}

impl ActiveGroupCall {
    /// A conference call this session joined or created.
    pub fn is_conference(&self) -> bool {
        self.conference.is_some()
    }

    /// What a conference rejoin passes to `joinGroupCall`: the call's own
    /// link once TDLib reported it, else the way it was first reached.
    pub fn conference_input(&self) -> Option<ConferenceInput> {
        if !self.invite_link.is_empty() {
            return Some(ConferenceInput::Link(self.invite_link.clone()));
        }
        self.conference.as_ref().and_then(|call| call.input.clone())
    }
}

impl Session {
    /// A conference join or create went out as `request`.
    pub fn begin_conference(&mut self, request: RequestId, origin: ConferenceOrigin, muted: bool) {
        self.calls.pending_conference = Some(PendingConference {
            request,
            origin,
            muted,
            follows: None,
        });
        self.calls.group_call_error = None;
    }

    /// The `groupCallInfo` answer to a conference join: the call is
    /// tracked from now on, with the tgcalls answer the driver connects
    /// with, and fetched in full. A rejoin's answer (no pending join) only
    /// stores the payload.
    pub(crate) fn accept_conference_joined(
        &mut self,
        extra: Option<RequestId>,
        group_call_id: i32,
        join_payload: String,
    ) {
        let pending = self
            .calls
            .pending_conference
            .take_if(|pending| Some(pending.request) == extra);
        if let Some(pending) = pending {
            let tracked = self
                .calls
                .active_group_call
                .get_or_insert_with(|| ActiveGroupCall::fresh(group_call_id));
            if tracked.id != group_call_id {
                *tracked = ActiveGroupCall::fresh(group_call_id);
            }
            tracked.is_muted_self = pending.muted;
            tracked.conference = Some(ConferenceCall {
                input: pending.origin.input(),
                upgrade: match pending.origin {
                    ConferenceOrigin::Create { upgrade } => upgrade,
                    _ => None,
                },
                migrated_from: None,
                link_open: false,
            });
            if self.calls.left_group_call == Some(group_call_id) {
                self.calls.left_group_call = None;
            }
        }
        if !self.calls.group_call_fetch_queue.contains(&group_call_id) {
            self.calls.group_call_fetch_queue.push(group_call_id);
        }
        self.set_group_call_join_payload(group_call_id, join_payload);
        self.calls.group_call_error = None;
    }

    /// The 1:1 call ended by being taken into a conference call
    /// (`callDiscardReasonUpgradeToGroupCall`): true when it shows no end
    /// screen. The side that didn't make the conference follows its link
    /// ([`CallsState::call_upgrade`], tdesktop `finishByMigration`); the
    /// side that did is already in it.
    pub(crate) fn note_call_migration(&mut self, call: &ParsedCall, ended: &CallSummary) -> bool {
        let migrated_here = self
            .calls
            .active_group_call
            .as_ref()
            .and_then(|tracked| tracked.conference.as_ref())
            .is_some_and(|conference| conference.migrated_from == Some(call.id));
        if migrated_here {
            return true;
        }
        let CallState::Discarded {
            reason: CallDiscardReason::UpgradeToGroupCall { invite_link },
            ..
        } = &call.state
        else {
            return false;
        };
        if invite_link.is_empty() {
            return false;
        }
        self.calls.call_upgrade = Some(CallUpgrade {
            invite_link: invite_link.clone(),
            muted: ended.muted,
            ended: ended.clone(),
        });
        true
    }

    /// A conference join or create failed: nothing is tracked. The
    /// failed join is returned when it was ours.
    pub(crate) fn conference_join_failed(
        &mut self,
        extra: Option<RequestId>,
    ) -> Option<PendingConference> {
        self.calls
            .pending_conference
            .take_if(|pending| Some(pending.request) == extra)
    }

    /// Following the other side into the conference failed: the 1:1 call
    /// ends as failed, with its end screen (tdesktop `finishByMigration`,
    /// `setState(State::Failed)`).
    pub fn conference_follow_failed(&mut self, ended: CallSummary) {
        self.calls.summary = Some(CallSummary {
            end_line: CONFERENCE_FOLLOW_FAILED.to_string(),
            ..ended
        });
    }
}

#[cfg(test)]
#[path = "conference_tests.rs"]
mod tests;
