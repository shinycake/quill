//! Applies TDLib updates and answers for one-to-one calls, group calls and video chats.
use crate::state::*;
use crate::telegram::envelope::CallsPayload;

impl Session {
    /// Applies one calls payload; called by
    /// [`Session::apply_payload`].
    pub(crate) fn apply_calls_payload(
        &mut self,
        payload: CallsPayload,
        pending: Option<&PendingRequest>,
        extra: Option<RequestId>,
        seq: u64,
    ) {
        match payload {
            // Phase C1: call signaling (schema 1.8.67, lines 10816 /
            // 10862). `updateCall` drives the single-call state machine;
            // signaling data is queued as a diagnostic record and also fed
            // to the engine by the C2b driver bridge; `callId` is the answer
            // that starts tracking the outgoing call.
            CallsPayload::UpdateCall { call } => {
                self.accept_call_update(&call);
            }
            CallsPayload::UpdateNewCallSignalingData { call_id, data } => {
                self.accept_call_signaling_data(call_id, data);
            }
            CallsPayload::CallId { id } => self.apply_call_id(id, pending, extra, seq),
            // Phase C3a: `groupCallId` — the `createVideoChat` answer.
            // Queue a `getGroupCall` fetch so tracking starts even if
            // the `updateGroupCall` is delayed; the update remains the
            // source of truth.
            CallsPayload::GroupCallId { id } => {
                if let Some(RequestPurpose::Calls(CallsPurpose::CreateVideoChat { .. })) =
                    pending.map(|p| p.purpose)
                    && !self.group_call_fetch_queue.contains(&id)
                {
                    self.group_call_fetch_queue.push(id);
                }
                self.group_call_error = None;
            }
            // Phase C2f: `groupCallInfo` — the `joinGroupCall`
            // answer to invitation acceptance. Queue a `getGroupCall`
            // fetch so tracking starts even if the `updateGroupCall`
            // is delayed; the update remains the source of truth for
            // `is_joined`. Store the tgcalls join payload like the
            // `joinVideoChat` Text arm does.
            CallsPayload::GroupCallInfo {
                group_call_id,
                join_payload,
            } => {
                if let Some(RequestPurpose::JoinGroupCallInvitation) = pending.map(|p| p.purpose) {
                    if !self.group_call_fetch_queue.contains(&group_call_id) {
                        self.group_call_fetch_queue.push(group_call_id);
                    }
                    self.set_group_call_join_payload(group_call_id, join_payload);
                }
                self.group_call_error = None;
            }
            // Phase C3a: group-call signaling (schema 1.8.67, lines
            // 10819 / 10824 / 10830 / 10836 / 10576). `updateGroupCall`
            // drives the tracked-call state; participant updates feed
            // the grid; the verification state feeds the E2E emoji UI;
            // `updateChatVideoChat` refreshes the chat's join affordance.
            // All signaling-only — no media transport until Phase C2.
            CallsPayload::UpdateGroupCall { group_call } => {
                self.accept_group_call_update(&group_call);
            }
            CallsPayload::UpdateGroupCallParticipant {
                group_call_id,
                participant,
            } => {
                self.accept_group_call_participant_update(group_call_id, &participant);
            }
            CallsPayload::UpdateGroupCallParticipants {
                group_call_id,
                participant_user_ids,
            } => {
                self.accept_group_call_participants_update(group_call_id, &participant_user_ids);
            }
            CallsPayload::UpdateGroupCallVerificationState {
                group_call_id,
                generation,
                emojis,
            } => {
                self.accept_group_call_verification_state(group_call_id, generation, &emojis);
            }
            CallsPayload::UpdateChatVideoChat {
                chat_id,
                video_chat,
            } => {
                self.accept_chat_video_chat(ChatId(chat_id), &video_chat);
            }
            // Phase C2h: in-call chat message updates.
            CallsPayload::UpdateNewGroupCallMessage {
                group_call_id,
                message,
            } => {
                self.accept_new_group_call_message(group_call_id, &message);
            }
            CallsPayload::UpdateGroupCallMessageSendFailed {
                group_call_id,
                message_id: _,
                error,
            } => {
                if self
                    .active_group_call
                    .as_ref()
                    .is_some_and(|c| c.id == group_call_id)
                {
                    self.group_call_error = Some(call_request_error_line(
                        &error,
                        "Could not send the message",
                    ));
                }
            }
            CallsPayload::UpdateGroupCallMessagesDeleted {
                group_call_id,
                message_ids,
            } => {
                self.accept_group_call_messages_deleted(group_call_id, &message_ids);
            }
            // Phase C2h: `rtmpUrl` — the `getVideoChatRtmpUrl` /
            // `replaceVideoChatRtmpUrl` answer. Stored on the tracked
            // call whose chat the request targeted.
            CallsPayload::RtmpUrl { url, stream_key } => {
                if let Some(
                    RequestPurpose::Calls(CallsPurpose::GetVideoChatRtmpUrl { chat_id })
                    | RequestPurpose::Calls(CallsPurpose::ReplaceVideoChatRtmpUrl { chat_id }),
                ) = pending.map(|p| p.purpose)
                {
                    let call_id = self
                        .chats
                        .get(&chat_id)
                        .and_then(|c| c.video_chat.as_ref())
                        .map(|vc| vc.group_call_id);
                    if let (Some(call_id), Some(tracked)) =
                        (call_id, self.active_group_call.as_mut())
                        && tracked.id == call_id
                    {
                        tracked.rtmp_url = Some(url);
                        tracked.rtmp_stream_key = Some(stream_key);
                    }
                }
            }
            // Phase C2f: `inviteGroupCallParticipant` answer. A success
            // clears any earlier invite error; the three failure
            // variants surface honestly via `group_call_error` (shown
            // on the group-call overlay).
            CallsPayload::InviteGroupCallParticipantResult(result) => {
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(RequestPurpose::Calls(
                        CallsPurpose::InviteGroupCallParticipant { .. }
                    ))
                ) {
                    self.group_call_error = match result {
                        InviteGroupCallParticipantResult::Success { .. } => None,
                        InviteGroupCallParticipantResult::UserPrivacyRestricted => Some(
                            "Couldn't invite: that user restricts group-call invitations."
                                .to_string(),
                        ),
                        InviteGroupCallParticipantResult::UserAlreadyParticipant => {
                            Some("That user is already in the voice chat.".to_string())
                        }
                        InviteGroupCallParticipantResult::UserWasBanned => {
                            Some("That user was banned from the voice chat.".to_string())
                        }
                    };
                }
            }
        }
    }
}
