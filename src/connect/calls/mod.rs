//! Connect driver: 1:1 call control (TDLib signaling + engine bridge).
use super::*;
use crate::calls::engine::{
    CallEngine, ConnectParams, EngineError, MediaDevice, MediaDeviceKind, RemoteVideoState,
    RtcServer, TransportState, VideoFrame, video_wanted,
};
use crate::ids::RequestId;
use crate::state::CallsPurpose;
use crate::state::RequestPurpose;
use crate::telegram::envelope::{CallState, ReadyParams};
use crate::telegram::requests::{
    CallPrivacySetting, PrivacyWho, accept_call_with_protocol, create_call_with_protocol,
    delete_all_call_messages, discard_call as discard_call_request, get_user_privacy_setting_rules,
    search_call_messages, send_call_debug_information, send_call_log, send_call_rating_detail,
    send_call_signaling_data, set_user_privacy_setting_rules,
};
use std::sync::Arc;

mod history;
mod lifecycle;

impl<S: JsonSender> ConnectDriver<S> {
    /// Phase C2b: install the driver-thread call engine and route its
    /// worker-thread signaling/transport emissions into the driver
    /// queues. Devices are fetched on demand (`refresh_call_devices`),
    /// never here.
    pub fn set_call_engine(&mut self, mut engine: Box<dyn CallEngine>) {
        let outbox = self.signaling_outbox.clone();
        engine.set_signaling_emitted_callback(Arc::new(move |call_id, data| {
            outbox
                .lock()
                .expect("call signaling outbox")
                .push_back((call_id, data));
        }));
        let transport_outbox = self.transport_outbox.clone();
        engine.set_transport_state_callback(Arc::new(move |call_id, state| {
            transport_outbox
                .lock()
                .expect("call transport outbox")
                .push_back((call_id, state));
        }));
        // Phase C2e: peer camera states and decoded frames ride the same
        // worker-thread -> driver-pump path as transport and signaling.
        let video_state_outbox = self.video_state_outbox.clone();
        engine.set_remote_video_state_callback(Arc::new(move |call_id, state| {
            video_state_outbox
                .lock()
                .expect("call video state outbox")
                .push_back((call_id, state));
        }));
        // Phase C2j: the peer's 1:1 screen-share state rides the same
        // worker-thread -> driver-pump path as the camera state.
        let screen_state_outbox = self.screen_state_outbox.clone();
        engine.set_remote_screen_state_callback(Arc::new(move |call_id, state| {
            screen_state_outbox
                .lock()
                .expect("call screen state outbox")
                .push_back((call_id, state));
        }));
        let video_frame_slots = self.video_frame_slots.clone();
        let group_video_frame_slots = self.group_video_frame_slots.clone();
        engine.set_video_frame_callback(Arc::new(move |call_id, frame| {
            // Phase C2g: group-call frames carry the participant they were
            // subscribed for; they live in their own slots so the 1:1
            // (call id, is_local) keys stay untouched.
            if let Some(user_id) = frame.participant_user_id {
                group_video_frame_slots
                    .lock()
                    .expect("group video frame slots")
                    .insert((call_id, user_id, frame.is_screen), frame);
            } else {
                // Phase C2j: the screen share gets its own slot keyed on
                // `is_screen` so it never clobbers the peer camera frame.
                video_frame_slots
                    .lock()
                    .expect("call video frame slots")
                    .insert((call_id, frame.is_local, frame.is_screen), frame);
            }
        }));
        self.call_engine = Some(engine);
    }

    /// Phase C2c: whether a call engine is installed at all.
    pub fn has_call_engine(&self) -> bool {
        self.call_engine
            .as_ref()
            .is_some_and(|engine| engine.is_available())
    }

    /// Phase C2c: last-enumerated audio devices; empty when the engine
    /// reported none (or is missing/unavailable).
    pub fn call_devices(&self) -> &[MediaDevice] {
        &self.call_devices_cache
    }

    /// Phase C2c: selected (microphone, speaker) device ids; `(None,
    /// None)` is the engine default. No fabricated devices: nothing is
    /// auto-selected from enumeration.
    pub fn selected_call_devices(&self) -> (Option<&str>, Option<&str>) {
        (
            self.selected_devices.0.as_deref(),
            self.selected_devices.1.as_deref(),
        )
    }

    /// Phase C2c: best-effort device re-enumeration. A missing or
    /// unavailable engine leaves the cache untouched.
    pub fn refresh_call_devices(&mut self) {
        if let Some(engine) = self.call_engine.as_deref()
            && let Ok(devices) = engine.media_devices()
        {
            self.call_devices_cache = devices;
        }
    }

