//! The `CallEngine` trait for tgcalls: 1:1 calls here, group calls in
//! `group.rs`. Live streams are the next step
//! (docs/decisions/tgcalls-engine.md): the broadcast answers keep the
//! trait's `Unavailable` defaults.
use super::*;
use crate::calls::stream_audio::watch::PlaybackHealth;
use crate::locks::LockExt;

impl CallEngine for TgcallsEngine {
    fn start_call(
        &mut self,
        call_id: i32,
        user_id: i64,
        _is_outgoing: bool,
    ) -> Result<(), EngineError> {
        // tgcalls needs nothing from the peer's id: TDLib carries the
        // signaling, keyed by call.
        let _ = user_id;
        if !self.calls.contains_key(&call_id) {
            self.calls
                .insert(call_id, new_call(call_id, &self.callback));
        }
        Ok(())
    }

    fn accept_call(&mut self, call_id: i32) -> Result<(), EngineError> {
        self.call(call_id).map(|_| ())
    }

    fn send_signaling_data(&mut self, call_id: i32, data: &[u8]) -> Result<(), EngineError> {
        let api = self.api;
        let call = self.call_mut(call_id)?;
        if data.is_empty() {
            return Ok(());
        }
        match call.handle {
            // SAFETY: a live call; `data` outlives this synchronous call.
            Some(handle) => unsafe {
                (api.qc_p2p_receive_signaling)(handle.as_ptr(), data.as_ptr(), data.len());
            },
            None => call.pending_signaling.push(data.to_vec()),
        }
        Ok(())
    }

    fn receive_signaling_data(&self, call_id: i32, data: &[u8]) {
        let hook = self.callback.hook.lock_or_recover().clone();
        if let Some(hook) = hook {
            hook(call_id, data.to_vec());
        }
    }

    fn set_signaling_emitted_callback(&mut self, callback: SignalingEmittedCallback) {
        *self.callback.hook.lock_or_recover() = Some(callback);
    }

    fn set_transport_state_callback(&mut self, callback: TransportStateCallback) {
        set_hook(&self.callback.transport_hook, callback);
    }

    fn set_remote_video_state_callback(&mut self, callback: RemoteVideoStateCallback) {
        set_hook(&self.callback.remote_video_hook, callback);
    }

    /// Never fired for a 1:1 call: tgcalls carries a screen share on the one
    /// video track, reported through the remote video state.
    fn set_remote_screen_state_callback(&mut self, callback: RemoteVideoStateCallback) {
        set_hook(&self.callback.remote_screen_hook, callback);
    }

    fn set_remote_audio_state_callback(&mut self, callback: RemoteAudioStateCallback) {
        set_hook(&self.callback.remote_audio_hook, callback);
    }

    fn connect(&mut self, call_id: i32, params: &ConnectParams) -> Result<(), EngineError> {
        let api = self.api;
        let max_layer = self.protocol.max_layer;
        let reaper = Arc::clone(&self.reaper);
        let call = self.call_mut(call_id)?;
        if call.handle.is_some() {
            // A retry: tgcalls can't connect a call twice, so start over
            // with a fresh instance and the same choices.
            call.retire_instance(api, &reaper);
        } else {
            // A share turned on before connect carries into the call.
            call.media = retained_call_media(params, Some(&call.media));
            if call.media.screen_share_on {
                call.media.camera = None;
            }
        }
        let config = config::p2p_config(params, max_layer).map_err(|err| {
            crate::advanced::debug_mode::call_log(&format!(
                "[call] {call_id} can't start tgcalls: {err:?}"
            ));
            EngineError::Engine {
                op: "qc_p2p_create",
                code: -1,
            }
        })?;
        let config = text("qc_p2p_create", &config)?;
        self.open_sound(call_id)?;
        self.apply_camera(call_id)?;
        let call = self.call_mut(call_id)?;
        let callbacks = call.context.callbacks();
        let video = call
            .camera
            .or(call.screen)
            .map_or(std::ptr::null_mut(), Native::as_ptr);
        // SAFETY: NUL-ended config; the context behind `callbacks.user`
        // lives in `call` until `qc_p2p_destroy` returns; the call takes
        // its own reference to `video`, Quill keeps its own.
        let handle = unsafe { (api.qc_p2p_create)(config.as_ptr(), &callbacks, video) };
        crate::advanced::debug_mode::call_log(&format!(
            "[call] {call_id} tgcalls create {:?} servers {} p2p {} -> {}",
            params.library_versions.first(),
            params.servers.len(),
            params.p2p_allowed,
            if handle.is_null() { "failed" } else { "ok" }
        ));
        let handle = Native::new(handle).ok_or(EngineError::Engine {
            op: "qc_p2p_create",
            code: -1,
        })?;
        call.handle = Some(handle);
        call.gate.open(api, handle.as_ptr());
        call.gate.set_screen(call.screen.map(Native::as_ptr));
        // SAFETY: a live call.
        unsafe { (api.qc_p2p_set_muted)(handle.as_ptr(), i32::from(call.muted)) };
        for data in std::mem::take(&mut call.pending_signaling) {
            // SAFETY: a live call; `data` outlives this synchronous call.
            unsafe { (api.qc_p2p_receive_signaling)(handle.as_ptr(), data.as_ptr(), data.len()) };
        }
        Ok(())
    }

