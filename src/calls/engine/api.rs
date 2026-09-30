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

    /// Phase C2e: engine -> app; register the hook receiving decoded video
    /// frames (local preview and peer camera).
    fn set_video_frame_callback(&mut self, callback: VideoFrameCallback);

    /// Phase C2e: engine -> app; register the hook receiving peer camera
    /// on/off/paused state.
    fn set_remote_video_state_callback(&mut self, callback: RemoteVideoStateCallback);

    /// Phase C2j: engine -> app; register the hook receiving the peer's
    /// 1:1 screen-share on/off/paused state. Same state enum as the
    /// camera hook; the driver drops retained screen frames when the
    /// peer's share goes inactive so no stale picture renders.
    fn set_remote_screen_state_callback(&mut self, callback: RemoteVideoStateCallback);

    /// Phase C2e: app -> engine; toggle the local camera. `camera` doubles
    /// as camera selection, `None` means the default device.
    fn set_camera_enabled(
        &mut self,
        call_id: i32,
        enabled: bool,
        camera: Option<&str>,
    ) -> Result<(), EngineError>;

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
    /// (`Text` response) goes back through `connect_group_call`.
    fn create_group_call(
        &mut self,
        group_call_id: i32,
        chat_id: i64,
    ) -> Result<String, EngineError>;

    /// Phase C2g: app -> engine; finish the group handshake with the
    /// `joinVideoChat` answer, then issue capture sources (camera only
    /// when `video_enabled` — there is no group audio transport in this
    /// slice). Idempotent per group call id.
    fn connect_group_call(
        &mut self,
        group_call_id: i32,
        answer_payload: &str,
        video_enabled: bool,
    ) -> Result<(), EngineError>;

    /// Phase C2g: app -> engine; reconcile the engine's incoming-video
    /// subscriptions with the desired participant set (add new endpoints,
    /// remove stale ones). Frames for subscribed endpoints arrive on the
    /// video frame hook with `participant_user_id` set.
    fn sync_group_video(
        &mut self,
        group_call_id: i32,
        sources: &[GroupVideoSource],
    ) -> Result<(), EngineError>;

    /// Phase C2g: app -> engine; toggle the outgoing group camera by
    /// re-issuing capture sources (mirrors the 1:1 `set_camera_enabled`).
    fn set_group_camera(
        &mut self,
        group_call_id: i32,
        enabled: bool,
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

    /// Phase C2b: engine -> app; enumerate capture and playback devices.
    fn media_devices(&self) -> Result<Vec<MediaDevice>, EngineError>;

    /// Phase C2b: engine -> app; return the protocol the engine itself reports.
    fn protocol(&self) -> EngineProtocol;

    /// Phase C2b: engine -> app; report whether operations are available.
    fn is_available(&self) -> bool;
}
