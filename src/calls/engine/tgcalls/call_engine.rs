//! The `CallEngine` trait for tgcalls: 1:1 calls. Group calls and streams
//! are the next steps (docs/decisions/tgcalls-engine.md).
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

    fn set_remote_screen_state_callback(&mut self, callback: RemoteVideoStateCallback) {
        set_hook(&self.callback.remote_screen_hook, callback);
    }

    fn set_remote_audio_state_callback(&mut self, callback: RemoteAudioStateCallback) {
        set_hook(&self.callback.remote_audio_hook, callback);
    }

    fn connect(&mut self, call_id: i32, params: &ConnectParams) -> Result<(), EngineError> {
        let api = self.api;
        let max_layer = self.protocol.max_layer;
        let call = self.call_mut(call_id)?;
        if let Some(handle) = call.handle.take() {
            // A retry: tgcalls can't connect a call twice, so start over
            // with a fresh instance and the same choices.
            call.gate.close();
            // SAFETY: a live call destroyed once, not from its callback.
            unsafe { (api.qc_p2p_destroy)(handle.as_ptr()) };
        } else {
            call.media = retained_call_media(params, None);
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
        let config = native_string(&config)?;
        self.open_sound(call_id)?;
        self.apply_camera(call_id)?;
        let call = self.call_mut(call_id)?;
        let callbacks = call.context.callbacks();
        let video = call.camera.map_or(std::ptr::null_mut(), NonNull::as_ptr);
        // SAFETY: NUL-ended config; the context behind `callbacks.user`
        // lives in `call` until `qc_p2p_destroy` returns; the call takes
        // its own reference to `video`.
        let handle = unsafe { (api.qc_p2p_create)(config.as_ptr(), &callbacks, video) };
        crate::advanced::debug_mode::call_log(&format!(
            "[call] {call_id} tgcalls create {:?} servers {} p2p {} -> {}",
            params.library_versions.first(),
            params.servers.len(),
            params.p2p_allowed,
            if handle.is_null() { "failed" } else { "ok" }
        ));
        let handle = NonNull::new(handle).ok_or(EngineError::Engine {
            op: "qc_p2p_create",
            code: -1,
        })?;
        call.handle = Some(handle);
        call.gate.open(api, handle.as_ptr());
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
        if let Some(mut call) = self.calls.remove(&call_id) {
            if let Some(handle) = call.handle {
                crate::advanced::debug_mode::call_log(&format!(
                    "[call] {call_id} tgcalls end: {}",
                    debug_info(self.api, handle)
                ));
            }
            destroy(self.api, &mut call);
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
        let call = self.call_mut(call_id)?;
        let previous = call.media.camera.clone();
        call.media.camera = camera.map(str::to_owned);
        if camera.is_some() {
            // The camera replaces a screen share, as in tdesktop.
            call.media.screen_share_on = false;
            call.gate.set_screen(None);
        }
        if let Err(err) = self.apply_camera(call_id) {
            self.call_mut(call_id)?.media.camera = previous;
            return Err(err);
        }
        Ok(())
    }

    fn set_screen_share_enabled(&mut self, call_id: i32, enabled: bool) -> Result<(), EngineError> {
        let api = self.api;
        if enabled {
            self.call_mut(call_id)?.media.camera = None;
            self.apply_camera(call_id)?;
        }
        let call = self.call_mut(call_id)?;
        call.media.screen_share_on = enabled;
        let Some(handle) = call.handle else {
            return Ok(());
        };
        if enabled {
            // SAFETY: returns a new external source owned by this scope.
            let screen = unsafe { (api.qc_video_capture_external)(1) };
            let screen = NonNull::new(screen).ok_or(EngineError::NoScreenSource)?;
            // SAFETY: both live; the call keeps its own reference, so
            // Quill's is released right away and the gate keeps a borrow
            // that is cleared before the call (and so the source) goes.
            unsafe { (api.qc_p2p_set_video_capture)(handle.as_ptr(), screen.as_ptr()) };
            call.gate.set_screen(Some(screen.as_ptr()));
            // SAFETY: Quill's reference; the call still holds the source.
            unsafe { (api.qc_video_capture_free)(screen.as_ptr()) };
        } else {
            call.gate.set_screen(None);
            // SAFETY: a live call; NULL stops sending video.
            unsafe { (api.qc_p2p_set_video_capture)(handle.as_ptr(), std::ptr::null_mut()) };
        }
        Ok(())
    }

    fn create_group_call(
        &mut self,
        _group_call_id: i32,
        _chat_id: i64,
        _role: GroupCallRole,
    ) -> Result<String, EngineError> {
        Err(EngineError::Unavailable)
    }

    fn connect_group_call(
        &mut self,
        _group_call_id: i32,
        _answer_payload: &str,
        _camera: Option<&str>,
    ) -> Result<(), EngineError> {
        Err(EngineError::Unavailable)
    }

    fn sync_group_video(
        &mut self,
        _group_call_id: i32,
        _sources: &[GroupVideoSource],
    ) -> Result<(), EngineError> {
        Err(EngineError::Unavailable)
    }

    fn set_group_camera(
        &mut self,
        _group_call_id: i32,
        _camera: Option<&str>,
    ) -> Result<(), EngineError> {
        Err(EngineError::Unavailable)
    }

    fn start_screen_share(&mut self, _group_call_id: i32) -> Result<String, EngineError> {
        Err(EngineError::Unavailable)
    }

    fn connect_screen_share(
        &mut self,
        _group_call_id: i32,
        _answer_payload: &str,
    ) -> Result<(), EngineError> {
        Err(EngineError::Unavailable)
    }

    fn stop_screen_share(&mut self, _group_call_id: i32) -> Result<(), EngineError> {
        Ok(())
    }

    fn presentation_active(&self, _group_call_id: i32) -> bool {
        false
    }

    fn set_live_frame_callback(&mut self, callback: LiveFrameCallback) {
        set_hook(&self.callback.live_frame_hook, callback);
    }

    fn call_sound_health(&self, call_id: i32) -> Option<PlaybackHealth> {
        self.calls
            .get(&call_id)?
            .sound
            .as_ref()
            .map(|player| player.audio().health())
    }

    fn use_device_call_sound(&mut self, call_id: i32) -> Result<(), EngineError> {
        // There is no device output inside the library: reopen Quill's.
        self.open_sound(call_id)
    }

    fn leave_group_call(&mut self, _group_call_id: i32) -> Result<(), EngineError> {
        Ok(())
    }

    fn external_frame_sender(
        &mut self,
        target: CaptureTarget,
    ) -> Option<Arc<dyn ExternalFrameSender>> {
        let CaptureTarget::Private(call_id) = target else {
            return None;
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

    fn protocol(&self) -> EngineProtocol {
        self.protocol.clone()
    }

    fn is_available(&self) -> bool {
        true
    }
}
