//! The `CallEngine` trait: the app-facing call-engine interface.
use super::*;

/// Phase C2b: 1:1 engine lifecycle and bidirectional signaling.
///
/// There is deliberately no `connect_ready` method in this slice. Transport
/// connect (`ntg_skip_exchange` + `ntg_connect_p2p` + stream sources) is C2c:
/// `P2PCall::connect()` starts real default-device audio I/O, an unverifiable
/// production side effect. The engine queues inbound signaling until C2c
/// connects it.
pub trait CallEngine {
    /// Phase C2b: app -> engine; create the engine-side call keyed by
    /// Telegram user id.
    fn start_call(
        &mut self,
        call_id: i32,
        user_id: i64,
        is_outgoing: bool,
    ) -> Result<(), EngineError>;

    /// Phase C2b: app -> engine; mark a tracked incoming call as accepted.
    fn accept_call(&mut self, call_id: i32) -> Result<(), EngineError>;

    /// Phase C2b: app -> engine; feed TDLib `updateNewCallSignalingData`
    /// (`schema/td_api.tl:10862`) to `ntg_send_signaling_data`.
    fn send_signaling_data(&mut self, call_id: i32, data: &[u8]) -> Result<(), EngineError>;

    /// Phase C2b: engine -> app; route emitted bytes to the registered hook.
    /// The shared callback makes this callable without borrowing the driver
    /// thread.
    fn receive_signaling_data(&self, call_id: i32, data: &[u8]);

    /// Phase C2b: register the bridge that forwards engine-emitted data
    /// through TDLib `sendCallSignalingData` (`schema/td_api.tl:14218`).
    fn set_signaling_emitted_callback(&mut self, callback: SignalingEmittedCallback);

    fn set_transport_state_callback(&mut self, callback: TransportStateCallback);

    /// Phase C2e: engine -> app; register the hook receiving peer camera
    /// on/off/paused state.
    fn set_remote_video_state_callback(&mut self, callback: RemoteVideoStateCallback);

    /// Phase C2j: engine -> app; register the hook receiving the peer's
    /// 1:1 screen-share on/off/paused state. Same state enum as the
    /// camera hook; the driver drops retained screen frames when the
    /// peer's share goes inactive so no stale picture renders.
    fn set_remote_screen_state_callback(&mut self, callback: RemoteVideoStateCallback);

    /// engine -> app; register the hook receiving the peer's 1:1
    /// microphone on/off state (`(call id, muted)`). Engines without
    /// the signal never call it.
    fn set_remote_audio_state_callback(&mut self, callback: RemoteAudioStateCallback) {
        let _ = callback;
    }

    /// Phase C2e: app -> engine; send `camera` (an enumerated device id)
    /// or, for `None`, turn the camera off. ntgcalls opens only a named
    /// device, so there is no "default camera" here: the driver picks one.
    fn set_camera(&mut self, call_id: i32, camera: Option<&str>) -> Result<(), EngineError>;

    /// Phase C2i: app -> engine; toggle 1:1 screen-share send by
    /// re-issuing capture sources with the desktop description
    /// (`NTG_MEDIA_SOURCE_DESKTOP`, NULL input = default display).
    /// Enabling clears the camera intent — ntgcalls rejects
    /// camera+screen in Capture mode (`stream_manager.cpp:69`,
    /// v3.0.0), so screen share replaces the camera. Disabling leaves
    /// the camera off; the user re-enables it explicitly. A failed
    /// issuance restores the retained config.
    fn set_screen_share_enabled(&mut self, call_id: i32, enabled: bool) -> Result<(), EngineError>;

    /// Phase C2g: app -> engine; create the ntgcalls group context keyed
    /// by TDLib chat id and return the WebRTC offer that `joinVideoChat`
    /// carries as its `payload` (`schema/td_api.tl:14292`). The answer
    /// (`Text` response) goes back through `connect_group_call`. A
    /// `Viewer` context opens no capture source.
    fn create_group_call(
        &mut self,
        group_call_id: i32,
        chat_id: i64,
        role: GroupCallRole,
    ) -> Result<String, EngineError>;

    /// Phase C2g: app -> engine; finish the group handshake with the
    /// `joinVideoChat` answer, then issue capture sources, sending
    /// `camera` (a device id) when one is given. Idempotent per group
    /// call id.
    fn connect_group_call(
        &mut self,
        group_call_id: i32,
        answer_payload: &str,
        camera: Option<&str>,
    ) -> Result<(), EngineError>;

