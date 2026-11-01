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
            Some(RequestPurpose::AcceptCall) => {
                // The call was never answered and keeps ringing, so a
                // close must leave it to ring.
                self.calls.answered_call = None;
                self.calls.error = Some(call_request_error_line(err, "Could not answer the call"));
            }
            Some(RequestPurpose::DiscardCall) => {
                self.calls.error = Some(call_request_error_line(err, "Could not hang up the call"));
            }
            Some(RequestPurpose::SendCallRating) => {
                self.calls.error = Some(call_request_error_line(err, "Could not send the rating"));
            }
            Some(RequestPurpose::SendCallDebugInformation) => {
                if let Some(summary) = self.calls.summary.as_mut() {
                    summary.debug_information_sent = false;
                    summary.debug_information_error =
                        Some(call_request_error_line(err, "Could not upload diagnostics"));
                }
            }
            // Phase C2i: call history / privacy / log failures
            // surface on the Recent-calls tab (the UI reads the
            // flags), not the call overlay.
            Some(RequestPurpose::SearchCallMessages) => {
                self.calls.recent_calls_loading = false;
                self.calls.recent_calls_error = true;
            }
            Some(RequestPurpose::DeleteAllCallMessages) => {
                self.calls.recent_calls_clearing = false;
                self.chats_state.chat_action_error =
                    Some(crate::chatlist_calls::clear_failed(err.code));
            }
            Some(RequestPurpose::Calls(CallsPurpose::GetCallPrivacyRules { .. })) => {
                self.privacy_roundtrip_done();
                self.calls.privacy_error = true;
            }
            // Phase C2i: a failed `setUserPrivacySettingRules`
            // clears the optimistic value (the next fetch
            // restores the truth) and flags the error.
            Some(RequestPurpose::Calls(CallsPurpose::SetCallPrivacyRules { setting })) => {
                match setting {
                    CallPrivacySetting::AllowCalls => self.calls.privacy_allow_calls = None,
                    CallPrivacySetting::PeerToPeer => self.calls.privacy_p2p = None,
                }
                self.privacy_roundtrip_done();
                self.calls.privacy_error = true;
            }
            Some(RequestPurpose::SendCallLog) => {
                if let Some(summary) = self.calls.summary.as_mut() {
                    summary.log_sent = false;
                    summary.log_error = Some(call_request_error_line(
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
            // rejoin re-arms `reconnecting`, as for a video chat.
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
                    && call.rejoin_attempts > 0
                {
                    call.reconnecting = true;
                    if call.rejoin_attempts >= crate::state::MAX_GROUP_CALL_REJOIN_ATTEMPTS {
                        self.calls.group_call_error =
                            Some("Reconnect attempts exhausted.".to_string());
                    }
                }
            }
            Some(RequestPurpose::Calls(CallsPurpose::JoinVideoChat { group_call_id })) => {
                // Phase C2f: a failed rejoin re-arms
                // `reconnecting` so the driver's auto-rejoin
                // retries (max 3 attempts, the C2d discipline);
                // a plain initial-join failure just reports.
                // `rejoin_attempts > 0` marks the failed join
                // as a rejoin (only `rejoin_group_call`
                // increments the counter).
                let current = self.calls.is_current_group_call(group_call_id);
                let Some(call) = self.calls.group_call_mut(group_call_id) else {
                    return;
                };
                if call.rejoin_attempts > 0 {
                    // Keep the banner + manual Rejoin
                    // available even after exhaustion.
                    call.reconnecting = true;
                    let exhausted =
                        call.rejoin_attempts >= crate::state::MAX_GROUP_CALL_REJOIN_ATTEMPTS;
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
                | RequestPurpose::Calls(CallsPurpose::SetGroupCallParticipantVolumeLevel { .. })
                | RequestPurpose::Calls(CallsPurpose::DeclineGroupCallInvitation { .. }),
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
