//! Native helpers for the ntgcalls adapter: FFI trampolines, frame routing, C strings.
use super::*;
use crate::locks::LockExt;
use ntgcalls_sys::{
    NTG_CONNECTION_STATE_CLOSED, NTG_CONNECTION_STATE_CONNECTED, NTG_CONNECTION_STATE_CONNECTING,
    NTG_CONNECTION_STATE_FAILED, NTG_CONNECTION_STATE_TIMEOUT, NTG_ERR_INVALID_PARAMS,
    NTG_MEDIA_SOURCE_DEVICE, NTG_STREAM_DEVICE_CAMERA, NTG_STREAM_DEVICE_MICROPHONE,
    NTG_STREAM_DEVICE_SCREEN, NTG_STREAM_MODE_CAPTURE, NTG_STREAM_MODE_PLAYBACK,
    NTG_STREAM_STATUS_ACTIVE, NTG_STREAM_STATUS_PAUSED, NTG_VIDEO_ROTATION_VIDEO_ROTATION_90,
    NTG_VIDEO_ROTATION_VIDEO_ROTATION_180, NTG_VIDEO_ROTATION_VIDEO_ROTATION_270,
    ntg_audio_description, ntg_connection_info, ntg_device_info, ntg_frame, ntg_instance,
    ntg_remote_source, ntg_segment_part_request, ntg_stream_device, ntg_stream_mode,
    ntg_stream_status, ntg_video_rotation,
};
use std::ffi::{CStr, CString, c_void};
use std::ptr::null_mut;
use std::sync::atomic::Ordering;

/// Phase C2e: device-metadata C string for stream sources; `None` selects
/// the default device.
pub(crate) fn native_input(input: Option<&str>) -> Result<Option<CString>, EngineError> {
    input
        .map(CString::new)
        .transpose()
        .map_err(|_| EngineError::Engine {
            op: "ntg_set_stream_sources",
            code: NTG_ERR_INVALID_PARAMS,
        })
}

pub(crate) fn audio_description(input: Option<&CString>) -> ntg_audio_description {
    ntg_audio_description {
        media_source: NTG_MEDIA_SOURCE_DEVICE,
        sample_rate: 48_000,
        channel_count: 1,
        // Device sources require JSON metadata, including system defaults.
        input: input.map_or(null_mut(), |value| value.as_ptr().cast_mut()),
        keep_open: false,
    }
}

pub(crate) unsafe extern "C" fn signaling_trampoline(
    _instance: *mut ntg_instance,
    user_id: i64,
    data: *const u8,
    len: usize,
    user_data: *mut c_void,
) {
    crate::thread_guard::ffi_boundary((), || {
        if user_data.is_null() || (data.is_null() && len != 0) {
            return;
        }
        // SAFETY: registration passes an Arc-owned `CallbackShared` pointer and
        // Drop unregisters the callback before releasing the Arc.
        let shared = unsafe { &*user_data.cast::<CallbackShared>() };
        let call_id = shared.user_to_call.lock_or_recover().get(&user_id).copied();
        let hook = shared.hook.lock_or_recover().clone();
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
    })
}

pub(crate) unsafe extern "C" fn connection_trampoline(
    _instance: *mut ntg_instance,
    user_id: i64,
    info: ntg_connection_info,
    user_data: *mut c_void,
) {
    crate::thread_guard::ffi_boundary((), || {
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
        // SAFETY: registration passes an Arc-owned `CallbackShared` pointer and
        // Drop unregisters the callback before releasing the Arc.
        let shared = unsafe { &*user_data.cast::<CallbackShared>() };
        let call_id = shared.user_to_call.lock_or_recover().get(&user_id).copied();
        let hook = shared.transport_hook.lock_or_recover().clone();
        if let (Some(call_id), Some(hook)) = (call_id, hook) {
            hook(call_id, state);
        }
    })
}