    /// Phase C2g: whether the engine enumerates a screen-capture
    /// source. The presentation handshake needs one; without it the UI
    /// offers no screen-share control ("No screen source available").
    /// Phase C2i: shared by the 1:1 screen-share toggle (same native
    /// `MediaDeviceKind::Screen` gate, no presentation handshake).
    pub fn call_screen_source_available(&self) -> bool {
        self.call_devices_cache
            .iter()
            .any(|device| device.kind == MediaDeviceKind::Screen)
    }

    /// Phase C2g: group-call alias of `call_screen_source_available`.
    pub fn group_call_screen_source_available(&self) -> bool {
        self.call_screen_source_available()
    }

    /// Phase C2g: whether the native call engine is installed and
    /// available, for honest group-call audio/video state copy.
    pub fn group_call_engine_available(&self) -> bool {
        self.call_engine
            .as_ref()
            .is_some_and(|engine| engine.is_available())
    }

    /// Phase C2b: advertise the engine's protocol only when an engine is
    /// installed AND available; otherwise fall back to the honest
    /// signaling-only shape.
    fn engine_protocol_json(&self) -> serde_json::Value {
        self.call_engine
            .as_ref()
            .filter(|engine| engine.is_available())
            .map(|engine| engine.protocol().to_json())
            .unwrap_or_else(crate::telegram::requests::call_protocol)
    }

    fn call_connect_params(&self, is_outgoing: bool, ready: &ReadyParams) -> ConnectParams {
        let library_versions = ready.library_versions.clone();
        let is_video = self.session.active_call.as_ref().is_some_and(|call| {
            // A camera toggle before the transport existed is stored in
            // `camera_on` and must survive into the connect params (a
            // camera-off toggle means the call is negotiated without
            // video). At initial connect `camera_on == is_video`, so
            // nothing changes there.
            call.is_video && call.camera_on
        });
        let (video_enabled, default_camera) = video_wanted(is_video, &self.call_devices_cache);
        let camera_input = video_enabled
            .then(|| self.selected_camera.clone().or(default_camera))
            .flatten();
        ConnectParams {
            encryption_key: ready.encryption_key.clone(),
            custom_parameters: ready.custom_parameters.clone(),
            is_outgoing,
            servers: ready
                .servers
                .iter()
                .map(|server| RtcServer {
                    id: server.id,
                    ipv4: server.ipv4.clone(),
                    ipv6: server.ipv6.clone(),
                    port: server.port,
                    username: server.username.clone(),
                    password: server.password.clone(),
                    turn: server.turn,
                    stun: server.stun,
                    tcp: server.tcp,
                    peer_tag: server.peer_tag.clone(),
                })
                .collect(),
            library_versions,
            p2p_allowed: ready.allow_p2p,
            mic_input: self.selected_devices.0.clone(),
            speaker_input: self.selected_devices.1.clone(),
            // Phase C2e: a video call negotiates video only when a camera
            // exists; the user's camera pick wins over the first
            // enumerated camera. The connect path refreshes the device
            // cache before calling this (cheap no-op when the engine is
            // missing).
            video_enabled,
            camera_input,
        }
    }

    /// Phase C2e: honest 1:1 video readiness — the active call is a video
    /// call, the engine is available, and a camera exists. The UI uses
    /// this for disabled states instead of guessing.
    pub fn call_video_ready(&self) -> bool {
        let is_video = self
            .session
            .active_call
            .as_ref()
            .is_some_and(|call| call.is_video);
        self.call_engine
            .as_ref()
            .is_some_and(|engine| engine.is_available())
            && video_wanted(is_video, &self.call_devices_cache).0
    }

    /// Phase C2e: newest decoded frame for a call; `is_local` selects the
    /// local preview (`true`) or the peer camera (`false`). `None` when
    /// no frame has arrived yet. The peer's screen share lives in its
    /// own slot — see `latest_screen_frame`.
    pub fn latest_video_frame(&self, call_id: i32, is_local: bool) -> Option<VideoFrame> {
        self.video_frame_slots
            .lock()
            .expect("call video frame slots")
            .get(&(call_id, is_local, false))
            .cloned()
    }

    /// Phase C2j: newest decoded frame of the peer's 1:1 screen share;
    /// `None` when the peer is not sharing (or no frame has arrived yet).
    /// Phase C2l renders this as the screen-share tile while
    /// `ActiveCall::remote_screen` is not inactive; the backend keeps
    /// the slot warm and drops it when the peer's share goes inactive.
    pub fn latest_screen_frame(&self, call_id: i32) -> Option<VideoFrame> {
        self.video_frame_slots
            .lock()
            .expect("call video frame slots")
            .get(&(call_id, false, true))
            .cloned()
    }

