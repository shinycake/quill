//! Call and group-call acceptors.
use super::*;

impl Session {
    /// Phase C1: `updateCall` state machine. Quill tracks at most one
    /// call at a time (TDLib / official clients allow a single active
    /// 1:1 call):
    /// - same call id → advance the state; terminal states
    ///   (`callStateDiscarded` / `callStateError`) end the call and
    ///   record a summary for the end screen / rating card;
    /// - a *different*, non-terminal, incoming `Pending` call while one
    ///   is active → queued in `call_busy_decline_queue` for the driver
    ///   to `discardCall` (busy);
    /// - a terminal update for an untracked call id (e.g. a missed call
    ///   we never saw pending) still records a summary so the end
    ///   screen can show "Missed call".
    pub(crate) fn accept_call_update(&mut self, call: &ParsedCall) {
        if let Some(active) = self.active_call.as_mut()
            && active.id == call.id
        {
            if call.state.is_terminal() {
                self.end_active_call(call);
            } else {
                if matches!(call.state, CallState::Ready) && active.ready_at.is_none() {
                    active.ready_at = Some(Instant::now());
                }
                if matches!(call.state, CallState::Ready) {
                    active.ready = call.ready.clone();
                }
                active.state = call.state.clone();
                active.is_video = call.is_video;
            }
            return;
        }
        if self.active_call.is_some() {
            if !call.is_outgoing
                && matches!(call.state, CallState::Pending { .. })
                && !self
                    .call_busy_decline_queue
                    .iter()
                    .any(|(id, _, _)| *id == call.id)
            {
                self.call_busy_decline_queue
                    .push((call.id, call.user_id, call.is_video));
                self.diagnostics.record(Diagnostic {
                    category: "call",
                    type_name: Some("updateCall".to_string()),
                    extra: None,
                    seq: Some(self.last_seq),
                    note: "incoming-while-active-busy-decline",
                });
            }
            return;
        }
        if call.state.is_terminal() {
            self.call_summary = Some(CallSummary::from_terminal(call, 0, false));
            return;
        }
        self.active_call = Some(ActiveCall {
            id: call.id,
            user_id: call.user_id,
            is_outgoing: call.is_outgoing,
            is_video: call.is_video,
            state: call.state.clone(),
            started_at: Instant::now(),
            ready_at: None,
            ready: call.ready.clone(),
            transport: None,
            transport_error: None,
            signaling_queue: Vec::new(),
            signaling_dropped: 0,
            muted: false,
            camera_on: call.is_video,
            screen_sharing: false,
            remote_video: RemoteVideoState::Inactive,
            remote_screen: RemoteVideoState::Inactive,
        });
        self.call_summary = None;
        self.call_error = None;
    }

    /// Phase C2b: `updateNewCallSignalingData`. The driver also feeds these
    /// bytes into the engine; this bounded queue remains the honest
    /// diagnostic record. Data for an unknown call id is dropped (never
    /// buffered without a tracked call).
    pub(crate) fn accept_call_signaling_data(&mut self, call_id: i32, data: Vec<u8>) {
        let Some(active) = self.active_call.as_mut() else {
            return;
        };
        if active.id != call_id {
            return;
        }
        if active.signaling_queue.len() >= MAX_QUEUED_SIGNALING_CHUNKS {
            active.signaling_dropped += 1;
            self.diagnostics.record(Diagnostic {
                category: "call",
                type_name: Some("updateNewCallSignalingData".to_string()),
                extra: None,
                seq: Some(self.last_seq),
                note: "signaling-queue-overflow-dropped",
            });
            return;
        }
        active.signaling_queue.push(data);
    }

    /// Phase C1: end the tracked call on a terminal `updateCall` and
    /// record the summary shown on the call-end screen. Any queued
    /// signaling diagnostic data is dropped with the call.
    pub(crate) fn end_active_call(&mut self, call: &ParsedCall) {
        let Some(active) = self.active_call.take() else {
            return;
        };
        let duration_secs = active
            .ready_at
            .map(|t| t.elapsed().as_secs() as i64)
            .unwrap_or(0);
        let mut summary = CallSummary::from_terminal(
            call,
            duration_secs,
            active.transport == Some(TransportState::Connected),
        );
        summary.final_transport = active.transport;
        summary.muted = active.muted;
        self.call_summary = Some(summary);
        self.call_busy_decline_queue
            .retain(|(id, _, _)| *id != call.id);
    }

