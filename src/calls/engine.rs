//! Phase C2b: call-engine abstraction plus the runtime-loaded ntgcalls adapter.

use ntgcalls_sys::{
    Loader, NTG_CONNECTION_STATE_CLOSED, NTG_CONNECTION_STATE_CONNECTED,
    NTG_CONNECTION_STATE_CONNECTING, NTG_CONNECTION_STATE_FAILED, NTG_CONNECTION_STATE_TIMEOUT,
    NTG_ERR_INVALID_PARAMS, NTG_MEDIA_SOURCE_DESKTOP, NTG_MEDIA_SOURCE_DEVICE, NTG_OK,
    NTG_STREAM_DEVICE_CAMERA, NTG_STREAM_DEVICE_SCREEN, NTG_STREAM_MODE_CAPTURE,
    NTG_STREAM_MODE_PLAYBACK, NTG_STREAM_STATUS_ACTIVE, NTG_STREAM_STATUS_PAUSED,
    NTG_VIDEO_ROTATION_VIDEO_ROTATION_90, NTG_VIDEO_ROTATION_VIDEO_ROTATION_180,
    NTG_VIDEO_ROTATION_VIDEO_ROTATION_270, ntg_audio_description, ntg_connection_info,
    ntg_device_info, ntg_frame, ntg_instance, ntg_media_description, ntg_media_devices,
    ntg_protocol, ntg_remote_source, ntg_rtc_server, ntg_ssrc_group, ntg_stream_device,
    ntg_stream_mode, ntg_stream_status, ntg_video_description, ntg_video_rotation,
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_void};
use std::marker::PhantomData;
use std::ptr::{NonNull, null_mut};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Phase C2b: engine-emitted signaling routed as `(tdlib_call_id, data)`.
pub type SignalingEmittedCallback = Arc<dyn Fn(i32, Vec<u8>) + Send + Sync + 'static>;

pub type TransportStateCallback = Arc<dyn Fn(i32, TransportState) + Send + Sync + 'static>;

/// Phase C2e: peer camera state, mirroring Telegram X's `VideoState`
/// annotation (`INACTIVE=0, PAUSED=1, ACTIVE=2`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteVideoState {
    Inactive,
    Paused,
    Active,
}

/// Phase C2e: one decoded video frame. `rgba` is RGBA8 row-major with the
/// frame rotation already applied; `is_local` marks the local camera
/// preview, `false` marks the peer's camera or screen share
/// (disambiguated by `is_screen`).
/// Phase C2g: group-call frames carry `participant_user_id` (the TDLib
/// user id the frame's ssrc was subscribed for); `None` for 1:1 calls
/// and the local preview. `is_screen` marks frames from a participant's
/// screen-share stream (`NTG_STREAM_DEVICE_SCREEN`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoFrame {
    pub seq: u64,
    pub width: u16,
    pub height: u16,
    pub rgba: Vec<u8>,
    pub is_local: bool,
    pub participant_user_id: Option<i64>,
    pub is_screen: bool,
}

pub type VideoFrameCallback = Arc<dyn Fn(i32, VideoFrame) + Send + Sync + 'static>;

pub type RemoteVideoStateCallback = Arc<dyn Fn(i32, RemoteVideoState) + Send + Sync + 'static>;

/// Phase C2g: extract the audio channel SSRC from ntgcalls' group join
/// offer (a raw SDP offer per the ntgcalls API docs: `create(chatId)`
/// "returns a WebRTC offer (SDP)"). TDLib's `groupCallJoinParameters`
/// carries it as `audio_source_id` ("received from tgcalls",
/// `schema/td_api.tl:7085`). The SSRC is the first `a=ssrc:` line of
/// the audio `m=` section; 0 when absent (honest fallback — the join
/// still goes out, only audio attribution is missing, and this slice
/// has no group audio transport anyway).
pub fn group_offer_audio_source_id(offer: &str) -> i32 {
    let mut in_audio = false;
    for line in offer.lines().map(str::trim) {
        if let Some(kind) = line.strip_prefix("m=") {
            in_audio = kind.starts_with("audio");
            continue;
        }
        if in_audio && let Some(rest) = line.strip_prefix("a=ssrc:") {
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(ssrc) = digits.parse::<u32>() {
                return ssrc as i32;
            }
        }
    }
    0
}
/// Phase C2g: one participant video channel to subscribe to, built from
/// TDLib's `groupCallParticipantVideoInfo` (schema 1.8.67, line 7163).
/// `endpoint` + `ssrc_groups` feed `ntg_add_incoming_video` verbatim;
/// frames are attributed back to `user_id` by ssrc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupVideoSourceGroup {
    pub semantics: String,
    pub ssrcs: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupVideoSource {
    pub user_id: i64,
    pub endpoint: String,
    pub ssrc_groups: Vec<GroupVideoSourceGroup>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportState {
    Connecting,
    /// Driver-only state while retrying the retained connect parameters.
    Reconnecting,
    Connected,
    Failed,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RtcServer {
    pub id: u64,
    pub ipv4: String,
    pub ipv6: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub turn: bool,
    pub stun: bool,
    pub tcp: bool,
    pub peer_tag: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectParams {
    pub encryption_key: Vec<u8>,
    pub is_outgoing: bool,
    pub servers: Vec<RtcServer>,
    pub library_versions: Vec<String>,
    pub p2p_allowed: bool,
    pub mic_input: Option<String>,
    pub speaker_input: Option<String>,
    /// Phase C2e: enable the local camera during connect.
    pub video_enabled: bool,
    /// Phase C2e: camera device metadata used to select the device;
    /// `None` means the engine default.
    pub camera_input: Option<String>,
}

/// Phase C2b: the exact `callProtocol` capability reported to TDLib.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineProtocol {
    pub udp_p2p: bool,
    pub udp_reflector: bool,
    pub min_layer: i32,
    pub max_layer: i32,
    pub library_versions: Vec<String>,
}

impl EngineProtocol {
    /// Phase C2b: the existing honest fallback when no engine is available.
    pub fn signaling_only() -> Self {
        Self {
            udp_p2p: false,
            udp_reflector: false,
            min_layer: 65,
            max_layer: 92,
            library_versions: Vec::new(),
        }
    }

    /// Phase C2b: render `callProtocol` (`schema/td_api.tl:7008`).
    pub fn to_json(&self) -> Value {
        json!({
            "@type": "callProtocol",
            "udp_p2p": self.udp_p2p,
            "udp_reflector": self.udp_reflector,
            "min_layer": self.min_layer,
            "max_layer": self.max_layer,
            "library_versions": self.library_versions,
        })
    }
}

/// Phase C2b: an engine device category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaDeviceKind {
    Microphone,
    Speaker,
    Camera,
    Screen,
}

/// Phase C2b: one engine-enumerated media device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaDevice {
    /// Phase C2b: engine metadata descriptor used to select the device.
    pub id: String,
    pub name: String,
    pub kind: MediaDeviceKind,
}

/// Phase C2b: safe call-engine failure surface.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EngineError {
    #[error("call engine is unavailable")]
    Unavailable,
    #[error("call engine does not track TDLib call {0}")]
    NoSuchCall(i32),
    #[error("call engine operation {op} failed with code {code}")]
    Engine { op: &'static str, code: i32 },
    #[error("call engine returned a null instance")]
    NullInstance,
    #[error("there is no active call")]
    NoActiveCall,
    /// Phase C2i: the engine enumerated no screen-capture source, so
    /// screen-share send cannot start.
    #[error("no screen-capture source available")]
    NoScreenSource,
}

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

#[derive(Default)]
struct MockInner {
    available: bool,
    calls: HashMap<i32, (i64, bool)>,
    started_calls: Vec<(i32, i64, bool)>,
    accepted_calls: Vec<i32>,
    signaling_received: Vec<(i32, Vec<u8>)>,
    hung_up_calls: Vec<i32>,
    mute_changes: Vec<(i32, bool)>,
    /// Test-only failure injection for `set_muted`.
    fail_mute: bool,
    /// Test-only failure injection for the next `connect`.
    fail_connect: bool,
    devices: Vec<MediaDevice>,
    hook: Option<SignalingEmittedCallback>,
    transport_hook: Option<TransportStateCallback>,
    /// Phase C2e: video frame and peer camera-state hooks (replace semantics).
    video_hook: Option<VideoFrameCallback>,
    remote_video_hook: Option<RemoteVideoStateCallback>,
    /// Phase C2j: peer 1:1 screen-share state hook (replace semantics).
    remote_screen_hook: Option<RemoteVideoStateCallback>,
    camera_changes: Vec<(i32, bool, Option<String>)>,
    /// Phase C2i: 1:1 screen-share toggle recording for driver tests.
    p2p_screen_share_changes: Vec<(i32, bool)>,
    frame_seq: u64,
    connects: Vec<(i32, ConnectParams)>,
    device_selections: Vec<(i32, Option<String>, Option<String>)>,
    /// Phase C2g: group transport recording for driver tests.
    group_offers: Vec<(i32, i64)>,
    group_connects: Vec<(i32, String, bool)>,
    group_video_syncs: Vec<(i32, Vec<GroupVideoSource>)>,
    group_camera_changes: Vec<(i32, bool, Option<String>)>,
    screen_share_offers: Vec<i32>,
    screen_share_connects: Vec<(i32, String)>,
    screen_share_stops: Vec<i32>,
    /// Phase C2g: group call ids with a live native presentation
    /// (initialized via `start_screen_share`, cleared by `stop_screen_share`).
    presentations: Vec<i32>,
    group_leaves: Vec<i32>,
}

/// Phase C2b: deterministic in-memory engine used by driver tests.
#[derive(Clone)]
pub struct MockEngine {
    inner: Arc<Mutex<MockInner>>,
}

