//! Call-engine shared types: callbacks, media descriptions, errors.
use super::*;
use ntgcalls_sys::ntg_rtc_server;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::ffi::CString;
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
    pub custom_parameters: String,
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

pub(crate) struct CallbackShared {
    pub(crate) user_to_call: Mutex<HashMap<i64, i32>>,
    pub(crate) hook: Mutex<Option<SignalingEmittedCallback>>,
    pub(crate) transport_hook: Mutex<Option<TransportStateCallback>>,
    pub(crate) frame_hook: Mutex<Option<VideoFrameCallback>>,
    pub(crate) remote_video_hook: Mutex<Option<RemoteVideoStateCallback>>,
    /// Phase C2j: peer 1:1 screen-share state hook.
    pub(crate) remote_screen_hook: Mutex<Option<RemoteVideoStateCallback>>,
    pub(crate) frame_seq: AtomicU64,
    /// Phase C2g: group-call callback routing: native `chat_id` -> the
    /// TDLib group call id the driver assigned, and the ssrc map that
    /// attributes incoming frames to participants.
    pub(crate) group_chat_to_call: Mutex<HashMap<i64, i32>>,
    pub(crate) group_video_ssrc_to_user: Mutex<HashMap<(i64, u32), i64>>,
}

/// Phase C2e: retained per-call media configuration; the engine re-issues
/// stream sources from this on camera toggles and device changes.
#[derive(Clone)]
pub(crate) struct CallMediaConfig {
    pub(crate) mic: Option<String>,
    pub(crate) speaker: Option<String>,
    pub(crate) camera_enabled: bool,
    pub(crate) camera: Option<String>,
    /// Phase C2i: screen-share send intent for the 1:1 call. When on,
    /// `set_media_sources` issues the desktop capture instead of the
    /// camera — ntgcalls rejects mixing both in Capture mode
    /// (`stream_manager.cpp:69`, v3.0.0).
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
        camera_enabled: params.video_enabled,
        camera: params.camera_input.clone(),
        screen_share_on: previous.is_some_and(|config| config.screen_share_on),
    }
}

/// Phase C2g: retained per-group-call media state for the ntgcalls
/// group transport (keyed by TDLib group call id).
#[derive(Clone, Default)]
pub(crate) struct GroupCallMedia {
    pub(crate) chat_id: i64,
    /// Your microphone is muted in this group call.
    pub(crate) muted: bool,
    pub(crate) camera_enabled: bool,
    pub(crate) camera: Option<String>,
    /// endpoint -> full source, mirroring the engine's subscriptions.
    /// Storing the source (not just the user id) lets resubscription
    /// drop only one endpoint's ssrcs and detect changed ssrc groups.
    pub(crate) endpoints: HashMap<String, GroupVideoSource>,
    /// (chat_id, ssrc) -> user id; attributed to incoming frames.
    pub(crate) ssrc_to_user: HashMap<(i64, u32), i64>,
    pub(crate) connected: bool,
    pub(crate) screen_sharing: bool,
    /// Phase C2g: `ntg_init_presentation` ran but the answer handshake
    /// has not connected yet. Tracked so the driver can reconcile and
    /// stop a stray presentation after a failed handshake.
    pub(crate) presentation_initialized: bool,
}

pub(crate) struct NativeRtcServers {
    pub(crate) raw: Vec<ntg_rtc_server>,
    _strings: Vec<[CString; 4]>,
    _peer_tags: Vec<Vec<u8>>,
}

impl NativeRtcServers {
    pub(crate) fn new(servers: &[RtcServer]) -> Result<Self, EngineError> {
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