    /// Phase C2g: newest decoded group video frame for a participant;
    /// `screen` selects the screen-share stream (`true`) or the camera
    /// (`false`). `None` when no frame has arrived yet.
    pub fn latest_group_video_frame(
        &self,
        group_call_id: i32,
        user_id: i64,
        screen: bool,
    ) -> Option<VideoFrame> {
        self.group_video_frame_slots
            .lock()
            .expect("group video frame slots")
            .get(&(group_call_id, user_id, screen))
            .cloned()
    }

    /// Phase C2e: selected camera device id; `None` is the engine default.
    pub fn selected_call_camera(&self) -> Option<&str> {
        self.selected_camera.as_deref()
    }

    /// Phase C2e: camera toggle through the native engine. The engine is
    /// called first and its error propagates *without* flipping the
    /// session flag (mirrors `set_call_muted`); without a connected
    /// transport the intent is only stored (it applies on connect).
    /// Phase C2i: enabling the camera clears the screen-share intent —
    /// ntgcalls forbids camera+screen in Capture mode.
    pub fn set_call_camera(&mut self, call_id: i32, enabled: bool) -> Result<(), EngineError> {
        let Some(call) = self.session.active_call.as_mut() else {
            return Err(EngineError::NoActiveCall);
        };
        if call.id != call_id {
            return Err(EngineError::NoSuchCall(call_id));
        }
        if call.transport.is_some()
            && let Some(engine) = self.call_engine.as_deref_mut()
            && engine.is_available()
        {
            engine.set_camera_enabled(call.id, enabled, self.selected_camera.as_deref())?;
        }
        call.camera_on = enabled;
        if enabled {
            call.screen_sharing = false;
        }
        Ok(())
    }

    /// Phase C2i: 1:1 screen-share send toggle through the native
    /// engine. Same contract as `set_call_camera`: the engine is
    /// called first and its error propagates *without* flipping the
    /// session flag; without a connected transport the intent is only
    /// stored (it applies on connect, mirroring the pre-transport
    /// mute in `pump_call_engine`). Enabling clears the camera intent
    /// — ntgcalls forbids camera+screen in Capture mode. Rejected
    /// without an enumerated screen source
    /// (`call_screen_source_available`) when *enabling*; stopping
    /// needs no source (a vanished display must not trap the user in
    /// "sharing").
    pub fn set_call_screen_share(
        &mut self,
        call_id: i32,
        enabled: bool,
    ) -> Result<(), EngineError> {
        // Gate first: it borrows `&self`, which can't overlap the
        // mutable call borrow below.
        let screen_available = self.call_screen_source_available();
        let Some(call) = self.session.active_call.as_mut() else {
            return Err(EngineError::NoActiveCall);
        };
        if call.id != call_id {
            return Err(EngineError::NoSuchCall(call_id));
        }
        if enabled && !screen_available {
            return Err(EngineError::NoScreenSource);
        }
        if call.transport.is_some()
            && let Some(engine) = self.call_engine.as_deref_mut()
            && engine.is_available()
        {
            engine.set_screen_share_enabled(call.id, enabled)?;
        }
        call.screen_sharing = enabled;
        if enabled {
            call.camera_on = false;
        }
        Ok(())
    }

    /// Phase C2e: pick the camera device id (`None` = engine default).
    /// The selection is stored (so it applies on connect) and forwarded
    /// to the native engine with the current camera intent only when a
    /// transport is already connected; a failed forward propagates
    /// before the stored selection changes.
    pub fn select_call_camera(&mut self, camera: Option<String>) -> Result<(), EngineError> {
        let camera_on = self
            .session
            .active_call
            .as_ref()
            .is_some_and(|call| call.camera_on);
        if let Some(call) = self.session.active_call.as_ref()
            && call.transport.is_some()
            && let Some(engine) = self.call_engine.as_deref_mut()
            && engine.is_available()
        {
            engine.set_camera_enabled(call.id, camera_on, camera.as_deref())?;
        }
        self.selected_camera = camera;
        Ok(())
    }