impl Default for MockEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl MockEngine {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(MockInner {
                available: true,
                ..MockInner::default()
            })),
        }
    }

    pub fn unavailable() -> Self {
        Self {
            inner: Arc::new(Mutex::new(MockInner::default())),
        }
    }

    pub fn started_calls(&self) -> Vec<(i32, i64, bool)> {
        self.inner
            .lock()
            .expect("mock call engine")
            .started_calls
            .clone()
    }

    pub fn accepted_calls(&self) -> Vec<i32> {
        self.inner
            .lock()
            .expect("mock call engine")
            .accepted_calls
            .clone()
    }

    pub fn signaling_received(&self) -> Vec<(i32, Vec<u8>)> {
        self.inner
            .lock()
            .expect("mock call engine")
            .signaling_received
            .clone()
    }

    pub fn hung_up_calls(&self) -> Vec<i32> {
        self.inner
            .lock()
            .expect("mock call engine")
            .hung_up_calls
            .clone()
    }

    pub fn mute_changes(&self) -> Vec<(i32, bool)> {
        self.inner
            .lock()
            .expect("mock call engine")
            .mute_changes
            .clone()
    }

    pub fn set_devices(&self, devices: Vec<MediaDevice>) {
        self.inner.lock().expect("mock call engine").devices = devices;
    }

    /// Test-only: make the next `set_muted` calls fail.
    pub fn fail_mute(&self) {
        self.inner.lock().expect("mock call engine").fail_mute = true;
    }

    pub fn fail_next_connect(&self) {
        self.inner.lock().expect("mock call engine").fail_connect = true;
    }

    pub fn connects(&self) -> Vec<(i32, ConnectParams)> {
        self.inner
            .lock()
            .expect("mock call engine")
            .connects
            .clone()
    }

    pub fn device_selections(&self) -> Vec<(i32, Option<String>, Option<String>)> {
        self.inner
            .lock()
            .expect("mock call engine")
            .device_selections
            .clone()
    }

    pub fn camera_changes(&self) -> Vec<(i32, bool, Option<String>)> {
        self.inner
            .lock()
            .expect("mock call engine")
            .camera_changes
            .clone()
    }

    /// Phase C2g: recorded group transport calls for driver tests.
    pub fn group_offers(&self) -> Vec<(i32, i64)> {
        self.inner
            .lock()
            .expect("mock call engine")
            .group_offers
            .clone()
    }

    pub fn group_connects(&self) -> Vec<(i32, String, bool)> {
        self.inner
            .lock()
            .expect("mock call engine")
            .group_connects
            .clone()
    }

    pub fn group_video_syncs(&self) -> Vec<(i32, Vec<GroupVideoSource>)> {
        self.inner
            .lock()
            .expect("mock call engine")
            .group_video_syncs
            .clone()
    }

    pub fn group_camera_changes(&self) -> Vec<(i32, bool, Option<String>)> {
        self.inner
            .lock()
            .expect("mock call engine")
            .group_camera_changes
            .clone()
    }

    pub fn screen_share_offers(&self) -> Vec<i32> {
        self.inner
            .lock()
            .expect("mock call engine")
            .screen_share_offers
            .clone()
    }

    pub fn screen_share_connects(&self) -> Vec<(i32, String)> {
        self.inner
            .lock()
            .expect("mock call engine")
            .screen_share_connects
            .clone()
    }

    pub fn screen_share_stops(&self) -> Vec<i32> {
        self.inner
            .lock()
            .expect("mock call engine")
            .screen_share_stops
            .clone()
    }

    /// Phase C2i: recorded 1:1 screen-share toggles.
    pub fn p2p_screen_share_changes(&self) -> Vec<(i32, bool)> {
        self.inner
            .lock()
            .expect("mock call engine")
            .p2p_screen_share_changes
            .clone()
    }

    pub fn group_leaves(&self) -> Vec<i32> {
        self.inner
            .lock()
            .expect("mock call engine")
            .group_leaves
            .clone()
    }

    pub fn emit_transport_state(&self, call_id: i32, state: TransportState) {
        let hook = self
            .inner
            .lock()
            .expect("mock call engine")
            .transport_hook
            .clone();
        if let Some(hook) = hook {
            hook(call_id, state);
        }
    }

    /// Phase C2e: deliver a synthetic video frame to the registered hook,
    /// assigning the next sequence number from the internal counter.
    pub fn emit_video_frame(&self, call_id: i32, mut frame: VideoFrame) {
        let mut inner = self.inner.lock().expect("mock call engine");
        frame.seq = inner.frame_seq;
        inner.frame_seq += 1;
        let hook = inner.video_hook.clone();
        drop(inner);
        if let Some(hook) = hook {
            hook(call_id, frame);
        }
    }

    /// Phase C2e: deliver a synthetic peer camera state to the hook.
    pub fn emit_remote_video_state(&self, call_id: i32, state: RemoteVideoState) {
        let hook = self
            .inner
            .lock()
            .expect("mock call engine")
            .remote_video_hook
            .clone();
        if let Some(hook) = hook {
            hook(call_id, state);
        }
    }

    /// Phase C2j: deliver a synthetic peer screen-share state to the hook.
    pub fn emit_remote_screen_state(&self, call_id: i32, state: RemoteVideoState) {
        let hook = self
            .inner
            .lock()
            .expect("mock call engine")
            .remote_screen_hook
            .clone();
        if let Some(hook) = hook {
            hook(call_id, state);
        }
    }

    fn engine_protocol() -> EngineProtocol {
        EngineProtocol {
            udp_p2p: true,
            udp_reflector: true,
            min_layer: 92,
            max_layer: 92,
            library_versions: vec![
                "8.0.0".into(),
                "9.0.0".into(),
                "12.0.0".into(),
                "13.0.0".into(),
            ],
        }
    }
}

impl CallEngine for MockEngine {
    fn start_call(
        &mut self,
        call_id: i32,
        user_id: i64,
        is_outgoing: bool,
    ) -> Result<(), EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        if inner.calls.contains_key(&call_id) {
            return Ok(());
        }
        inner.calls.insert(call_id, (user_id, is_outgoing));
        inner.started_calls.push((call_id, user_id, is_outgoing));
        Ok(())
    }

    fn accept_call(&mut self, call_id: i32) -> Result<(), EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        if !inner.calls.contains_key(&call_id) {
            return Err(EngineError::NoSuchCall(call_id));
        }
        inner.accepted_calls.push(call_id);
        Ok(())
    }

    fn send_signaling_data(&mut self, call_id: i32, data: &[u8]) -> Result<(), EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        if !inner.calls.contains_key(&call_id) {
            return Err(EngineError::NoSuchCall(call_id));
        }
        inner.signaling_received.push((call_id, data.to_vec()));
        Ok(())
    }

    fn receive_signaling_data(&self, call_id: i32, data: &[u8]) {
        let hook = self.inner.lock().expect("mock call engine").hook.clone();
        if let Some(hook) = hook {
            hook(call_id, data.to_vec());
        }
    }

    fn set_signaling_emitted_callback(&mut self, callback: SignalingEmittedCallback) {
        self.inner.lock().expect("mock call engine").hook = Some(callback);
    }

    fn set_transport_state_callback(&mut self, callback: TransportStateCallback) {
        self.inner.lock().expect("mock call engine").transport_hook = Some(callback);
    }

    fn set_video_frame_callback(&mut self, callback: VideoFrameCallback) {
        self.inner.lock().expect("mock call engine").video_hook = Some(callback);
    }

    fn set_remote_video_state_callback(&mut self, callback: RemoteVideoStateCallback) {
        self.inner
            .lock()
            .expect("mock call engine")
            .remote_video_hook = Some(callback);
    }

    fn set_remote_screen_state_callback(&mut self, callback: RemoteVideoStateCallback) {
        self.inner
            .lock()
            .expect("mock call engine")
            .remote_screen_hook = Some(callback);
    }

    fn connect(&mut self, call_id: i32, params: &ConnectParams) -> Result<(), EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        if !inner.calls.contains_key(&call_id) {
            return Err(EngineError::NoSuchCall(call_id));
        }
        inner.connects.push((call_id, params.clone()));
        if inner.fail_connect {
            inner.fail_connect = false;
            return Err(EngineError::Engine {
                op: "connect",
                code: -1,
            });
        }
        Ok(())
    }

    fn select_devices(
        &mut self,
        call_id: i32,
        microphone: Option<&str>,
        speaker: Option<&str>,
    ) -> Result<(), EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        if !inner.calls.contains_key(&call_id) {
            return Err(EngineError::NoSuchCall(call_id));
        }
        inner.device_selections.push((
            call_id,
            microphone.map(str::to_owned),
            speaker.map(str::to_owned),
        ));
        Ok(())
    }

    fn hangup(&mut self, call_id: i32) -> Result<(), EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        if inner.calls.remove(&call_id).is_some() {
            inner.hung_up_calls.push(call_id);
        }
        Ok(())
    }

    fn set_muted(&mut self, call_id: i32, muted: bool) -> Result<(), EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        if !inner.calls.contains_key(&call_id) {
            return Err(EngineError::NoSuchCall(call_id));
        }
        if inner.fail_mute {
            return Err(EngineError::Engine {
                op: "set_muted",
                code: -1,
            });
        }
        inner.mute_changes.push((call_id, muted));
        Ok(())
    }

    fn set_camera_enabled(
        &mut self,
        call_id: i32,
        enabled: bool,
        camera: Option<&str>,
    ) -> Result<(), EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        if !inner.calls.contains_key(&call_id) {
            return Err(EngineError::NoSuchCall(call_id));
        }
        inner
            .camera_changes
            .push((call_id, enabled, camera.map(str::to_owned)));
        Ok(())
    }

    fn set_screen_share_enabled(&mut self, call_id: i32, enabled: bool) -> Result<(), EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        if !inner.calls.contains_key(&call_id) {
            return Err(EngineError::NoSuchCall(call_id));
        }
        inner.p2p_screen_share_changes.push((call_id, enabled));
        Ok(())
    }

    fn create_group_call(
        &mut self,
        group_call_id: i32,
        chat_id: i64,
    ) -> Result<String, EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        inner.group_offers.push((group_call_id, chat_id));
        Ok(format!("mock-group-offer-{group_call_id}"))
    }

    fn connect_group_call(
        &mut self,
        group_call_id: i32,
        answer_payload: &str,
        video_enabled: bool,
    ) -> Result<(), EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        inner
            .group_connects
            .push((group_call_id, answer_payload.to_owned(), video_enabled));
        Ok(())
    }

    fn sync_group_video(
        &mut self,
        group_call_id: i32,
        sources: &[GroupVideoSource],
    ) -> Result<(), EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        inner
            .group_video_syncs
            .push((group_call_id, sources.to_vec()));
        Ok(())
    }

    fn set_group_camera(
        &mut self,
        group_call_id: i32,
        enabled: bool,
        camera: Option<&str>,
    ) -> Result<(), EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        inner
            .group_camera_changes
            .push((group_call_id, enabled, camera.map(str::to_owned)));
        Ok(())
    }

    fn start_screen_share(&mut self, group_call_id: i32) -> Result<String, EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        inner.screen_share_offers.push(group_call_id);
        if !inner.presentations.contains(&group_call_id) {
            inner.presentations.push(group_call_id);
        }
        Ok(format!("mock-presentation-offer-{group_call_id}"))
    }

    fn connect_screen_share(
        &mut self,
        group_call_id: i32,
        answer_payload: &str,
    ) -> Result<(), EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        inner
            .screen_share_connects
            .push((group_call_id, answer_payload.to_owned()));
        Ok(())
    }

    fn stop_screen_share(&mut self, group_call_id: i32) -> Result<(), EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        inner.screen_share_stops.push(group_call_id);
        inner.presentations.retain(|id| *id != group_call_id);
        Ok(())
    }

    fn presentation_active(&self, group_call_id: i32) -> bool {
        self.inner
            .lock()
            .expect("mock call engine")
            .presentations
            .contains(&group_call_id)
    }

    fn leave_group_call(&mut self, group_call_id: i32) -> Result<(), EngineError> {
        let mut inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        inner.group_leaves.push(group_call_id);
        // Mirror the real engine: a live presentation is stopped first
        // (privacy — capture ends before the call).
        if inner.presentations.contains(&group_call_id) {
            inner.screen_share_stops.push(group_call_id);
        }
        inner.presentations.retain(|id| *id != group_call_id);
        Ok(())
    }

    fn media_devices(&self) -> Result<Vec<MediaDevice>, EngineError> {
        let inner = self.inner.lock().expect("mock call engine");
        if !inner.available {
            return Err(EngineError::Unavailable);
        }
        Ok(inner.devices.clone())
    }

    fn protocol(&self) -> EngineProtocol {
        if self.is_available() {
            Self::engine_protocol()
        } else {
            EngineProtocol::signaling_only()
        }
    }

    fn is_available(&self) -> bool {
        self.inner.lock().expect("mock call engine").available
    }
}

struct CallbackShared {
    user_to_call: Mutex<HashMap<i64, i32>>,
    hook: Mutex<Option<SignalingEmittedCallback>>,
    transport_hook: Mutex<Option<TransportStateCallback>>,
    frame_hook: Mutex<Option<VideoFrameCallback>>,
    remote_video_hook: Mutex<Option<RemoteVideoStateCallback>>,
    /// Phase C2j: peer 1:1 screen-share state hook.
    remote_screen_hook: Mutex<Option<RemoteVideoStateCallback>>,
    frame_seq: AtomicU64,
    /// Phase C2g: group-call callback routing: native `chat_id` -> the
    /// TDLib group call id the driver assigned, and the ssrc map that
    /// attributes incoming frames to participants.
    group_chat_to_call: Mutex<HashMap<i64, i32>>,
    group_video_ssrc_to_user: Mutex<HashMap<(i64, u32), i64>>,
}

/// Phase C2e: retained per-call media configuration; the engine re-issues
/// stream sources from this on camera toggles and device changes.
#[derive(Clone)]
struct CallMediaConfig {
    mic: Option<String>,
    speaker: Option<String>,
    camera_enabled: bool,
    camera: Option<String>,
    /// Phase C2i: screen-share send intent for the 1:1 call. When on,
    /// `set_media_sources` issues the desktop capture instead of the
    /// camera — ntgcalls rejects mixing both in Capture mode
    /// (`stream_manager.cpp:69`, v3.0.0).
    screen_share_on: bool,
}

/// Phase C2i: retained media config for `connect`. A transport
/// reconnect re-runs `connect` with the retained params; an
/// in-flight screen-share intent survives so the engine re-issues
/// screen-only instead of flipping back to the camera. A fresh
/// connect has no prior entry, so the flag starts off.
fn retained_call_media(
    params: &ConnectParams,
    previous: Option<&CallMediaConfig>,
) -> CallMediaConfig {
    CallMediaConfig {
        mic: params.mic_input.clone(),
        speaker: params.speaker_input.clone(),
        camera_enabled: params.video_enabled,
        camera: params.camera_input.clone(),
        screen_share_on: previous.is_some_and(|config| config.screen_share_on),
    }
}

