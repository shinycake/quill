//! Call-engine shared types: callbacks, media descriptions, errors.
use super::*;
use crate::calls::stream_audio::StreamAudio;
use crate::locks::LockExt;
use arc_swap::ArcSwap;
use ntgcalls_sys::ntg_rtc_server;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
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

pub type RemoteVideoStateCallback = Arc<dyn Fn(i32, RemoteVideoState) + Send + Sync + 'static>;

/// The peer's microphone in a 1:1 call: `(call id, muted)`, from the
/// MediaState signaling message (ntgcalls `p2p_call.cpp` reports it as
/// the remote `Microphone` source going idle or active).
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
    /// The participant's screen share (`screen_sharing_video_info`)
    /// rather than their camera. ntgcalls tells them apart by track;
    /// tgcalls only by endpoint.
    pub screen: bool,
    /// The participant's audio ssrc (`audio_source_id`), which tgcalls
    /// syncs the video's playback to (lip sync).
    pub audio_ssrc: u32,
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

impl EngineError {
    /// `ntg_connect` refused the stream the call joined
    /// (`NTG_ERR_RTMP_STREAMING_UNSUPPORTED`). A fatal join error: the
    /// call shows the Unsupported state instead of a picture. The same
    /// code from any other operation is an ordinary engine error.
    pub fn is_stream_unsupported(&self) -> bool {
        matches!(
            self,
            EngineError::Engine {
                op: "ntg_connect",
                code: ntgcalls_sys::NTG_ERR_RTMP_STREAMING_UNSUPPORTED,
            }
        )
    }
}

pub(crate) struct CallbackShared {
    pub(crate) user_to_call: Mutex<HashMap<i64, i32>>,
    pub(crate) hook: Mutex<Option<SignalingEmittedCallback>>,
    pub(crate) transport_hook: Mutex<Option<TransportStateCallback>>,
    pub(crate) remote_video_hook: Mutex<Option<RemoteVideoStateCallback>>,
    /// Phase C2j: peer 1:1 screen-share state hook.
    pub(crate) remote_screen_hook: Mutex<Option<RemoteVideoStateCallback>>,
    /// The peer's 1:1 microphone on/off hook.
    pub(crate) remote_audio_hook: Mutex<Option<RemoteAudioStateCallback>>,
    /// Peers (by user id) whose 1:1 screen share is on. A 1:1 call has
    /// one video track, as in tgcalls: a shared screen arrives on it as
    /// camera frames, and only the MediaState says it is a screen.
    pub(crate) peer_screencasting: Mutex<HashSet<i64>>,
    pub(crate) frame_seq: AtomicU64,
    /// Phase C2g: group-call callback routing: native `chat_id` -> the
    /// TDLib group call id the driver assigned, and the ssrc map that
    /// attributes incoming frames to participants.
    pub(crate) group_chat_to_call: Mutex<HashMap<i64, i32>>,
    pub(crate) group_video_ssrc_to_user: Mutex<HashMap<(i64, u32), i64>>,
    /// Chats whose group call runs in stream mode: their frames go to the
    /// stream slot instead of the ssrc lookup.
    pub(crate) stream_chats: Mutex<HashSet<i64>>,
    /// Broadcast time and part requests (stream mode).
    pub(crate) engine_event_hook: Mutex<Option<EngineEventCallback>>,
    /// ntgcalls' open part requests by the request id Quill gave them.
    pub(crate) ntg_parts: Mutex<super::native::NtgPartRequests>,
    /// Every decoded picture (group-call tracks, streams, 1:1 calls), as
    /// I420 planes.
    pub(crate) live_frame_hook: Mutex<Option<LiveFrameCallback>>,
    pub(crate) stream_audio: ArcSwap<HashMap<i64, Arc<StreamAudio>>>,
}

impl CallbackShared {
    /// Drop a group call's callback routing: the chat-to-call entry and
    /// every video ssrc attributed in that chat. Runs when the call ends,
    /// whatever the native stop returned.
    pub(crate) fn forget_group(&self, chat_id: i64) {
        self.group_chat_to_call.lock_or_recover().remove(&chat_id);
        self.group_video_ssrc_to_user
            .lock_or_recover()
            .retain(|(chat, _), _| *chat != chat_id);
        self.stream_chats.lock_or_recover().remove(&chat_id);
        self.ntg_parts.lock_or_recover().forget_chat(chat_id);
        self.route_stream_audio(chat_id, None);
    }

