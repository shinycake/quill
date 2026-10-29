//! Connect driver: in-call controls (screen share, mute, video, participants, invitations).
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
    /// Phase C2g: start or stop screen sharing in the tracked group
    /// call. Starting goes through the native presentation handshake:
    /// `ntg_init_presentation` yields the offer that
    /// `startGroupCallScreenSharing` (schema 1.8.67, :14303) carries;
    /// its `Text` answer is consumed by the driver pump
    /// (`ntg_connect(..., is_presentation=true)` + desktop capture).
    /// Stopping pairs `endGroupCallScreenSharing` (:14309) with
    /// `ntg_stop_presentation`. Requires the joined call and an
    /// available engine; without one the toggle is rejected.
    pub fn toggle_group_call_screen_share(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.calls.active_group_call {
            Some(call) if call.is_joined => call.id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let engine_available = self
            .call_engine
            .as_ref()
            .is_some_and(|engine| engine.is_available());
        if !engine_available {
            return Err(ConnectSendError::InvalidRequest);
        }
        let sharing = self
            .session
            .calls
            .active_group_call
            .as_ref()
            .is_some_and(|call| call.screen_sharing || call.screen_share_pending);
        if sharing {
            let extra = self.session.request(
                RequestPurpose::Calls(CallsPurpose::EndGroupCallScreenSharing { group_call_id }),
                None,
            );
            if let Err(err) = self
                .sender
                .send_json(&end_group_call_screen_sharing(extra, group_call_id))
            {
                self.session.requests.take(extra);
                return Err(err);
            }
            if let Some(engine) = self.call_engine.as_deref_mut()
                && let Err(err) = engine.stop_screen_share(group_call_id)
                && let Some(call) = self.session.calls.active_group_call.as_mut()
            {
                call.transport_error = Some(err.to_string());
            }
            if let Some(call) = self.session.calls.active_group_call.as_mut() {
                call.screen_sharing = false;
                call.screen_share_pending = false;
                call.screen_share_paused = false;
                call.screen_share_answer.clear();
            }
            return Ok(extra);
        }
        // Honest gate: the presentation handshake needs a screen-capture
        // source (the engine's `MediaDeviceKind::Screen` list, or the
        // chooser's pick when Quill captures the screen). Stopping needs
        // none: a vanished source must not trap the user in "sharing".
        if !self.group_call_screen_source_available() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(engine) = self.call_engine.as_deref_mut() else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let offer = match engine.start_screen_share(group_call_id) {
            Ok(offer) => offer,
            Err(err) => {
                if let Some(call) = self.session.calls.active_group_call.as_mut() {
                    call.transport_error = Some(err.to_string());
                }
                return Err(ConnectSendError::InvalidRequest);
            }
        };
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::StartGroupCallScreenSharing { group_call_id }),
            None,
        );
        if let Err(err) = self.sender.send_json(&start_group_call_screen_sharing(
            extra,
            group_call_id,
            crate::calls::engine::group_offer_audio_source_id(&offer),
            &offer,
        )) {
            self.session.requests.take(extra);
            if let Some(engine) = self.call_engine.as_deref_mut() {
                let _ = engine.stop_screen_share(group_call_id);
            }
            return Err(err);
        }
        if let Some(call) = self.session.calls.active_group_call.as_mut() {
            call.screen_share_pending = true;
        }
        Ok(extra)
    }

    /// Mute or unmute yourself in the joined group call (see
    /// [`Self::set_group_call_self_mute`]).
    pub fn toggle_group_call_self_mute(&mut self) -> Result<(), SelfMuteError> {
        let Some(call) = self.session.calls.active_group_call.as_ref() else {
            return Ok(());
        };
        let muted = !call.is_muted_self;
        self.set_group_call_self_mute(muted)
    }

    /// Set (rather than flip) the self-mute state; push-to-talk drives
    /// this. A no-op when the state already matches.
    ///
    /// The tracked `is_muted_self` flag is what the mute button shows and
    /// what the capture gates read. Muting fails closed: the flag is set
    /// even when the engine refuses (the driver-side capture gate then
    /// stops the audio) and the engine error is reported; the pump retries
    /// the engine. Unmuting is engine-first (the 1:1 `set_call_muted`
    /// discipline): an engine error leaves you muted. Unmuting is refused
    /// while an admin holds you muted (`can_unmute_self` false, which
    /// includes a raised hand). Telegram is told afterwards
    /// (`toggleGroupCallParticipantIsMuted` on yourself); while that
    /// request is in flight `Session` ignores own-participant updates that
    /// disagree, and the driver pump makes the engine follow the settled
    /// flag.
    pub fn set_group_call_self_mute(&mut self, muted: bool) -> Result<(), SelfMuteError> {
        let Some(call) = self.session.calls.active_group_call.as_ref() else {
            return Ok(());
        };
        if call.is_muted_self == muted {
            return Ok(());
        }
        let group_call_id = call.id;
        let me_row = call.participants.iter().find(|p| p.is_current_user);
        if !muted && call.is_force_muted() {
            return Err(SelfMuteError::NotAllowed);
        }
        let me = me_row.map(|p| p.participant_id);
        // Before the transport is up the engine only stores the state for
        // the connect; with it up the call is real and may fail.
        let applied = match self.call_engine.as_deref_mut() {
            Some(engine) if engine.is_available() => {
                Some(engine.set_group_muted(group_call_id, muted))
            }
            _ => None,
        };
        let mut engine_error = None;
        match applied {
            Some(Ok(())) => {
                self.group_mute_state.insert(group_call_id, muted);
            }
            Some(Err(err)) if !muted => return Err(SelfMuteError::Engine(err)),
            Some(Err(err)) => engine_error = Some(err),
            None => {}
        }
        self.session.set_group_call_self_muted(muted);
        if let Some(me) = me {
            self.toggle_group_call_participant_muted(me, muted)
                .map_err(SelfMuteError::Send)?;
        }
        match engine_error {
            Some(err) => Err(SelfMuteError::Engine(err)),
            None => Ok(()),
        }
    }

    /// Phase C3a: `toggleGroupCallIsMyVideoEnabled` (schema 1.8.67,
    /// :14414). Tracks the TDLib flag; Phase C2g applies it to the
    /// native transport in the driver pump (`set_group_camera`).
    pub fn toggle_group_call_my_video(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let (group_call_id, enable) = match &self.session.calls.active_group_call {
            Some(call) => (call.id, !call.is_my_video_enabled),
            None => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::ToggleGroupCallVideo { group_call_id }),
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&toggle_group_call_is_my_video_enabled(
                extra,
                group_call_id,
                enable,
            ))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C3a: `toggleGroupCallIsMyVideoPaused` (schema 1.8.67,
    /// :14411). Signaling-only: tracks state, no camera (Phase C2).
    pub fn toggle_group_call_my_video_paused(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let (group_call_id, pause) = match &self.session.calls.active_group_call {
            Some(call) if call.is_my_video_enabled => (call.id, !call.is_my_video_paused),
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::ToggleGroupCallVideo { group_call_id }),
            None,
        );
        if let Err(err) = self.sender.send_json(&toggle_group_call_is_my_video_paused(
            extra,
            group_call_id,
            pause,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C3a: `toggleGroupCallParticipantIsMuted` (schema 1.8.67,
    /// :14431). The caller gates on the participant's
    /// `can_be_muted_for_all_users` / `can_be_unmuted_for_all_users`.
    pub fn toggle_group_call_participant_muted(
        &mut self,
        participant_id: MessageSender,
        mute: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.calls.active_group_call {
            Some(call) => call.id,
            None => return Err(ConnectSendError::InvalidRequest),
        };
        let sender_ref = match participant_id {
            MessageSender::User { user_id } => MessageSenderRef::User(user_id),
            MessageSender::Chat { chat_id } => MessageSenderRef::Chat(chat_id),
        };
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::ToggleGroupCallParticipantMute { group_call_id }),
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&toggle_group_call_participant_is_muted(
                extra,
                group_call_id,
                &sender_ref,
                mute,
            ))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C3a: `toggleGroupCallParticipantIsHandRaised` (schema
    /// 1.8.67, :14444). Only the self hand can be raised; lowering
    /// others' hands requires `groupCall.can_be_managed` (gated by the
    /// caller).
    pub fn toggle_group_call_participant_hand(
        &mut self,
        participant_id: MessageSender,
        raise: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.calls.active_group_call {
            Some(call) => call.id,
            None => return Err(ConnectSendError::InvalidRequest),
        };
        let sender_ref = match participant_id {
            MessageSender::User { user_id } => MessageSenderRef::User(user_id),
            MessageSender::Chat { chat_id } => MessageSenderRef::Chat(chat_id),
        };
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::ToggleGroupCallParticipantHand { group_call_id }),
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&toggle_group_call_participant_is_hand_raised(
                extra,
                group_call_id,
                &sender_ref,
                raise,
            ))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Invite a user to the tracked call. A video chat (bound to a chat)
    /// takes `inviteVideoChatParticipants`; a group call that isn't bound
    /// to a chat takes `inviteGroupCallParticipant` ("for group calls not
    /// bound to a chat only", schema 1.8.68), whose `is_video` is false:
    /// Quill joins those calls without video.
    pub fn invite_group_call_participant(
        &mut self,
        user_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let (group_call_id, is_video_chat) = match &self.session.calls.active_group_call {
            Some(call) => (call.id, call.is_video_chat),
            None => return Err(ConnectSendError::InvalidRequest),
        };
        let purpose = if is_video_chat {
            CallsPurpose::InviteVideoChatParticipants { group_call_id }
        } else {
            CallsPurpose::InviteGroupCallParticipant { group_call_id }
        };
        let extra = self.session.request(RequestPurpose::Calls(purpose), None);
        let json = if is_video_chat {
            invite_video_chat_participants(extra, group_call_id, &[user_id])
        } else {
            invite_group_call_participant(extra, group_call_id, user_id, false)
        };
        if let Err(err) = self.sender.send_json(&json) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C2f: `banGroupCallParticipants` (schema 1.8.67, :14385)
    /// for a single participant. Takes `user_ids` (int64 user ids —
    /// `messageSenderChat` participants cannot be banned); requires
    /// `groupCall.is_owned` — the owner can ban, not `can_be_managed`
    /// admins (that's "for video chats and live stories only").
    pub fn ban_group_call_participant(
        &mut self,
        user_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.calls.active_group_call {
            Some(call) if call.is_owned => call.id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::BanGroupCallParticipants { group_call_id }),
            None,
        );
        if let Err(err) = self.sender.send_json(&ban_group_call_participants(
            extra,
            group_call_id,
            &[user_id],
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C2f: `setGroupCallParticipantVolumeLevel` (schema 1.8.67,
    /// :14438). Clamps to the schema's 1-20000 (hundreds of percents)
    /// before sending.
    pub fn set_group_call_participant_volume(
        &mut self,
        participant_id: MessageSender,
        volume_level: i32,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.calls.active_group_call {
            Some(call) => call.id,
            None => return Err(ConnectSendError::InvalidRequest),
        };
        let sender_ref = match participant_id {
            MessageSender::User { user_id } => MessageSenderRef::User(user_id),
            MessageSender::Chat { chat_id } => MessageSenderRef::Chat(chat_id),
        };
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::SetGroupCallParticipantVolumeLevel {
                group_call_id,
            }),
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&set_group_call_participant_volume_level(
                extra,
                group_call_id,
                &sender_ref,
                volume_level.clamp(1, 20000),
            ))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C2f: accept a `messageGroupCall` invitation via
    /// `joinGroupCall` (schema 1.8.67, line 5288: "Use joinGroupCall
    /// to accept the call"). Refuses while a 1:1 call is active, like
    /// the chat-bound join; the joined call is tracked via
    /// `updateGroupCall`.
    pub fn accept_group_call_invitation(
        &mut self,
        chat_id: i64,
        message_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.calls.active_call.is_some() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::JoinGroupCallInvitation, None);
        let input = InputGroupCallRef::Message {
            chat_id,
            message_id,
        };
        let params = GroupCallJoinParams::honest_no_device();
        if let Err(err) = self
            .sender
            .send_json(&join_group_call(extra, &input, &params))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C2f: `declineGroupCallInvitation` (schema 1.8.67,
    /// :14380) — declines (or cancels, for the sender) a
    /// `messageGroupCall` invitation.
    pub fn decline_group_call_invitation(
        &mut self,
        chat_id: i64,
        message_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::DeclineGroupCallInvitation {
                chat_id,
                message_id,
            }),
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&decline_group_call_invitation(extra, chat_id, message_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }
}

/// Why a self-mute change did not fully go through.
#[derive(Debug)]
pub enum SelfMuteError {
    /// The engine refused. An unmute changed nothing; a mute still took
    /// effect (the capture gate closes the microphone) and the pump retries
    /// the engine.
    Engine(crate::calls::engine::EngineError),
    /// The microphone changed but Telegram could not be told, so other
    /// participants may still see the old icon.
    Send(ConnectSendError),
    /// An admin holds you muted (`can_unmute_self` false): you cannot unmute
    /// yourself. A raised hand is the same server state, so it is covered
    /// (tdesktop treats `RaisedHand` and `ForceMuted` alike here).
    NotAllowed,
}