/// Phase C2g: retained per-group-call media state for the ntgcalls
/// group transport (keyed by TDLib group call id).
#[derive(Clone, Default)]
struct GroupCallMedia {
    chat_id: i64,
    camera_enabled: bool,
    camera: Option<String>,
    /// endpoint -> full source, mirroring the engine's subscriptions.
    /// Storing the source (not just the user id) lets resubscription
    /// drop only one endpoint's ssrcs and detect changed ssrc groups.
    endpoints: HashMap<String, GroupVideoSource>,
    /// (chat_id, ssrc) -> user id; attributed to incoming frames.
    ssrc_to_user: HashMap<(i64, u32), i64>,
    connected: bool,
    screen_sharing: bool,
    /// Phase C2g: `ntg_init_presentation` ran but the answer handshake
    /// has not connected yet. Tracked so the driver can reconcile and
    /// stop a stray presentation after a failed handshake.
    presentation_initialized: bool,
}

/// Phase C2b: safe driver-thread adapter over the runtime-loaded C ABI.
///
/// `Rc` in the marker intentionally keeps this type `!Send`: the native
/// instance is created, used, and destroyed on the connect-driver thread.
pub struct NtgcallsEngine {
    api: Loader,
    instance: Option<NonNull<ntg_instance>>,
    protocol: EngineProtocol,
    call_to_user: HashMap<i32, i64>,
    call_media: HashMap<i32, CallMediaConfig>,
    /// Phase C2g: native group transport per TDLib group call id.
    group_calls: HashMap<i32, GroupCallMedia>,
    callback: Arc<CallbackShared>,
    _driver_thread_only: PhantomData<Rc<()>>,
}

impl NtgcallsEngine {
    /// Phase C2b: load the sidecar and cache its reported call protocol.
    pub fn load() -> Result<Self, EngineError> {
        let api = Loader::load_default().map_err(|_| EngineError::Unavailable)?;
        let mut raw: ntg_protocol = unsafe { std::mem::zeroed() };
        // SAFETY: `raw` is a valid writable C output and the loaded function
        // pointer remains live through `api`.
        let rc = unsafe { (api.ntg_get_protocol)(&mut raw) };
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_get_protocol",
                code: rc,
            });
        }
        let library_versions =
            c_string_pointer_array(raw.library_versions, raw.library_versions_len);
        let protocol = EngineProtocol {
            udp_p2p: raw.udp_p2p,
            udp_reflector: raw.udp_reflector,
            min_layer: raw.min_layer,
            max_layer: raw.max_layer,
            library_versions,
        };
        // SAFETY: ntgcalls allocated the successful protocol output and its
        // matching free function accepts the same stack wrapper by pointer.
        unsafe { (api.ntg_protocol_free)(&mut raw) };

        Ok(Self {
            api,
            instance: None,
            protocol,
            call_to_user: HashMap::new(),
            call_media: HashMap::new(),
            group_calls: HashMap::new(),
            callback: Arc::new(CallbackShared {
                user_to_call: Mutex::new(HashMap::new()),
                hook: Mutex::new(None),
                transport_hook: Mutex::new(None),
                frame_hook: Mutex::new(None),
                remote_video_hook: Mutex::new(None),
                remote_screen_hook: Mutex::new(None),
                frame_seq: AtomicU64::new(0),
                group_chat_to_call: Mutex::new(HashMap::new()),
                group_video_ssrc_to_user: Mutex::new(HashMap::new()),
            }),
            _driver_thread_only: PhantomData,
        })
    }

    fn ensure_instance(&mut self) -> Result<NonNull<ntg_instance>, EngineError> {
        if let Some(instance) = self.instance {
            return Ok(instance);
        }
        // SAFETY: the loaded constructor takes no arguments and ownership is
        // retained here until `Drop` calls the matching destroy function.
        let instance = unsafe { (self.api.ntg_instance_create)() };
        let instance = NonNull::new(instance).ok_or(EngineError::NullInstance)?;
        let user_data = Arc::as_ptr(&self.callback).cast_mut().cast::<c_void>();
        // SAFETY: `user_data` points to `self.callback`, which outlives the
        // registered callback. Drop unregisters before releasing the Arc.
        let rc = unsafe {
            (self.api.ntg_on_signaling_data_callback)(
                instance.as_ptr(),
                Some(signaling_trampoline),
                user_data,
            )
        };
        if rc != NTG_OK {
            // SAFETY: registration failed, so no callback can reference the
            // engine; destroy the freshly-created instance immediately.
            unsafe { (self.api.ntg_instance_destroy)(instance.as_ptr()) };
            return Err(EngineError::Engine {
                op: "ntg_on_signaling_data_callback",
                code: rc,
            });
        }
        let rc = unsafe {
            (self.api.ntg_on_connection_change_callback)(
                instance.as_ptr(),
                Some(connection_trampoline),
                user_data,
            )
        };
        if rc != NTG_OK {
            unsafe {
                let _ =
                    (self.api.ntg_on_signaling_data_callback)(instance.as_ptr(), None, null_mut());
                (self.api.ntg_instance_destroy)(instance.as_ptr());
            }
            return Err(EngineError::Engine {
                op: "ntg_on_connection_change_callback",
                code: rc,
            });
        }
        let rc = unsafe {
            (self.api.ntg_on_frames_callback)(instance.as_ptr(), Some(frames_trampoline), user_data)
        };
        if rc != NTG_OK {
            unsafe {
                let _ =
                    (self.api.ntg_on_signaling_data_callback)(instance.as_ptr(), None, null_mut());
                let _ = (self.api.ntg_on_connection_change_callback)(
                    instance.as_ptr(),
                    None,
                    null_mut(),
                );
                (self.api.ntg_instance_destroy)(instance.as_ptr());
            }
            return Err(EngineError::Engine {
                op: "ntg_on_frames_callback",
                code: rc,
            });
        }
        let rc = unsafe {
            (self.api.ntg_on_remote_source_change_callback)(
                instance.as_ptr(),
                Some(remote_source_trampoline),
                user_data,
            )
        };
        if rc != NTG_OK {
            unsafe {
                let _ =
                    (self.api.ntg_on_signaling_data_callback)(instance.as_ptr(), None, null_mut());
                let _ = (self.api.ntg_on_connection_change_callback)(
                    instance.as_ptr(),
                    None,
                    null_mut(),
                );
                let _ = (self.api.ntg_on_frames_callback)(instance.as_ptr(), None, null_mut());
                (self.api.ntg_instance_destroy)(instance.as_ptr());
            }
            return Err(EngineError::Engine {
                op: "ntg_on_remote_source_change_callback",
                code: rc,
            });
        }
        self.instance = Some(instance);
        Ok(instance)
    }

    fn user_id(&self, call_id: i32) -> Result<i64, EngineError> {
        self.call_to_user
            .get(&call_id)
            .copied()
            .ok_or(EngineError::NoSuchCall(call_id))
    }

    /// Phase C2e: re-issue stream sources from the retained per-call media
    /// config. Capture carries microphone + camera (the camera description
    /// is present only while video is enabled; NULL removes the camera
    /// reader and ntgcalls tells the peer via media_state video_stopped).
    /// Playback carries the speaker only.
    /// Phase C2g: (re)issue the group call's capture sources from the
    /// retained config. Group calls carry no audio transport in this
    /// slice — only the camera, and only while it is enabled.
    fn issue_group_sources(&mut self, group_call_id: i32) -> Result<(), EngineError> {
        let media = self
            .group_calls
            .get(&group_call_id)
            .cloned()
            .ok_or(EngineError::NoSuchCall(group_call_id))?;
        let instance = self.ensure_instance()?;
        let camera = native_input(media.camera.as_deref())?;
        let mut camera_video = ntg_video_description {
            media_source: NTG_MEDIA_SOURCE_DEVICE,
            // Telegram's group video default.
            width: 640,
            height: 480,
            fps: 30,
            input: camera
                .as_ref()
                .map_or(null_mut(), |value| value.as_ptr().cast_mut()),
            keep_open: false,
        };
        let capture = ntg_media_description {
            microphone: null_mut(),
            speaker: null_mut(),
            camera: if media.camera_enabled {
                &mut camera_video
            } else {
                null_mut()
            },
            screen: null_mut(),
        };
        // SAFETY: `instance` is live and the descriptions are stack-local
        // for the duration of this synchronous C call.
        let rc = unsafe {
            (self.api.ntg_set_stream_sources)(
                instance.as_ptr(),
                media.chat_id,
                NTG_STREAM_MODE_CAPTURE,
                capture,
            )
        };
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_set_stream_sources",
                code: rc,
            });
        }
        Ok(())
    }

    /// Phase C2i: desktop-capture video description for screen-share
    /// send (1:1 and group presentation alike). NULL input selects the
    /// default display; desktop capture itself is ntgcalls' job
    /// (libwebrtc capturer) — Quill carries no Linux capture code.
    fn screen_video_description() -> ntg_video_description {
        ntg_video_description {
            media_source: NTG_MEDIA_SOURCE_DESKTOP,
            width: 1920,
            height: 1080,
            fps: 15,
            // NULL input selects the default display.
            input: null_mut(),
            keep_open: false,
        }
    }

    /// Phase C2g: attach desktop capture to the presentation transport
    /// after `ntg_connect(..., is_presentation=true)`. The presentation
    /// carries only the screen track; the main group transport is
    /// untouched.
    fn issue_presentation_sources(&mut self, group_call_id: i32) -> Result<(), EngineError> {
        let media = self
            .group_calls
            .get(&group_call_id)
            .cloned()
            .ok_or(EngineError::NoSuchCall(group_call_id))?;
        let instance = self.ensure_instance()?;
        let mut screen_video = Self::screen_video_description();
        let capture = ntg_media_description {
            microphone: null_mut(),
            speaker: null_mut(),
            camera: null_mut(),
            screen: &mut screen_video,
        };
        // SAFETY: `instance` is live and the descriptions are stack-local
        // for the duration of this synchronous C call.
        let rc = unsafe {
            (self.api.ntg_set_stream_sources)(
                instance.as_ptr(),
                media.chat_id,
                NTG_STREAM_MODE_CAPTURE,
                capture,
            )
        };
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_set_stream_sources",
                code: rc,
            });
        }
        Ok(())
    }

    fn set_media_sources(&self, call_id: i32) -> Result<(), EngineError> {
        let config = self
            .call_media
            .get(&call_id)
            .cloned()
            .ok_or(EngineError::NoSuchCall(call_id))?;
        let user_id = self.user_id(call_id)?;
        let instance = self.instance.ok_or(EngineError::NullInstance)?;
        let mic = native_input(config.mic.as_deref())?;
        let speaker = native_input(config.speaker.as_deref())?;
        let camera = native_input(config.camera.as_deref())?;

        let mut mic_audio = audio_description(mic.as_ref());
        let mut camera_video = ntg_video_description {
            media_source: NTG_MEDIA_SOURCE_DEVICE,
            // Conventional tgvoip P2P default: 640x480@30.
            width: 640,
            height: 480,
            fps: 30,
            // NULL input selects the default device.
            input: camera
                .as_ref()
                .map_or(null_mut(), |value| value.as_ptr().cast_mut()),
            keep_open: false,
        };
        let mut screen_video = Self::screen_video_description();
        let capture = ntg_media_description {
            microphone: &mut mic_audio,
            speaker: null_mut(),
            // Phase C2i: screen share replaces the camera — ntgcalls
            // rejects camera+screen in Capture mode
            // (`stream_manager.cpp:69`, v3.0.0).
            camera: if config.camera_enabled && !config.screen_share_on {
                &mut camera_video
            } else {
                null_mut()
            },
            screen: if config.screen_share_on {
                &mut screen_video
            } else {
                null_mut()
            },
        };
        // SAFETY: `instance` is live and the descriptions are stack-local for
        // the duration of this synchronous C call.
        let rc = unsafe {
            (self.api.ntg_set_stream_sources)(
                instance.as_ptr(),
                user_id,
                NTG_STREAM_MODE_CAPTURE,
                capture,
            )
        };
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_set_stream_sources",
                code: rc,
            });
        }

        let mut speaker_audio = audio_description(speaker.as_ref());
        let playback = ntg_media_description {
            microphone: null_mut(),
            speaker: &mut speaker_audio,
            camera: null_mut(),
            screen: null_mut(),
        };
        let rc = unsafe {
            (self.api.ntg_set_stream_sources)(
                instance.as_ptr(),
                user_id,
                NTG_STREAM_MODE_PLAYBACK,
                playback,
            )
        };
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_set_stream_sources",
                code: rc,
            });
        }
        Ok(())
    }
}