    /// Phase C2b: synchronize reducer call lifecycle/signaling with the
    /// engine and flush engine-emitted bytes through TDLib.
    pub(crate) fn pump_call_engine(
        &mut self,
        active_call_before: Option<i32>,
        bridge_signaling: Option<(i32, Vec<u8>)>,
    ) -> Result<(), ConnectSendError> {
        let active_call_after = self
            .session
            .active_call
            .as_ref()
            .map(|call| (call.id, call.user_id, call.is_outgoing, call.ready.clone()));
        let tracked_call_id = active_call_after
            .as_ref()
            .map(|(call_id, _, _, _)| *call_id);

        if tracked_call_id != active_call_before && tracked_call_id.is_some() {
            self.call_connect_params = None;
            self.reconnect_attempts = 0;
        }

        if let Some((call_id, user_id, is_outgoing, _)) = &active_call_after
            && Some(*call_id) != active_call_before
            && let Some(engine) = self.call_engine.as_deref_mut()
        {
            // TDLib remains the source of truth; engine startup failures must
            // not erase the reducer's honest signaling-only call state.
            if let Err(err) = engine.start_call(*call_id, *user_id, *is_outgoing)
                && let Some(call) = self.session.active_call.as_mut()
            {
                call.transport_error = Some(err.to_string());
            }
        }
        if tracked_call_id.is_none()
            && let Some(call_id) = active_call_before
        {
            if let Some(summary) = self
                .session
                .call_summary
                .as_mut()
                .filter(|summary| summary.call_id == call_id)
            {
                summary.reconnect_attempts = self.reconnect_attempts;
            }
            if let Some(engine) = self.call_engine.as_deref_mut() {
                let _ = engine.hangup(call_id);
            }
            // Phase C2e: drop any retained frames for the ended call so
            // the UI cannot render a stale picture.
            self.video_frame_slots
                .lock()
                .expect("call video frame slots")
                .retain(|(slot_call_id, _, _), _| *slot_call_id != call_id);
            self.call_connect_params = None;
            self.reconnect_attempts = 0;
        }
        if let Some((call_id, data)) = bridge_signaling
            && tracked_call_id == Some(call_id)
            && let Some(engine) = self.call_engine.as_deref_mut()
        {
            // Gate on the reducer-tracked call: the reducer drops signaling
            // for unknown or ended calls, and this bridge follows the same
            // gate. The session queue remains the honest diagnostic record
            // if the optional engine rejects or cannot consume these bytes.
            let _ = engine.send_signaling_data(call_id, &data);
        }

        // Phase C2c: connect the native audio transport exactly once per
        // call — gated on the reducer-tracked transport state, not on a
        // separate attempted set. A Ready call without an encryption key
        // cannot build the native encryption parameters; the call stays
        // up (TDLib owns signaling) but carries no sound.
        if let Some((call_id, _, is_outgoing, Some(ready))) = &active_call_after
            && self
                .session
                .active_call
                .as_ref()
                .is_some_and(|call| call.transport.is_none())
        {
            let call_id = *call_id;
            let is_outgoing = *is_outgoing;
            if ready.encryption_key.is_empty() {
                if let Some(call) = self.session.active_call.as_mut() {
                    call.transport = Some(TransportState::Failed);
                    call.transport_error =
                        Some("call became ready without an encryption key".into());
                }
            } else {
                // Refresh devices before connecting so the native engine
                // sees the current device ids.
                self.refresh_call_devices();
                let params = self.call_connect_params(is_outgoing, ready);
                self.call_connect_params = Some(params.clone());
                self.reconnect_attempts = 0;
                let result = self.connect_call_transport(call_id, &params);
                // Phase C2i: a screen-share toggle made before the
                // transport existed applies once it does (mirrors the
                // pre-transport mute above; non-fatal so the UI toggle
                // can retry).
                let pre_sharing = self
                    .session
                    .active_call
                    .as_ref()
                    .is_some_and(|call| call.screen_sharing);
                if result.is_ok()
                    && pre_sharing
                    && let Some(engine) = self.call_engine.as_deref_mut()
                {
                    let _ = engine.set_screen_share_enabled(call_id, true);
                }
                if let Some(call) = self.session.active_call.as_mut() {
                    match result {
                        Ok(()) => {
                            call.transport = Some(TransportState::Connecting);
                            call.transport_error = None;
                        }
                        Err(err) => {
                            call.transport = Some(TransportState::Failed);
                            call.transport_error = Some(err.to_string());
                        }
                    }
                }
            }
        }

        loop {
            let update = self
                .transport_outbox
                .lock()
                .expect("call transport outbox")
                .pop_front();
            let Some((call_id, state)) = update else {
                break;
            };
            if !self
                .session
                .active_call
                .as_ref()
                .is_some_and(|call| call.id == call_id)
            {
                continue;
            }
            if state == TransportState::Failed {
                let failure = if self.reconnect_attempts >= 3 {
                    Some("reconnect attempts exhausted".to_string())
                } else if !self
                    .call_engine
                    .as_ref()
                    .is_some_and(|engine| engine.is_available())
                {
                    Some("call engine is unavailable for reconnect".to_string())
                } else if let Some(params) = self.call_connect_params.clone() {
                    self.reconnect_attempts += 1;
                    if let Some(call) = self.session.active_call.as_mut() {
                        call.transport = Some(TransportState::Reconnecting);
                        call.transport_error = None;
                    }
                    match self.connect_call_transport(call_id, &params) {
                        // The retry is in flight: leave the call in
                        // `Reconnecting` so the UI can show it. The
                        // engine's own state callbacks move it to
                        // `Connecting`/`Connected` (or back to `Failed`)
                        // when they arrive.
                        Ok(()) => None,
                        Err(err) => Some(err.to_string()),
                    }
                } else {
                    Some("no retained call parameters for reconnect".to_string())
                };
                if let Some(error) = failure
                    && let Some(call) = self.session.active_call.as_mut()
                {
                    call.transport = Some(TransportState::Failed);
                    call.transport_error = Some(error);
                }
            } else if let Some(call) = self.session.active_call.as_mut() {
                if state == TransportState::Connected {
                    self.reconnect_attempts = 0;
                }
                call.transport = Some(state);
                call.transport_error = None;
            }
        }

        // Phase C2e: peer camera state follows the same gate as
        // transport — only the reducer-tracked active call is updated;
        // emissions for an unknown or ended call are dropped.
        loop {
            let update = self
                .video_state_outbox
                .lock()
                .expect("call video state outbox")
                .pop_front();
            let Some((call_id, state)) = update else {
                break;
            };
            if let Some(call) = self
                .session
                .active_call
                .as_mut()
                .filter(|call| call.id == call_id)
            {
                call.remote_video = state;
            }
        }

        // Phase C2j: the peer's 1:1 screen-share state follows the same
        // gate as the camera state. When the share goes inactive the
        // retained screen frames are dropped so a stale picture can
        // never render; while active the frames themselves carry the
        // picture, so no persistent state is kept.
        // Phase C2l: the state itself is also recorded on the call —
        // the UI renders the screen tile only while it is not
        // `Inactive`, closing the race where a late frame arriving
        // after the drain would otherwise repopulate the slot.
        loop {
            let update = self
                .screen_state_outbox
                .lock()
                .expect("call screen state outbox")
                .pop_front();
            let Some((call_id, state)) = update else {
                break;
            };
            if let Some(call) = self
                .session
                .active_call
                .as_mut()
                .filter(|call| call.id == call_id)
            {
                call.remote_screen = state;
            }
            if state != RemoteVideoState::Inactive {
                continue;
            }
            self.video_frame_slots
                .lock()
                .expect("call video frame slots")
                .retain(|(slot_call_id, _, slot_is_screen), _| {
                    *slot_call_id != call_id || !slot_is_screen
                });
        }

        loop {
            let emitted = self
                .signaling_outbox
                .lock()
                .expect("call signaling outbox")
                .pop_front();
            let Some((call_id, data)) = emitted else {
                break;
            };
            let extra = self
                .session
                .request(RequestPurpose::SendCallSignalingData, None);
            let payload = send_call_signaling_data(extra, call_id, &data);
            if let Err(err) = self.sender.send_json(&payload) {
                // Keep request bookkeeping honest and return the unsent
                // bytes to the head of the outbox so a later ingest retries
                // them in order instead of dropping them.
                self.session.requests.take(extra);
                self.signaling_outbox
                    .lock()
                    .expect("call signaling outbox")
                    .push_front((call_id, data));
                return Err(err);
            }
        }

        Ok(())
    }

    fn connect_call_transport(
        &mut self,
        call_id: i32,
        params: &ConnectParams,
    ) -> Result<(), EngineError> {
        let muted = self
            .session
            .active_call
            .as_ref()
            .is_some_and(|call| call.muted);
        let engine = self
            .call_engine
            .as_deref_mut()
            .ok_or(EngineError::Unavailable)?;
        engine.connect(call_id, params)?;
        if muted && let Err(error) = engine.set_muted(call_id, true) {
            // Never keep an unmuted replacement transport behind a muted UI.
            let _ = engine.hangup(call_id);
            self.transport_outbox
                .lock()
                .expect("call transport outbox")
                .retain(|(id, _)| *id != call_id);
            return Err(error);
        }
        Ok(())
    }
}