    /// Phase C2g: app -> engine; reconcile the engine's incoming-video
    /// subscriptions with the desired participant set (add new endpoints,
    /// remove stale ones). Frames for subscribed endpoints arrive on the
    /// live frame hook keyed `LiveKey::Video`.
    fn sync_group_video(
        &mut self,
        group_call_id: i32,
        sources: &[GroupVideoSource],
    ) -> Result<(), EngineError>;

    /// Phase C2g: app -> engine; send `camera` (a device id) in the group
    /// call, or turn it off for `None`, by re-issuing capture sources
    /// (mirrors the 1:1 `set_camera`).
    fn set_group_camera(
        &mut self,
        group_call_id: i32,
        camera: Option<&str>,
    ) -> Result<(), EngineError>;

    /// Phase C2g: app -> engine; begin screen sharing: `ntg_init_presentation`
    /// returns the offer that `startGroupCallScreenSharing`
    /// (`schema/td_api.tl:14303`) carries as its `payload`.
    fn start_screen_share(&mut self, group_call_id: i32) -> Result<String, EngineError>;

    /// Phase C2g: app -> engine; finish the presentation handshake with
    /// the `startGroupCallScreenSharing` answer and attach the desktop
    /// capture source.
    fn connect_screen_share(
        &mut self,
        group_call_id: i32,
        answer_payload: &str,
    ) -> Result<(), EngineError>;

    /// Phase C2g: app -> engine; stop the presentation transport
    /// (`ntg_stop_presentation`); pair with `endGroupCallScreenSharing`
    /// (`schema/td_api.tl:14309`).
    fn stop_screen_share(&mut self, group_call_id: i32) -> Result<(), EngineError>;

    /// Phase C2g: whether the native side has a presentation transport
    /// (initialized or connected). The driver reconciles this against
    /// the tracked screen-share flags, so a failed handshake never
    /// leaves a stray native presentation.
    fn presentation_active(&self, group_call_id: i32) -> bool;

    /// app -> engine; mute or unmute your microphone in a group call
    /// (`ntg_mute` / `ntg_unmute` on its chat). Kept for the next
    /// connect when the transport isn't up yet.
    fn set_group_muted(&mut self, group_call_id: i32, muted: bool) -> Result<(), EngineError> {
        let _ = (group_call_id, muted);
        Ok(())
    }

    /// app -> engine; the output device for a group call's sound (`None`
    /// is the system default) and whether you muted it for yourself
    /// (tdesktop's local mute of a stream). Stored for the next connect;
    /// a connected call re-issues only its playback sources.
    fn set_group_playback(
        &mut self,
        group_call_id: i32,
        speaker: Option<&str>,
        muted: bool,
    ) -> Result<(), EngineError> {
        let _ = (group_call_id, speaker, muted);
        Ok(())
    }

    /// app -> engine; the volume (0.0 to 1.0, the media viewer's) of
    /// every stream the engine plays itself, now and for later joins.
    fn set_stream_volume(&mut self, volume: f32) {
        let _ = volume;
    }

    /// How the stream in group call `group_call_id` plays through the
    /// engine's own output; `None` when the engine does not play it
    /// (not a stream, device playback, no output), so it has no volume.
    fn stream_sound_health(
        &self,
        group_call_id: i32,
    ) -> Option<crate::calls::stream_audio::watch::PlaybackHealth> {
        let _ = group_call_id;
        None
    }

    /// app -> engine; the self-healing fallback: play the stream in
    /// `group_call_id` on the device from now on.
    fn use_device_playback(&mut self, group_call_id: i32) -> Result<(), EngineError> {
        let _ = group_call_id;
        Ok(())
    }

    /// How 1:1 call `call_id`'s sound plays through the engine's own
    /// output; `None` when it plays on the device.
    fn call_sound_health(
        &self,
        call_id: i32,
    ) -> Option<crate::calls::stream_audio::watch::PlaybackHealth> {
        let _ = call_id;
        None
    }

    /// app -> engine; the self-healing fallback: play 1:1 call
    /// `call_id` on the device from now on.
    fn use_device_call_sound(&mut self, call_id: i32) -> Result<(), EngineError> {
        let _ = call_id;
        Ok(())
    }