/// Phase C2e: device-metadata C string for stream sources; `None` selects
/// the default device.
fn native_input(input: Option<&str>) -> Result<Option<CString>, EngineError> {
    input
        .map(CString::new)
        .transpose()
        .map_err(|_| EngineError::Engine {
            op: "ntg_set_stream_sources",
            code: NTG_ERR_INVALID_PARAMS,
        })
}

fn audio_description(input: Option<&CString>) -> ntg_audio_description {
    ntg_audio_description {
        media_source: NTG_MEDIA_SOURCE_DEVICE,
        sample_rate: 48_000,
        channel_count: 1,
        // ntgcalls' wrapper convention uses device metadata here; NULL
        // selects the default (the C header is silent).
        input: input.map_or(null_mut(), |value| value.as_ptr().cast_mut()),
        keep_open: false,
    }
}

impl CallEngine for NtgcallsEngine {
    fn start_call(
        &mut self,
        call_id: i32,
        user_id: i64,
        _is_outgoing: bool,
    ) -> Result<(), EngineError> {
        if self.call_to_user.contains_key(&call_id) {
            return Ok(());
        }
        let instance = self.ensure_instance()?;
        // SAFETY: `instance` is live and ntgcalls keys P2P calls by user id.
        let rc = unsafe { (self.api.ntg_create_p2p_call)(instance.as_ptr(), user_id) };
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_create_p2p_call",
                code: rc,
            });
        }
        self.call_to_user.insert(call_id, user_id);
        self.callback
            .user_to_call
            .lock()
            .expect("ntgcalls callback map")
            .insert(user_id, call_id);
        Ok(())
    }

    fn accept_call(&mut self, call_id: i32) -> Result<(), EngineError> {
        self.user_id(call_id).map(|_| ())
    }

    fn send_signaling_data(&mut self, call_id: i32, data: &[u8]) -> Result<(), EngineError> {
        let user_id = self.user_id(call_id)?;
        if data.is_empty() {
            return Ok(());
        }
        let instance = self.ensure_instance()?;
        // SAFETY: `instance` is live and `data` remains valid for the duration
        // of this synchronous C call.
        let rc = unsafe {
            (self.api.ntg_send_signaling_data)(
                instance.as_ptr(),
                user_id,
                data.as_ptr(),
                data.len(),
            )
        };
        if rc == NTG_OK {
            Ok(())
        } else {
            Err(EngineError::Engine {
                op: "ntg_send_signaling_data",
                code: rc,
            })
        }
    }

    fn receive_signaling_data(&self, call_id: i32, data: &[u8]) {
        let hook = self
            .callback
            .hook
            .lock()
            .expect("ntgcalls callback hook")
            .clone();
        if let Some(hook) = hook {
            hook(call_id, data.to_vec());
        }
    }

    fn set_signaling_emitted_callback(&mut self, callback: SignalingEmittedCallback) {
        *self.callback.hook.lock().expect("ntgcalls callback hook") = Some(callback);
    }

    fn set_transport_state_callback(&mut self, callback: TransportStateCallback) {
        *self
            .callback
            .transport_hook
            .lock()
            .expect("ntgcalls transport callback hook") = Some(callback);
    }

    fn set_video_frame_callback(&mut self, callback: VideoFrameCallback) {
        *self
            .callback
            .frame_hook
            .lock()
            .expect("ntgcalls video frame hook") = Some(callback);
    }

    fn set_remote_video_state_callback(&mut self, callback: RemoteVideoStateCallback) {
        *self
            .callback
            .remote_video_hook
            .lock()
            .expect("ntgcalls remote video state hook") = Some(callback);
    }

    fn set_remote_screen_state_callback(&mut self, callback: RemoteVideoStateCallback) {
        *self
            .callback
            .remote_screen_hook
            .lock()
            .expect("ntgcalls remote screen state hook") = Some(callback);
    }

    fn connect(&mut self, call_id: i32, params: &ConnectParams) -> Result<(), EngineError> {
        let user_id = self.user_id(call_id)?;
        let instance = self.ensure_instance()?;
        if params.encryption_key.is_empty() {
            return Err(EngineError::Engine {
                op: "ntg_skip_exchange",
                code: NTG_ERR_INVALID_PARAMS,
            });
        }
        let servers = NativeRtcServers::new(&params.servers)?;
        let versions = c_strings(&params.library_versions)?;
        let version_ptrs: Vec<_> = versions.iter().map(|value| value.as_ptr()).collect();
        let rc = unsafe {
            (self.api.ntg_skip_exchange)(
                instance.as_ptr(),
                user_id,
                params.encryption_key.as_ptr(),
                params.encryption_key.len(),
                params.is_outgoing,
            )
        };
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_skip_exchange",
                code: rc,
            });
        }
        let rc = unsafe {
            (self.api.ntg_connect_p2p)(
                instance.as_ptr(),
                user_id,
                servers.raw.as_ptr(),
                servers.raw.len(),
                version_ptrs.as_ptr(),
                version_ptrs.len(),
                params.p2p_allowed,
                std::ptr::null(),
            )
        };
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_connect_p2p",
                code: rc,
            });
        }
        // Phase C2e: retain the media config before issuing sources so
        // later toggles and device changes re-issue from the same state.
        // On failure, drop the retained config so no stale config lingers
        // until hangup.
        // Phase C2i: `connect` re-runs on transport reconnect with the
        // retained params — the previous entry's screen-share intent is
        // preserved (a fresh connect has no prior entry, so the flag
        // starts off).
        let previous = self.call_media.get(&call_id);
        let config = retained_call_media(params, previous);
        self.call_media.insert(call_id, config);
        let result = self.set_media_sources(call_id);
        if result.is_err() {
            self.call_media.remove(&call_id);
        }
        result
    }

    fn select_devices(
        &mut self,
        call_id: i32,
        microphone: Option<&str>,
        speaker: Option<&str>,
    ) -> Result<(), EngineError> {
        let config = self
            .call_media
            .get_mut(&call_id)
            .ok_or(EngineError::NoSuchCall(call_id))?;
        config.mic = microphone.map(str::to_owned);
        config.speaker = speaker.map(str::to_owned);
        self.set_media_sources(call_id)
    }

    fn hangup(&mut self, call_id: i32) -> Result<(), EngineError> {
        let Some(user_id) = self.call_to_user.remove(&call_id) else {
            return Ok(());
        };
        self.call_media.remove(&call_id);
        self.callback
            .user_to_call
            .lock()
            .expect("ntgcalls callback map")
            .remove(&user_id);
        if let Some(instance) = self.instance {
            // SAFETY: best-effort teardown on a live instance. The return
            // value is intentionally ignored so hangup remains idempotent.
            unsafe { (self.api.ntg_stop)(instance.as_ptr(), user_id) };
        }
        Ok(())
    }

    fn set_muted(&mut self, call_id: i32, muted: bool) -> Result<(), EngineError> {
        let user_id = self.user_id(call_id)?;
        let instance = self.ensure_instance()?;
        let mut state = false;
        // SAFETY: the instance and output pointer are valid for the duration
        // of this synchronous C call.
        let rc = unsafe {
            if muted {
                (self.api.ntg_mute)(instance.as_ptr(), user_id, &mut state)
            } else {
                (self.api.ntg_unmute)(instance.as_ptr(), user_id, &mut state)
            }
        };
        if rc == NTG_OK {
            Ok(())
        } else {
            Err(EngineError::Engine {
                op: if muted { "ntg_mute" } else { "ntg_unmute" },
                code: rc,
            })
        }
    }

    fn set_camera_enabled(
        &mut self,
        call_id: i32,
        enabled: bool,
        camera: Option<&str>,
    ) -> Result<(), EngineError> {
        let config = self
            .call_media
            .get_mut(&call_id)
            .ok_or(EngineError::NoSuchCall(call_id))?;
        let previous_enabled = config.camera_enabled;
        let previous_camera = config.camera.clone();
        let previous_screen_share = config.screen_share_on;
        config.camera_enabled = enabled;
        config.camera = camera.map(str::to_owned);
        // Phase C2i: the camera replaces screen share — ntgcalls
        // rejects camera+screen in Capture mode
        // (`stream_manager.cpp:69`, v3.0.0).
        if enabled {
            config.screen_share_on = false;
        }
        if let Err(err) = self.set_media_sources(call_id) {
            // Phase C2e: restore the retained config so it can't desync
            // from ntgcalls when issuance fails.
            let config = self
                .call_media
                .get_mut(&call_id)
                .ok_or(EngineError::NoSuchCall(call_id))?;
            config.camera_enabled = previous_enabled;
            config.camera = previous_camera;
            config.screen_share_on = previous_screen_share;
            return Err(err);
        }
        Ok(())
    }

    fn set_screen_share_enabled(&mut self, call_id: i32, enabled: bool) -> Result<(), EngineError> {
        let config = self
            .call_media
            .get_mut(&call_id)
            .ok_or(EngineError::NoSuchCall(call_id))?;
        let previous_enabled = config.screen_share_on;
        let previous_camera_enabled = config.camera_enabled;
        config.screen_share_on = enabled;
        if enabled {
            config.camera_enabled = false;
        }
        if let Err(err) = self.set_media_sources(call_id) {
            // Restore the retained config so it can't desync from
            // ntgcalls when issuance fails (mirrors
            // `set_camera_enabled`).
            let config = self
                .call_media
                .get_mut(&call_id)
                .ok_or(EngineError::NoSuchCall(call_id))?;
            config.screen_share_on = previous_enabled;
            config.camera_enabled = previous_camera_enabled;
            return Err(err);
        }
        Ok(())
    }

    fn create_group_call(
        &mut self,
        group_call_id: i32,
        chat_id: i64,
    ) -> Result<String, EngineError> {
        let instance = self.ensure_instance()?;
        let mut out: *mut c_char = null_mut();
        // SAFETY: `out` is a valid writable C output; ntgcalls allocates the
        // returned join-payload string on success.
        let rc = unsafe { (self.api.ntg_create_call)(instance.as_ptr(), chat_id, &mut out) };
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_create_call",
                code: rc,
            });
        }
        if out.is_null() {
            return Err(EngineError::Engine {
                op: "ntg_create_call",
                code: NTG_ERR_INVALID_PARAMS,
            });
        }
        // SAFETY: `out` points at a NUL-terminated string allocated by the
        // call above; copy it then free with the matching free function.
        let offer = unsafe { CStr::from_ptr(out) }
            .to_string_lossy()
            .into_owned();
        unsafe { (self.api.ntg_string_free)(out) };
        self.group_calls.insert(
            group_call_id,
            GroupCallMedia {
                chat_id,
                ..GroupCallMedia::default()
            },
        );
        self.callback
            .group_chat_to_call
            .lock()
            .expect("ntgcalls group call map")
            .insert(chat_id, group_call_id);
        Ok(offer)
    }

    fn connect_group_call(
        &mut self,
        group_call_id: i32,
        answer_payload: &str,
        video_enabled: bool,
    ) -> Result<(), EngineError> {
        let (chat_id, already) = {
            let media = self
                .group_calls
                .get(&group_call_id)
                .ok_or(EngineError::NoSuchCall(group_call_id))?;
            (media.chat_id, media.connected)
        };
        if already {
            return Ok(());
        }
        let instance = self.ensure_instance()?;
        let params = CString::new(answer_payload).map_err(|_| EngineError::Engine {
            op: "ntg_connect",
            code: NTG_ERR_INVALID_PARAMS,
        })?;
        // SAFETY: instance is live and `params` is a valid C string; the
        // TDLib answer is the `Text` from `joinVideoChat`.
        let rc =
            unsafe { (self.api.ntg_connect)(instance.as_ptr(), chat_id, params.as_ptr(), false) };
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_connect",
                code: rc,
            });
        }
        // Group calls have no audio transport in this slice; the capture
        // sources carry only the camera when it is enabled.
        let media = self
            .group_calls
            .get_mut(&group_call_id)
            .ok_or(EngineError::NoSuchCall(group_call_id))?;
        media.connected = true;
        media.camera_enabled = video_enabled;
        self.issue_group_sources(group_call_id)
    }

    fn sync_group_video(
        &mut self,
        group_call_id: i32,
        sources: &[GroupVideoSource],
    ) -> Result<(), EngineError> {
        let instance = self.ensure_instance()?;
        let chat_id = self
            .group_calls
            .get(&group_call_id)
            .ok_or(EngineError::NoSuchCall(group_call_id))?
            .chat_id;
        // Add new endpoints first so the video never blips on reorder.
        let mut desired: HashMap<&str, &GroupVideoSource> = HashMap::new();
        for source in sources {
            desired.insert(source.endpoint.as_str(), source);
        }
        let stale: Vec<String> = {
            let media = self
                .group_calls
                .get(&group_call_id)
                .ok_or(EngineError::NoSuchCall(group_call_id))?;
            media
                .endpoints
                .keys()
                .filter(|endpoint| !desired.contains_key(endpoint.as_str()))
                .cloned()
                .collect()
        };
        for source in sources {
            let stored = self
                .group_calls
                .get(&group_call_id)
                .and_then(|media| media.endpoints.get(&source.endpoint))
                .cloned();
            match stored {
                // Subscription matches; nothing to do.
                Some(existing) if existing == *source => continue,
                // Ssrc groups changed; drop the stale subscription so the
                // add below re-issues it with the fresh groups.
                Some(_) => {
                    self.remove_group_video_endpoint(group_call_id, chat_id, &source.endpoint)?;
                }
                None => {}
            }
            let endpoint =
                CString::new(source.endpoint.as_str()).map_err(|_| EngineError::Engine {
                    op: "ntg_add_incoming_video",
                    code: NTG_ERR_INVALID_PARAMS,
                })?;
            let groups: Vec<ntg_ssrc_group> = source
                .ssrc_groups
                .iter()
                .map(|group| ntg_ssrc_group {
                    semantics: CString::new(group.semantics.as_str())
                        .map(|s| s.into_raw())
                        .unwrap_or(null_mut()),
                    ssrcs: group.ssrcs.as_ptr().cast_mut(),
                    ssrcs_len: group.ssrcs.len(),
                })
                .collect();
            let mut sink: u32 = 0;
            // SAFETY: instance is live; `endpoint` and the group metadata
            // are valid for this call; `sink` is a valid C output.
            let rc = unsafe {
                (self.api.ntg_add_incoming_video)(
                    instance.as_ptr(),
                    chat_id,
                    source.user_id,
                    endpoint.as_ptr(),
                    groups.as_ptr(),
                    groups.len(),
                    &mut sink,
                )
            };
            // The semantics C strings were `into_raw` above; reclaim them.
            for group in &groups {
                if !group.semantics.is_null() {
                    let _ = unsafe { CString::from_raw(group.semantics) };
                }
            }
            if rc != NTG_OK {
                return Err(EngineError::Engine {
                    op: "ntg_add_incoming_video",
                    code: rc,
                });
            }
            {
                let media = self
                    .group_calls
                    .get_mut(&group_call_id)
                    .ok_or(EngineError::NoSuchCall(group_call_id))?;
                media
                    .endpoints
                    .insert(source.endpoint.clone(), source.clone());
                for group in &source.ssrc_groups {
                    for ssrc in &group.ssrcs {
                        media.ssrc_to_user.insert((chat_id, *ssrc), source.user_id);
                    }
                }
            }
            self.callback
                .group_video_ssrc_to_user
                .lock()
                .expect("ntgcalls group ssrc map")
                .extend(
                    source
                        .ssrc_groups
                        .iter()
                        .flat_map(|group| group.ssrcs.iter())
                        .map(|ssrc| ((chat_id, *ssrc), source.user_id)),
                );
        }
        for endpoint in stale {
            self.remove_group_video_endpoint(group_call_id, chat_id, &endpoint)?;
        }
        // Prune the callback ssrc map of entries for removed endpoints.
        // Keys are (chat_id, ssrc), so entries for other group calls
        // (other chat ids) are left untouched.
        {
            let media = self
                .group_calls
                .get(&group_call_id)
                .ok_or(EngineError::NoSuchCall(group_call_id))?;
            let live = media.ssrc_to_user.clone();
            self.callback
                .group_video_ssrc_to_user
                .lock()
                .expect("ntgcalls group ssrc map")
                .retain(|key, user| key.0 != chat_id || live.get(key) == Some(user));
        }
        Ok(())
    }

    fn set_group_camera(
        &mut self,
        group_call_id: i32,
        enabled: bool,
        camera: Option<&str>,
    ) -> Result<(), EngineError> {
        let media = self
            .group_calls
            .get_mut(&group_call_id)
            .ok_or(EngineError::NoSuchCall(group_call_id))?;
        media.camera_enabled = enabled;
        media.camera = camera.map(str::to_owned);
        self.issue_group_sources(group_call_id)
    }

    fn start_screen_share(&mut self, group_call_id: i32) -> Result<String, EngineError> {
        let instance = self.ensure_instance()?;
        let chat_id = self
            .group_calls
            .get(&group_call_id)
            .ok_or(EngineError::NoSuchCall(group_call_id))?
            .chat_id;
        let mut out: *mut c_char = null_mut();
        // SAFETY: `out` is a valid writable C output; ntgcalls allocates
        // the returned presentation offer on success.
        let rc = unsafe { (self.api.ntg_init_presentation)(instance.as_ptr(), chat_id, &mut out) };
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_init_presentation",
                code: rc,
            });
        }
        if out.is_null() {
            return Err(EngineError::Engine {
                op: "ntg_init_presentation",
                code: NTG_ERR_INVALID_PARAMS,
            });
        }
        // SAFETY: as in `create_group_call`.
        let offer = unsafe { CStr::from_ptr(out) }
            .to_string_lossy()
            .into_owned();
        unsafe { (self.api.ntg_string_free)(out) };
        if let Some(media) = self.group_calls.get_mut(&group_call_id) {
            media.presentation_initialized = true;
        }
        Ok(offer)
    }

    fn connect_screen_share(
        &mut self,
        group_call_id: i32,
        answer_payload: &str,
    ) -> Result<(), EngineError> {
        let media = self
            .group_calls
            .get_mut(&group_call_id)
            .ok_or(EngineError::NoSuchCall(group_call_id))?;
        if media.screen_sharing {
            return Ok(());
        }
        let chat_id = media.chat_id;
        let instance = self.ensure_instance()?;
        let params = CString::new(answer_payload).map_err(|_| EngineError::Engine {
            op: "ntg_connect",
            code: NTG_ERR_INVALID_PARAMS,
        })?;
        // SAFETY: instance is live and `params` is valid; the answer is the
        // `Text` from `startGroupCallScreenSharing`.
        let rc =
            unsafe { (self.api.ntg_connect)(instance.as_ptr(), chat_id, params.as_ptr(), true) };
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_connect",
                code: rc,
            });
        }
        self.group_calls
            .get_mut(&group_call_id)
            .ok_or(EngineError::NoSuchCall(group_call_id))?
            .screen_sharing = true;
        self.issue_presentation_sources(group_call_id)
    }

    fn stop_screen_share(&mut self, group_call_id: i32) -> Result<(), EngineError> {
        let (instance, chat_id) = (
            self.ensure_instance()?,
            self.group_calls
                .get(&group_call_id)
                .ok_or(EngineError::NoSuchCall(group_call_id))?
                .chat_id,
        );
        // SAFETY: instance is live; `chat_id` is this group's chat.
        let rc = unsafe { (self.api.ntg_stop_presentation)(instance.as_ptr(), chat_id) };
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_stop_presentation",
                code: rc,
            });
        }
        if let Some(media) = self.group_calls.get_mut(&group_call_id) {
            media.screen_sharing = false;
            media.presentation_initialized = false;
        }
        Ok(())
    }

    fn presentation_active(&self, group_call_id: i32) -> bool {
        self.group_calls
            .get(&group_call_id)
            .is_some_and(|media| media.screen_sharing || media.presentation_initialized)
    }

    fn leave_group_call(&mut self, group_call_id: i32) -> Result<(), EngineError> {
        // Unknown group call ids succeed: leave is cleanup, never an error.
        let Some(media) = self.group_calls.remove(&group_call_id) else {
            return Ok(());
        };
        if let Some(instance) = self.instance {
            // Privacy: tear a live presentation down FIRST so screen
            // capture stops before the call itself.
            if media.screen_sharing || media.presentation_initialized {
                // SAFETY: instance is live; `media.chat_id` is this group's chat.
                let rc =
                    unsafe { (self.api.ntg_stop_presentation)(instance.as_ptr(), media.chat_id) };
                if rc != NTG_OK {
                    return Err(EngineError::Engine {
                        op: "ntg_stop_presentation",
                        code: rc,
                    });
                }
            }
            // SAFETY: instance is live; `media.chat_id` is this group's chat.
            let rc = unsafe { (self.api.ntg_stop)(instance.as_ptr(), media.chat_id) };
            if rc != NTG_OK {
                return Err(EngineError::Engine {
                    op: "ntg_stop",
                    code: rc,
                });
            }
        }
        let mut maps = self
            .callback
            .group_chat_to_call
            .lock()
            .expect("ntgcalls group call map");
        maps.remove(&media.chat_id);
        self.callback
            .group_video_ssrc_to_user
            .lock()
            .expect("ntgcalls group ssrc map")
            .retain(|(chat_id, _), _| *chat_id != media.chat_id);
        Ok(())
    }

    fn media_devices(&self) -> Result<Vec<MediaDevice>, EngineError> {
        let mut raw: ntg_media_devices = unsafe { std::mem::zeroed() };
        // SAFETY: device enumeration needs no instance and writes to `raw`.
        let rc = unsafe { (self.api.ntg_get_media_devices)(&mut raw) };
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_get_media_devices",
                code: rc,
            });
        }
        let mut devices = Vec::new();
        append_devices(
            &mut devices,
            raw.microphone,
            raw.microphone_len,
            MediaDeviceKind::Microphone,
        );
        append_devices(
            &mut devices,
            raw.speaker,
            raw.speaker_len,
            MediaDeviceKind::Speaker,
        );
        append_devices(
            &mut devices,
            raw.camera,
            raw.camera_len,
            MediaDeviceKind::Camera,
        );
        append_devices(
            &mut devices,
            raw.screen,
            raw.screen_len,
            MediaDeviceKind::Screen,
        );
        // SAFETY: successful enumeration owns the nested allocations until
        // released with the matching ntgcalls free function.
        unsafe { (self.api.ntg_media_devices_free)(&mut raw) };
        Ok(devices)
    }

    fn protocol(&self) -> EngineProtocol {
        self.protocol.clone()
    }

    fn is_available(&self) -> bool {
        true
    }
}