    /// Phase C3a: `updateGroupCall` state machine. Quill tracks at most
    /// one group call at a time (like the single 1:1 call):
    /// - a different call id replaces the tracked call (participants
    ///   reload via updates);
    /// - `need_rejoin` sets the `reconnecting` flag so the UI shows
    ///   "Reconnecting…" and the driver re-issues the join;
    /// - a fresh joined, non-`need_rejoin` update clears `reconnecting`;
    /// - `!is_active` (ended) drops the tracked call.
    ///
    /// Everything here is signaling — `join_payload` is stored, never
    /// consumed (no media transport until Phase C2).
    pub(crate) fn accept_group_call_update(&mut self, group_call: &ParsedGroupCall) {
        if !group_call.is_active {
            // Phase C2h: a scheduled (not yet started) video chat is
            // still tracked — the overlay shows "starts in …" plus an
            // admin-only "Start now" (startScheduledVideoChat,
            // schema/td_api.tl:14277). Join appears once TDLib
            // activates the call.
            if group_call.scheduled_start_date > 0 {
                let tracked = self
                    .active_group_call
                    .get_or_insert_with(|| ActiveGroupCall::fresh(group_call.id));
                if tracked.id != group_call.id {
                    *tracked = ActiveGroupCall::fresh(group_call.id);
                }
                let tracked = self.active_group_call.as_mut().expect("just inserted");
                tracked.title = group_call.title.clone();
                tracked.can_be_managed = group_call.can_be_managed;
                tracked.is_owned = group_call.is_owned;
                tracked.is_video_chat = group_call.is_video_chat;
                tracked.scheduled_start_date = group_call.scheduled_start_date;
                tracked.enabled_start_notification = group_call.enabled_start_notification;
                return;
            }
            if self
                .active_group_call
                .as_ref()
                .is_some_and(|c| c.id == group_call.id)
            {
                self.active_group_call = None;
                self.diagnostics.record(Diagnostic {
                    category: "group_call",
                    type_name: Some("updateGroupCall".to_string()),
                    extra: None,
                    seq: Some(self.last_seq),
                    note: "call-ended-tracked-cleared",
                });
            }
            return;
        }
        let tracked = self
            .active_group_call
            .get_or_insert_with(|| ActiveGroupCall::fresh(group_call.id));
        if tracked.id != group_call.id {
            // A different call took over the slot — reset participant
            // state; fresh updates repopulate it.
            *tracked = ActiveGroupCall::fresh(group_call.id);
        }
        let tracked = self.active_group_call.as_mut().expect("just inserted");
        tracked.title = group_call.title.clone();
        tracked.is_video_chat = group_call.is_video_chat;
        tracked.scheduled_start_date = 0;
        tracked.is_joined = group_call.is_joined;
        tracked.need_rejoin = group_call.need_rejoin;
        tracked.can_be_managed = group_call.can_be_managed;
        tracked.is_owned = group_call.is_owned;
        tracked.participant_count = group_call.participant_count;
        tracked.loaded_all_participants = group_call.loaded_all_participants;
        tracked.is_my_video_enabled = group_call.is_my_video_enabled;
        tracked.is_my_video_paused = group_call.is_my_video_paused;
        tracked.can_enable_video = group_call.can_enable_video;
        tracked.mute_new_participants = group_call.mute_new_participants;
        tracked.can_toggle_mute_new_participants = group_call.can_toggle_mute_new_participants;
        // Phase C2h: in-call chat flags + recording state drive the
        // management UI.
        tracked.can_send_messages = group_call.can_send_messages;
        tracked.are_messages_allowed = group_call.are_messages_allowed;
        tracked.can_toggle_are_messages_allowed = group_call.can_toggle_are_messages_allowed;
        tracked.can_delete_messages = group_call.can_delete_messages;
        tracked.record_duration = group_call.record_duration;
        tracked.is_video_recorded = group_call.is_video_recorded;
        tracked.recent_speaker_order = group_call
            .recent_speakers
            .iter()
            .map(|(sender, _)| *sender)
            .collect();
        if group_call.need_rejoin {
            tracked.reconnecting = true;
        } else if group_call.is_joined {
            tracked.reconnecting = false;
            // Phase C2f: a clean joined update means the rejoin
            // succeeded — the attempt counter starts over.
            tracked.rejoin_attempts = 0;
            // Phase C2f: the failure is resolved — a stale
            // group-call error line would lie now.
            self.group_call_error = None;
        }
        tracked.sort_participants();
    }

    /// Phase C3a: `updateGroupCallParticipant`. Upserts the participant;
    /// an empty `order` removes them (schema note on
    /// `groupCallParticipant.order`). Updates for an untracked call id
    /// are ignored.
    pub(crate) fn accept_group_call_participant_update(
        &mut self,
        group_call_id: i32,
        participant: &ParsedGroupCallParticipant,
    ) {
        let Some(tracked) = self.active_group_call.as_mut() else {
            return;
        };
        if tracked.id != group_call_id {
            return;
        }
        if participant.order.is_empty() {
            tracked
                .participants
                .retain(|p| p.participant_id != participant.participant_id);
        } else if let Some(existing) = tracked
            .participants
            .iter_mut()
            .find(|p| p.participant_id == participant.participant_id)
        {
            *existing = participant.clone();
        } else {
            tracked.participants.push(participant.clone());
        }
        tracked.sort_participants();
    }