    pub(crate) fn route_stream_audio(&self, chat_id: i64, audio: Option<Arc<StreamAudio>>) {
        self.stream_audio.rcu(|routes| {
            let mut routes = HashMap::clone(routes);
            match audio.clone() {
                Some(audio) => routes.insert(chat_id, audio),
                None => routes.remove(&chat_id),
            };
            routes
        });
    }

    /// Empty routing with no hooks installed.
    pub(crate) fn new() -> Self {
        Self {
            user_to_call: Mutex::new(HashMap::new()),
            hook: Mutex::new(None),
            transport_hook: Mutex::new(None),
            remote_video_hook: Mutex::new(None),
            remote_screen_hook: Mutex::new(None),
            remote_audio_hook: Mutex::new(None),
            peer_screencasting: Mutex::new(HashSet::new()),
            frame_seq: AtomicU64::new(0),
            group_chat_to_call: Mutex::new(HashMap::new()),
            group_video_ssrc_to_user: Mutex::new(HashMap::new()),
            stream_chats: Mutex::new(HashSet::new()),
            engine_event_hook: Mutex::new(None),
            ntg_parts: Mutex::new(super::native::NtgPartRequests::default()),
            live_frame_hook: Mutex::new(None),
            stream_audio: ArcSwap::from_pointee(HashMap::new()),
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

/// Phase C2g: retained per-group-call media state for the ntgcalls
/// group transport (keyed by TDLib group call id).
#[derive(Clone, Default)]
pub(crate) struct GroupCallMedia {
    pub(crate) chat_id: i64,
    pub(crate) role: GroupCallRole,
    /// Your microphone is muted in this group call.
    pub(crate) muted: bool,
    /// The camera being sent, by device id; `None` while it is off.
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
    /// How the connected call carries media (`ntg_get_connection_mode`).
    pub(crate) mode: GroupCallMode,
    /// Last quality the stream tile reported (stream mode only).
    pub(crate) stream_quality: Option<StreamQuality>,
    /// The output device for the call's sound; `None` is the default.
    pub(crate) speaker: Option<String>,
    pub(crate) playback_muted: bool,
    pub(crate) forced_device_playback: bool,
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
            .zip(&strings)
            .zip(&peer_tags)
            .map(
                |((server, [ipv4, ipv6, username, password]), peer_tag)| ntg_rtc_server {
                    id: server.id,
                    ipv4: ipv4.as_ptr().cast_mut(),
                    ipv6: ipv6.as_ptr().cast_mut(),
                    port: server.port,
                    username: username.as_ptr().cast_mut(),
                    password: password.as_ptr().cast_mut(),
                    turn: server.turn,
                    stun: server.stun,
                    tcp: server.tcp,
                    peer_tag: peer_tag.as_ptr().cast_mut(),
                    peer_tag_len: peer_tag.len(),
                },
            )
            .collect();
        Ok(Self {
            raw,
            _strings: strings,
            _peer_tags: peer_tags,
        })
    }
}

/// Stores `hook` in a callback slot of the ntgcalls state shared with its
/// native callbacks.
pub(crate) fn set_hook<T>(slot: &std::sync::Mutex<Option<T>>, hook: T) {
    *slot.lock_or_recover() = Some(hook);
}

#[cfg(test)]
mod callback_tests {
    use super::CallbackShared;

    fn shared() -> CallbackShared {
        CallbackShared::new()
    }

    #[test]
    fn forget_group_clears_only_that_chats_routing() {
        let shared = shared();
        shared
            .group_chat_to_call
            .lock()
            .unwrap()
            .extend([(1, 10), (2, 20)]);
        shared.group_video_ssrc_to_user.lock().unwrap().extend([
            ((1, 100), 5),
            ((1, 101), 6),
            ((2, 200), 7),
        ]);
        shared.forget_group(1);
        let chats = shared.group_chat_to_call.lock().unwrap();
        assert_eq!(chats.len(), 1);
        assert!(chats.contains_key(&2));
        let ssrcs = shared.group_video_ssrc_to_user.lock().unwrap();
        assert_eq!(ssrcs.len(), 1);
        assert_eq!(ssrcs.get(&(2, 200)), Some(&7));
    }

    #[test]
    fn forget_group_clears_the_stream_chat() {
        let shared = shared();
        shared.stream_chats.lock().unwrap().extend([1, 2]);
        shared.forget_group(1);
        let chats = shared.stream_chats.lock().unwrap();
        assert!(!chats.contains(&1));
        assert!(chats.contains(&2));
    }
}
