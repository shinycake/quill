//! Failed requests for one-to-one calls, group calls and video chats.
use crate::state::*;

impl Session {
    /// Reacts to a failed calls request; called by
    /// [`Session::apply_error`] after the shared handling.
    pub(crate) fn apply_calls_error(
        &mut self,
        err: &TdError,
        pending: Option<&PendingRequest>,
        extra: Option<RequestId>,
        _seq: u64,
    ) {
        // "Stream with...": a failed URL request of the process is shown
        // as a toast by the UI (tdesktop shows the server error), not on a
        // call overlay.
        if let Some(
            RequestPurpose::Calls(CallsPurpose::GetVideoChatRtmpUrl { .. })
            | RequestPurpose::Calls(CallsPurpose::ReplaceVideoChatRtmpUrl { .. }),
        ) = pending.map(|p| p.purpose)
            && let Some(start) = self.calls.rtmp_start.as_mut()
            && start.fail(extra)
        {
            return;
        }
        // One of "Add People"'s invitations failed: its toast tells.
        if let Some(results) = self.calls.invite_results.as_mut()
            && let Some(extra) = extra
            && results.owns(Some(extra))
        {
            results.answered(extra, None);
            return;
        }
        // Phase C1: a failed call request surfaces on the call
        // overlay (shown and cleared by the UI). A failed
        // `createCall` also drops the half-tracked outgoing call.
        match pending.map(|p| p.purpose) {
            Some(RequestPurpose::Calls(CallsPurpose::CreateCall { .. })) => {
                // Note: the tracked outgoing call is *not*
                // cleared here — a failed `createCall` never
                // produced a `callId`, so any tracked call came
                // from elsewhere and must survive. The driver
                // also refuses a second `createCall` while one
                // is active.
                self.calls.error = Some(call_request_error_line(err, "Could not start the call"));
            }
            Some(RequestPurpose::Calls(CallsPurpose::AcceptCall { call_id })) => {
                // The call was never answered and keeps ringing, so a
                // close must leave it to ring.
                if self.calls.answered_call == Some(call_id) {
                    self.calls.answered_call = None;
                }
                // An answer that fails after its call went must not land on
                // the next one (tdesktop drops the request with its call).
                if self.calls.active_call.as_ref().map(|call| call.id) == Some(call_id) {
                    self.calls.error =
                        Some(call_request_error_line(err, "Could not answer the call"));
                }
            }
            Some(RequestPurpose::DiscardCall) => {
                // The active call's hang-up failed: the call goes on in
                // the phase it had, and a swap waiting on it asks again.
                if let Some(hang_up) = self.calls.hang_up.take_if(|h| Some(h.request) == extra) {
                    if let Some(call) = self.calls.active_call.as_mut()
                        && call.id == hang_up.call_id
                    {
                        call.state = hang_up.resume;
                    }
                    self.calls.second_call.back_to_prompt();
                    // The call stays up, so no link waits for it to end.
                    self.calls
                        .call_link_stage
                        .drop_waiting_for(crate::call_link::LeavingCall::Private(hang_up.call_id));
                    self.calls.error =
                        Some(call_request_error_line(err, "Could not hang up the call"));
                }
            }
            // The card comes back with the error; an answer for a rating
            // that is no longer the one in flight is dropped.
            Some(RequestPurpose::SendCallRating) => {
                if let Some(summary) = self.calls.summary.as_mut()
                    && extra.is_some_and(|extra| summary.rating == CallRating::Sending(extra))
                {
                    summary.rating = CallRating::Failed(call_request_error_line(
                        err,
                        "Could not send the rating",
                    ));
                }
            }
            Some(RequestPurpose::SendCallDebugInformation) => {
                if let Some(summary) = self.calls.summary.as_mut()
                    && extra.is_some_and(|extra| {
                        summary.debug_information == CallUpload::Sending(extra)
                    })
                {
                    summary.debug_information = CallUpload::Failed(call_request_error_line(
                        err,
                        "Could not upload diagnostics",
                    ));
                }
            }
            // Phase C2i: call history / privacy / log failures
            // surface on the Recent-calls tab (the UI reads the
            // flags), not the call overlay.
            Some(RequestPurpose::SearchCallMessages) => {
                if pending.is_some_and(|p| self.calls.take_recent_calls_request(p.id)) {
                    self.calls.recent_calls_error = true;
                }
            }
            Some(RequestPurpose::DeleteAllCallMessages) => {
                self.calls.recent_calls_clearing = false;
                self.chats_state
                    .chat_action_error
                    .push(crate::chatlist_calls::clear_failed(err.code));
            }
            Some(RequestPurpose::Calls(CallsPurpose::GetCallPrivacyRules { setting })) => {
                if let Some(pending) = pending {
                    self.calls.privacy_asks.failed_by(setting, pending.id);
                }
            }
            // Phase C2i: a failed `setUserPrivacySettingRules`
            // clears the optimistic value (the next fetch
            // restores the truth) and flags the error.
            Some(RequestPurpose::Calls(CallsPurpose::SetCallPrivacyRules { setting })) => {
                // Only the set the rule waits for: an earlier one that
                // fails after a later one was sent clears nothing.
                if let Some(pending) = pending
                    && self.calls.privacy_asks.failed_by(setting, pending.id)
                {
                    match setting {
                        CallPrivacySetting::AllowCalls => self.calls.privacy_allow_calls = None,
                        CallPrivacySetting::PeerToPeer => self.calls.privacy_p2p = None,
                    }
                }
            }
            Some(RequestPurpose::SendCallLog) => {
                if let Some(summary) = self.calls.summary.as_mut()
                    && extra.is_some_and(|extra| summary.log == CallUpload::Sending(extra))
                {
                    summary.log = CallUpload::Failed(call_request_error_line(
                        err,
                        "Could not upload the call log",
                    ));
                }
            }
            // Phase C3a: group-call request failures surface on
            // the group-call overlay (shown and cleared by the
            // UI). A failed `joinVideoChat` leaves any tracked
            // call in place — `updateGroupCall` is the source
            // of truth for join state.
            // Starting a video chat: there is no call panel yet, so the
            // UI shows the failure for its own request and drops the join.
            Some(RequestPurpose::Calls(CallsPurpose::CreateVideoChat { .. })) => {
                if let Some(extra) = extra {
                    self.calls.failed_creates.push(extra);
                    // Nobody drains this for an account off screen: keep the
                    // latest few (the box that asked is on screen, so asks
                    // for one of the last few).
                    let extra_old = self.calls.failed_creates.len().saturating_sub(8);
                    self.calls.failed_creates.drain(..extra_old);
                }
            }
            Some(RequestPurpose::Calls(CallsPurpose::CreateVideoChatJoinAs { .. })) => {
                if let Some(join_as) = self.calls.create_join_as.as_mut()
                    && Some(join_as.request) == extra
                {
                    join_as.answer = Some(Err(()));
                }
            }
            // A conference join or create: nothing is tracked yet. A failed
            // rejoin makes the next attempt due, as for a video chat.
            Some(RequestPurpose::JoinConference) => {
                if let Some(pending) = self.conference_join_failed(extra) {
                    match pending.follows {
                        Some(ended) => self.conference_follow_failed(ended),
                        None => {
                            self.calls.group_call_error =
                                Some(call_request_error_line(err, "Could not join the call"));
                        }
                    }
                } else if let Some(call) = self.calls.active_group_call.as_mut()
                    && call.is_conference()
                    && extra.is_some_and(|request| call.rejoin.attempt_failed(request))
                    && call.rejoin == crate::state::GroupCallRejoin::GaveUp
                {
                    self.calls.group_call_error = Some("Reconnect attempts exhausted.".to_string());
                }
            }
            Some(RequestPurpose::Calls(CallsPurpose::JoinVideoChat { group_call_id })) => {
                // Phase C2f: a failed rejoin makes the next attempt
                // due so the driver's auto-rejoin retries (max 3
                // attempts, the C2d discipline); a plain initial-join
                // failure just reports. The rejoin in flight names its
                // request, so only its own failure counts.
                let current = self.calls.is_current_group_call(group_call_id);
                let Some(call) = self.calls.group_call_mut(group_call_id) else {
                    return;
                };
                if extra.is_some_and(|request| call.rejoin.attempt_failed(request)) {
                    // The banner + manual Rejoin stay after exhaustion.
                    let exhausted = call.rejoin == crate::state::GroupCallRejoin::GaveUp;
                    if current && exhausted {
                        self.calls.group_call_error =
                            Some("Reconnect attempts exhausted.".to_string());
                    }
                } else if current {
                    self.calls.group_call_error = Some(call_request_error_line(
                        err,
                        "Could not join the voice chat",
                    ));
                } else {
                    call.transport_error = Some(call_request_error_line(
                        err,
                        "Could not join the live story",
                    ));
                }
            }
            // The live story's own leave (its slot is already gone, and it
            // never sets `left_group_call`): nothing to show on the call.
            Some(RequestPurpose::Calls(CallsPurpose::LeaveGroupCall { group_call_id }))
                if !self.calls.is_current_group_call(group_call_id)
                    && self.calls.left_group_call != Some(group_call_id) => {}
            // Phase C2g: a failed screen-sharing handshake must
            // not leave the call stuck "sharing" — clear the
            // pending/active flags and surface an honest error.
            Some(RequestPurpose::Calls(CallsPurpose::StartGroupCallScreenSharing {
                group_call_id,
            })) => {
                if let Some(tracked) = self.calls.active_group_call.as_mut()
                    && tracked.id == group_call_id
                {
                    tracked.screen_share_pending = false;
                    tracked.screen_sharing = false;
                    tracked.screen_share_answer.clear();
                }
                self.calls.group_call_error = Some(call_request_error_line(
                    err,
                    "Could not start screen sharing",
                ));
            }
            Some(RequestPurpose::Calls(CallsPurpose::EndGroupCallScreenSharing {
                group_call_id,
            })) => {
                if let Some(tracked) = self.calls.active_group_call.as_mut()
                    && tracked.id == group_call_id
                {
                    tracked.screen_share_pending = false;
                    tracked.screen_sharing = false;
                    tracked.screen_share_answer.clear();
                }
                self.calls.group_call_error = Some(call_request_error_line(
                    err,
                    "Could not stop screen sharing",
                ));
            }
            // The server never got the pause edge: your own tile shows what
            // it still has, and the next edge sends again. No box error: the
            // share itself works.
            Some(RequestPurpose::Calls(CallsPurpose::ToggleGroupCallScreenPaused {
                group_call_id,
                paused,
            })) => {
                if let Some(tracked) = self.calls.active_group_call.as_mut()
                    && tracked.id == group_call_id
                    && tracked.screen_share_paused == paused
                {
                    tracked.screen_share_paused = !paused;
                }
            }
            Some(
                RequestPurpose::Calls(CallsPurpose::LeaveGroupCall { .. })
                | RequestPurpose::Calls(CallsPurpose::EndGroupCall { .. })
                | RequestPurpose::Calls(CallsPurpose::GetGroupCall { .. })
                | RequestPurpose::Calls(CallsPurpose::LoadGroupCallParticipants { .. })
                | RequestPurpose::Calls(CallsPurpose::SetVideoChatDefaultParticipant { .. })
                | RequestPurpose::Calls(CallsPurpose::SetVideoChatTitle { .. })
                | RequestPurpose::Calls(CallsPurpose::RevokeVideoChatInviteLink { .. })
                | RequestPurpose::Calls(CallsPurpose::StartGroupCallRecording { .. })
                | RequestPurpose::Calls(CallsPurpose::EndGroupCallRecording { .. })
                | RequestPurpose::Calls(CallsPurpose::StartScheduledVideoChat { .. })
                | RequestPurpose::Calls(CallsPurpose::ToggleVideoChatEnabledStartNotification {
                    ..
                })
                | RequestPurpose::Calls(CallsPurpose::GetVideoChatRtmpUrl { .. })
                | RequestPurpose::Calls(CallsPurpose::ReplaceVideoChatRtmpUrl { .. })
                | RequestPurpose::Calls(CallsPurpose::SendGroupCallMessage { .. })
                | RequestPurpose::Calls(CallsPurpose::ToggleGroupCallAreMessagesAllowed { .. })
                | RequestPurpose::Calls(CallsPurpose::ToggleGroupCallVideo { .. })
                | RequestPurpose::Calls(CallsPurpose::ToggleGroupCallParticipantMute { .. })
                | RequestPurpose::Calls(CallsPurpose::ToggleGroupCallParticipantHand { .. })
                | RequestPurpose::Calls(CallsPurpose::ToggleVideoChatMuteNew { .. })
                | RequestPurpose::Calls(CallsPurpose::InviteGroupCallParticipant { .. })
                | RequestPurpose::Calls(CallsPurpose::InviteVideoChatParticipants { .. })
                | RequestPurpose::Calls(CallsPurpose::BanGroupCallParticipants { .. })
                | RequestPurpose::Calls(CallsPurpose::SetGroupCallParticipantVolumeLevel { .. }),
            ) => {
                self.calls.group_call_error =
                    Some(call_request_error_line(err, "Voice chat request failed"));
            }
            // A failed speaker link is left out of the share box (tdesktop's
            // `fail` handler); a failed listener link is reported and can be
            // asked for again.
            Some(RequestPurpose::Calls(CallsPurpose::GetVideoChatInviteLink {
                group_call_id,
                can_self_unmute,
            })) => {
                if let Some(tracked) = self.calls.active_group_call.as_mut()
                    && tracked.id == group_call_id
                {
                    tracked.share_links.failed(can_self_unmute);
                }
                if !can_self_unmute {
                    self.calls.group_call_error =
                        Some(call_request_error_line(err, "Couldn't get the invite link"));
                }
            }
            // A declined invitation has left the screen, and a decline sent
            // because you were busy is not about the voice chat you are
            // in: nothing shows (tdesktop sends it with no fail handler).
            Some(RequestPurpose::Calls(CallsPurpose::DeclineGroupCallInvitation { .. })) => {}
            // The "join as" list is optional: a failure just leaves the
            // picker out and the join goes ahead as yourself.
            Some(RequestPurpose::Calls(CallsPurpose::GetVideoChatAvailableParticipants {
                ..
            })) => {}
            // A failed "Add People" search ends it with what the other
            // half found (tdesktop's `fail` handler refreshes the rows).
            Some(
                RequestPurpose::SearchAddPeopleOnServer | RequestPurpose::SearchAddPeoplePublic,
            ) => {
                if let Some(extra) = extra {
                    self.calls.add_people_search.accept(extra, &[]);
                }
            }
            _ => {}
        }
    }
}