    /// Stream mode: engine -> app; register the hook receiving ntgcalls'
    /// broadcast timestamp and part requests. The hook runs on an ntgcalls
    /// thread and must only queue the event.
    fn set_engine_event_callback(&mut self, callback: EngineEventCallback) {
        let _ = callback;
    }

    /// Engine -> app; register the hook receiving every decoded picture
    /// (group-call tracks, streams, 1:1 calls) as I420 planes, keyed by
    /// its live feed.
    fn set_live_frame_callback(&mut self, callback: LiveFrameCallback) {
        let _ = callback;
    }

    /// How the connected group call in `chat_id` carries media, read with
    /// `ntg_get_connection_mode` after `ntg_connect`. `None` when no
    /// connected call uses that chat.
    fn group_call_mode(&self, chat_id: i64) -> Option<GroupCallMode> {
        let _ = chat_id;
        None
    }

    /// Stream mode: the quality the stream tile needs (from its size). The
    /// connect side reads it back with `stream_quality` for the next part
    /// request; ntgcalls asks for quality-update parts itself.
    fn set_stream_quality(&mut self, chat_id: i64, quality: StreamQuality) {
        let _ = (chat_id, quality);
    }

    /// The last quality set with `set_stream_quality`.
    fn stream_quality(&self, chat_id: i64) -> Option<StreamQuality> {
        let _ = chat_id;
        None
    }

    /// Stream mode: answer a timestamp request
    /// (`ntg_send_broadcast_timestamp`); 0 means "no stream yet".
    fn send_broadcast_timestamp(
        &mut self,
        chat_id: i64,
        timestamp_ms: i64,
    ) -> Result<(), EngineError> {
        let _ = (chat_id, timestamp_ms);
        Err(EngineError::Unavailable)
    }

    /// Stream mode: answer a part request (`ntg_send_broadcast_part`).
    fn send_broadcast_part(
        &mut self,
        chat_id: i64,
        answer: &BroadcastPartAnswer,
    ) -> Result<(), EngineError> {
        let _ = (chat_id, answer);
        Err(EngineError::Unavailable)
    }

    /// Phase C2g: app -> engine; tear down the group transport
    /// (`ntg_stop` on the chat id). Unknown group call ids succeed.
    fn leave_group_call(&mut self, group_call_id: i32) -> Result<(), EngineError>;

    fn connect(&mut self, call_id: i32, params: &ConnectParams) -> Result<(), EngineError>;

    fn select_devices(
        &mut self,
        call_id: i32,
        microphone: Option<&str>,
        speaker: Option<&str>,
    ) -> Result<(), EngineError>;

    /// Phase C2b: app -> engine; idempotent teardown, where an unknown call
    /// id succeeds.
    fn hangup(&mut self, call_id: i32) -> Result<(), EngineError>;

    /// Phase C2b: app -> engine; set microphone mute state for a tracked call.
    fn set_muted(&mut self, call_id: i32, muted: bool) -> Result<(), EngineError>;

    /// app -> engine; a sender for `target`'s external sources, for a
    /// feeder thread (`calls::capture`). `None` until the call's sources
    /// are issued (1:1 connected, group transport connected) or when the
    /// engine has no external sources. Frames sent before the sources
    /// exist are rejected by ntgcalls, so callers wait for `Some`.
    fn external_frame_sender(
        &mut self,
        target: CaptureTarget,
    ) -> Option<std::sync::Arc<dyn ExternalFrameSender>> {
        let _ = target;
        None
    }

    /// app -> engine; push one frame into `target`'s external source
    /// (`ntg_send_external_frame`). The feeder uses the sender from
    /// [`Self::external_frame_sender`] directly; this is the one-shot form.
    fn send_external_frame(
        &mut self,
        target: CaptureTarget,
        device: ExternalDevice,
        data: &[u8],
        meta: ExternalFrameMeta,
    ) -> Result<(), EngineError> {
        self.external_frame_sender(target)
            .ok_or(EngineError::Unavailable)?
            .send(device, data, meta)
    }

    /// Phase C2b: engine -> app; enumerate capture and playback devices.
    fn media_devices(&self) -> Result<Vec<MediaDevice>, EngineError>;

    /// Phase C2b: engine -> app; return the protocol the engine itself reports.
    fn protocol(&self) -> EngineProtocol;

    /// Phase C2b: engine -> app; report whether operations are available.
    fn is_available(&self) -> bool;
}