    fn select_devices(
        &mut self,
        call_id: i32,
        microphone: Option<&str>,
        speaker: Option<&str>,
    ) -> Result<(), EngineError> {
        let call = self.call_mut(call_id)?;
        // The microphone is Quill's capture, which reads it from here.
        call.media.mic = microphone.map(str::to_owned);
        let reopen = call.media.speaker.as_deref() != speaker;
        call.media.speaker = speaker.map(str::to_owned);
        if reopen && call.handle.is_some() {
            self.open_sound(call_id)?;
        }
        Ok(())
    }

    fn hangup(&mut self, call_id: i32) -> Result<(), EngineError> {
        if let Some(call) = self.calls.remove(&call_id) {
            retire(self.api, &self.reaper, call);
        }
        Ok(())
    }

    fn set_muted(&mut self, call_id: i32, muted: bool) -> Result<(), EngineError> {
        let api = self.api;
        let call = self.call_mut(call_id)?;
        call.muted = muted;
        if let Some(handle) = call.handle {
            // SAFETY: a live call.
            unsafe { (api.qc_p2p_set_muted)(handle.as_ptr(), i32::from(muted)) };
        }
        Ok(())
    }

    fn set_camera(&mut self, call_id: i32, camera: Option<&str>) -> Result<(), EngineError> {
        let api = self.api;
        let call = self.call_mut(call_id)?;
        let previous = call.media.camera.clone();
        call.media.camera = camera.map(str::to_owned);
        if let Err(err) = self.apply_camera(call_id) {
            self.call_mut(call_id)?.media.camera = previous;
            return Err(err);
        }
        if camera.is_some() {
            // The camera replaced a screen share in the call, as in
            // tdesktop; its source can go now.
            let call = self.call_mut(call_id)?;
            call.media.screen_share_on = false;
            release_screen(api, call);
        }
        Ok(())
    }

    fn set_screen_share_enabled(&mut self, call_id: i32, enabled: bool) -> Result<(), EngineError> {
        let api = self.api;
        if !enabled {
            let call = self.call_mut(call_id)?;
            call.media.screen_share_on = false;
            if let (Some(handle), Some(_)) = (call.handle, call.screen) {
                // SAFETY: a live call; NULL stops sending video.
                unsafe { (api.qc_p2p_set_video_capture)(handle.as_ptr(), std::ptr::null_mut()) };
            }
            release_screen(api, call);
            return Ok(());
        }
        if self.call(call_id)?.screen.is_some() {
            return Ok(());
        }
        // The source comes first, so a failure leaves the camera on.
        // SAFETY: returns a new external source; Quill keeps the reference
        // in `call.screen` until `release_screen`.
        let screen = unsafe { (api.qc_video_capture_external)(1) };
        let screen = Native::new(screen).ok_or(EngineError::NoScreenSource)?;
        self.call_mut(call_id)?.screen = Some(screen);
        self.call_mut(call_id)?.media.camera = None;
        self.apply_camera(call_id)?;
        let call = self.call_mut(call_id)?;
        call.media.screen_share_on = true;
        if let Some(handle) = call.handle {
            // SAFETY: both live; the call takes its own reference.
            unsafe { (api.qc_p2p_set_video_capture)(handle.as_ptr(), screen.as_ptr()) };
            call.gate.set_screen(Some(screen.as_ptr()));
        }
        Ok(())
    }

    fn create_group_call(
        &mut self,
        group_call_id: i32,
        chat_id: i64,
        role: GroupCallRole,
    ) -> Result<(), EngineError> {
        self.create_group(group_call_id, chat_id, role, None)
    }

    fn take_group_offers(&mut self) -> Vec<GroupOffer> {
        self.land_group_offers()
    }

    fn create_conference_call(
        &mut self,
        key: i32,
        crypto: Arc<dyn FrameCrypto>,
    ) -> Result<(), EngineError> {
        // A conference call is in no chat: no stream is ever keyed by it.
        self.create_group(key, 0, GroupCallRole::Member, Some(crypto))
    }

    fn rekey_group_call(&mut self, from: i32, to: i32) -> Result<(), EngineError> {
        self.rekey_group(from, to)
    }

    fn set_group_ssrc_owners(&mut self, group_call_id: i32, owners: &SsrcOwners) {
        self.set_ssrc_owners(group_call_id, owners);
    }

    fn connect_group_call(
        &mut self,
        group_call_id: i32,
        answer_payload: &str,
        camera: Option<&str>,
    ) -> Result<(), EngineError> {
        self.connect_group(group_call_id, answer_payload, camera)
    }

