//! Connect driver: video chat settings (title, links, recording, RTMP, messages).
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
    /// Phase C3a: `toggleVideoChatMuteNewParticipants` (schema 1.8.67,
    /// :14317). Gated on `groupCall.can_toggle_mute_new_participants`.
    pub fn toggle_video_chat_mute_new(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let (group_call_id, mute_new) = match &self.session.calls.active_group_call {
            Some(call) if call.can_toggle_mute_new_participants => {
                (call.id, !call.mute_new_participants)
            }
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::ToggleVideoChatMuteNew { group_call_id }),
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&toggle_video_chat_mute_new_participants(
                extra,
                group_call_id,
                mute_new,
            ))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C3a: `setVideoChatTitle` (schema 1.8.67, :14312). Gated on
    /// `groupCall.can_be_managed`.
    pub fn set_video_chat_title(&mut self, title: String) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.calls.active_group_call {
            Some(call) if call.can_be_managed => call.id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let title = title.trim().to_string();
        if title.is_empty() || title.chars().count() > 64 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::SetVideoChatTitle { group_call_id }),
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&set_video_chat_title(extra, group_call_id, &title))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C3a: `getVideoChatInviteLink` (schema 1.8.67, :14395). The
    /// `HttpUrl` answer is stored on the tracked call for the UI to
    /// show. `can_self_unmute: true` requires `can_be_managed` — the
    /// caller passes `call.can_be_managed`.
    pub fn fetch_video_chat_invite_link(
        &mut self,
        can_self_unmute: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.calls.active_group_call {
            Some(call) => call.id,
            None => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::GetVideoChatInviteLink { group_call_id }),
            None,
        );
        if let Err(err) = self.sender.send_json(&get_video_chat_invite_link(
            extra,
            group_call_id,
            can_self_unmute,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C2h: `revokeGroupCallInviteLink` (schema 1.8.67,
    /// :14398). Gated on `groupCall.can_be_managed` (video chats).
    pub fn revoke_video_chat_invite_link(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.calls.active_group_call {
            Some(call) if call.can_be_managed => call.id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::RevokeVideoChatInviteLink { group_call_id }),
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&revoke_group_call_invite_link(extra, group_call_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C2h: `startGroupCallRecording` (schema 1.8.67, :14405).
    /// Gated on `groupCall.can_be_managed` and `is_video_chat`
    /// (schema: "for video chats only"). Recording state arrives as
    /// `updateGroupCall` (`record_duration` / `is_video_recorded`).
    pub fn start_group_call_recording(
        &mut self,
        title: String,
        record_video: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.calls.active_group_call {
            Some(call) if call.can_be_managed && call.is_video_chat => call.id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let title = title.trim().to_string();
        if title.chars().count() > 64 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::StartGroupCallRecording { group_call_id }),
            None,
        );
        if let Err(err) = self.sender.send_json(&start_group_call_recording(
            extra,
            group_call_id,
            &title,
            record_video,
            false,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C2h: `endGroupCallRecording` (schema 1.8.67, :14408).
    /// Gated on `groupCall.can_be_managed` and `is_video_chat`
    /// (schema: "for video chats only"), matching the start gate.
    pub fn stop_group_call_recording(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.calls.active_group_call {
            Some(call) if call.can_be_managed && call.is_video_chat => call.id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::EndGroupCallRecording { group_call_id }),
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&end_group_call_recording(extra, group_call_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C2h: `startScheduledVideoChat` (schema 1.8.67, :14277).
    /// Starts the tracked scheduled (not-yet-active) video chat early.
    /// Gated on `groupCall.can_be_managed && scheduled_start_date > 0`
    /// — the schema names no explicit right for this constructor, so
    /// `can_be_managed` (the tracked proxy for the
    /// `can_manage_video_chats` admin right) matches the other
    /// video-chat admin actions.
    pub fn start_scheduled_video_chat(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.calls.active_group_call {
            Some(call) if call.can_be_managed && call.scheduled_start_date > 0 => call.id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::StartScheduledVideoChat { group_call_id }),
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&start_scheduled_video_chat(extra, group_call_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// `toggleVideoChatEnabledStartNotification` (schema 1.8.67,
    /// :14282): "notify me when this scheduled video chat starts".
    /// Gated on the tracked call still being scheduled — the schema
    /// marks the constructor for video chats (any viewer can set it;
    /// no admin right needed). The new flag arrives back as
    /// `updateGroupCall` (`enabled_start_notification`, :7154), which
    /// the reducer already stores on the tracked call.
    pub fn toggle_video_chat_start_notification(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let (group_call_id, enabled) = match &self.session.calls.active_group_call {
            Some(call) if call.scheduled_start_date > 0 => {
                (call.id, !call.enabled_start_notification)
            }
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::ToggleVideoChatEnabledStartNotification {
                group_call_id,
                enabled,
            }),
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&toggle_video_chat_enabled_start_notification(
                extra,
                group_call_id,
                enabled,
            ))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C2h: `getVideoChatRtmpUrl` (schema 1.8.67, :14261) — the
    /// request is chat-bound, so resolve the chat from the tracked
    /// call. Gated on `groupCall.can_be_managed` (the schema's
    /// `can_manage_video_chats` admin right is the closest tracked
    /// flag; a 403 surfaces honestly via `group_call_error`).
    pub fn fetch_video_chat_rtmp_url(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let call = match &self.session.calls.active_group_call {
            Some(call) if call.can_be_managed => call,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let call_id = call.id;
        let chat_id = self
            .session
            .chats
            .iter()
            .find(|(_, c)| {
                c.video_chat
                    .as_ref()
                    .is_some_and(|vc| vc.group_call_id == call_id)
            })
            .map(|(id, _)| *id);
        let Some(chat_id) = chat_id else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::GetVideoChatRtmpUrl { chat_id }),
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&get_video_chat_rtmp_url(extra, chat_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C2h: `replaceVideoChatRtmpUrl` (schema 1.8.67, :14264) —
    /// regenerates the RTMP URL + stream key. Requires owner
    /// privileges; `groupCall.is_owned` is the closest tracked flag
    /// and a 403 surfaces honestly.
    pub fn replace_video_chat_rtmp_url(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let call = match &self.session.calls.active_group_call {
            Some(call) if call.is_owned => call,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let call_id = call.id;
        let chat_id = self
            .session
            .chats
            .iter()
            .find(|(_, c)| {
                c.video_chat
                    .as_ref()
                    .is_some_and(|vc| vc.group_call_id == call_id)
            })
            .map(|(id, _)| *id);
        let Some(chat_id) = chat_id else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::ReplaceVideoChatRtmpUrl { chat_id }),
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&replace_video_chat_rtmp_url(extra, chat_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C2h: `sendGroupCallMessage` (schema 1.8.67, :14341).
    /// Gated on `groupCall.can_send_messages` and
    /// `are_messages_allowed`. The echo arrives as
    /// `updateNewGroupCallMessage`; there is no history getter, so
    /// the UI shows the live feed only.
    pub fn send_group_call_message(&mut self, text: String) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.calls.active_group_call {
            Some(call) if call.can_send_messages && call.are_messages_allowed => call.id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let text = text.trim().to_string();
        // ponytail: the true cap is getOption
        // "group_call_message_text_length_max" (server-enforced);
        // 4096 chars is just a client-side sanity guard.
        if text.is_empty() || text.chars().count() > 4096 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::SendGroupCallMessage { group_call_id }),
            None,
        );
        if let Err(err) =
            self.sender
                .send_json(&send_group_call_message(extra, group_call_id, &text))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C2h: `toggleGroupCallAreMessagesAllowed` (schema 1.8.67,
    /// :14322). Gated on `can_toggle_are_messages_allowed`; flips the
    /// current `are_messages_allowed`.
    pub fn toggle_group_call_are_messages_allowed(
        &mut self,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let (group_call_id, new_value) = match &self.session.calls.active_group_call {
            Some(call) if call.can_toggle_are_messages_allowed => {
                (call.id, !call.are_messages_allowed)
            }
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::ToggleGroupCallAreMessagesAllowed {
                group_call_id,
            }),
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&toggle_group_call_are_messages_allowed(
                extra,
                group_call_id,
                new_value,
            ))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C3a: `loadGroupCallParticipants` (schema 1.8.67, :14455)
    /// — page more participants (up to 100). Gated on
    /// `!loaded_all_participants`.
    pub fn load_more_group_call_participants(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.calls.active_group_call {
            Some(call) if !call.loaded_all_participants => call.id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::LoadGroupCallParticipants { group_call_id }),
            None,
        );
        if let Err(err) =
            self.sender
                .send_json(&load_group_call_participants(extra, group_call_id, 100))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }
}