impl NtgcallsEngine {
    /// Phase C2g: remove one group video endpoint subscription. Only
    /// that endpoint's ssrcs leave the attribution maps, so another
    /// endpoint of the same user keeps receiving frames.
    fn remove_group_video_endpoint(
        &mut self,
        group_call_id: i32,
        chat_id: i64,
        endpoint: &str,
    ) -> Result<(), EngineError> {
        let instance = self.ensure_instance()?;
        let native_endpoint = CString::new(endpoint).map_err(|_| EngineError::Engine {
            op: "ntg_remove_incoming_video",
            code: NTG_ERR_INVALID_PARAMS,
        })?;
        let mut removed: bool = false;
        // SAFETY: instance is live; `native_endpoint` is valid for this
        // call; `removed` is a valid C output.
        let rc = unsafe {
            (self.api.ntg_remove_incoming_video)(
                instance.as_ptr(),
                chat_id,
                native_endpoint.as_ptr(),
                &mut removed,
            )
        };
        if rc != NTG_OK {
            return Err(EngineError::Engine {
                op: "ntg_remove_incoming_video",
                code: rc,
            });
        }
        let media = self
            .group_calls
            .get_mut(&group_call_id)
            .ok_or(EngineError::NoSuchCall(group_call_id))?;
        if let Some(stored) = media.endpoints.remove(endpoint) {
            let ssrcs: Vec<u32> = stored
                .ssrc_groups
                .iter()
                .flat_map(|group| group.ssrcs.iter().copied())
                .collect();
            for ssrc in ssrcs {
                media.ssrc_to_user.remove(&(chat_id, ssrc));
            }
        }
        Ok(())
    }
}