    /// Phase C3a: `updateGroupCallParticipants`. Drops user participants
    /// not in the reported id list. Chat senders (`MessageSender::Chat`)
    /// are kept — the update only carries user ids, so chat senders
    /// can't be verified against it.
    pub(crate) fn accept_group_call_participants_update(
        &mut self,
        group_call_id: i32,
        participant_user_ids: &[i64],
    ) {
        let Some(tracked) = self.active_group_call.as_mut() else {
            return;
        };
        if tracked.id != group_call_id {
            return;
        }
        tracked.participants.retain(|p| match p.participant_id {
            MessageSender::User { user_id } => participant_user_ids.contains(&user_id),
            MessageSender::Chat { .. } => true,
        });
        tracked.sort_participants();
    }

    /// Phase C2h: `updateNewGroupCallMessage`. Appends to the
    /// in-call chat feed (dedup by message_id; capped — TDLib offers
    /// no history getter for group-call messages).
    pub(crate) fn accept_new_group_call_message(
        &mut self,
        group_call_id: i32,
        message: &ParsedGroupCallMessage,
    ) {
        let Some(tracked) = self.active_group_call.as_mut() else {
            return;
        };
        if tracked.id != group_call_id {
            return;
        }
        if !tracked
            .messages
            .iter()
            .any(|m| m.message_id == message.message_id)
        {
            tracked.messages.push(message.clone());
            // ponytail: hard cap — no history API exists to backfill.
            if tracked.messages.len() > 200 {
                tracked.messages.remove(0);
            }
        }
    }

    /// Phase C2h: `updateGroupCallMessagesDeleted`. Drops the
    /// deleted ids from the in-call chat feed.
    pub(crate) fn accept_group_call_messages_deleted(
        &mut self,
        group_call_id: i32,
        message_ids: &[i32],
    ) {
        let Some(tracked) = self.active_group_call.as_mut() else {
            return;
        };
        if tracked.id != group_call_id {
            return;
        }
        tracked
            .messages
            .retain(|m| !message_ids.contains(&m.message_id));
    }

    /// Phase C3a: `updateGroupCallVerificationState`. Stores the E2E
    /// emoji check for the tracked call; ignored on id mismatch.
    pub(crate) fn accept_group_call_verification_state(
        &mut self,
        group_call_id: i32,
        generation: i32,
        emojis: &[String],
    ) {
        let Some(tracked) = self.active_group_call.as_mut() else {
            return;
        };
        if tracked.id != group_call_id {
            return;
        }
        tracked.verification = Some(GroupCallVerificationState {
            generation,
            emojis: emojis.to_vec(),
        });
    }

    /// Phase C3a: `updateChatVideoChat`. Refreshes the chat's join
    /// affordance (`group_call_id` 0 → no active video chat).
    pub(crate) fn accept_chat_video_chat(&mut self, chat_id: ChatId, video_chat: &ParsedVideoChat) {
        let chat = self
            .chats
            .entry(chat_id.0)
            .or_insert_with(|| placeholder_chat(chat_id));
        chat.video_chat = if video_chat.group_call_id == 0 {
            None
        } else {
            Some(VideoChatInfo {
                group_call_id: video_chat.group_call_id,
                has_participants: video_chat.has_participants,
            })
        };
    }

    /// Phase C3a: drop the tracked group call after the local user
    /// leaves or ends it.
    pub fn leave_group_call_local(&mut self) {
        self.active_group_call = None;
    }

    /// Phase C3a: flip the local-only self-mute state. There is no
    /// TDLib "mute self" request for group calls outside the join
    /// parameters, and no audio path exists yet (C2) — the UI labels
    /// this honestly as local-only.
    pub fn set_group_call_self_muted(&mut self, muted: bool) {
        if let Some(tracked) = self.active_group_call.as_mut() {
            tracked.is_muted_self = muted;
        }
    }

    /// Phase C3a: clear the `reconnecting` flag once a rejoin has been
    /// issued by the driver.
    pub fn clear_group_call_reconnecting(&mut self) {
        if let Some(tracked) = self.active_group_call.as_mut() {
            tracked.reconnecting = false;
        }
    }

    /// Phase C3a: store the `joinVideoChat` `Text` response payload on
    /// the tracked call. Phase C2g consumes it in the driver pump to
    /// finish the native group handshake.
    pub fn set_group_call_join_payload(&mut self, group_call_id: i32, payload: String) {
        if let Some(tracked) = self.active_group_call.as_mut()
            && tracked.id == group_call_id
        {
            tracked.join_payload = payload;
        }
    }

    /// Phase C2g: store the `startGroupCallScreenSharing` `Text`
    /// response on the tracked call; the driver pump consumes it to
    /// finish the presentation handshake.
    pub fn set_group_call_screen_share_answer(&mut self, group_call_id: i32, payload: String) {
        if let Some(tracked) = self.active_group_call.as_mut()
            && tracked.id == group_call_id
        {
            tracked.screen_share_answer = payload;
        }
    }

    /// Phase C3a: store the `getVideoChatInviteLink` `HttpUrl` response
    /// on the tracked call.
    pub fn set_group_call_invite_link(&mut self, group_call_id: i32, link: String) {
        if let Some(tracked) = self.active_group_call.as_mut()
            && tracked.id == group_call_id
        {
            tracked.invite_link = Some(link);
        }
    }
}
