//! Connect driver: 1:1 call control (TDLib signaling + engine bridge).
use super::*;
use crate::calls::engine::{
    CallEngine, ConnectParams, EngineError, MediaDevice, MediaDeviceKind, RemoteVideoState,
    RtcServer, TransportState, VideoFrame, video_wanted,
};
use crate::ids::RequestId;
use crate::state::RequestPurpose;
use crate::telegram::envelope::{CallState, ReadyParams};
use crate::telegram::requests::{
    CallPrivacySetting, PrivacyWho, accept_call_with_protocol, create_call_with_protocol,
    discard_call as discard_call_request, get_user_privacy_setting_rules, search_call_messages,
    send_call_debug_information, send_call_log, send_call_rating_detail, send_call_signaling_data,
    set_user_privacy_setting_rules,
};
use std::sync::Arc;

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
        self.call_engine.is_some()
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

    /// Phase C1: `createCall` for a user. Gated on a known non-bot
    /// user (like `createNewSecretChat`) and on no call already being
    /// active. Phase C1b: `is_video: true` starts video-call
    /// *signaling* — media transport is still Phase C2, so the call
    /// carries no audio or video; the UI says so. The `callId` answer
    /// starts tracking the outgoing call; its states arrive as
    /// `updateCall`.
    pub fn start_call(
        &mut self,
        user_id: i64,
        is_video: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.active_call.is_some() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let user = self.session.user(user_id);
        let is_bot = user.is_some_and(|u| u.is_bot);
        let is_self = self.session.my_user_id.is_some_and(|me| me == user_id);
        if user.is_none() || is_bot || is_self {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request_for_user(RequestPurpose::CreateCall { is_video }, user_id);
        let protocol = self.engine_protocol_json();
        if let Err(err) = self.sender.send_json(&create_call_with_protocol(
            extra, user_id, is_video, &protocol,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C1: `acceptCall` for the tracked incoming call. Requires an
    /// incoming `Pending` call — answering a call that already moved on
    /// is rejected here, not sent.
    pub fn accept_call(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let call_id = match &self.session.active_call {
            Some(call) if !call.is_outgoing && matches!(call.state, CallState::Pending { .. }) => {
                call.id
            }
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(RequestPurpose::AcceptCall, None);
        let protocol = self.engine_protocol_json();
        if let Err(err) = self
            .sender
            .send_json(&accept_call_with_protocol(extra, call_id, &protocol))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        if let Some(engine) = self.call_engine.as_deref_mut() {
            // TDLib remains the call-state source of truth; this only marks
            // the already-tracked engine-side call accepted.
            let _ = engine.accept_call(call_id);
        }
        Ok(extra)
    }

    /// Phase C1: `discardCall` for the tracked call (decline an
    /// incoming call / hang up an active one). Marks the call
    /// `HangingUp` optimistically; the discard states arrive as
    /// `updateCall`. `duration` is the connected time in seconds.
    pub fn discard_call(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let (call_id, duration_secs, is_video) = match &self.session.active_call {
            Some(call) => (call.id, call.connected_secs() as i32, call.is_video),
            None => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(RequestPurpose::DiscardCall, None);
        if let Err(err) = self.sender.send_json(&discard_call_request(
            extra,
            call_id,
            false,
            duration_secs,
            is_video,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        if let Some(call) = self.session.active_call.as_mut() {
            call.state = CallState::HangingUp;
        }
        if let Some(engine) = self.call_engine.as_deref_mut() {
            // Prompt teardown; the later terminal updateCall repeats this
            // idempotently through `pump_call_engine`.
            let _ = engine.hangup(call_id);
        }
        Ok(extra)
    }

    /// Swap prompt: decline the pending incoming call as busy.
    /// Clears the prompt; the caller's terminal update is a no-op
    /// afterwards.
    pub fn decline_swap_call(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let (call_id, _user_id, is_video) = self
            .session
            .call_swap_pending
            .take()
            .ok_or(ConnectSendError::InvalidRequest)?;
        let extra = self.session.request(RequestPurpose::DiscardCall, None);
        if let Err(err) = self
            .sender
            .send_json(&discard_call_request(extra, call_id, false, 0, is_video))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(())
    }

    /// Swap prompt: end the current call and answer the pending
    /// incoming one. `discardCall` for the active call goes out
    /// immediately; `acceptCall` for the pending call fires from the
    /// driver pump once the terminal updateCall lands (TDLib allows a
    /// single active call, so the accept must wait for the discard).
    pub fn accept_swap_call(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let (pending_id, _user_id, is_video) = self
            .session
            .call_swap_pending
            .take()
            .ok_or(ConnectSendError::InvalidRequest)?;
        self.session.call_swap_accept_queued = Some((pending_id, is_video));
        if self.session.active_call.is_some() {
            self.discard_call().map(|_| ())
        } else {
            self.maybe_accept_queued_swap()
        }
    }

    /// Driver pump: fire the queued post-swap `acceptCall` once no
    /// call is active. Called from `ingest` next to
    /// `maybe_decline_busy_calls`. If the caller hung up meanwhile,
    /// the server rejects the accept and the update stream records it.
    pub(crate) fn maybe_accept_queued_swap(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() || self.session.active_call.is_some() {
            return Ok(());
        }
        let Some((call_id, _is_video)) = self.session.call_swap_accept_queued.take() else {
            return Ok(());
        };
        let extra = self.session.request(RequestPurpose::AcceptCall, None);
        let protocol = self.engine_protocol_json();
        if let Err(err) = self
            .sender
            .send_json(&accept_call_with_protocol(extra, call_id, &protocol))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        if let Some(engine) = self.call_engine.as_deref_mut() {
            let _ = engine.accept_call(call_id);
        }
        Ok(())
    }

    /// Phase C2c: real mute through the native engine. When an available
    /// engine is installed and the transport exists, the engine is
    /// called first and its error propagates *without* flipping the
    /// session flag; otherwise the flag is stored (it applies to the
    /// transport on connect).
    pub fn set_call_muted(&mut self, muted: bool) -> Result<(), EngineError> {
        let Some(call) = self.session.active_call.as_mut() else {
            return Err(EngineError::NoActiveCall);
        };
        if call.transport.is_some()
            && let Some(engine) = self.call_engine.as_deref_mut()
            && engine.is_available()
        {
            // Engine first: a failed native call must not flip the flag.
            engine.set_muted(call.id, muted)?;
        }
        call.muted = muted;
        Ok(())
    }

    /// Phase C2c: pick the (microphone, speaker) device ids. `None`
    /// means the engine default. The pair is always stored; it is
    /// forwarded to the native engine only when a transport is already
    /// connected, and a failed forward propagates before the stored
    /// selection changes.
    pub fn select_call_devices(
        &mut self,
        mic: Option<String>,
        speaker: Option<String>,
    ) -> Result<(), EngineError> {
        let selection = (mic, speaker);
        if let Some(call) = self.session.active_call.as_ref()
            && call.transport.is_some()
            && let Some(engine) = self.call_engine.as_deref_mut()
        {
            engine.select_devices(call.id, selection.0.as_deref(), selection.1.as_deref())?;
        }
        self.selected_devices = selection;
        Ok(())
    }

    /// Phase C1: `sendCallRating` for the last ended call (the 1–5
    /// rating card, `callStateDiscarded.need_rating`). Marks the summary
    /// so the card can show "Thanks" while the `ok` confirms.
    /// Phase C2i: `sendCallRating` with problems + comment (schema
    /// 1.8.67 :14234). `problems` are `CallProblem` constructor names
    /// (`callProblemEcho`, …).
    pub fn send_call_rating(
        &mut self,
        rating: i32,
        comment: &str,
        problems: &[&str],
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let call_id = match &self.session.call_summary {
            Some(summary) if summary.need_rating && !summary.rating_sent => summary.call_id,
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        let extra = self.session.request(RequestPurpose::SendCallRating, None);
        if let Err(err) = self.sender.send_json(&send_call_rating_detail(
            extra, call_id, rating, comment, problems,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        if let Some(summary) = self.session.call_summary.as_mut() {
            summary.rating_sent = true;
        }
        Ok(extra)
    }

    pub(crate) fn call_debug_information(&self) -> Result<String, ConnectSendError> {
        let summary = self
            .session
            .call_summary
            .as_ref()
            .filter(|summary| summary.need_debug_information && !summary.debug_information_sent)
            .ok_or(ConnectSendError::InvalidRequest)?;
        Ok(self.call_log_payload(summary).to_string())
    }

    /// Phase C2i: the local call-log payload shared by
    /// `sendCallDebugInformation` (inline text) and `sendCallLog` (the
    /// same text as a file). The honest local record: app/engine
    /// identity, call outcome, transport states — never invented media
    /// stats.
    fn call_log_payload(&self, summary: &crate::state::CallSummary) -> serde_json::Value {
        let engine_available = self
            .call_engine
            .as_ref()
            .is_some_and(|engine| engine.is_available());
        let transport = summary.final_transport.map(|state| match state {
            TransportState::Connecting => "connecting",
            TransportState::Reconnecting => "reconnecting",
            TransportState::Connected => "connected",
            TransportState::Failed => "failed",
            TransportState::Closed => "closed",
        });
        let mut payload = serde_json::json!({
            "app": env!("CARGO_PKG_NAME"),
            "app_version": env!("CARGO_PKG_VERSION"),
            "os": std::env::consts::OS,
            "engine_available": engine_available,
            "call_id": summary.call_id,
            "duration_secs": summary.duration_secs,
            "had_audio": summary.had_audio,
            "final_transport_state": transport,
            "reconnect_attempts": summary.reconnect_attempts,
            "muted": summary.muted,
            "microphone_device_id": self.selected_devices.0,
            "speaker_device_id": self.selected_devices.1,
        });
        if engine_available {
            let protocol = self
                .call_engine
                .as_ref()
                .expect("available engine")
                .protocol();
            payload["engine_protocol"] = serde_json::json!({
                "udp_p2p": protocol.udp_p2p,
                "udp_reflector": protocol.udp_reflector,
                "min_layer": protocol.min_layer,
                "max_layer": protocol.max_layer,
                "library_versions": protocol.library_versions,
            });
        }
        payload
    }

    /// Phase C2d: upload real local call diagnostics for the last discarded
    /// call (`schema/td_api.tl:14237`).
    pub fn send_call_debug_information(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let call_id = self
            .session
            .call_summary
            .as_ref()
            .map(|summary| summary.call_id)
            .ok_or(ConnectSendError::InvalidRequest)?;
        let debug_information = self.call_debug_information()?;
        let extra = self
            .session
            .request(RequestPurpose::SendCallDebugInformation, None);
        if let Err(err) = self.sender.send_json(&send_call_debug_information(
            extra,
            call_id,
            &debug_information,
        )) {
            self.session.requests.take(extra);
            if let Some(summary) = self.session.call_summary.as_mut() {
                summary.debug_information_error = Some(match err {
                    ConnectSendError::InvalidRequest => {
                        "Could not upload diagnostics: invalid request".into()
                    }
                    ConnectSendError::Native => {
                        "Could not upload diagnostics: TDLib send failed".into()
                    }
                    // MED4: caption-length errors can't arise from a
                    // diagnostics upload; categorized as invalid request.
                    ConnectSendError::CaptionTooLong { .. } => {
                        "Could not upload diagnostics: invalid request".into()
                    }
                });
            }
            return Err(err);
        }
        if let Some(summary) = self.session.call_summary.as_mut() {
            summary.debug_information_sent = true;
            summary.debug_information_error = None;
        }
        Ok(extra)
    }

    /// Phase C2i: `sendCallLog` (schema 1.8.67 :14240) — uploads the
    /// ended call's log file. The file is the local diagnostics
    /// payload written under the account's exports dir (schema allows
    /// only `inputFileLocal` / `inputFileGenerated`).
    pub fn send_call_log(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let summary = self
            .session
            .call_summary
            .as_ref()
            .filter(|s| s.need_log && !s.log_sent)
            .ok_or(ConnectSendError::InvalidRequest)?;
        let log_text = self.call_log_payload(summary).to_string();
        let call_id = summary.call_id;
        let path = self.paths.exports.join(format!("call-{call_id}.log"));
        std::fs::create_dir_all(&self.paths.exports).map_err(|_| ConnectSendError::Native)?;
        std::fs::write(&path, log_text).map_err(|_| ConnectSendError::Native)?;
        let extra = self.session.request(RequestPurpose::SendCallLog, None);
        let path_str = path.to_string_lossy().into_owned();
        if let Err(err) = self
            .sender
            .send_json(&send_call_log(extra, call_id, &path_str))
        {
            self.session.requests.take(extra);
            if let Some(summary) = self.session.call_summary.as_mut() {
                summary.log_error = Some("Could not upload the call log".into());
            }
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase C2i: `searchCallMessages` (schema 1.8.67 :11903) — first
    /// page of the server-side recent-calls list. Called when the
    /// Recent-calls tab opens.
    pub fn fetch_call_history(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.recent_calls.clear();
        self.session.recent_calls_offset.clear();
        self.session.recent_calls_error = false;
        self.fetch_call_history_page()
    }

    /// Phase C2i: next `searchCallMessages` page, continuing from the
    /// stored `next_offset`.
    pub fn fetch_more_call_history(&mut self) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.fetch_call_history_page()
    }

    fn fetch_call_history_page(&mut self) -> Result<RequestId, ConnectSendError> {
        if self.session.recent_calls_loading {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SearchCallMessages, None);
        let offset = self.session.recent_calls_offset.clone();
        if let Err(err) = self
            .sender
            .send_json(&search_call_messages(extra, &offset, 40))
        {
            self.session.requests.take(extra);
            self.session.recent_calls_error = true;
            return Err(err);
        }
        self.session.recent_calls_loading = true;
        Ok(extra)
    }

    /// Phase C2i: fetch both call privacy settings
    /// (`userPrivacySettingAllowCalls` /
    /// `userPrivacySettingAllowPeerToPeerCalls`, schema 1.8.67
    /// :15620).
    pub fn fetch_call_privacy(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.call_privacy_loading = true;
        self.session.call_privacy_error = false;
        for setting in [
            CallPrivacySetting::AllowCalls,
            CallPrivacySetting::PeerToPeer,
        ] {
            let extra = self
                .session
                .request(RequestPurpose::GetCallPrivacyRules { setting }, None);
            if let Err(err) = self
                .sender
                .send_json(&get_user_privacy_setting_rules(extra, setting))
            {
                self.session.requests.take(extra);
                self.session.call_privacy_loading = false;
                self.session.call_privacy_error = true;
                return Err(err);
            }
            self.session.call_privacy_pending += 1;
        }
        Ok(())
    }

    /// Phase C2i: change a call privacy setting (schema 1.8.67
    /// :15617). Applied optimistically; the `ok` / error response
    /// confirms or clears it.
    pub fn set_call_privacy(
        &mut self,
        setting: CallPrivacySetting,
        who: PrivacyWho,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SetCallPrivacyRules { setting }, None);
        if let Err(err) = self
            .sender
            .send_json(&set_user_privacy_setting_rules(extra, setting, who))
        {
            self.session.requests.take(extra);
            self.session.call_privacy_loading = false;
            self.session.call_privacy_error = true;
            return Err(err);
        }
        match setting {
            CallPrivacySetting::AllowCalls => self.session.call_privacy_allow_calls = Some(who),
            CallPrivacySetting::PeerToPeer => self.session.call_privacy_p2p = Some(who),
        }
        self.session.call_privacy_loading = true;
        self.session.call_privacy_pending += 1;
        Ok(extra)
    }

    /// Phase C1: drain `Session::call_busy_decline_queue` — incoming
    /// calls that arrived while another call was active are declined
    /// (busy) with `discardCall`. Called from `ingest`.
    /// Phase C2i: the declined peer is recorded in
    /// `Session::call_busy_declined` so the UI can say so honestly
    /// instead of declining silently (TDLib has no hold/swap API —
    /// hold-and-answer is not possible).
    pub(crate) fn maybe_decline_busy_calls(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        let queued: Vec<(i32, i64, bool)> =
            std::mem::take(&mut self.session.call_busy_decline_queue);
        for (call_id, user_id, is_video) in queued {
            let extra = self.session.request(RequestPurpose::DiscardCall, None);
            if let Err(err) = self
                .sender
                .send_json(&discard_call_request(extra, call_id, false, 0, is_video))
            {
                self.session.requests.take(extra);
                return Err(err);
            }
            // ponytail: cap the banner list — it is drained by the UI.
            if self.session.call_busy_declined.len() < 4 {
                self.session.call_busy_declined.push((user_id, is_video));
            }
        }
        Ok(())
    }
}
