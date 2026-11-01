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
        if let Some(active) = self.calls.active_call.as_mut()
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
        if self.calls.active_call.is_some() {
            // A *different* call id while one is active. A terminal
            // update for the swap-pending/queued incoming call (the
            // caller hung up) clears it; the first pending incoming
            // raises the swap prompt; further ones auto-decline busy.
            let swap_id = self
                .calls
                .swap_pending
                .map(|(id, _, _)| id)
                .or_else(|| self.calls.swap_accept_queued.map(|(id, _)| id));
            if Some(call.id) == swap_id {
                if call.state.is_terminal() {
                    self.calls.swap_pending = None;
                    self.calls.swap_accept_queued = None;
                }
                return;
            }
            if !call.is_outgoing
                && matches!(call.state, CallState::Pending { .. })
                && !self
                    .calls
                    .busy_decline_queue
                    .iter()
                    .any(|(id, _, _)| *id == call.id)
            {
                let note = if self.calls.swap_pending.is_none()
                    && self.calls.swap_accept_queued.is_none()
                {
                    self.calls.swap_pending = Some((call.id, call.user_id, call.is_video));
                    "incoming-while-active-swap-prompt"
                } else {
                    self.calls
                        .busy_decline_queue
                        .push((call.id, call.user_id, call.is_video));
                    "incoming-while-active-busy-decline"
                };
                self.diagnostics.record(Diagnostic {
                    category: "call",
                    type_name: Some("updateCall".to_string()),
                    extra: None,
                    seq: Some(self.last_seq),
                    note,
                });
            }
            return;
        }
        if call.state.is_terminal() {
            self.calls.summary = Some(CallSummary::from_terminal(call, 0, false));
            return;
        }
        self.calls.active_call = Some(ActiveCall {
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
            remote_audio_muted: false,
            remote_battery_low: false,
            signal_bars: None,
        });
        self.calls.summary = None;
        self.calls.error = None;
    }

    /// Phase C2b: `updateNewCallSignalingData`. The driver also feeds these
    /// bytes into the engine; this bounded queue remains the honest
    /// diagnostic record. Data for an unknown call id is dropped (never
    /// buffered without a tracked call).
    pub(crate) fn accept_call_signaling_data(&mut self, call_id: i32, data: Vec<u8>) {
        let Some(active) = self.calls.active_call.as_mut() else {
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
        let Some(active) = self.calls.active_call.take() else {
            return;
        };
        // "Add People" closes with its call: its search goes too.
        self.calls.add_people_search = ShareSearch::default();
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
        // A call taken into a conference call goes on there, with no end
        // screen (tdesktop's panel migrates).
        if !self.note_call_migration(call, &summary) {
            self.calls.summary = Some(summary);
        }
        // The active call ended on its own — a still-open swap prompt
        // is moot (the incoming call now flows through the normal
        // incoming path). A queued post-swap accept survives: the
        // driver issues acceptCall once this terminal update lands.
        self.calls.swap_pending = None;
        self.calls
            .busy_decline_queue
            .retain(|(id, _, _)| *id != call.id);
    }

    /// Phase C3a: `updateGroupCall` state machine. The current call holds
    /// one group call at a time (like the single 1:1 call):
    /// - a different call id replaces the tracked call (participants
    ///   reload via updates);
    /// - `need_rejoin` sets the `reconnecting` flag so the UI shows
    ///   "Reconnecting…" and the driver re-issues the join;
    /// - a fresh joined, non-`need_rejoin` update clears `reconnecting`;
    /// - `!is_active` (ended) drops the tracked call.
    ///
    /// The watched live story's call goes to its own slot
    /// (`story_stream`); any other live story is not tracked.
    pub(crate) fn accept_group_call_update(&mut self, group_call: &ParsedGroupCall) {
        // TDLib confirms the leave: later joined updates are real again.
        if !group_call.is_joined && self.calls.left_group_call == Some(group_call.id) {
            self.calls.left_group_call = None;
        }
        let watched = self.stories.live.as_ref().map(|live| live.group_call_id);
        if watched == Some(group_call.id) {
            self.accept_story_stream_update(group_call);
            return;
        }
        if group_call.is_live_story || !self.group_call_update_may_take_slot(group_call) {
            return;
        }
        if !group_call.is_active {
            // Phase C2h: a scheduled (not yet started) video chat is
            // still tracked — the overlay shows "starts in …" plus an
            // admin-only "Start now" (startScheduledVideoChat,
            // schema/td_api.tl:14277). Join appears once TDLib
            // activates the call.
            if group_call.scheduled_start_date > 0 {
                let tracked = self
                    .calls
                    .active_group_call
                    .get_or_insert_with(|| ActiveGroupCall::fresh(group_call.id));
                if tracked.id != group_call.id {
                    *tracked = ActiveGroupCall::fresh(group_call.id);
                }
                tracked.title = group_call.title.clone();
                tracked.can_be_managed = group_call.can_be_managed;
                tracked.is_owned = group_call.is_owned;
                tracked.is_video_chat = group_call.is_video_chat;
                tracked.is_live_story = group_call.is_live_story;
                tracked.is_rtmp_stream = group_call.is_rtmp_stream;
                tracked.scheduled_start_date = group_call.scheduled_start_date;
                tracked.enabled_start_notification = group_call.enabled_start_notification;
                return;
            }
            if self
                .calls
                .active_group_call
                .as_ref()
                .is_some_and(|c| c.id == group_call.id)
            {
                self.calls.active_group_call = None;
                self.calls.invite_results = None;
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
            .calls
            .active_group_call
            .get_or_insert_with(|| ActiveGroupCall::fresh(group_call.id));
        if tracked.id != group_call.id {
            // A different call took over the slot — reset participant
            // state; fresh updates repopulate it.
            *tracked = ActiveGroupCall::fresh(group_call.id);
        }
        apply_group_call_fields(tracked, group_call);
        if group_call.is_joined && !group_call.need_rejoin {
            // Phase C2f: the failure is resolved — a stale
            // group-call error line would lie now.
            self.calls.group_call_error = None;
        }
    }

    fn accept_story_stream_update(&mut self, group_call: &ParsedGroupCall) {
        if !group_call.is_active {
            self.calls.story_stream = None;
            return;
        }
        let fresh = || ActiveGroupCall {
            is_muted_self: true,
            ..ActiveGroupCall::fresh(group_call.id)
        };
        let tracked = self.calls.story_stream.get_or_insert_with(fresh);
        if tracked.id != group_call.id {
            *tracked = fresh();
        }
        apply_group_call_fields(tracked, group_call);
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
        // Your own mute change is in flight: an own-row update that
        // disagrees is older than the change (a stale speaking or volume
        // update) and must not undo it, or the microphone would reopen for
        // a round trip. Once the request completed the next update settles.
        let toggle_in_flight = self.requests.has_purpose(RequestPurpose::Calls(
            CallsPurpose::ToggleGroupCallParticipantMute { group_call_id },
        ));
        let Some(tracked) = self.calls.group_call_mut(group_call_id) else {
            return;
        };
        // Telegram is the truth for your own mute once joined (it reports
        // a mute you asked for, and one an admin imposed); the driver pump
        // makes the engine follow.
        if participant.is_current_user
            && tracked.is_joined
            && !participant.order.is_empty()
            && !toggle_in_flight
        {
            tracked.is_muted_self = participant.is_muted_for_all_users;
        }
        let participants = tracked.participants.edit();
        if participant.order.is_empty() {
            participants.retain(|p| p.participant_id != participant.participant_id);
        } else if let Some(existing) = participants
            .iter_mut()
            .find(|p| p.participant_id == participant.participant_id)
        {
            *existing = participant.clone();
        } else {
            participants.push(participant.clone());
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
        let Some(tracked) = self.calls.group_call_mut(group_call_id) else {
            return;
        };
        tracked
            .participants
            .edit()
            .retain(|p| match p.participant_id {
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
        let Some(tracked) = self.calls.group_call_mut(group_call_id) else {
            return;
        };
        if !tracked
            .messages
            .iter()
            .any(|m| m.message_id == message.message_id)
        {
            tracked.messages.push(message.clone());
            if tracked.is_live_story {
                // Live stories keep pinned comments past the cap.
                self.after_live_story_message(group_call_id, message);
                return;
            }
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
        let Some(tracked) = self.calls.group_call_mut(group_call_id) else {
            return;
        };
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
        let Some(tracked) = self.calls.group_call_mut(group_call_id) else {
            return;
        };
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
                default_participant_id: video_chat.default_participant_id,
            })
        };
    }

    /// `getVideoChatAvailableParticipants` answer: store the "join as"
    /// choices on the tracked call and settle the current pick.
    pub(crate) fn set_group_call_join_as_options(
        &mut self,
        group_call_id: i32,
        senders: Vec<MessageSender>,
    ) {
        let default = self.chats.values().find_map(|chat| {
            chat.video_chat
                .as_ref()
                .filter(|vc| vc.group_call_id == group_call_id)
                .and_then(|vc| vc.default_participant_id)
        });
        if let Some(tracked) = self.calls.active_group_call.as_mut()
            && tracked.id == group_call_id
        {
            tracked.join_as = pick_join_as(&senders, tracked.join_as, default);
            tracked.join_as_options = senders;
        }
    }

    /// Choose the identity for the next join (also `None` = yourself).
    pub fn set_group_call_join_as(&mut self, sender: Option<MessageSender>) {
        if let Some(tracked) = self.calls.active_group_call.as_mut() {
            tracked.join_as = sender;
        }
    }

    /// Whether an update (or a `getGroupCall` answer) for `group_call`
    /// may take the current call slot. An update for the tracked call
    /// always may. One you are not in never replaces a call you are in,
    /// scheduled updates included (a looked-up `getGroupCall` or another
    /// chat's call). A late joined update for the call just left is not
    /// tracked either.
    fn group_call_update_may_take_slot(&self, group_call: &ParsedGroupCall) -> bool {
        if group_call.is_joined {
            return self.calls.left_group_call != Some(group_call.id);
        }
        match self.calls.active_group_call.as_ref() {
            Some(tracked) => tracked.id == group_call.id || !tracked.is_joined,
            None => true,
        }
    }

    /// Phase C3a: drop the tracked group call after the local user
    /// leaves or ends it. The driver's `drop_group_call_locally` also
    /// stops the call's native transport.
    pub fn leave_group_call_local(&mut self) {
        self.calls.active_group_call = None;
        self.calls.call_link = None;
        self.calls.invite_results = None;
    }

    /// Set the tracked self-mute flag (what the mute button shows and the
    /// capture gates read). The driver applies it to the engine and tells
    /// Telegram; the demo session just flips it.
    pub fn set_group_call_self_muted(&mut self, muted: bool) {
        if let Some(tracked) = self.calls.active_group_call.as_mut() {
            tracked.is_muted_self = muted;
        }
    }

    /// Phase C3a: clear the `reconnecting` flag once a rejoin has been
    /// issued by the driver.
    pub fn clear_group_call_reconnecting(&mut self) {
        if let Some(tracked) = self.calls.active_group_call.as_mut() {
            tracked.reconnecting = false;
        }
    }

    /// Phase C3a: store the `joinVideoChat` `Text` response payload on
    /// the tracked call. Phase C2g consumes it in the driver pump to
    /// finish the native group handshake.
    pub fn set_group_call_join_payload(&mut self, group_call_id: i32, payload: String) {
        if let Some(tracked) = self.calls.group_call_mut(group_call_id) {
            tracked.join_payload = payload;
        }
    }

    /// Phase C2g: store the `startGroupCallScreenSharing` `Text`
    /// response on the tracked call; the driver pump consumes it to
    /// finish the presentation handshake.
    pub fn set_group_call_screen_share_answer(&mut self, group_call_id: i32, payload: String) {
        if let Some(tracked) = self.calls.active_group_call.as_mut()
            && tracked.id == group_call_id
        {
            tracked.screen_share_answer = payload;
        }
    }

    /// Store a `getVideoChatInviteLink` `HttpUrl` answer on the tracked
    /// call: the speaker link when the request had `can_self_unmute`.
    pub fn set_group_call_invite_link(&mut self, group_call_id: i32, speaker: bool, link: String) {
        if let Some(tracked) = self.calls.active_group_call.as_mut()
            && tracked.id == group_call_id
        {
            tracked.share_links.answered(speaker, link);
        }
    }
}

fn apply_group_call_fields(tracked: &mut ActiveGroupCall, group_call: &ParsedGroupCall) {
    tracked.title = group_call.title.clone();
    tracked.invite_link = group_call.invite_link.clone();
    tracked.is_video_chat = group_call.is_video_chat;
    tracked.is_live_story = group_call.is_live_story;
    tracked.is_rtmp_stream = group_call.is_rtmp_stream;
    tracked.scheduled_start_date = 0;
    tracked.is_joined = group_call.is_joined;
    tracked.need_rejoin = group_call.need_rejoin;
    tracked.can_be_managed = group_call.can_be_managed;
    tracked.is_owned = group_call.is_owned;
    tracked.participant_count = group_call.participant_count;
    tracked.viewer_count = u32::try_from(group_call.participant_count).unwrap_or(0);
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
    tracked.paid_message_star_count = group_call.paid_message_star_count;
    tracked.message_sender_id = group_call.message_sender_id;
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
    }
    tracked.sort_participants();
}

/// The "join as" identity to preselect: an earlier pick that is still on
/// offer wins, then the chat's saved default, otherwise nothing (the
/// server joins as the current user).
pub(crate) fn pick_join_as(
    options: &[MessageSender],
    current: Option<MessageSender>,
    default: Option<MessageSender>,
) -> Option<MessageSender> {
    current
        .filter(|c| options.contains(c))
        .or_else(|| default.filter(|d| options.contains(d)))
}

#[cfg(test)]
mod join_as_tests {
    use super::pick_join_as;
    use crate::telegram::envelope::MessageSender;

    const ME: MessageSender = MessageSender::User { user_id: 1 };
    const CHANNEL: MessageSender = MessageSender::Chat { chat_id: -100 };
    const OTHER: MessageSender = MessageSender::Chat { chat_id: -200 };

    #[test]
    fn earlier_pick_beats_the_saved_default() {
        let options = [ME, CHANNEL];
        assert_eq!(pick_join_as(&options, Some(ME), Some(CHANNEL)), Some(ME));
    }

    #[test]
    fn saved_default_is_used_when_nothing_was_picked() {
        let options = [ME, CHANNEL];
        assert_eq!(pick_join_as(&options, None, Some(CHANNEL)), Some(CHANNEL));
    }

    #[test]
    fn identities_that_are_no_longer_offered_are_dropped() {
        let options = [ME];
        assert_eq!(pick_join_as(&options, Some(OTHER), Some(CHANNEL)), None);
    }
}