impl Drop for NtgcallsEngine {
    fn drop(&mut self) {
        let Some(instance) = self.instance.take() else {
            return;
        };
        // SAFETY: unregister the callback before destroying the instance, so
        // no new native callbacks can fire after teardown begins; the Arc
        // field keeps `CallbackShared` alive until the struct is fully
        // dropped.
        unsafe {
            let _ = (self.api.ntg_on_signaling_data_callback)(instance.as_ptr(), None, null_mut());
            let _ =
                (self.api.ntg_on_connection_change_callback)(instance.as_ptr(), None, null_mut());
            let _ = (self.api.ntg_on_frames_callback)(instance.as_ptr(), None, null_mut());
            let _ = (self.api.ntg_on_remote_source_change_callback)(
                instance.as_ptr(),
                None,
                null_mut(),
            );
        }
        self.call_to_user.clear();
        self.callback
            .user_to_call
            .lock()
            .expect("ntgcalls callback map")
            .clear();
        // SAFETY: callback registration is gone and this is the matching
        // destroy for the constructor used by `ensure_instance`.
        unsafe { (self.api.ntg_instance_destroy)(instance.as_ptr()) };
    }
}

unsafe extern "C" fn signaling_trampoline(
    _instance: *mut ntg_instance,
    user_id: i64,
    data: *const u8,
    len: usize,
    user_data: *mut c_void,
) {
    if user_data.is_null() || (data.is_null() && len != 0) {
        return;
    }
    // SAFETY: registration passes an Arc-owned `CallbackShared` pointer and
    // Drop unregisters the callback before releasing the Arc.
    let shared = unsafe { &*user_data.cast::<CallbackShared>() };
    let call_id = shared
        .user_to_call
        .lock()
        .expect("ntgcalls callback map")
        .get(&user_id)
        .copied();
    let hook = shared.hook.lock().expect("ntgcalls callback hook").clone();
    let (Some(call_id), Some(hook)) = (call_id, hook) else {
        return;
    };
    let bytes = if len == 0 {
        Vec::new()
    } else {
        // SAFETY: ntgcalls guarantees the callback byte span for this call;
        // copy it before returning to the worker thread.
        unsafe { std::slice::from_raw_parts(data, len) }.to_vec()
    };
    hook(call_id, bytes);
}

unsafe extern "C" fn connection_trampoline(
    _instance: *mut ntg_instance,
    user_id: i64,
    info: ntg_connection_info,
    user_data: *mut c_void,
) {
    if user_data.is_null() {
        return;
    }
    let state = match info.state {
        NTG_CONNECTION_STATE_CONNECTING => TransportState::Connecting,
        NTG_CONNECTION_STATE_CONNECTED => TransportState::Connected,
        NTG_CONNECTION_STATE_FAILED => TransportState::Failed,
        NTG_CONNECTION_STATE_TIMEOUT => TransportState::Failed,
        NTG_CONNECTION_STATE_CLOSED => TransportState::Closed,
        _ => return,
    };
    let shared = unsafe { &*user_data.cast::<CallbackShared>() };
    let call_id = shared
        .user_to_call
        .lock()
        .expect("ntgcalls callback map")
        .get(&user_id)
        .copied();
    let hook = shared
        .transport_hook
        .lock()
        .expect("ntgcalls transport callback hook")
        .clone();
    if let (Some(call_id), Some(hook)) = (call_id, hook) {
        hook(call_id, state);
    }
}

/// Phase C2j: P2P frame kinds that reach the app: the local camera
/// preview (`CAPTURE+CAMERA`), the peer's camera (`PLAYBACK+CAMERA`),
/// and the peer's screen share (`PLAYBACK+SCREEN`). Returns
/// `Some(is_screen)` for accepted kinds; audio and anything else is
/// dropped before it can be misattributed to a tile.
fn p2p_frame_kind(mode: ntg_stream_mode, device: ntg_stream_device) -> Option<bool> {
    if mode == NTG_STREAM_MODE_CAPTURE && device == NTG_STREAM_DEVICE_CAMERA {
        return Some(false);
    }
    if mode == NTG_STREAM_MODE_PLAYBACK && device == NTG_STREAM_DEVICE_CAMERA {
        return Some(false);
    }
    if mode == NTG_STREAM_MODE_PLAYBACK && device == NTG_STREAM_DEVICE_SCREEN {
        return Some(true);
    }
    None
}

/// Phase C2e: ntgcalls emits decoded video frames here. Incoming peer camera
/// frames arrive as PLAYBACK, local camera preview frames as CAPTURE; the
/// newest frame of a batch is delivered.
/// Phase C2g: group-call frames arrive keyed by chat id (the native
/// `user_id` parameter doubles as chat id for groups); the participant
/// is resolved per frame from the ssrc subscribed via
/// `ntg_add_incoming_video`.
unsafe extern "C" fn frames_trampoline(
    _instance: *mut ntg_instance,
    user_id: i64,
    mode: ntg_stream_mode,
    device: ntg_stream_device,
    frames: *const ntg_frame,
    frames_len: usize,
    user_data: *mut c_void,
) {
    if user_data.is_null() || frames.is_null() || frames_len == 0 {
        return;
    }
    // SAFETY: registration passes an Arc-owned `CallbackShared` pointer and
    // Drop unregisters the callback before releasing the Arc; ntgcalls
    // guarantees the frames array for the duration of this call.
    let shared = unsafe { &*user_data.cast::<CallbackShared>() };
    // Phase C2g: group-chat routing is checked first; the native callback
    // key is the chat id for group calls and the user id for P2P.
    let (call_id, participant_user_id, is_local, is_screen) = if let Some(group_call_id) = shared
        .group_chat_to_call
        .lock()
        .expect("ntgcalls group call map")
        .get(&user_id)
        .copied()
    {
        // Screen frames arrive as PLAYBACK+SCREEN; camera as
        // PLAYBACK+CAMERA. Slice calls-group-self-tile: the local
        // camera preview arrives as CAPTURE+CAMERA keyed by the chat id
        // (ntgcalls emits capture frames per connection — v3.0.0
        // ntgcalls.cpp:126 wires on_frames for group connections too,
        // and stream_manager emits device captures to the callback);
        // it renders as the self tile instead of being dropped.
        if mode == NTG_STREAM_MODE_CAPTURE && device == NTG_STREAM_DEVICE_CAMERA {
            (group_call_id, None, true, false)
        } else {
            let is_screen = device == NTG_STREAM_DEVICE_SCREEN;
            if mode != NTG_STREAM_MODE_PLAYBACK
                || (!is_screen && device != NTG_STREAM_DEVICE_CAMERA)
            {
                // Screen-share capture preview has no tile yet; drop
                // rather than misattribute.
                return;
            }
            // SAFETY: frames_len > 0 was checked above.
            let ssrc = unsafe { &*frames.add(frames_len - 1) }.ssrc as u32;
            let Some(participant) = shared
                .group_video_ssrc_to_user
                .lock()
                .expect("ntgcalls group ssrc map")
                .get(&(user_id, ssrc))
                .copied()
            else {
                return;
            };
            (group_call_id, Some(participant), false, is_screen)
        }
    } else {
        let is_local = mode == NTG_STREAM_MODE_CAPTURE && device == NTG_STREAM_DEVICE_CAMERA;
        // Phase C2j: the peer's screen-share stream is accepted into its
        // own slot instead of being dropped; audio frames still return.
        let Some(is_screen) = p2p_frame_kind(mode, device) else {
            return;
        };
        let Some(call_id) = shared
            .user_to_call
            .lock()
            .expect("ntgcalls callback map")
            .get(&user_id)
            .copied()
        else {
            return;
        };
        (call_id, None, is_local, is_screen)
    };
    let frame = unsafe { &*frames.add(frames_len - 1) };
    let bytes = if frame.data.is_null() || frame.data_len == 0 {
        Vec::new()
    } else {
        // SAFETY: ntgcalls guarantees the frame byte span for this call;
        // copy it before returning to the worker thread.
        unsafe { std::slice::from_raw_parts(frame.data, frame.data_len) }.to_vec()
    };
    let rotation = frame.frame_data.rotation;
    let Some(rgba) = i420_to_rgba(
        frame.frame_data.width,
        frame.frame_data.height,
        &bytes,
        rotation,
    ) else {
        return;
    };
    let (width, height) = match rotation {
        NTG_VIDEO_ROTATION_VIDEO_ROTATION_90 | NTG_VIDEO_ROTATION_VIDEO_ROTATION_270 => {
            (frame.frame_data.height, frame.frame_data.width)
        }
        _ => (frame.frame_data.width, frame.frame_data.height),
    };
    let hook = shared
        .frame_hook
        .lock()
        .expect("ntgcalls video frame hook")
        .clone();
    if let Some(hook) = hook {
        let seq = shared.frame_seq.fetch_add(1, Ordering::Relaxed);
        hook(
            call_id,
            VideoFrame {
                seq,
                width,
                height,
                rgba,
                is_local,
                participant_user_id,
                is_screen,
            },
        );
    }
}

/// Phase C2e: peer camera on/off/paused state driven by the MediaState
/// signaling message. The state is passed by value.
unsafe extern "C" fn remote_source_trampoline(
    _instance: *mut ntg_instance,
    user_id: i64,
    state: ntg_remote_source,
    user_data: *mut c_void,
) {
    if user_data.is_null()
        || (state.device != NTG_STREAM_DEVICE_CAMERA && state.device != NTG_STREAM_DEVICE_SCREEN)
    {
        return;
    }
    let shared = unsafe { &*user_data.cast::<CallbackShared>() };
    let call_id = shared
        .user_to_call
        .lock()
        .expect("ntgcalls callback map")
        .get(&user_id)
        .copied();
    // Phase C2j: camera and screen-share states ride separate hooks so the
    // driver can clear retained screen frames without touching the camera
    // state. Audio and other devices never reach this path.
    let hook = if state.device == NTG_STREAM_DEVICE_SCREEN {
        shared
            .remote_screen_hook
            .lock()
            .expect("ntgcalls remote screen state hook")
            .clone()
    } else {
        shared
            .remote_video_hook
            .lock()
            .expect("ntgcalls remote video state hook")
            .clone()
    };
    if let (Some(call_id), Some(hook)) = (call_id, hook) {
        hook(call_id, remote_video_state_from(state.state));
    }
}

/// Phase C2e: map an ntgcalls stream status to the app-level camera state,
/// mirroring Telegram X's `VideoState` (Active=2, Paused=1, Inactive=0).
fn remote_video_state_from(status: ntg_stream_status) -> RemoteVideoState {
    match status {
        NTG_STREAM_STATUS_ACTIVE => RemoteVideoState::Active,
        NTG_STREAM_STATUS_PAUSED => RemoteVideoState::Paused,
        _ => RemoteVideoState::Inactive,
    }
}

/// Phase C2e: I420 planar byte size: Y (w*h) + U (cw*ch) + V (cw*ch).
fn i420_frame_size(width: u16, height: u16) -> usize {
    let (w, h) = (usize::from(width), usize::from(height));
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    w * h + 2 * cw * ch
}

/// Phase C2e: convert an I420 frame to RGBA8 with BT.601 full-range math,
/// then apply the frame rotation. Returns `None` on invalid input so we
/// never render garbage bytes.
fn i420_to_rgba(
    width: u16,
    height: u16,
    yuv: &[u8],
    rotation: ntg_video_rotation,
) -> Option<Vec<u8>> {
    if width == 0 || height == 0 || yuv.len() != i420_frame_size(width, height) {
        return None;
    }
    let (w, h) = (usize::from(width), usize::from(height));
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    let (y_plane, uv) = yuv.split_at(w * h);
    let (u_plane, v_plane) = uv.split_at(cw * ch);

    let mut rgba = vec![0u8; w * h * 4];
    for row in 0..h {
        for col in 0..w {
            let y = f32::from(y_plane[row * w + col]);
            let u = f32::from(u_plane[(row / 2) * cw + col / 2]);
            let v = f32::from(v_plane[(row / 2) * cw + col / 2]);
            let r = y + 1.402 * (v - 128.0);
            let g = y - 0.344_136 * (u - 128.0) - 0.714_136 * (v - 128.0);
            let b = y + 1.772 * (u - 128.0);
            let out = (row * w + col) * 4;
            rgba[out] = r.clamp(0.0, 255.0).round() as u8;
            rgba[out + 1] = g.clamp(0.0, 255.0).round() as u8;
            rgba[out + 2] = b.clamp(0.0, 255.0).round() as u8;
            rgba[out + 3] = 255;
        }
    }
    Some(match rotation {
        NTG_VIDEO_ROTATION_VIDEO_ROTATION_90
        | NTG_VIDEO_ROTATION_VIDEO_ROTATION_180
        | NTG_VIDEO_ROTATION_VIDEO_ROTATION_270 => rotate_rgba(&rgba, w, h, rotation),
        _ => rgba,
    })
}

