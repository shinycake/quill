//! Connect driver: 1:1 call control (TDLib signaling + engine bridge).
use super::*;
use crate::calls::engine::{
    CallEngine, ConnectParams, EngineError, MediaDevice, MediaDeviceKind, RtcServer,
    TransportState, pick_camera,
};
use crate::ids::RequestId;
use crate::locks::LockExt;
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
mod sound_watch;

impl<S: JsonSender> ConnectDriver<S> {
    /// Phase C2b: install the driver-thread call engine and route its
    /// worker-thread signaling/transport emissions into the driver
    /// queues. Devices are fetched on demand (`refresh_call_devices`),
    /// never here.
    /// An engine installed while a call is already tracked (it rang on an
    /// account that was in the background, which loads its engine on
    /// demand): start that call in the engine, as `pump_call_engine` does
    /// when a call first appears.
    pub fn start_engine_for_tracked_call(&mut self) {
        let Some((call_id, user_id, is_outgoing)) = self
            .session
            .calls
            .active_call
            .as_ref()
            .map(|call| (call.id, call.user_id, call.is_outgoing))
        else {
            return;
        };
        self.yield_story_stream_key(user_id);
        let Some(engine) = self.call_engine.as_deref_mut() else {
            return;
        };
        if let Err(err) = engine.start_call(call_id, user_id, is_outgoing)
            && let Some(call) = self.session.calls.active_call.as_mut()
        {
            call.transport_error = Some(err.to_string());
        }
    }