    fn sync_group_video(
        &mut self,
        group_call_id: i32,
        sources: &[GroupVideoSource],
    ) -> Result<(), EngineError> {
        self.sync_group_videos(group_call_id, sources)
    }

    fn set_group_camera(
        &mut self,
        group_call_id: i32,
        camera: Option<&str>,
    ) -> Result<(), EngineError> {
        self.set_group_camera_device(group_call_id, camera)
    }

    fn start_screen_share(&mut self, group_call_id: i32) -> Result<(), EngineError> {
        self.start_presentation(group_call_id)
    }

    fn connect_screen_share(
        &mut self,
        group_call_id: i32,
        answer_payload: &str,
    ) -> Result<(), EngineError> {
        self.connect_presentation(group_call_id, answer_payload)
    }

    fn stop_screen_share(&mut self, group_call_id: i32) -> Result<(), EngineError> {
        self.stop_group_presentation(group_call_id)
    }

    fn presentation_active(&self, group_call_id: i32) -> bool {
        self.has_presentation(group_call_id)
    }

    fn set_group_muted(&mut self, group_call_id: i32, muted: bool) -> Result<(), EngineError> {
        self.set_group_mute(group_call_id, muted)
    }

    fn set_group_playback(
        &mut self,
        group_call_id: i32,
        speaker: Option<&str>,
        muted: bool,
    ) -> Result<(), EngineError> {
        self.set_group_output(group_call_id, speaker, muted)
    }

    fn set_stream_volume(&mut self, volume: f32) {
        self.set_streams_volume(volume);
    }

    fn stream_sound_health(&self, group_call_id: i32) -> Option<PlaybackHealth> {
        self.group_stream_health(group_call_id)
    }

    fn reopen_stream_sound(&mut self, group_call_id: i32) -> Result<(), EngineError> {
        self.reopen_group_sound(group_call_id)
    }

    fn set_engine_event_callback(&mut self, callback: EngineEventCallback) {
        set_hook(&self.callback.engine_event_hook, callback);
    }

    fn group_call_mode(&self, chat_id: i64) -> Option<GroupCallMode> {
        self.connected_group_mode(chat_id)
    }

    fn set_stream_quality(&mut self, chat_id: i64, quality: StreamQuality) {
        self.set_group_stream_quality(chat_id, quality);
    }

    fn stream_quality(&self, chat_id: i64) -> Option<StreamQuality> {
        self.group_stream_quality(chat_id)
    }

    fn answer_broadcast_time(
        &mut self,
        chat_id: i64,
        request: BroadcastRequestId,
        timestamp_ms: i64,
    ) -> Result<(), EngineError> {
        self.answer_group_time(chat_id, request, timestamp_ms);
        Ok(())
    }

    fn answer_broadcast_part(
        &mut self,
        chat_id: i64,
        answer: &BroadcastPartAnswer,
    ) -> Result<(), EngineError> {
        self.answer_group_part(chat_id, answer);
        Ok(())
    }

    fn set_live_frame_callback(&mut self, callback: LiveFrameCallback) {
        set_hook(&self.callback.live_frame_hook, callback);
    }

    fn call_sound_health(&self, call_id: i32) -> Option<PlaybackHealth> {
        self.calls
            .get(&call_id)?
            .context
            .sound
            .audio()
            .as_ref()
            .map(|audio| audio.health())
    }

    fn reopen_call_sound(&mut self, call_id: i32) -> Result<(), EngineError> {
        // There is no device output inside the library: reopen Quill's. A
        // failed open leaves no output, so `call_sound_health` is `None`.
        self.open_sound(call_id)
    }

    fn leave_group_call(&mut self, group_call_id: i32) -> Result<(), EngineError> {
        self.leave_group(group_call_id);
        Ok(())
    }

    fn external_frame_sender(
        &mut self,
        target: CaptureTarget,
    ) -> Option<Arc<dyn ExternalFrameSender>> {
        let call_id = match target {
            CaptureTarget::Private(call_id) => call_id,
            CaptureTarget::Group(group_call_id) => return self.group_sender(group_call_id),
        };
        let call = self.calls.get(&call_id)?;
        call.handle?;
        Some(Arc::clone(&call.gate) as Arc<dyn ExternalFrameSender>)
    }

    fn media_devices(&self) -> Result<Vec<MediaDevice>, EngineError> {
        let mut devices = devices::sound_devices();
        devices.extend(devices::cameras(self.api));
        Ok(devices)
    }

    fn list_media_devices(&self, answer: std::sync::mpsc::Sender<Vec<MediaDevice>>) {
        let api = self.api;
        let _ = crate::thread_guard::spawn_guarded("quill-call-devices", move || {
            let mut devices = devices::sound_devices();
            devices.extend(devices::cameras(api));
            let _ = answer.send(devices);
            crate::wake::UPDATES.poke();
        });
    }

    fn protocol(&self) -> EngineProtocol {
        self.protocol.clone()
    }

    fn is_available(&self) -> bool {
        true
    }
}