/// Phase C2e: rotate an RGBA8 buffer (row-major, `w` x `h`) by the given
/// frame rotation; 90/270 swap the dimensions.
fn rotate_rgba(src: &[u8], w: usize, h: usize, rotation: ntg_video_rotation) -> Vec<u8> {
    let (dw, dh) = match rotation {
        NTG_VIDEO_ROTATION_VIDEO_ROTATION_90 | NTG_VIDEO_ROTATION_VIDEO_ROTATION_270 => (h, w),
        _ => (w, h),
    };
    let mut dst = vec![0u8; src.len()];
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = match rotation {
                // 90° clockwise: the right column lands on top.
                NTG_VIDEO_ROTATION_VIDEO_ROTATION_90 => (y, w - 1 - x),
                NTG_VIDEO_ROTATION_VIDEO_ROTATION_180 => (w - 1 - x, h - 1 - y),
                NTG_VIDEO_ROTATION_VIDEO_ROTATION_270 => (h - 1 - y, x),
                _ => (x, y),
            };
            let src_off = (y * w + x) * 4;
            let dst_off = (dy * dw + dx) * 4;
            dst[dst_off..dst_off + 4].copy_from_slice(&src[src_off..src_off + 4]);
        }
    }
    debug_assert_eq!(dw * dh, w * h);
    dst
}

/// Phase C2e: honest no-camera decision for the driver (session 2): a video
/// call is only wanted when a camera exists; otherwise video stays off.
pub fn video_wanted(is_video_call: bool, devices: &[MediaDevice]) -> (bool, Option<String>) {
    if !is_video_call {
        return (false, None);
    }
    devices
        .iter()
        .find(|device| device.kind == MediaDeviceKind::Camera)
        .map(|camera| (true, Some(camera.id.clone())))
        .unwrap_or((false, None))
}

fn c_strings(values: &[String]) -> Result<Vec<CString>, EngineError> {
    values
        .iter()
        .map(|value| {
            CString::new(value.as_str()).map_err(|_| EngineError::Engine {
                op: "ntg_connect_p2p",
                code: NTG_ERR_INVALID_PARAMS,
            })
        })
        .collect()
}

struct NativeRtcServers {
    raw: Vec<ntg_rtc_server>,
    _strings: Vec<[CString; 4]>,
    _peer_tags: Vec<Vec<u8>>,
}

impl NativeRtcServers {
    fn new(servers: &[RtcServer]) -> Result<Self, EngineError> {
        let mut strings = Vec::with_capacity(servers.len());
        let mut peer_tags = Vec::with_capacity(servers.len());
        for server in servers {
            strings.push([
                native_string(&server.ipv4)?,
                native_string(&server.ipv6)?,
                native_string(&server.username)?,
                native_string(&server.password)?,
            ]);
            peer_tags.push(server.peer_tag.clone());
        }
        let raw = servers
            .iter()
            .enumerate()
            .map(|(index, server)| ntg_rtc_server {
                id: server.id,
                ipv4: strings[index][0].as_ptr().cast_mut(),
                ipv6: strings[index][1].as_ptr().cast_mut(),
                port: server.port,
                username: strings[index][2].as_ptr().cast_mut(),
                password: strings[index][3].as_ptr().cast_mut(),
                turn: server.turn,
                stun: server.stun,
                tcp: server.tcp,
                peer_tag: peer_tags[index].as_ptr().cast_mut(),
                peer_tag_len: peer_tags[index].len(),
            })
            .collect();
        Ok(Self {
            raw,
            _strings: strings,
            _peer_tags: peer_tags,
        })
    }
}

fn native_string(value: &str) -> Result<CString, EngineError> {
    CString::new(value).map_err(|_| EngineError::Engine {
        op: "ntg_connect_p2p",
        code: NTG_ERR_INVALID_PARAMS,
    })
}

fn c_string_pointer_array(values: *mut *mut std::ffi::c_char, len: usize) -> Vec<String> {
    if values.is_null() || len == 0 {
        return Vec::new();
    }
    // SAFETY: a successful ntgcalls output provides `len` pointer entries;
    // each individual pointer is still checked for null by `c_string`.
    unsafe { std::slice::from_raw_parts(values, len) }
        .iter()
        .map(|value| c_string(*value))
        .collect()
}

fn append_devices(
    output: &mut Vec<MediaDevice>,
    devices: *mut ntg_device_info,
    len: usize,
    kind: MediaDeviceKind,
) {
    if devices.is_null() || len == 0 {
        return;
    }
    // SAFETY: a successful device enumeration provides `len` entries; all
    // nested C strings are individually null-checked below.
    for device in unsafe { std::slice::from_raw_parts(devices, len) } {
        output.push(MediaDevice {
            id: c_string(device.metadata),
            name: c_string(device.name),
            kind,
        });
    }
}

