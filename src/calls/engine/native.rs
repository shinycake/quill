//! Native helpers for the ntgcalls adapter: FFI trampolines, frame conversion, C strings.
use super::*;
use ntgcalls_sys::{
    NTG_CONNECTION_STATE_CLOSED, NTG_CONNECTION_STATE_CONNECTED, NTG_CONNECTION_STATE_CONNECTING,
    NTG_CONNECTION_STATE_FAILED, NTG_CONNECTION_STATE_TIMEOUT, NTG_ERR_INVALID_PARAMS,
    NTG_MEDIA_SOURCE_DEVICE, NTG_STREAM_DEVICE_CAMERA, NTG_STREAM_DEVICE_MICROPHONE,
    NTG_STREAM_DEVICE_SCREEN, NTG_STREAM_MODE_CAPTURE, NTG_STREAM_MODE_PLAYBACK,
    NTG_STREAM_STATUS_ACTIVE, NTG_STREAM_STATUS_PAUSED, NTG_VIDEO_ROTATION_VIDEO_ROTATION_90,
    NTG_VIDEO_ROTATION_VIDEO_ROTATION_180, NTG_VIDEO_ROTATION_VIDEO_ROTATION_270,
    ntg_audio_description, ntg_connection_info, ntg_device_info, ntg_frame, ntg_instance,
    ntg_remote_source, ntg_stream_device, ntg_stream_mode, ntg_stream_status, ntg_video_rotation,
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

pub(crate) unsafe extern "C" fn connection_trampoline(
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
pub(crate) unsafe extern "C" fn remote_source_trampoline(
    _instance: *mut ntg_instance,
    user_id: i64,
    state: ntg_remote_source,
    user_data: *mut c_void,
) {
    if user_data.is_null() {
        return;
    }
    let shared = unsafe { &*user_data.cast::<CallbackShared>() };
    let call_id = shared
        .user_to_call
        .lock()
        .expect("ntgcalls callback map")
        .get(&user_id)
        .copied();
    // The peer's microphone (ntgcalls `p2p_call.cpp`: the MediaState
    // message's `is_muted` arrives as the Microphone source idling).
    if state.device == NTG_STREAM_DEVICE_MICROPHONE {
        let hook = shared
            .remote_audio_hook
            .lock()
            .expect("ntgcalls remote audio state hook")
            .clone();
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

/// Phase C2e: I420 planar byte size: Y (w*h) + U (cw*ch) + V (cw*ch).
fn i420_frame_size(width: u16, height: u16) -> usize {
    let (w, h) = (usize::from(width), usize::from(height));
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    w * h + 2 * cw * ch
}

/// Phase C2e: convert an I420 frame to RGBA8 with BT.601 full-range math,
/// then apply the frame rotation. Returns `None` on invalid input so we
/// never render garbage bytes.
pub(crate) fn i420_to_rgba(
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
