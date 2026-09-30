//! Connect driver: group calls and video chats.
use super::*;
use crate::calls::engine::{GroupVideoSource, group_offer_audio_source_id};
use crate::ids::RequestId;
use crate::state::RequestPurpose;
use crate::telegram::envelope::{ChatKind, MessageSender};
use crate::telegram::requests::{
    GroupCallJoinParams, InputGroupCallRef, MessageSenderRef, ban_group_call_participants,
    create_video_chat, decline_group_call_invitation, end_group_call, end_group_call_recording,
    end_group_call_screen_sharing, get_group_call, get_video_chat_invite_link,
    get_video_chat_rtmp_url, invite_group_call_participant, join_group_call, join_video_chat,
    leave_group_call, load_group_call_participants, replace_video_chat_rtmp_url,
    revoke_group_call_invite_link, send_group_call_message,
    set_group_call_participant_volume_level, set_video_chat_title, start_group_call_recording,
    start_group_call_screen_sharing, start_scheduled_video_chat,
    toggle_group_call_are_messages_allowed, toggle_group_call_is_my_video_enabled,
    toggle_group_call_is_my_video_paused, toggle_group_call_participant_is_hand_raised,
    toggle_group_call_participant_is_muted, toggle_video_chat_enabled_start_notification,
    toggle_video_chat_mute_new_participants,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// Phase C2g: native group-call transport lifecycle, after every
    /// reducer update. Finishes the `ntg_connect` handshake once the
    /// `joinVideoChat` `Text` answer arrives, keeps the outgoing camera
    /// and the incoming video subscriptions in sync with the tracked
    /// participants, finishes the presentation handshake, and tears the
    /// transport down when the tracked call goes away.
    pub(crate) fn pump_group_call_transport(
        &mut self,
        active_group_call_before: Option<i32>,
    ) -> Result<(), ConnectSendError> {
        let active_group_call_after = self.session.active_group_call.as_ref().map(|call| call.id);
        if active_group_call_after != active_group_call_before
            && let Some(before_id) = active_group_call_before
        {
            // The tracked call ended or was replaced: the native transport
            // must not linger, and stale frames must not render.
            if let Some(engine) = self.call_engine.as_deref_mut() {
                let _ = engine.leave_group_call(before_id);
            }
            self.group_video_frame_slots
                .lock()
                .expect("group video frame slots")
                .retain(|(slot_call_id, _, _), _| *slot_call_id != before_id);
            // Slice calls-group-self-tile: the self tile lives in the
            // shared (call id, is_local, is_screen) slots — clear it too so
            // a stale local preview can't render after the call ends.
            self.video_frame_slots
                .lock()
                .expect("call video frame slots")
                .retain(|(slot_call_id, _, _), _| *slot_call_id != before_id);
            self.group_camera_state.remove(&before_id);
        }
        let Some(group_call_id) = active_group_call_after else {
            return Ok(());
        };
        let engine_available = self
            .call_engine
            .as_ref()
            .is_some_and(|engine| engine.is_available());
        if !engine_available {
            return Ok(());
        }
        // Finish the join handshake once the `joinVideoChat` answer is
        // stored on the tracked call.
        let join_answer = self
            .session
            .active_group_call
            .as_ref()
            .filter(|call| !call.transport_ready)
            .map(|call| {
                (
                    call.join_payload.clone(),
                    call.is_my_video_enabled && !call.is_my_video_paused,
                )
            });
        if let Some((answer, video_enabled)) = join_answer
            && !answer.is_empty()
        {
            let result = self
                .call_engine
                .as_deref_mut()
                .expect("available engine")
                .connect_group_call(group_call_id, &answer, video_enabled);
            let connected = result.is_ok();
            if let Some(call) = self.session.active_group_call.as_mut() {
                match result {
                    Ok(()) => {
                        call.transport_ready = true;
                        call.transport_error = None;
                    }
                    Err(err) => {
                        call.transport_error = Some(err.to_string());
                    }
                }
            }
            if connected {
                self.group_camera_state.insert(group_call_id, video_enabled);
                // Refresh the device cache on connect so the screen-share
                // availability gate sees the engine's real sources.
                self.refresh_call_devices();
            }
        }
        let transport_ready = self
            .session
            .active_group_call
            .as_ref()
            .is_some_and(|call| call.transport_ready);
        if !transport_ready {
            return Ok(());
        }
        // Outgoing camera follows the TDLib video flags; re-issue only on
        // change (mirrors the 1:1 `set_call_camera` discipline).
        let wanted_camera = self
            .session
            .active_group_call
            .as_ref()
            .map(|call| call.is_my_video_enabled && !call.is_my_video_paused)
            .unwrap_or(false);
        if self.group_camera_state.get(&group_call_id) != Some(&wanted_camera) {
            let camera = self.selected_camera.clone();
            let result = self
                .call_engine
                .as_deref_mut()
                .expect("available engine")
                .set_group_camera(group_call_id, wanted_camera, camera.as_deref());
            match result {
                Ok(()) => {
                    self.group_camera_state.insert(group_call_id, wanted_camera);
                    if let Some(call) = self.session.active_group_call.as_mut() {
                        call.transport_error = None;
                    }
                }
                Err(err) => {
                    if let Some(call) = self.session.active_group_call.as_mut() {
                        call.transport_error = Some(err.to_string());
                    }
                }
            }
        }
        // Incoming video follows the participants' `video_info` /
        // `screen_sharing_video_info`; the engine diffs add/remove.
        let sources: Vec<GroupVideoSource> = self
            .session
            .active_group_call
            .as_ref()
            .map(|call| group_video_sources(&call.participants))
            .unwrap_or_default();
        if let Err(err) = self
            .call_engine
            .as_deref_mut()
            .expect("available engine")
            .sync_group_video(group_call_id, &sources)
            && let Some(call) = self.session.active_group_call.as_mut()
        {
            call.transport_error = Some(err.to_string());
        }
        // Finish the presentation handshake once the
        // `startGroupCallScreenSharing` answer is stored.
        let share_answer = self
            .session
            .active_group_call
            .as_ref()
            .filter(|call| call.screen_share_pending)
            .map(|call| call.screen_share_answer.clone());
        if let Some(answer) = share_answer
            && !answer.is_empty()
        {
            let result = self
                .call_engine
                .as_deref_mut()
                .expect("available engine")
                .connect_screen_share(group_call_id, &answer);
            if let Some(call) = self.session.active_group_call.as_mut() {
                match result {
                    Ok(()) => {
                        call.screen_share_pending = false;
                        call.screen_sharing = true;
                        call.screen_share_answer.clear();
                        call.transport_error = None;
                    }
                    Err(err) => {
                        call.screen_share_pending = false;
                        call.screen_share_answer.clear();
                        call.transport_error = Some(err.to_string());
                    }
                }
            }
        }
        // Phase C2g: reconcile the native presentation against the
        // tracked screen-share flags. A failed start request or a bad
        // answer clears the tracked flags without touching the engine,
        // so a stray initialized-but-unwanted presentation is stopped
        // here instead of lingering.
        let want_presentation = self
            .session
            .active_group_call
            .as_ref()
            .is_some_and(|call| call.screen_sharing || call.screen_share_pending);
        if !want_presentation
            && let Some(engine) = self.call_engine.as_deref_mut()
            && engine.presentation_active(group_call_id)
        {
            let _ = engine.stop_screen_share(group_call_id);
        }
        Ok(())
    }

    /// Phase C3a / C2h: `createVideoChat` — start a voice chat on a
    /// group or channel (schema 1.8.67, :14256). Signaling only: the
    /// chat-bound creation path. The `groupCallId` answer queues a
    /// `getGroupCall` fetch; live state arrives as `updateGroupCall`.
    /// `start_date`: Unix timestamp, 0 = start immediately; otherwise
    /// at least 10s and at most 8 days in the future (schema). Empty
    /// title falls back to the chat title (schema).
    pub fn start_video_chat(
        &mut self,
        chat_id: i64,
        title: String,
        start_date: i64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let is_group_or_channel = self.session.chats.get(&chat_id).is_some_and(|chat| {
            matches!(
                chat.kind,
                ChatKind::BasicGroup { .. } | ChatKind::Supergroup { .. }
            )
        });
        if !is_group_or_channel {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.active_group_call.is_some() || self.session.active_call.is_some() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let title = title.trim().to_string();
        if title.chars().count() > 64 {
            return Err(ConnectSendError::InvalidRequest);
        }
        // Schema :14256 — scheduled start must be ≥10s and ≤8d out.
        let start_date = if start_date == 0 {
            0
        } else {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            if !(now + 10..=now + 8 * 86400).contains(&start_date) {
                return Err(ConnectSendError::InvalidRequest);
            }
            start_date.min(i32::MAX as i64) as i32
        };
        let extra = self
            .session
            .request(RequestPurpose::CreateVideoChat { chat_id }, None);
        if let Err(err) = self.sender.send_json(&create_video_chat(
            extra, chat_id, &title, start_date, false,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C3a: `joinVideoChat` for a chat-bound voice chat (schema
    /// 1.8.67, :14292). Joins as self; the TDLib `Text` answer is stored
    /// on the tracked call and consumed by the driver pump to finish the
    /// native handshake.
    /// Phase C2g: the native group transport is created first so the
    /// join carries the real tgcalls offer (`ntg_create_call`) as its
    /// payload, with `audio_source_id` parsed from the offer SDP. When
    /// no engine is available (or the offer fails), the join still goes
    /// out with the honest no-device params — signaling-only, as before.
    pub fn join_video_chat(&mut self, group_call_id: i32) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        // Never join while a 1:1 call is active. A tracked group call is
        // fine to join when it is the same call and not yet joined (the
        // normal flow: getGroupCall creates the unjoined tracker, then the
        // overlay's Join button calls this). Reject a different tracked call
        // or one already joined.
        if self.session.active_call.is_some() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let is_muted_self = match &self.session.active_group_call {
            Some(call) if call.id == group_call_id && !call.is_joined => call.is_muted_self,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        // Phase C2g: the native group context is created inside
        // `group_join_params` so the join carries the real tgcalls offer.
        let params = self.group_join_params(group_call_id, is_muted_self);
        let extra = self
            .session
            .request(RequestPurpose::JoinVideoChat { group_call_id }, None);
        if let Err(err) =
            self.sender
                .send_json(&join_video_chat(extra, group_call_id, None, &params, ""))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C3a: `getGroupCall` for a known call id (schema 1.8.67,
    /// :14274). Used to start tracking a voice chat found via a chat's
    /// `video_chat` affordance.
    pub fn fetch_group_call(&mut self, group_call_id: i32) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetGroupCall { group_call_id }, None);
        if let Err(err) = self.sender.send_json(&get_group_call(extra, group_call_id)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C3a: drain `Session::group_call_fetch_queue` — `getGroupCall`
    /// for freshly created voice chats. Called from `ingest`.
    pub(crate) fn maybe_fetch_group_calls(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        let queued: Vec<i32> = std::mem::take(&mut self.session.group_call_fetch_queue);
        for group_call_id in queued {
            if let Err(err) = self.fetch_group_call(group_call_id) {
                // Best-effort: re-queue for the next ingest tick; the
                // `updateGroupCall` backstop still tracks the call.
                let _ = err;
                self.session.group_call_fetch_queue.push(group_call_id);
            }
        }
        Ok(())
    }

    /// Phase C2f: rejoin after `need_rejoin` (schema 1.8.67, line 7154
    /// docs: "user was kicked from the call because of network loss and
    /// the call needs to be rejoined"). Same attempt discipline as the
    /// C2d 1:1 reconnect: at most 3 attempts with identical join params
    /// (current self-mute state, honest no-device). A failed attempt
    /// re-arms `reconnecting` via the `JoinVideoChat` error arm so the
    /// driver's auto-rejoin retries; `manual` (the UI Rejoin button)
    /// resets the counter — explicit user intent starts the attempts
    /// over.
    pub fn rejoin_group_call(&mut self, manual: bool) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if manual && let Some(call) = self.session.active_group_call.as_mut() {
            call.rejoin_attempts = 0;
        }
        let (group_call_id, is_muted) = match &self.session.active_group_call {
            Some(call) if call.reconnecting && call.rejoin_attempts < 3 => {
                (call.id, call.is_muted_self)
            }
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self
            .session
            .request(RequestPurpose::JoinVideoChat { group_call_id }, None);
        // Review fix: the new native context starts unconnected, and
        // `create_group_call` below replaces the old media entry — a
        // live presentation would be orphaned (still capturing on the
        // native side). Snapshot it before the entry is replaced.
        let orphaned_presentation = self
            .call_engine
            .as_ref()
            .is_some_and(|engine| engine.presentation_active(group_call_id));
        let params = self.group_join_params(group_call_id, is_muted);
        if let Err(err) =
            self.sender
                .send_json(&join_video_chat(extra, group_call_id, None, &params, ""))
        {
            self.session.requests.take(extra);
            // The attempt never went out: re-arm so the next ingest
            // retries instead of stranding the call.
            if let Some(call) = self.session.active_group_call.as_mut() {
                call.reconnecting = true;
            }
            return Err(err);
        }
        if orphaned_presentation && let Some(engine) = self.call_engine.as_deref_mut() {
            let _ = engine.stop_screen_share(group_call_id);
        }
        if let Some(call) = self.session.active_group_call.as_mut() {
            call.rejoin_attempts += 1;
            call.reconnecting = false;
            // Review fix: the fresh native context must run the join
            // handshake again — reset the transport gate, the stale
            // answer, and the stale screen-share state so the pump
            // doesn't drop the new `joinVideoChat` answer.
            call.transport_ready = false;
            call.join_payload.clear();
            call.screen_sharing = false;
            call.screen_share_pending = false;
            call.screen_share_answer.clear();
        }
        Ok(extra)
    }

    /// Phase C2f: auto-rejoin a dropped group call (`need_rejoin`) —
    /// one attempt per ingest tick while the tracked call still wants
    /// reconnecting and attempts remain (the `rejoin_group_call`
    /// guard caps at 3, the C2d discipline).
    pub(crate) fn maybe_auto_rejoin_group_call(&mut self) -> Result<(), ConnectSendError> {
        let wants = self
            .session
            .active_group_call
            .as_ref()
            .is_some_and(|call| call.reconnecting && call.rejoin_attempts < 3);
        if wants {
            let _ = self.rejoin_group_call(false);
        }
        Ok(())
    }

    /// Phase C2g: build `joinVideoChat` params for a (re)join. Creates
    /// the native group context first so the payload is the real tgcalls
    /// offer; degrades to the honest no-device params when no engine is
    /// available or the offer fails (the join is never blocked).
    fn group_join_params(&mut self, group_call_id: i32, is_muted: bool) -> GroupCallJoinParams {
        let is_my_video_enabled = self
            .session
            .active_group_call
            .as_ref()
            .is_some_and(|call| call.is_my_video_enabled);
        // The native call key is the chat id; resolve it through the
        // chat's `video_chat` association (schema `groupCall` carries no
        // chat id).
        let chat_id = self
            .session
            .chats
            .values()
            .find(|chat| {
                chat.video_chat
                    .as_ref()
                    .is_some_and(|video_chat| video_chat.group_call_id == group_call_id)
            })
            .map(|chat| chat.id.0);
        let offer = match (chat_id, self.call_engine.as_deref_mut()) {
            (Some(chat_id), Some(engine)) if engine.is_available() => {
                Some(engine.create_group_call(group_call_id, chat_id))
            }
            _ => None,
        };
        match offer {
            Some(Ok(payload)) => GroupCallJoinParams {
                audio_source_id: group_offer_audio_source_id(&payload),
                payload,
                is_muted,
                is_my_video_enabled,
            },
            Some(Err(err)) => {
                if let Some(call) = self.session.active_group_call.as_mut() {
                    call.transport_error = Some(err.to_string());
                }
                let mut params = GroupCallJoinParams::honest_no_device();
                params.is_muted = is_muted;
                params.is_my_video_enabled = is_my_video_enabled;
                params
            }
            None => {
                let mut params = GroupCallJoinParams::honest_no_device();
                params.is_muted = is_muted;
                params.is_my_video_enabled = is_my_video_enabled;
                params
            }
        }
    }

    /// Phase C3a: `leaveGroupCall` (schema 1.8.67, :14458). Drops the
    /// tracked call; the `ok` confirms (state also handles the
    /// `!is_active` backstop).
    pub fn leave_group_call(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.active_group_call {
            Some(call) => call.id,
            None => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self
            .session
            .request(RequestPurpose::LeaveGroupCall { group_call_id }, None);
        if let Err(err) = self
            .sender
            .send_json(&leave_group_call(extra, group_call_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        // Phase C2g: tear the native group transport down eagerly; the
        // pump's before/after backstop covers server-driven ends.
        self.teardown_group_call_transport(group_call_id);
        Ok(extra)
    }

    /// Phase C2g: drop the native transport and retained frames for one
    /// group call id. Unknown ids succeed (idempotent cleanup).
    fn teardown_group_call_transport(&mut self, group_call_id: i32) {
        if let Some(engine) = self.call_engine.as_deref_mut() {
            let _ = engine.leave_group_call(group_call_id);
        }
        self.group_video_frame_slots
            .lock()
            .expect("group video frame slots")
            .retain(|(slot_call_id, _, _), _| *slot_call_id != group_call_id);
        // Slice calls-group-self-tile: the self tile lives in the
        // shared (call id, is_local) slots — clear it too.
        self.video_frame_slots
            .lock()
            .expect("call video frame slots")
            .retain(|(slot_call_id, _, _), _| *slot_call_id != group_call_id);
        self.group_camera_state.remove(&group_call_id);
    }

    /// Phase C3a: `endGroupCall` (schema 1.8.67, :14461). Gated on
    /// `groupCall.can_be_managed` (video chats).
    pub fn end_group_call(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match &self.session.active_group_call {
            Some(call) if call.can_be_managed => call.id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self
            .session
            .request(RequestPurpose::EndGroupCall { group_call_id }, None);
        if let Err(err) = self.sender.send_json(&end_group_call(extra, group_call_id)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        // Phase C2g: same native teardown as leaving; the server-driven
        // end also lands via the pump backstop.
        self.teardown_group_call_transport(group_call_id);
        Ok(extra)
    }

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
        let group_call_id = match &self.session.active_group_call {
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
        // Honest gate: the presentation handshake needs a screen-capture
        // source, and the native engine enumerates them via
        // `media_devices` (`MediaDeviceKind::Screen`).
        if !self.group_call_screen_source_available() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let sharing = self
            .session
            .active_group_call
            .as_ref()
            .is_some_and(|call| call.screen_sharing || call.screen_share_pending);
        if sharing {
            let extra = self.session.request(
                RequestPurpose::EndGroupCallScreenSharing { group_call_id },
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
                && let Some(call) = self.session.active_group_call.as_mut()
            {
                call.transport_error = Some(err.to_string());
            }
            if let Some(call) = self.session.active_group_call.as_mut() {
                call.screen_sharing = false;
                call.screen_share_pending = false;
                call.screen_share_answer.clear();
            }
            return Ok(extra);
        }
        let offer = match self
            .call_engine
            .as_deref_mut()
            .expect("available engine")
            .start_screen_share(group_call_id)
        {
            Ok(offer) => offer,
            Err(err) => {
                if let Some(call) = self.session.active_group_call.as_mut() {
                    call.transport_error = Some(err.to_string());
                }
                return Err(ConnectSendError::InvalidRequest);
            }
        };
        let extra = self.session.request(
            RequestPurpose::StartGroupCallScreenSharing { group_call_id },
            None,
        );
        if let Err(err) = self.sender.send_json(&start_group_call_screen_sharing(
            extra,
            group_call_id,
            &offer,
        )) {
            self.session.requests.take(extra);
            if let Some(engine) = self.call_engine.as_deref_mut() {
                let _ = engine.stop_screen_share(group_call_id);
            }
            return Err(err);
        }
        if let Some(call) = self.session.active_group_call.as_mut() {
            call.screen_share_pending = true;
        }
        Ok(extra)
    }

    /// Phase C3a: local-only self mute toggle. There is no TDLib "mute
    /// self" for group calls outside the join parameters — the UI labels
    /// this honestly as local-only; the state rides on the next (re)join.
    pub fn toggle_group_call_self_mute(&mut self) {
        let muted = !self
            .session
            .active_group_call
            .as_ref()
            .is_some_and(|c| c.is_muted_self);
        self.session.set_group_call_self_muted(muted);
    }

    /// Phase C3a: `toggleGroupCallIsMyVideoEnabled` (schema 1.8.67,
    /// :14414). Tracks the TDLib flag; Phase C2g applies it to the
    /// native transport in the driver pump (`set_group_camera`).
    pub fn toggle_group_call_my_video(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let (group_call_id, enable) = match &self.session.active_group_call {
            Some(call) => (call.id, !call.is_my_video_enabled),
            None => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self
            .session
            .request(RequestPurpose::ToggleGroupCallVideo { group_call_id }, None);
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
        let (group_call_id, pause) = match &self.session.active_group_call {
            Some(call) if call.is_my_video_enabled => (call.id, !call.is_my_video_paused),
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self
            .session
            .request(RequestPurpose::ToggleGroupCallVideo { group_call_id }, None);
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
        let group_call_id = match &self.session.active_group_call {
            Some(call) => call.id,
            None => return Err(ConnectSendError::InvalidRequest),
        };
        let sender_ref = match participant_id {
            MessageSender::User { user_id } => MessageSenderRef::User(user_id),
            MessageSender::Chat { chat_id } => MessageSenderRef::Chat(chat_id),
        };
        let extra = self.session.request(
            RequestPurpose::ToggleGroupCallParticipantMute { group_call_id },
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
        let group_call_id = match &self.session.active_group_call {
            Some(call) => call.id,
            None => return Err(ConnectSendError::InvalidRequest),
        };
        let sender_ref = match participant_id {
            MessageSender::User { user_id } => MessageSenderRef::User(user_id),
            MessageSender::Chat { chat_id } => MessageSenderRef::Chat(chat_id),
        };
        let extra = self.session.request(
            RequestPurpose::ToggleGroupCallParticipantHand { group_call_id },
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

    /// Phase C2f: `inviteGroupCallParticipant` (schema 1.8.67,
    /// :14375). `is_video` follows the tracked call's `is_video_chat`.
    pub fn invite_group_call_participant(
        &mut self,
        user_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let (group_call_id, is_video) = match &self.session.active_group_call {
            Some(call) => (call.id, call.is_video_chat),
            None => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::InviteGroupCallParticipant { group_call_id },
            None,
        );
        if let Err(err) = self.sender.send_json(&invite_group_call_participant(
            extra,
            group_call_id,
            user_id,
            is_video,
        )) {
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
        let group_call_id = match &self.session.active_group_call {
            Some(call) if call.is_owned => call.id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::BanGroupCallParticipants { group_call_id },
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
        let group_call_id = match &self.session.active_group_call {
            Some(call) => call.id,
            None => return Err(ConnectSendError::InvalidRequest),
        };
        let sender_ref = match participant_id {
            MessageSender::User { user_id } => MessageSenderRef::User(user_id),
            MessageSender::Chat { chat_id } => MessageSenderRef::Chat(chat_id),
        };
        let extra = self.session.request(
            RequestPurpose::SetGroupCallParticipantVolumeLevel { group_call_id },
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
        if self.session.active_call.is_some() {
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
            RequestPurpose::DeclineGroupCallInvitation {
                chat_id,
                message_id,
            },
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

    /// Phase C3a: `toggleVideoChatMuteNewParticipants` (schema 1.8.67,
    /// :14317). Gated on `groupCall.can_toggle_mute_new_participants`.
    pub fn toggle_video_chat_mute_new(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let (group_call_id, mute_new) = match &self.session.active_group_call {
            Some(call) if call.can_toggle_mute_new_participants => {
                (call.id, !call.mute_new_participants)
            }
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::ToggleVideoChatMuteNew { group_call_id },
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
        let group_call_id = match &self.session.active_group_call {
            Some(call) if call.can_be_managed => call.id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let title = title.trim().to_string();
        if title.is_empty() || title.chars().count() > 64 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SetVideoChatTitle { group_call_id }, None);
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
        let group_call_id = match &self.session.active_group_call {
            Some(call) => call.id,
            None => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::GetVideoChatInviteLink { group_call_id },
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
        let group_call_id = match &self.session.active_group_call {
            Some(call) if call.can_be_managed => call.id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::RevokeVideoChatInviteLink { group_call_id },
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
        let group_call_id = match &self.session.active_group_call {
            Some(call) if call.can_be_managed && call.is_video_chat => call.id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let title = title.trim().to_string();
        if title.chars().count() > 64 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::StartGroupCallRecording { group_call_id },
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
        let group_call_id = match &self.session.active_group_call {
            Some(call) if call.can_be_managed && call.is_video_chat => call.id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::EndGroupCallRecording { group_call_id },
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
        let group_call_id = match &self.session.active_group_call {
            Some(call) if call.can_be_managed && call.scheduled_start_date > 0 => call.id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::StartScheduledVideoChat { group_call_id },
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
        let (group_call_id, enabled) = match &self.session.active_group_call {
            Some(call) if call.scheduled_start_date > 0 => {
                (call.id, !call.enabled_start_notification)
            }
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::ToggleVideoChatEnabledStartNotification {
                group_call_id,
                enabled,
            },
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
        let call = match &self.session.active_group_call {
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
        let extra = self
            .session
            .request(RequestPurpose::GetVideoChatRtmpUrl { chat_id }, None);
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
        let call = match &self.session.active_group_call {
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
        let extra = self
            .session
            .request(RequestPurpose::ReplaceVideoChatRtmpUrl { chat_id }, None);
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
        let group_call_id = match &self.session.active_group_call {
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
        let extra = self
            .session
            .request(RequestPurpose::SendGroupCallMessage { group_call_id }, None);
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
        let (group_call_id, new_value) = match &self.session.active_group_call {
            Some(call) if call.can_toggle_are_messages_allowed => {
                (call.id, !call.are_messages_allowed)
            }
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::ToggleGroupCallAreMessagesAllowed { group_call_id },
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
        let group_call_id = match &self.session.active_group_call {
            Some(call) if !call.loaded_all_participants => call.id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(
            RequestPurpose::LoadGroupCallParticipants { group_call_id },
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