fn c_string(value: *mut std::ffi::c_char) -> String {
    if value.is_null() {
        return String::new();
    }
    // SAFETY: non-null strings in successful ntgcalls outputs are NUL
    // terminated and remain valid until their parent output is freed.
    unsafe { CStr::from_ptr(value) }
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntgcalls_sys::{NTG_STREAM_STATUS_IDLING, NTG_VIDEO_ROTATION_VIDEO_ROTATION_0};

    /// Phase C2g: the audio SSRC is the first `a=ssrc:` line of the
    /// audio `m=` section; video sections are ignored, and 0 is the
    /// honest fallback (the join still goes out).
    #[test]
    fn group_offer_audio_source_id_parses_first_audio_ssrc() {
        let offer = "v=0\r\n\
            m=audio 9 UDP/TLS/RTP/SAVPF 111\r\n\
            a=ssrc:12345 cname:audio\r\n\
            a=ssrc:12346 cname:audio\r\n\
            m=video 9 UDP/TLS/RTP/SAVPF 96\r\n\
            a=ssrc:99999 cname:video\r\n";
        assert_eq!(group_offer_audio_source_id(offer), 12345);
    }

    #[test]
    fn group_offer_audio_source_id_zero_without_audio_ssrc() {
        assert_eq!(
            group_offer_audio_source_id("v=0\r\nm=video 9 UDP/TLS/RTP/SAVPF 96\r\n"),
            0
        );
        assert_eq!(group_offer_audio_source_id(""), 0);
    }

    #[test]
    fn mock_engine_lifecycle_and_protocol() {
        let mut engine = MockEngine::new();
        let handle = engine.clone();
        let emitted = Arc::new(Mutex::new(Vec::new()));
        let emitted_for_hook = emitted.clone();
        engine.set_signaling_emitted_callback(Arc::new(move |call_id, data| {
            emitted_for_hook
                .lock()
                .expect("emitted signaling")
                .push((call_id, data));
        }));
        let devices = vec![MediaDevice {
            id: "mic-1".into(),
            name: "Test microphone".into(),
            kind: MediaDeviceKind::Microphone,
        }];
        engine.set_devices(devices.clone());

        engine.start_call(77, 41, false).unwrap();
        engine.accept_call(77).unwrap();
        let params = ConnectParams {
            encryption_key: vec![1; 256],
            is_outgoing: false,
            servers: vec![RtcServer {
                id: 7,
                ipv4: "149.154.167.40".into(),
                ipv6: String::new(),
                port: 443,
                username: String::new(),
                password: String::new(),
                turn: true,
                stun: false,
                tcp: true,
                peer_tag: vec![0, 1, 2],
            }],
            library_versions: vec!["13.0.0".into()],
            p2p_allowed: true,
            mic_input: Some("mic-1".into()),
            speaker_input: Some("speaker-1".into()),
            video_enabled: false,
            camera_input: None,
        };
        engine.connect(77, &params).unwrap();
        engine
            .select_devices(77, Some("mic-1"), Some("speaker-1"))
            .unwrap();
        engine.send_signaling_data(77, b"inbound").unwrap();
        handle.receive_signaling_data(77, b"outbound");
        engine.set_muted(77, true).unwrap();
        assert_eq!(engine.media_devices().unwrap(), devices);
        assert_eq!(engine.protocol(), MockEngine::engine_protocol());
        assert!(engine.is_available());
        engine.hangup(77).unwrap();

        assert_eq!(handle.started_calls(), vec![(77, 41, false)]);
        assert_eq!(handle.accepted_calls(), vec![77]);
        assert_eq!(handle.connects(), vec![(77, params)]);
        assert_eq!(
            handle.device_selections(),
            vec![(77, Some("mic-1".to_string()), Some("speaker-1".to_string()))]
        );
        assert_eq!(handle.signaling_received(), vec![(77, b"inbound".to_vec())]);
        assert_eq!(handle.mute_changes(), vec![(77, true)]);
        assert_eq!(handle.hung_up_calls(), vec![77]);
        assert_eq!(
            *emitted.lock().expect("emitted signaling"),
            vec![(77, b"outbound".to_vec())]
        );
    }

    #[test]
    fn mock_engine_unknown_calls_and_idempotent_hangup() {
        let mut engine = MockEngine::new();
        assert_eq!(engine.accept_call(77), Err(EngineError::NoSuchCall(77)));
        assert_eq!(
            engine.send_signaling_data(77, b"data"),
            Err(EngineError::NoSuchCall(77))
        );
        assert_eq!(engine.set_muted(77, true), Err(EngineError::NoSuchCall(77)));
        assert_eq!(engine.hangup(77), Ok(()));
    }

    #[test]
    fn unavailable_mock_rejects_operations() {
        let mut engine = MockEngine::unavailable();
        assert!(!engine.is_available());
        assert_eq!(engine.protocol(), EngineProtocol::signaling_only());
        assert_eq!(
            engine.start_call(77, 41, false),
            Err(EngineError::Unavailable)
        );
        assert_eq!(engine.accept_call(77), Err(EngineError::Unavailable));
        assert_eq!(
            engine.send_signaling_data(77, b"data"),
            Err(EngineError::Unavailable)
        );
        assert_eq!(engine.hangup(77), Err(EngineError::Unavailable));
        assert_eq!(engine.set_muted(77, true), Err(EngineError::Unavailable));
        assert_eq!(engine.media_devices(), Err(EngineError::Unavailable));
    }

    #[test]
    fn mock_engine_replaces_signaling_hook() {
        let mut engine = MockEngine::new();
        let first = Arc::new(Mutex::new(Vec::new()));
        let first_hook = first.clone();
        engine.set_signaling_emitted_callback(Arc::new(move |_, data| {
            first_hook.lock().expect("first hook").push(data);
        }));
        let second = Arc::new(Mutex::new(Vec::new()));
        let second_hook = second.clone();
        engine.set_signaling_emitted_callback(Arc::new(move |_, data| {
            second_hook.lock().expect("second hook").push(data);
        }));

        engine.receive_signaling_data(77, b"replacement");
        assert!(first.lock().expect("first hook").is_empty());
        assert_eq!(
            *second.lock().expect("second hook"),
            vec![b"replacement".to_vec()]
        );
    }

    #[test]
    fn mock_engine_delivers_transport_state_and_replaces_hook() {
        let mut engine = MockEngine::new();
        let first = Arc::new(Mutex::new(Vec::new()));
        let first_hook = first.clone();
        engine.set_transport_state_callback(Arc::new(move |call_id, state| {
            first_hook
                .lock()
                .expect("first transport hook")
                .push((call_id, state));
        }));
        let second = Arc::new(Mutex::new(Vec::new()));
        let second_hook = second.clone();
        engine.set_transport_state_callback(Arc::new(move |call_id, state| {
            second_hook
                .lock()
                .expect("second transport hook")
                .push((call_id, state));
        }));

        engine.emit_transport_state(77, TransportState::Connected);
        assert!(first.lock().expect("first transport hook").is_empty());
        assert_eq!(
            *second.lock().expect("second transport hook"),
            vec![(77, TransportState::Connected)]
        );
    }

    #[test]
    fn mock_engine_connect_and_device_selection_need_known_call() {
        let mut engine = MockEngine::new();
        let params = ConnectParams {
            encryption_key: vec![1; 256],
            is_outgoing: true,
            servers: Vec::new(),
            library_versions: Vec::new(),
            p2p_allowed: false,
            mic_input: None,
            speaker_input: None,
            video_enabled: false,
            camera_input: None,
        };
        assert_eq!(
            engine.connect(77, &params),
            Err(EngineError::NoSuchCall(77))
        );
        assert_eq!(
            engine.select_devices(77, Some("mic-1"), None),
            Err(EngineError::NoSuchCall(77))
        );
        assert!(engine.connects().is_empty());
        assert!(engine.device_selections().is_empty());
    }

    /// Phase C2e: validates the BT.601 conversion math. Neutral chroma
    /// passes Y straight through, so Y=235 ~ white and Y=16 ~ black.
    /// A saturated case with no clamping on red pins the 1.402 coefficient:
    /// Y=64, U=128, V=255 gives R = 64 + 1.402*127 ~ 242.
    #[test]
    fn i420_to_rgba_known_pixels() {
        let white = i420_to_rgba(
            2,
            2,
            &[235, 235, 235, 235, 128, 128],
            NTG_VIDEO_ROTATION_VIDEO_ROTATION_0,
        )
        .expect("valid 2x2 frame");
        assert_eq!(white.len(), 16);
        let (pixels, _) = white.as_chunks::<4>();
        for pixel in pixels {
            assert_pixel_close(pixel, &[235, 235, 235, 255]);
        }
        let black = i420_to_rgba(
            2,
            2,
            &[16, 16, 16, 16, 128, 128],
            NTG_VIDEO_ROTATION_VIDEO_ROTATION_0,
        )
        .expect("valid 2x2 frame");
        let (pixels, _) = black.as_chunks::<4>();
        for pixel in pixels {
            assert_pixel_close(pixel, &[16, 16, 16, 255]);
        }
        // I420 planes: Y(2x2) then U(1x1) then V(1x1).
        let saturated = i420_to_rgba(
            2,
            2,
            &[64, 64, 64, 64, 128, 255],
            NTG_VIDEO_ROTATION_VIDEO_ROTATION_0,
        )
        .expect("valid 2x2 frame");
        let (pixels, _) = saturated.as_chunks::<4>();
        for pixel in pixels {
            assert_pixel_close(pixel, &[242, 0, 64, 255]);
        }
    }

    fn assert_pixel_close(actual: &[u8], expected: &[u8; 4]) {
        assert_eq!(actual.len(), 4);
        for (index, (&a, &e)) in actual.iter().zip(expected.iter()).enumerate() {
            assert!(
                (i16::from(a) - i16::from(e)).abs() <= 2,
                "channel {index}: got {a}, want ~{e}"
            );
        }
    }

    /// Phase C2e: validates we never render garbage bytes.
    #[test]
    fn i420_to_rgba_rejects_bad_sizes() {
        assert_eq!(
            i420_to_rgba(2, 2, &[0; 5], NTG_VIDEO_ROTATION_VIDEO_ROTATION_0),
            None
        );
        assert_eq!(
            i420_to_rgba(2, 2, &[0; 7], NTG_VIDEO_ROTATION_VIDEO_ROTATION_0),
            None
        );
        assert_eq!(
            i420_to_rgba(0, 2, &[], NTG_VIDEO_ROTATION_VIDEO_ROTATION_0),
            None
        );
        assert_eq!(
            i420_to_rgba(2, 0, &[], NTG_VIDEO_ROTATION_VIDEO_ROTATION_0),
            None
        );
    }

    /// Phase C2e: validates rotation handling. 2x1 with distinct pixels,
    /// rotated 90° clockwise, becomes 1x2 with the right column on top.
    #[test]
    fn i420_to_rgba_rotation_90() {
        let rgba = i420_to_rgba(
            2,
            1,
            &[235, 16, 128, 128],
            NTG_VIDEO_ROTATION_VIDEO_ROTATION_90,
        )
        .expect("valid 2x1 frame");
        assert_eq!(
            rgba,
            vec![16, 16, 16, 255, 235, 235, 235, 255],
            "black pixel (right column) lands on top"
        );
    }

    /// Phase C2i: validates the 1:1 screen-share toggle contract the
    /// UI will drive (mirrors `mock_camera_toggle_state_machine`).
    #[test]
    fn mock_p2p_screen_share_toggle_state_machine() {
        let mut engine = MockEngine::new();
        engine.start_call(77, 41, false).unwrap();
        engine.set_screen_share_enabled(77, true).unwrap();
        engine.set_screen_share_enabled(77, false).unwrap();
        assert_eq!(
            engine.p2p_screen_share_changes(),
            vec![(77, true), (77, false)]
        );
        assert_eq!(
            engine.set_screen_share_enabled(99, true),
            Err(EngineError::NoSuchCall(99))
        );

        let mut unavailable = MockEngine::unavailable();
        assert_eq!(
            unavailable.set_screen_share_enabled(77, true),
            Err(EngineError::Unavailable)
        );
    }

    /// Phase C2i: a mid-call screen-share intent survives a transport
    /// reconnect — `connect` re-runs with the retained params and the
    /// engine must re-issue screen-only (not flip back to the camera).
    #[test]
    fn retained_call_media_preserves_screen_share_across_reconnect() {
        let params = ConnectParams {
            encryption_key: vec![1; 256],
            is_outgoing: false,
            servers: Vec::new(),
            library_versions: vec!["13.0.0".into()],
            p2p_allowed: true,
            mic_input: Some("mic-1".into()),
            speaker_input: Some("speaker-1".into()),
            video_enabled: true,
            camera_input: Some("cam-1".into()),
        };
        // Fresh connect: no prior entry, share starts off.
        let fresh = retained_call_media(&params, None);
        assert!(!fresh.screen_share_on);
        assert_eq!(fresh.mic.as_deref(), Some("mic-1"));
        // Mid-call the user starts sharing, then the transport drops
        // and `connect` re-runs with the same retained params: the
        // share intent must survive so `set_media_sources` re-issues
        // screen-only (`camera_enabled && !screen_share_on` stays
        // false for the camera).
        let mut sharing = fresh;
        sharing.screen_share_on = true;
        sharing.camera_enabled = false;
        let reconnected = retained_call_media(&params, Some(&sharing));
        assert!(reconnected.screen_share_on);
        // Stopping share before the drop must not stick the flag on
        // either.
        let mut stopped = sharing;
        stopped.screen_share_on = false;
        assert!(!retained_call_media(&params, Some(&stopped)).screen_share_on);
    }

    /// Phase C2e: validates the toggle contract the UI will drive.
    #[test]
    fn mock_camera_toggle_state_machine() {
        let mut engine = MockEngine::new();
        engine.start_call(77, 41, false).unwrap();
        engine.set_camera_enabled(77, true, Some("cam-1")).unwrap();
        engine.set_camera_enabled(77, false, None).unwrap();
        assert_eq!(
            engine.camera_changes(),
            vec![(77, true, Some("cam-1".to_string())), (77, false, None)]
        );
        assert_eq!(
            engine.set_camera_enabled(99, true, None),
            Err(EngineError::NoSuchCall(99))
        );

        let mut unavailable = MockEngine::unavailable();
        assert_eq!(
            unavailable.set_camera_enabled(77, true, None),
            Err(EngineError::Unavailable)
        );
    }

    /// Phase C2e: validates frame delivery and hook replacement.
    #[test]
    fn mock_frame_callback_plumbing() {
        let mut engine = MockEngine::new();
        let first = Arc::new(Mutex::new(Vec::new()));
        let first_hook = first.clone();
        engine.set_video_frame_callback(Arc::new(move |call_id, frame| {
            first_hook
                .lock()
                .expect("first frame hook")
                .push((call_id, frame));
        }));
        let frame = VideoFrame {
            seq: 999, // overwritten by the mock's internal counter
            width: 2,
            height: 2,
            rgba: vec![1, 2, 3, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            is_local: true,
            participant_user_id: None,
            is_screen: false,
        };
        engine.emit_video_frame(77, frame.clone());
        let second = Arc::new(Mutex::new(Vec::new()));
        let second_hook = second.clone();
        engine.set_video_frame_callback(Arc::new(move |call_id, frame| {
            second_hook
                .lock()
                .expect("second frame hook")
                .push((call_id, frame));
        }));
        engine.emit_video_frame(77, frame);

        let first = first.lock().expect("first frame hook");
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].0, 77);
        assert!(first[0].1.is_local);
        assert_eq!(first[0].1.seq, 0);
        let second = second.lock().expect("second frame hook");
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].1.seq, 1);
        assert_eq!(second[0].1.width, 2);
    }

    /// Phase C2j: P2P frame acceptance — local preview, peer camera, and
    /// peer screen share reach the app; anything else is dropped.
    #[test]
    fn p2p_frame_kind_accepts_screen_share() {
        assert_eq!(
            p2p_frame_kind(NTG_STREAM_MODE_CAPTURE, NTG_STREAM_DEVICE_CAMERA),
            Some(false)
        );
        assert_eq!(
            p2p_frame_kind(NTG_STREAM_MODE_PLAYBACK, NTG_STREAM_DEVICE_CAMERA),
            Some(false)
        );
        assert_eq!(
            p2p_frame_kind(NTG_STREAM_MODE_PLAYBACK, NTG_STREAM_DEVICE_SCREEN),
            Some(true)
        );
        // Capture-side screen frames and unknown kinds never reach a tile.
        assert_eq!(
            p2p_frame_kind(NTG_STREAM_MODE_CAPTURE, NTG_STREAM_DEVICE_SCREEN),
            None
        );
        assert_eq!(p2p_frame_kind(999, 999), None);
    }

    /// Phase C2j: validates peer screen-share state delivery ordering.
    #[test]
    fn mock_remote_screen_state_emission() {
        let mut engine = MockEngine::new();
        let states = Arc::new(Mutex::new(Vec::new()));
        let states_hook = states.clone();
        engine.set_remote_screen_state_callback(Arc::new(move |call_id, state| {
            states_hook
                .lock()
                .expect("remote screen hook")
                .push((call_id, state));
        }));
        engine.emit_remote_screen_state(77, RemoteVideoState::Active);
        engine.emit_remote_screen_state(77, RemoteVideoState::Inactive);
        assert_eq!(
            *states.lock().expect("remote screen hook"),
            vec![
                (77, RemoteVideoState::Active),
                (77, RemoteVideoState::Inactive)
            ]
        );
    }

    /// Phase C2e: validates peer-state delivery ordering.
    #[test]
    fn mock_remote_video_state_emission() {
        let mut engine = MockEngine::new();
        let states = Arc::new(Mutex::new(Vec::new()));
        let states_hook = states.clone();
        engine.set_remote_video_state_callback(Arc::new(move |call_id, state| {
            states_hook
                .lock()
                .expect("remote video hook")
                .push((call_id, state));
        }));
        engine.emit_remote_video_state(77, RemoteVideoState::Inactive);
        engine.emit_remote_video_state(77, RemoteVideoState::Active);
        assert_eq!(
            *states.lock().expect("remote video hook"),
            vec![
                (77, RemoteVideoState::Inactive),
                (77, RemoteVideoState::Active)
            ]
        );
    }

    /// Phase C2e: validates the ntg -> app mapping mirrors TGX VideoState.
    #[test]
    fn remote_video_state_mapping() {
        assert_eq!(
            remote_video_state_from(NTG_STREAM_STATUS_ACTIVE),
            RemoteVideoState::Active
        );
        assert_eq!(
            remote_video_state_from(NTG_STREAM_STATUS_PAUSED),
            RemoteVideoState::Paused
        );
        assert_eq!(
            remote_video_state_from(NTG_STREAM_STATUS_IDLING),
            RemoteVideoState::Inactive
        );
    }

    /// Phase C2e: validates the honest no-camera decision the driver will use.
    #[test]
    fn video_wanted_requires_camera() {
        let camera = MediaDevice {
            id: "cam-1".into(),
            name: "Cam".into(),
            kind: MediaDeviceKind::Camera,
        };
        let mic = MediaDevice {
            id: "mic-1".into(),
            name: "Mic".into(),
            kind: MediaDeviceKind::Microphone,
        };
        assert_eq!(
            video_wanted(true, &[camera.clone(), mic]),
            (true, Some("cam-1".to_string()))
        );
        assert_eq!(video_wanted(true, &[]), (false, None));
        assert_eq!(video_wanted(false, &[camera]), (false, None));
    }
}
