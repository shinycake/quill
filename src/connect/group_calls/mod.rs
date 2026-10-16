//! Connect driver: group calls and video chats.
use super::*;
use crate::calls::engine::{GroupVideoSource, group_offer_audio_source_id};
use crate::ids::RequestId;
use crate::locks::LockExt;
use crate::state::CallsPurpose;
use crate::state::RequestPurpose;
use crate::telegram::envelope::{ChatKind, MessageSender};
use crate::telegram::requests::{
    GroupCallJoinParams, InputGroupCallRef, MessageSenderRef, ban_group_call_participants,
    create_video_chat, decline_group_call_invitation, end_group_call, end_group_call_recording,
    end_group_call_screen_sharing, get_group_call, get_video_chat_available_participants,
    get_video_chat_invite_link, get_video_chat_rtmp_url, invite_group_call_participant,
    invite_video_chat_participants, join_group_call, join_live_story as join_live_story_request,
    join_video_chat, leave_group_call, load_group_call_participants, replace_video_chat_rtmp_url,
    revoke_group_call_invite_link, send_group_call_message,
    set_group_call_participant_volume_level, set_video_chat_default_participant,
    set_video_chat_title, start_group_call_recording, start_group_call_screen_sharing,
    start_scheduled_video_chat, toggle_group_call_are_messages_allowed,
    toggle_group_call_is_my_video_enabled, toggle_group_call_is_my_video_paused,
    toggle_group_call_participant_is_hand_raised, toggle_group_call_participant_is_muted,
    toggle_video_chat_enabled_start_notification, toggle_video_chat_mute_new_participants,
};

mod admin;
mod controls;
mod join;
mod link;
mod live_story;
mod rtmp_start;
pub(crate) mod stream;

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
        let active_group_call_after = self
            .session
            .calls
            .active_group_call
            .as_ref()
            .map(|call| call.id);
        if active_group_call_after != active_group_call_before
            && let Some(before_id) = active_group_call_before
        {
            // The tracked call ended or was replaced: the native transport
            // must not linger, and stale frames must not render. A stream's
            // pending parts are answered first.
            self.drain_stream_feeder(before_id);
            if let Some(engine) = self.call_engine.as_deref_mut() {
                let _ = engine.leave_group_call(before_id);
            }
            self.group_video_frame_slots
                .lock_or_recover()
                .retain(|(slot_call_id, _, _), _| *slot_call_id != before_id);
            // Slice calls-group-self-tile: the self tile lives in the
            // shared (call id, is_local, is_screen) slots — clear it too so
            // a stale local preview can't render after the call ends.
            self.video_frame_slots
                .lock_or_recover()
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
            .calls
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
            let Some(engine) = self.call_engine.as_deref_mut() else {
                return Ok(());
            };
            let result = engine.connect_group_call(group_call_id, &answer, video_enabled);
            // Join muted if you muted before the transport was up.
            if result.is_ok() {
                let muted = self
                    .session
                    .calls
                    .active_group_call
                    .as_ref()
                    .is_some_and(|call| call.is_muted_self);
                if let Some(engine) = self.call_engine.as_deref_mut() {
                    let _ = engine.set_group_muted(group_call_id, muted);
                }
            }
            let connected = result.is_ok();
            let unsupported = result
                .as_ref()
                .err()
                .is_some_and(|err| err.is_stream_unsupported());
            if let Some(call) = self.session.calls.active_group_call.as_mut() {
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
            self.note_stream_rtmp_info(group_call_id, &answer);
            if unsupported {
                self.mark_stream_unsupported(group_call_id, &answer);
            }
            if connected {
                self.start_stream_feeder(group_call_id);
                self.group_camera_state.insert(group_call_id, video_enabled);
                // Refresh the device cache on connect so the screen-share
                // availability gate sees the engine's real sources.
                self.refresh_call_devices();
            }
        }
        let transport_ready = self
            .session
            .calls
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
            .calls
            .active_group_call
            .as_ref()
            .map(|call| call.is_my_video_enabled && !call.is_my_video_paused)
            .unwrap_or(false);
        if self.group_camera_state.get(&group_call_id) != Some(&wanted_camera) {
            let camera = self.selected_camera.clone();
            let Some(engine) = self.call_engine.as_deref_mut() else {
                return Ok(());
            };
            let result = engine.set_group_camera(group_call_id, wanted_camera, camera.as_deref());
            match result {
                Ok(()) => {
                    self.group_camera_state.insert(group_call_id, wanted_camera);
                    if let Some(call) = self.session.calls.active_group_call.as_mut() {
                        call.transport_error = None;
                    }
                }
                Err(err) => {
                    if let Some(call) = self.session.calls.active_group_call.as_mut() {
                        call.transport_error = Some(err.to_string());
                    }
                }
            }
        }
        // Incoming video follows the participants' `video_info` /
        // `screen_sharing_video_info`; the engine diffs add/remove.
        let sources: Vec<GroupVideoSource> = self
            .session
            .calls
            .active_group_call
            .as_ref()
            .map(|call| group_video_sources(&call.participants))
            .unwrap_or_default();
        // A live story's streamer may not be in the participant list (and
        // may be a channel): subscribe to its video as well.
        let mut sources = sources;
        for source in self.live_story_video_sources(group_call_id) {
            if !sources.iter().any(|s| s.endpoint == source.endpoint) {
                sources.push(source);
            }
        }
        let Some(engine) = self.call_engine.as_deref_mut() else {
            return Ok(());
        };
        if let Err(err) = engine.sync_group_video(group_call_id, &sources)
            && let Some(call) = self.session.calls.active_group_call.as_mut()
        {
            call.transport_error = Some(err.to_string());
        }
        // Finish the presentation handshake once the
        // `startGroupCallScreenSharing` answer is stored.
        let share_answer = self
            .session
            .calls
            .active_group_call
            .as_ref()
            .filter(|call| call.screen_share_pending)
            .map(|call| call.screen_share_answer.clone());
        if let Some(answer) = share_answer
            && !answer.is_empty()
        {
            let Some(engine) = self.call_engine.as_deref_mut() else {
                return Ok(());
            };
            let result = engine.connect_screen_share(group_call_id, &answer);
            if let Some(call) = self.session.calls.active_group_call.as_mut() {
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
            .calls
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
}