    pub fn set_call_engine(&mut self, mut engine: Box<dyn CallEngine>) {
        let outbox = self.signaling_outbox.clone();
        // Each hook wakes the poll loop, which applies the report at once
        // (`pump_call_engine_reports`) instead of after its idle sleep.
        engine.set_signaling_emitted_callback(Arc::new(move |call_id, data| {
            outbox.lock_or_recover().push_back((call_id, data));
            crate::wake::UPDATES.poke();
        }));
        let transport_outbox = self.transport_outbox.clone();
        engine.set_transport_state_callback(Arc::new(move |call_id, state| {
            crate::advanced::debug_mode::debug_log(&format!(
                "[call] {call_id} transport {state:?}"
            ));
            transport_outbox
                .lock_or_recover()
                .push_back((call_id, state));
            crate::wake::UPDATES.poke();
        }));
        self.install_remote_state_hooks(engine.as_mut());
        self.install_stream_hooks(engine.as_mut());
        // Every picture (1:1 calls, group calls, streams): planes into
        // their live feeds.
        engine.set_live_frame_callback(crate::connect::group_calls::live_feeds::live_frame_hook(
            self.live_feeds.clone(),
        ));
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

    /// The listed name of device `id` of `kind` (what ffmpeg capture
    /// matches on); `None` for the default or an unknown id.
    fn call_device_name(&self, id: Option<&str>, kind: MediaDeviceKind) -> Option<String> {
        let id = id?;
        self.call_devices_cache
            .iter()
            .find(|device| device.kind == kind && device.id == id)
            .map(|device| device.name.clone())
    }

    /// The camera and microphone the user picked in the call device menu,
    /// by listed name, for ffmpeg capture (round videos, profile-photo
    /// camera). Derived on demand from the selected ids and the device list,
    /// so it can never disagree with the menu; an unset or unlisted pick is
    /// `None` (system default).
    pub fn capture_choice(&self) -> crate::capture_choice::CaptureChoice {
        crate::capture_choice::CaptureChoice {
            camera: self.call_device_name(self.selected_camera.as_deref(), MediaDeviceKind::Camera),
            microphone: self.call_device_name(
                self.selected_devices.0.as_deref(),
                MediaDeviceKind::Microphone,
            ),
        }
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
    /// When Quill captures the screen itself (`connect::call_screen`),
    /// the source is the one picked in the chooser.
    pub fn call_screen_source_available(&self) -> bool {
        if self.call_screen.enabled() {
            return self.call_screen.chosen().is_some();
        }
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
        // A camera toggle before the transport existed is stored in
        // `camera_on` and survives into the connect params: a video call
        // starts with it on, a voice call with it off, and either may
        // have been flipped while ringing.
        let camera_input = self
            .session
            .calls
            .active_call
            .as_ref()
            .is_some_and(|call| call.camera_on)
            .then(|| self.call_camera_device())
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
            // The connect path refreshes the device cache before calling
            // this (cheap no-op when the engine is missing).
            camera_input,
        }
    }

    /// Phase C2e: honest 1:1 video readiness — the engine is available
    /// and lists a camera. Any call can turn the camera on (tdesktop
    /// offers it in voice calls too); the UI uses this for disabled
    /// states instead of guessing.
    pub fn call_video_ready(&self) -> bool {
        self.call_engine
            .as_ref()
            .is_some_and(|engine| engine.is_available())
            && pick_camera(None, &self.call_devices_cache).is_some()
    }

    /// Phase C2e: selected camera device id; `None` is the first camera.
    pub fn selected_call_camera(&self) -> Option<&str> {
        self.selected_camera.as_deref()
    }

    /// The camera a call would turn on now, from a fresh device list:
    /// `Unavailable` without the call library (no list to read, which is
    /// not the same as no camera), `NoCamera` when none is listed.
    pub fn camera_to_turn_on(&mut self) -> Result<String, EngineError> {
        if !self
            .call_engine
            .as_ref()
            .is_some_and(|engine| engine.is_available())
        {
            return Err(EngineError::Unavailable);
        }
        self.refresh_call_devices();
        self.call_camera_device().ok_or(EngineError::NoCamera)
    }

    /// The camera calls send from now, by device id, from the cached
    /// device list (`pick_camera`).
    pub(crate) fn call_camera_device(&self) -> Option<String> {
        pick_camera(self.selected_camera.as_deref(), &self.call_devices_cache)
    }

    /// Phase C2e: camera toggle through the native engine. The engine is
    /// called first and its error propagates *without* flipping the
    /// session flag (mirrors `set_call_muted`); without a connected
    /// transport the intent is only stored (it applies on connect).
    /// Turning it on re-reads the device list and fails with `NoCamera`
    /// when there is none.
    /// Phase C2i: enabling the camera clears the screen-share intent —
    /// ntgcalls forbids camera+screen in Capture mode.
    pub fn set_call_camera(&mut self, call_id: i32, enabled: bool) -> Result<(), EngineError> {
        if !self
            .session
            .calls
            .active_call
            .as_ref()
            .is_some_and(|call| call.id == call_id)
        {
            return Err(if self.session.calls.active_call.is_some() {
                EngineError::NoSuchCall(call_id)
            } else {
                EngineError::NoActiveCall
            });
        }
        let camera = if enabled {
            Some(self.camera_to_turn_on()?)
        } else {
            None
        };
        let Some(call) = self.session.calls.active_call.as_mut() else {
            return Err(EngineError::NoActiveCall);
        };
        if call.transport.is_some()
            && let Some(engine) = self.call_engine.as_deref_mut()
            && engine.is_available()
        {
            engine.set_camera(call.id, camera.as_deref())?;
        }
        call.camera_on = enabled;
        if enabled {
            call.screen_sharing = false;
        }
        self.sync_one_to_one_feeds();
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
        let Some(call) = self.session.calls.active_call.as_mut() else {
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

    /// Phase C2e: pick the camera device id (`None` = the first camera).
    /// The selection is stored (so it applies on connect) and, while the
    /// camera is on over a connected transport, the engine switches to
    /// it; a failed switch propagates before the stored selection
    /// changes.
    pub fn select_call_camera(&mut self, camera: Option<String>) -> Result<(), EngineError> {
        let sending = self
            .session
            .calls
            .active_call
            .as_ref()
            .filter(|call| call.camera_on && call.transport.is_some())
            .map(|call| call.id);
        if let Some(call_id) = sending
            && let Some(engine) = self.call_engine.as_deref_mut()
            && engine.is_available()
        {
            // An empty list must not switch a running camera off while the
            // call still says it is on.
            let device = pick_camera(camera.as_deref(), &self.call_devices_cache)
                .ok_or(EngineError::NoCamera)?;
            engine.set_camera(call_id, Some(&device))?;
        }
        self.selected_camera = camera;
        Ok(())
    }

    /// Per poll: apply what the call engine reported since the last pump
    /// (signaling to relay, transport and peer media states). A connected
    /// call's own signaling runs over its data channel, so Telegram may
    /// send nothing for a long time and these must not wait for an
    /// unrelated update. Returns whether anything was pending.
    pub(crate) fn pump_call_engine_reports(&mut self) -> bool {
        let pending = !self.signaling_outbox.lock_or_recover().is_empty()
            || !self.transport_outbox.lock_or_recover().is_empty()
            || !self.video_state_outbox.lock_or_recover().is_empty()
            || !self.screen_state_outbox.lock_or_recover().is_empty()
            || !self.audio_state_outbox.lock_or_recover().is_empty();
        if !pending {
            return false;
        }
        let active = self.session.calls.active_call.as_ref().map(|call| call.id);
        // A send failure leaves the bytes queued for the next poll.
        let _ = self.pump_call_engine(active, None);
        self.drain_remote_audio_state();
        true
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
            .calls
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

        if let Some((_, user_id, _, _)) = &active_call_after
            && tracked_call_id != active_call_before
        {
            self.yield_story_stream_key(*user_id);
        }
        if let Some((call_id, user_id, is_outgoing, _)) = &active_call_after
            && Some(*call_id) != active_call_before
            && let Some(engine) = self.call_engine.as_deref_mut()
        {
            // TDLib remains the source of truth; engine startup failures must
            // not erase the reducer's honest signaling-only call state.
            if let Err(err) = engine.start_call(*call_id, *user_id, *is_outgoing)
                && let Some(call) = self.session.calls.active_call.as_mut()
            {
                call.transport_error = Some(err.to_string());
            }
        }
        if tracked_call_id.is_none()
            && let Some(call_id) = active_call_before
        {
            if let Some(summary) = self
                .session
                .calls
                .summary
                .as_mut()
                .filter(|summary| summary.call_id == call_id)
            {
                summary.reconnect_attempts = self.reconnect_attempts;
            }
            self.log_call_sound_summary(call_id);
            if let Some(engine) = self.call_engine.as_deref_mut() {
                let _ = engine.hangup(call_id);
            }
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
                .calls
                .active_call
                .as_ref()
                .is_some_and(|call| call.transport.is_none())
        {
            let call_id = *call_id;
            let is_outgoing = *is_outgoing;
            if ready.encryption_key.is_empty() {
                if let Some(call) = self.session.calls.active_call.as_mut() {
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
                    .calls
                    .active_call
                    .as_ref()
                    .is_some_and(|call| call.screen_sharing);
                if result.is_ok()
                    && pre_sharing
                    && let Some(engine) = self.call_engine.as_deref_mut()
                {
                    let _ = engine.set_screen_share_enabled(call_id, true);
                }
                if let Some(call) = self.session.calls.active_call.as_mut() {
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
            let update = self.transport_outbox.lock_or_recover().pop_front();
            let Some((call_id, state)) = update else {
                break;
            };
            if !self
                .session
                .calls
                .active_call
                .as_ref()
                .is_some_and(|call| call.id == call_id)
            {
                continue;
            }
            crate::advanced::debug_mode::call_log(&format!("[call] {call_id} transport {state:?}"));
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
                    if let Some(call) = self.session.calls.active_call.as_mut() {
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
                    && let Some(call) = self.session.calls.active_call.as_mut()
                {
                    call.transport = Some(TransportState::Failed);
                    call.transport_error = Some(error);
                }
            } else if let Some(call) = self.session.calls.active_call.as_mut() {
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
            let update = self.video_state_outbox.lock_or_recover().pop_front();
            let Some((call_id, state)) = update else {
                break;
            };
            if let Some(call) = self
                .session
                .calls
                .active_call
                .as_mut()
                .filter(|call| call.id == call_id)
            {
                call.remote_video = state;
            }
        }

        // Phase C2j: the peer's 1:1 screen-share state follows the same
        // gate as the camera state, and is recorded on the call.
        loop {
            let update = self.screen_state_outbox.lock_or_recover().pop_front();
            let Some((call_id, state)) = update else {
                break;
            };
            if let Some(call) = self
                .session
                .calls
                .active_call
                .as_mut()
                .filter(|call| call.id == call_id)
            {
                call.remote_screen = state;
            }
        }
        // A picture that went off (the peer's camera or share inactive,
        // your camera off) or a call that ended ends its feed, so no
        // stale picture can render and one turned on again starts fresh.
        self.sync_one_to_one_feeds();

        loop {
            let emitted = self.signaling_outbox.lock_or_recover().pop_front();
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
                    .lock_or_recover()
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
            .calls
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
                .lock_or_recover()
                .retain(|(id, _)| *id != call_id);
            return Err(error);
        }
        Ok(())
    }
}
