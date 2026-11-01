//! Call-engine shared types: callbacks, media descriptions, errors.
use super::*;
use crate::locks::LockExt;
use serde_json::{Value, json};
use std::sync::atomic::AtomicU64;
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

pub type RemoteVideoStateCallback = Arc<dyn Fn(i32, RemoteVideoState) + Send + Sync + 'static>;

/// The peer's microphone in a 1:1 call: `(call id, muted)`, from the
/// MediaState signaling message (tgcalls' `remoteMediaStateUpdated`).
pub type RemoteAudioStateCallback = Arc<dyn Fn(i32, bool) + Send + Sync + 'static>;

/// The audio ssrc a group join payload announces, for TDLib's
/// `groupCallJoinParameters.audio_source_id` ("received from tgcalls",
/// `schema/td_api.tl:7085`). tgcalls' payload is tdesktop's
/// `GroupJoinPayload` JSON, whose `ssrc` is the audio ssrc as a signed
/// 32-bit number. A raw SDP offer (a test engine's) is read too: the
/// first `a=ssrc:` line of the audio `m=` section. 0 when absent (the
/// join still goes out, only audio attribution is missing).
pub fn group_offer_audio_source_id(offer: &str) -> i32 {
    if let Ok(Value::Object(payload)) = serde_json::from_str::<Value>(offer) {
        return payload
            .get("ssrc")
            .and_then(Value::as_i64)
            .map_or(0, |ssrc| ssrc as i32);
    }
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
/// tgcalls subscribes by `endpoint` and routes its pictures back to
/// `user_id` by endpoint.
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
    /// The participant's screen share (`screen_sharing_video_info`)
    /// rather than their camera; tgcalls tells them apart only by
    /// endpoint.
    pub screen: bool,
    /// The participant's audio ssrc (`audio_source_id`), which tgcalls
    /// syncs the video's playback to (lip sync).
    pub audio_ssrc: u32,
    /// Shown large where someone can see it. Only this video is worth
    /// more than Thumbnail.
    pub large: bool,
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
    pub custom_parameters: String,
    pub is_outgoing: bool,
    pub servers: Vec<RtcServer>,
    pub library_versions: Vec<String>,
    pub p2p_allowed: bool,
    pub mic_input: Option<String>,
    pub speaker_input: Option<String>,
    /// Phase C2e: the camera sent from the start, by enumerated device
    /// id; `None` connects without video.
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

/// The camera a call sends from: the one picked in the call menu while
/// the engine still lists it, else the first camera listed. `None` when
/// there is no camera (the engine opens only a listed device, by its id).
pub fn pick_camera(selected: Option<&str>, devices: &[MediaDevice]) -> Option<String> {
    let mut cameras = devices
        .iter()
        .filter(|device| device.kind == MediaDeviceKind::Camera);
    let first = cameras.clone().next();
    cameras
        .find(|device| Some(device.id.as_str()) == selected)
        .or(first)
        .map(|device| device.id.clone())
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
    /// The engine lists no camera, so the camera cannot turn on.
    #[error("no camera found")]
    NoCamera,
}

/// The hooks the driver installs, shared with the engine's native
/// callbacks.
pub(crate) struct CallbackShared {
    pub(crate) hook: Mutex<Option<SignalingEmittedCallback>>,
    pub(crate) transport_hook: Mutex<Option<TransportStateCallback>>,
    pub(crate) remote_video_hook: Mutex<Option<RemoteVideoStateCallback>>,
    /// Phase C2j: peer 1:1 screen-share state hook.
    pub(crate) remote_screen_hook: Mutex<Option<RemoteVideoStateCallback>>,
    /// The peer's 1:1 microphone on/off hook.
    pub(crate) remote_audio_hook: Mutex<Option<RemoteAudioStateCallback>>,
    pub(crate) frame_seq: AtomicU64,
    /// Broadcast time and part requests (stream mode), speaker levels.
    pub(crate) engine_event_hook: Mutex<Option<EngineEventCallback>>,
    /// Every decoded picture (group-call tracks, streams, 1:1 calls), as
    /// I420 planes.
    pub(crate) live_frame_hook: Mutex<Option<LiveFrameCallback>>,
}

impl CallbackShared {
    /// No hooks installed.
    pub(crate) fn new() -> Self {
        Self {
            hook: Mutex::new(None),
            transport_hook: Mutex::new(None),
            remote_video_hook: Mutex::new(None),
            remote_screen_hook: Mutex::new(None),
            remote_audio_hook: Mutex::new(None),
            frame_seq: AtomicU64::new(0),
            engine_event_hook: Mutex::new(None),
            live_frame_hook: Mutex::new(None),
        }
    }
}

/// Phase C2e: retained per-call media configuration; the engine re-issues
/// stream sources from this on camera toggles and device changes.
#[derive(Clone)]
pub(crate) struct CallMediaConfig {
    pub(crate) mic: Option<String>,
    pub(crate) speaker: Option<String>,
    /// The camera being sent, by device id; `None` while it is off.
    pub(crate) camera: Option<String>,
    /// Phase C2i: screen-share send intent for the 1:1 call. When on,
    /// the call sends the screen instead of the camera: a 1:1 call has
    /// one video track.
    pub(crate) screen_share_on: bool,
}

/// Phase C2i: retained media config for `connect`. A transport
/// reconnect re-runs `connect` with the retained params; an
/// in-flight screen-share intent survives so the engine re-issues
/// screen-only instead of flipping back to the camera. A fresh
/// connect has no prior entry, so the flag starts off.
pub(crate) fn retained_call_media(
    params: &ConnectParams,
    previous: Option<&CallMediaConfig>,
) -> CallMediaConfig {
    CallMediaConfig {
        mic: params.mic_input.clone(),
        speaker: params.speaker_input.clone(),
        camera: params.camera_input.clone(),
        screen_share_on: previous.is_some_and(|config| config.screen_share_on),
    }
}

/// What you do in a group call: take part, or only watch (a live story's
/// stream), which captures and sends nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GroupCallRole {
    #[default]
    Member,
    Viewer,
}

/// Stores `hook` in a callback slot of the state shared with the
/// engine's native callbacks.
pub(crate) fn set_hook<T>(slot: &std::sync::Mutex<Option<T>>, hook: T) {
    *slot.lock_or_recover() = Some(hook);
}