/// Phase C2j: P2P frame kinds that reach the app: the local camera
/// preview (`CAPTURE+CAMERA`), the peer's camera (`PLAYBACK+CAMERA`),
/// and the peer's screen share (`PLAYBACK+SCREEN`). Returns
/// `Some(is_screen)` for accepted kinds; audio and anything else is
/// dropped before it can be misattributed to a tile.
pub(crate) fn p2p_frame_kind(mode: ntg_stream_mode, device: ntg_stream_device) -> Option<bool> {
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
pub(crate) unsafe extern "C" fn frames_trampoline(
    _instance: *mut ntg_instance,
    user_id: i64,
    mode: ntg_stream_mode,
    device: ntg_stream_device,
    frames: *const ntg_frame,
    frames_len: usize,
    user_data: *mut c_void,
) {
    crate::thread_guard::ffi_boundary((), || {
        if user_data.is_null() || frames.is_null() || frames_len == 0 {
            return;
        }
        // SAFETY: registration passes an Arc-owned `CallbackShared` pointer and
        // Drop unregisters the callback before releasing the Arc; ntgcalls
        // guarantees the frames array for the duration of this call.
        let shared = unsafe { &*user_data.cast::<CallbackShared>() };
        // Sound reaches this callback only from a call Quill plays (a
        // stream or a 1:1 call; external playback writes
        // PLAYBACK+MICROPHONE); every other call's sound stays on
        // ntgcalls' device output.
        if mode == NTG_STREAM_MODE_PLAYBACK && device == NTG_STREAM_DEVICE_MICROPHONE {
            if let Some(audio) = shared.stream_audio.load().get(&user_id) {
                // SAFETY: frames is non-null with frames_len entries
                // (checked above); ntgcalls keeps them for this call.
                let batch = unsafe { std::slice::from_raw_parts(frames, frames_len) };
                audio.push_frames(batch.iter().filter_map(frame_bytes));
            }
            return;
        }
        // Phase C2g: group-chat routing is checked first; the native callback
        // key is the chat id for group calls and the user id for P2P.
        let group_call = shared
            .group_chat_to_call
            .lock_or_recover()
            .get(&user_id)
            .copied();
        let key = if let Some(group_call_id) = group_call {
            // Screen frames arrive as PLAYBACK+SCREEN; camera as
            // PLAYBACK+CAMERA. Slice calls-group-self-tile: the local
            // camera preview arrives as CAPTURE+CAMERA keyed by the chat id
            // (ntgcalls emits capture frames per connection — v3.0.0
            // ntgcalls.cpp:126 wires on_frames for group connections too,
            // and stream_manager emits device captures to the callback);
            // it renders as the self tile instead of being dropped. A
            // stream-mode chat has no ssrc map: its pictures go to the
            // chat's stream feed.
            let is_stream_chat = shared.stream_chats.lock_or_recover().contains(&user_id);
            let target = group_video_target(user_id, is_stream_chat, mode, device, || {
                // SAFETY: frames_len > 0 was checked above.
                let ssrc = unsafe { &*frames.add(frames_len - 1) }.ssrc as u32;
                shared
                    .group_video_ssrc_to_user
                    .lock_or_recover()
                    .get(&(user_id, ssrc))
                    .copied()
            });
            match target {
                Some(GroupVideoTarget::Local) => LiveKey::Local {
                    call_id: group_call_id,
                },
                Some(GroupVideoTarget::Participant { user_id, screen }) => LiveKey::Video {
                    call_id: group_call_id,
                    user_id,
                    screen,
                },
                Some(GroupVideoTarget::Stream { chat_id }) => LiveKey::Stream { chat_id },
                // Audio, the screen-share capture preview and unknown
                // ssrcs have no tile: drop rather than misattribute.
                None => return,
            }
        } else {
            let is_local = mode == NTG_STREAM_MODE_CAPTURE && device == NTG_STREAM_DEVICE_CAMERA;
            // Phase C2j: the peer's screen-share stream is accepted into its
            // own slot instead of being dropped; audio frames still return.
            let Some(is_screen) = p2p_frame_kind(mode, device) else {
                return;
            };
            // The peer's shared screen comes over the camera track.
            let is_screen = is_screen
                || (!is_local
                    && shared
                        .peer_screencasting
                        .lock_or_recover()
                        .contains(&user_id));
            let Some(call_id) = shared.user_to_call.lock_or_recover().get(&user_id).copied() else {
                return;
            };
            // A 1:1 call's picture goes to its live feed like every other.
            LiveKey::Call {
                call_id,
                local: is_local,
                screen: is_screen,
            }
        };
        // SAFETY: frames is non-null with frames_len > 0 entries (checked at
        // the top); ntgcalls keeps the array alive for this call.
        let frame = unsafe { &*frames.add(frames_len - 1) };
        let bytes = if frame.data.is_null() || frame.data_len == 0 {
            Vec::new()
        } else {
            // SAFETY: ntgcalls guarantees the frame byte span for this call;
            // copy it before returning to the worker thread.
            unsafe { std::slice::from_raw_parts(frame.data, frame.data_len) }.to_vec()
        };
        let rotation = frame.frame_data.rotation;
        let (raw_width, raw_height) = (frame.frame_data.width, frame.frame_data.height);
        let seq = shared.frame_seq.fetch_add(1, Ordering::Relaxed);
        // The planes go over as they came: one copy out of ntgcalls'
        // buffer, no conversion on this thread. The UI turns and converts
        // only the frame it draws.
        let Some(frame) = LiveFrame::new(
            seq,
            raw_width,
            raw_height,
            live_rotation(rotation),
            bytes,
            std::time::Instant::now(),
        ) else {
            return;
        };
        let hook = shared.live_frame_hook.lock_or_recover().clone();
        if let Some(hook) = hook {
            hook(key, frame);
        }
    })
}

fn frame_bytes(frame: &ntg_frame) -> Option<&[u8]> {
    if frame.data.is_null() || frame.data_len == 0 {
        return None;
    }
    // SAFETY: ntgcalls guarantees the frame's byte span for the duration
    // of the frames callback, which outlives this borrow.
    Some(unsafe { std::slice::from_raw_parts(frame.data, frame.data_len) })
}

/// ntgcalls' rotation flag as a live feed carries it.
pub(crate) fn live_rotation(rotation: ntg_video_rotation) -> LiveRotation {
    match rotation {
        NTG_VIDEO_ROTATION_VIDEO_ROTATION_90 => LiveRotation::R90,
        NTG_VIDEO_ROTATION_VIDEO_ROTATION_180 => LiveRotation::R180,
        NTG_VIDEO_ROTATION_VIDEO_ROTATION_270 => LiveRotation::R270,
        _ => LiveRotation::None,
    }
}

/// Stream mode (RTMP only): ntgcalls needs the stream's current time. The
/// request is handed to the connect side and answered later with
/// `ntg_send_broadcast_timestamp`; nothing blocks this ntgcalls thread.
pub(crate) unsafe extern "C" fn broadcast_timestamp_trampoline(
    _instance: *mut ntg_instance,
    chat_id: i64,
    user_data: *mut c_void,
) {
    crate::thread_guard::ffi_boundary((), || {
        if user_data.is_null() {
            return;
        }
        // SAFETY: registration passes an Arc-owned `CallbackShared` pointer and
        // Drop unregisters the callback before releasing the Arc.
        let shared = unsafe { &*user_data.cast::<CallbackShared>() };
        emit_engine_event(shared, EngineEvent::BroadcastTimestampRequested { chat_id });
    })
}

/// Stream mode: ntgcalls needs a segment part. The request struct is
/// passed by value and owned by ntgcalls; it is copied into an owned
/// `BroadcastPartRequest` and nothing is freed here.
pub(crate) unsafe extern "C" fn broadcast_part_trampoline(
    _instance: *mut ntg_instance,
    chat_id: i64,
    request: ntg_segment_part_request,
    user_data: *mut c_void,
) {
    crate::thread_guard::ffi_boundary((), || {
        if user_data.is_null() {
            return;
        }
        // SAFETY: registration passes an Arc-owned `CallbackShared` pointer and
        // Drop unregisters the callback before releasing the Arc.
        let shared = unsafe { &*user_data.cast::<CallbackShared>() };
        emit_engine_event(
            shared,
            EngineEvent::BroadcastPartRequested {
                chat_id,
                request: BroadcastPartRequest::from_native(&request),
            },
        );
    })
}

/// Hands one engine event to the connect side's hook (which only queues
/// it). Events for chats with no group call are dropped.
fn emit_engine_event(shared: &CallbackShared, event: EngineEvent) {
    let chat_id = match &event {
        EngineEvent::BroadcastTimestampRequested { chat_id }
        | EngineEvent::BroadcastPartRequested { chat_id, .. } => *chat_id,
        // ntgcalls reports no speaker levels.
        EngineEvent::GroupAudioLevels { .. } => return,
    };
    if !shared
        .group_chat_to_call
        .lock_or_recover()
        .contains_key(&chat_id)
    {
        return;
    }
    let hook = shared.engine_event_hook.lock_or_recover().clone();
    if let Some(hook) = hook {
        hook(event);
    }
}

/// Phase C2e: peer camera on/off/paused state driven by the MediaState
/// signaling message. The state is passed by value.
pub(crate) unsafe extern "C" fn remote_source_trampoline(
    _instance: *mut ntg_instance,
    user_id: i64,
    state: ntg_remote_source,
    user_data: *mut c_void,
) {
    crate::thread_guard::ffi_boundary((), || {
        if user_data.is_null() {
            return;
        }
        // SAFETY: registration passes an Arc-owned `CallbackShared` pointer and
        // Drop unregisters the callback before releasing the Arc.
        let shared = unsafe { &*user_data.cast::<CallbackShared>() };
        let call_id = shared.user_to_call.lock_or_recover().get(&user_id).copied();
        // The peer's microphone (ntgcalls `p2p_call.cpp`: the MediaState
        // message's `is_muted` arrives as the Microphone source idling).
        if state.device == NTG_STREAM_DEVICE_MICROPHONE {
            let hook = shared.remote_audio_hook.lock_or_recover().clone();
            if let (Some(call_id), Some(hook)) = (call_id, hook) {
                hook(call_id, remote_audio_muted_from(state.state));
            }
            return;
        }
        if state.device != NTG_STREAM_DEVICE_CAMERA && state.device != NTG_STREAM_DEVICE_SCREEN {
            return;
        }
        // Phase C2j: camera and screen-share states ride separate hooks so the
        // driver can clear retained screen frames without touching the camera
        // state. Audio and other devices never reach this path.
        let video = remote_video_state_from(state.state);
        if state.device == NTG_STREAM_DEVICE_SCREEN {
            let mut casting = shared.peer_screencasting.lock_or_recover();
            if video == RemoteVideoState::Inactive {
                casting.remove(&user_id);
            } else {
                casting.insert(user_id);
            }
        }
        let hook = if state.device == NTG_STREAM_DEVICE_SCREEN {
            shared.remote_screen_hook.lock_or_recover().clone()
        } else {
            shared.remote_video_hook.lock_or_recover().clone()
        };
        if let (Some(call_id), Some(hook)) = (call_id, hook) {
            hook(call_id, video);
        }
    })
}

/// The peer's microphone is off unless its source is active.
pub(crate) fn remote_audio_muted_from(status: ntg_stream_status) -> bool {
    status != NTG_STREAM_STATUS_ACTIVE
}

/// Phase C2e: map an ntgcalls stream status to the app-level camera state,
/// mirroring Telegram X's `VideoState` (Active=2, Paused=1, Inactive=0).
pub(crate) fn remote_video_state_from(status: ntg_stream_status) -> RemoteVideoState {
    match status {
        NTG_STREAM_STATUS_ACTIVE => RemoteVideoState::Active,
        NTG_STREAM_STATUS_PAUSED => RemoteVideoState::Paused,
        _ => RemoteVideoState::Inactive,
    }
}

/// The camera a call sends from: the one picked in the call menu while
/// the engine still lists it, else the first camera listed. `None` when
/// there is no camera (ntgcalls opens only a listed device, by its id).
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

pub(crate) fn c_strings(values: &[String]) -> Result<Vec<CString>, EngineError> {
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

pub(crate) fn native_string(value: &str) -> Result<CString, EngineError> {
    CString::new(value).map_err(|_| EngineError::Engine {
        op: "ntg_connect_p2p",
        code: NTG_ERR_INVALID_PARAMS,
    })
}

pub(crate) fn c_string_pointer_array(
    values: *mut *mut std::ffi::c_char,
    len: usize,
) -> Vec<String> {
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

pub(crate) fn append_devices(
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
