//! Raw FFI bindings for ntgcalls v3.0.2 — Quill's vendored call media engine.
//!
//! Quill's call UI keeps its honest "no audio yet" stance until Phase C2b
//! wires this engine up; this crate is the un-wired seam.
//!
//! **License (LGPLv3 sidecar).** The native library `libntgcalls.so`
//! (pytgcalls/ntgcalls v3.0.2, LGPL-3.0) is **never linked into the Quill
//! binary** — it is procured unmodified by `scripts/vendor-ntgcalls.sh`
//! (checksum-pinned, git-ignored) and loaded at runtime via `dlopen` by
//! [`Loader`]. See `THIRD_PARTY.md` for attribution and the LGPL source
//! offer.
//!
//! Every declaration below was verified verbatim against the vendored
//! `include/ntgcalls.h` (generated from `schema/ntgcalls.ntl`), not against
//! any third-party binding crate. There is no `ntg_destroy` in the header:
//! instance lifecycle is `ntg_instance_create` / `ntg_instance_destroy`.
//!
//! No safe wrappers live here (C2b builds them), and no test does any
//! network or audio I/O.

#![allow(non_camel_case_types)]

use std::ffi::{OsStr, c_char, c_int, c_void};
use std::fmt;
use std::path::PathBuf;

/// Native sidecar name, using Rust's platform library naming convention.
pub fn library_filename() -> String {
    format!(
        "{}ntgcalls{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    )
}

// ---------------------------------------------------------------------------
// Enums (bindgen-style: c_int aliases + constants, matching the C header)
// ---------------------------------------------------------------------------

pub type ntg_result = c_int;
pub const NTG_OK: ntg_result = 0;
pub const NTG_ERR_UNKNOWN: ntg_result = -1;
pub const NTG_ERR_NULL_POINTER: ntg_result = -2;
pub const NTG_ERR_RTC: ntg_result = -100;
pub const NTG_ERR_SDP_PARSE: ntg_result = -101;
pub const NTG_ERR_TRANSPORT_PARSE: ntg_result = -102;
pub const NTG_ERR_RTMP_STREAMING_UNSUPPORTED: ntg_result = -103;
pub const NTG_ERR_CONNECTION: ntg_result = -200;
pub const NTG_ERR_CONNECTION_NOT_FOUND: ntg_result = -201;
pub const NTG_ERR_CONNECTION_ERROR: ntg_result = -202;
pub const NTG_ERR_CRYPTO_ERROR: ntg_result = -203;
pub const NTG_ERR_TELEGRAM_SERVER_ERROR: ntg_result = -204;
pub const NTG_ERR_RTC_CONNECTION_NEEDED: ntg_result = -205;
pub const NTG_ERR_INVALID_PARAMS: ntg_result = -206;
pub const NTG_ERR_SIGNALING: ntg_result = -300;
pub const NTG_ERR_SIGNALING_ERROR: ntg_result = -301;
pub const NTG_ERR_SIGNALING_UNSUPPORTED: ntg_result = -302;
pub const NTG_ERR_MEDIA: ntg_result = -400;
pub const NTG_ERR_FILE_ERROR: ntg_result = -401;
pub const NTG_ERR_FFMPEG_ERROR: ntg_result = -402;
pub const NTG_ERR_SHELL_ERROR: ntg_result = -403;
pub const NTG_ERR_MEDIA_DEVICE_ERROR: ntg_result = -404;

pub type ntg_log_level = c_int;
pub const NTG_LOG_DEBUG: ntg_log_level = 1;
pub const NTG_LOG_INFO: ntg_log_level = 2;
pub const NTG_LOG_WARNING: ntg_log_level = 4;
pub const NTG_LOG_ERROR: ntg_log_level = 8;

pub type ntg_log_source = c_int;
pub const NTG_LOG_SOURCE_WEBRTC: ntg_log_source = 1;
pub const NTG_LOG_SOURCE_SELF: ntg_log_source = 2;

pub type ntg_media_source = c_int;
pub const NTG_MEDIA_SOURCE_UNKNOWN: ntg_media_source = 0;
pub const NTG_MEDIA_SOURCE_FILE: ntg_media_source = 1;
pub const NTG_MEDIA_SOURCE_SHELL: ntg_media_source = 2;
pub const NTG_MEDIA_SOURCE_FFMPEG: ntg_media_source = 3;
pub const NTG_MEDIA_SOURCE_DEVICE: ntg_media_source = 4;
pub const NTG_MEDIA_SOURCE_DESKTOP: ntg_media_source = 5;
pub const NTG_MEDIA_SOURCE_EXTERNAL: ntg_media_source = 6;

pub type ntg_stream_type = c_int;
pub const NTG_STREAM_TYPE_AUDIO: ntg_stream_type = 0;
pub const NTG_STREAM_TYPE_VIDEO: ntg_stream_type = 1;

pub type ntg_stream_status = c_int;
pub const NTG_STREAM_STATUS_ACTIVE: ntg_stream_status = 0;
pub const NTG_STREAM_STATUS_PAUSED: ntg_stream_status = 1;
pub const NTG_STREAM_STATUS_IDLING: ntg_stream_status = 2;

pub type ntg_stream_mode = c_int;
pub const NTG_STREAM_MODE_CAPTURE: ntg_stream_mode = 0;
pub const NTG_STREAM_MODE_PLAYBACK: ntg_stream_mode = 1;

pub type ntg_stream_device = c_int;
pub const NTG_STREAM_DEVICE_MICROPHONE: ntg_stream_device = 0;
pub const NTG_STREAM_DEVICE_SPEAKER: ntg_stream_device = 1;
pub const NTG_STREAM_DEVICE_CAMERA: ntg_stream_device = 2;
pub const NTG_STREAM_DEVICE_SCREEN: ntg_stream_device = 3;

pub type ntg_connection_state = c_int;
pub const NTG_CONNECTION_STATE_CONNECTING: ntg_connection_state = 0;
pub const NTG_CONNECTION_STATE_CONNECTED: ntg_connection_state = 1;
pub const NTG_CONNECTION_STATE_FAILED: ntg_connection_state = 2;
pub const NTG_CONNECTION_STATE_TIMEOUT: ntg_connection_state = 3;
pub const NTG_CONNECTION_STATE_CLOSED: ntg_connection_state = 4;

pub type ntg_connection_kind = c_int;
pub const NTG_CONNECTION_KIND_NORMAL: ntg_connection_kind = 0;
pub const NTG_CONNECTION_KIND_PRESENTATION: ntg_connection_kind = 1;

pub type ntg_call_type = c_int;
pub const NTG_CALL_TYPE_GROUP: ntg_call_type = 0;
pub const NTG_CALL_TYPE_OUTGOING: ntg_call_type = 1;
pub const NTG_CALL_TYPE_INCOMING: ntg_call_type = 2;
pub const NTG_CALL_TYPE_P2P: ntg_call_type = 3;
pub const NTG_CALL_TYPE_CONFERENCE: ntg_call_type = 4;

pub type ntg_media_segment_quality = c_int;
pub const NTG_MEDIA_SEGMENT_QUALITY_NONE: ntg_media_segment_quality = 0;
pub const NTG_MEDIA_SEGMENT_QUALITY_THUMBNAIL: ntg_media_segment_quality = 1;
pub const NTG_MEDIA_SEGMENT_QUALITY_MEDIUM: ntg_media_segment_quality = 2;
pub const NTG_MEDIA_SEGMENT_QUALITY_FULL: ntg_media_segment_quality = 3;

pub type ntg_media_segment_part_status = c_int;
pub const NTG_MEDIA_SEGMENT_PART_STATUS_NOT_READY: ntg_media_segment_part_status = 0;
pub const NTG_MEDIA_SEGMENT_PART_STATUS_RESYNC_NEEDED: ntg_media_segment_part_status = 1;
pub const NTG_MEDIA_SEGMENT_PART_STATUS_DOWNLOADING: ntg_media_segment_part_status = 2;
pub const NTG_MEDIA_SEGMENT_PART_STATUS_SUCCESS: ntg_media_segment_part_status = 3;

pub type ntg_connection_mode = c_int;
pub const NTG_CONNECTION_MODE_NONE: ntg_connection_mode = 0;
pub const NTG_CONNECTION_MODE_RTC: ntg_connection_mode = 1;
pub const NTG_CONNECTION_MODE_STREAM: ntg_connection_mode = 2;
pub const NTG_CONNECTION_MODE_RTMP: ntg_connection_mode = 3;

pub type ntg_video_rotation = c_int;
pub const NTG_VIDEO_ROTATION_VIDEO_ROTATION_0: ntg_video_rotation = 0;
pub const NTG_VIDEO_ROTATION_VIDEO_ROTATION_90: ntg_video_rotation = 1;
pub const NTG_VIDEO_ROTATION_VIDEO_ROTATION_180: ntg_video_rotation = 2;
pub const NTG_VIDEO_ROTATION_VIDEO_ROTATION_270: ntg_video_rotation = 3;

// ---------------------------------------------------------------------------
// Structs
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ntg_bytes {
    pub data: *mut u8,
    pub len: usize,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ntg_log_message {
    pub level: ntg_log_level,
    pub source: ntg_log_source,
    pub file: *const c_char,
    pub line: u32,
    pub message: *const c_char,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ntg_connection_info {
    pub state: ntg_connection_state,
    pub kind: ntg_connection_kind,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ntg_remote_source {
    pub ssrc: u32,
    pub state: ntg_stream_status,
    pub device: ntg_stream_device,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ntg_subchain_request {
    pub subchain: i32,
    pub height: i32,
    pub limit: i32,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ntg_participants_request {
    pub ssrcs: *mut u32,
    pub ssrcs_len: usize,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ntg_audio_description {
    pub media_source: ntg_media_source,
    pub sample_rate: u32,
    pub channel_count: u8,
    pub input: *mut c_char,
    pub keep_open: bool,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ntg_call_info {
    pub playback: ntg_stream_status,
    pub capture: ntg_stream_status,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ntg_device_info {
    pub name: *mut c_char,
    pub metadata: *mut c_char,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ntg_media_devices {
    pub microphone: *mut ntg_device_info,
    pub microphone_len: usize,
    pub speaker: *mut ntg_device_info,
    pub speaker_len: usize,
    pub camera: *mut ntg_device_info,
    pub camera_len: usize,
    pub screen: *mut ntg_device_info,
    pub screen_len: usize,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ntg_media_state {
    pub muted: bool,
    pub video_paused: bool,
    pub video_stopped: bool,
    pub presentation_paused: bool,
    pub presentation_stopped: bool,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ntg_video_description {
    pub media_source: ntg_media_source,
    pub width: i16,
    pub height: i16,
    pub fps: u8,
    pub input: *mut c_char,
    pub keep_open: bool,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ntg_media_description {
    pub microphone: *mut ntg_audio_description,
    pub speaker: *mut ntg_audio_description,
    pub camera: *mut ntg_video_description,
    pub screen: *mut ntg_video_description,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ntg_frame_data {
    pub absolute_capture_timestamp_ms: i64,
    pub rotation: ntg_video_rotation,
    pub width: u16,
    pub height: u16,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ntg_ssrc_group {
    pub semantics: *mut c_char,
    pub ssrcs: *mut u32,
    pub ssrcs_len: usize,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ntg_frame {
    pub ssrc: i64,
    pub data: *mut u8,
    pub data_len: usize,
    pub frame_data: ntg_frame_data,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ntg_segment_part_request {
    pub segment_id: i64,
    pub part_id: i32,
    pub limit: i32,
    pub timestamp: i64,
    pub quality_update: bool,
    pub channel_id: i32,
    pub quality: ntg_media_segment_quality,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ntg_ssrc_mapping {
    pub user_id: i64,
    pub ssrc: i32,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ntg_auth_params {
    pub key_fingerprint: i64,
    pub g_a_or_b: *mut u8,
    pub g_a_or_b_len: usize,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ntg_conference_join_params {
    pub payload: *mut c_char,
    pub public_key: *mut u8,
    pub public_key_len: usize,
    pub block: *mut u8,
    pub block_len: usize,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ntg_dh_config {
    pub g: i32,
    pub p: *mut u8,
    pub p_len: usize,
    pub random: *mut u8,
    pub random_len: usize,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ntg_protocol {
    pub min_layer: i32,
    pub max_layer: i32,
    pub udp_p2p: bool,
    pub udp_reflector: bool,
    pub library_versions: *mut *mut c_char,
    pub library_versions_len: usize,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ntg_rtc_server {
    pub id: u64,
    pub ipv4: *mut c_char,
    pub ipv6: *mut c_char,
    pub port: u16,
    pub username: *mut c_char,
    pub password: *mut c_char,
    pub turn: bool,
    pub stun: bool,
    pub tcp: bool,
    pub peer_tag: *mut u8,
    pub peer_tag_len: usize,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ntg_call_info_entry {
    pub key: i64,
    pub value: ntg_call_info,
}

/// Opaque engine instance handle.
#[repr(C)]
pub struct ntg_instance {
    _unused: [u8; 0],
}

// ---------------------------------------------------------------------------
// Callback typedefs
// ---------------------------------------------------------------------------

pub type ntg_log_cb = Option<unsafe extern "C" fn(ntg_log_message, *mut c_void)>;
pub type ntg_upgrade_callback_cb =
    Option<unsafe extern "C" fn(*mut ntg_instance, i64, ntg_media_state, *mut c_void)>;
pub type ntg_stream_end_callback_cb = Option<
    unsafe extern "C" fn(*mut ntg_instance, i64, ntg_stream_type, ntg_stream_device, *mut c_void),
>;
pub type ntg_connection_change_callback_cb =
    Option<unsafe extern "C" fn(*mut ntg_instance, i64, ntg_connection_info, *mut c_void)>;
pub type ntg_frames_callback_cb = Option<
    unsafe extern "C" fn(
        *mut ntg_instance,
        i64,
        ntg_stream_mode,
        ntg_stream_device,
        *const ntg_frame,
        usize,
        *mut c_void,
    ),
>;
pub type ntg_signaling_data_callback_cb =
    Option<unsafe extern "C" fn(*mut ntg_instance, i64, *const u8, usize, *mut c_void)>;
pub type ntg_remote_source_change_callback_cb =
    Option<unsafe extern "C" fn(*mut ntg_instance, i64, ntg_remote_source, *mut c_void)>;
pub type ntg_request_broadcast_part_callback_cb =
    Option<unsafe extern "C" fn(*mut ntg_instance, i64, ntg_segment_part_request, *mut c_void)>;
pub type ntg_request_broadcast_timestamp_callback_cb =
    Option<unsafe extern "C" fn(*mut ntg_instance, i64, *mut c_void)>;
pub type ntg_request_participants_callback_cb =
    Option<unsafe extern "C" fn(*mut ntg_instance, i64, ntg_participants_request, *mut c_void)>;
pub type ntg_outbound_block_callback_cb =
    Option<unsafe extern "C" fn(*mut ntg_instance, i64, *const u8, usize, *mut c_void)>;
pub type ntg_subchain_request_callback_cb =
    Option<unsafe extern "C" fn(*mut ntg_instance, i64, ntg_subchain_request, *mut c_void)>;
pub type ntg_update_emojis_callback_cb =
    Option<unsafe extern "C" fn(*mut ntg_instance, i64, *const c_char, *mut c_void)>;

// ---------------------------------------------------------------------------
// Raw extern "C" declarations (verified against vendor/ntgcalls/include/ntgcalls.h)
// ---------------------------------------------------------------------------

unsafe extern "C" {
    pub fn ntg_set_log_callback(callback: ntg_log_cb, user_data: *mut c_void);
    pub fn ntg_get_version() -> *const c_char;
    pub fn ntg_last_error() -> *const c_char;

    pub fn ntg_instance_create() -> *mut ntg_instance;
    pub fn ntg_instance_destroy(handle: *mut ntg_instance);

    pub fn ntg_connection_info_free(value: *mut ntg_connection_info);
    pub fn ntg_remote_source_free(value: *mut ntg_remote_source);
    pub fn ntg_subchain_request_free(value: *mut ntg_subchain_request);
    pub fn ntg_audio_description_free(value: *mut ntg_audio_description);
    pub fn ntg_call_info_free(value: *mut ntg_call_info);
    pub fn ntg_device_info_free(value: *mut ntg_device_info);
    pub fn ntg_media_devices_free(value: *mut ntg_media_devices);
    pub fn ntg_media_state_free(value: *mut ntg_media_state);
    pub fn ntg_video_description_free(value: *mut ntg_video_description);
    pub fn ntg_media_description_free(value: *mut ntg_media_description);
    pub fn ntg_frame_data_free(value: *mut ntg_frame_data);
    pub fn ntg_ssrc_group_free(value: *mut ntg_ssrc_group);
    pub fn ntg_frame_free(value: *mut ntg_frame);
    pub fn ntg_segment_part_request_free(value: *mut ntg_segment_part_request);
    pub fn ntg_participants_request_free(value: *mut ntg_participants_request);
    pub fn ntg_ssrc_mapping_free(value: *mut ntg_ssrc_mapping);
    pub fn ntg_auth_params_free(value: *mut ntg_auth_params);
    pub fn ntg_conference_join_params_free(value: *mut ntg_conference_join_params);
    pub fn ntg_dh_config_free(value: *mut ntg_dh_config);
    pub fn ntg_protocol_free(value: *mut ntg_protocol);
    pub fn ntg_rtc_server_free(value: *mut ntg_rtc_server);
    pub fn ntg_call_info_entry_free(entries: *mut ntg_call_info_entry, len: usize);
    pub fn ntg_string_free(value: *mut c_char);
    pub fn ntg_bytes_free(value: *mut c_void);

    pub fn ntg_create_p2p_call(handle: *mut ntg_instance, user_id: i64) -> ntg_result;
    pub fn ntg_init_exchange(
        handle: *mut ntg_instance,
        user_id: i64,
        dh_config: ntg_dh_config,
        ga_hash: *const u8,
        ga_hash_len: usize,
        out: *mut *mut u8,
        out_len: *mut usize,
    ) -> ntg_result;
    pub fn ntg_exchange_keys(
        handle: *mut ntg_instance,
        user_id: i64,
        g_a_or_b: *const u8,
        g_a_or_b_len: usize,
        fingerprint: i64,
        out: *mut ntg_auth_params,
    ) -> ntg_result;
    pub fn ntg_skip_exchange(
        handle: *mut ntg_instance,
        user_id: i64,
        encryption_key: *const u8,
        encryption_key_len: usize,
        is_outgoing: bool,
    ) -> ntg_result;
    pub fn ntg_connect_p2p(
        handle: *mut ntg_instance,
        user_id: i64,
        servers: *const ntg_rtc_server,
        servers_len: usize,
        versions: *const *const c_char,
        versions_len: usize,
        p2p_allowed: bool,
        custom_parameters: *const c_char,
    ) -> ntg_result;
    pub fn ntg_create_call(
        handle: *mut ntg_instance,
        chat_id: i64,
        out: *mut *mut c_char,
    ) -> ntg_result;
    pub fn ntg_init_presentation(
        handle: *mut ntg_instance,
        chat_id: i64,
        out: *mut *mut c_char,
    ) -> ntg_result;
    pub fn ntg_init_conference(
        handle: *mut ntg_instance,
        chat_id: i64,
        user_id: i64,
        last_block: *const u8,
        last_block_len: usize,
        out: *mut ntg_conference_join_params,
    ) -> ntg_result;
    pub fn ntg_connect(
        handle: *mut ntg_instance,
        chat_id: i64,
        params: *const c_char,
        is_presentation: bool,
    ) -> ntg_result;
    pub fn ntg_add_incoming_video(
        handle: *mut ntg_instance,
        chat_id: i64,
        user_id: i64,
        endpoint: *const c_char,
        ssrc_groups: *const ntg_ssrc_group,
        ssrc_groups_len: usize,
        out: *mut u32,
    ) -> ntg_result;
    pub fn ntg_remove_incoming_video(
        handle: *mut ntg_instance,
        chat_id: i64,
        endpoint: *const c_char,
        out: *mut bool,
    ) -> ntg_result;
    pub fn ntg_set_stream_sources(
        handle: *mut ntg_instance,
        chat_id: i64,
        mode: ntg_stream_mode,
        media: ntg_media_description,
    ) -> ntg_result;
    pub fn ntg_pause(handle: *mut ntg_instance, chat_id: i64, out: *mut bool) -> ntg_result;
    pub fn ntg_resume(handle: *mut ntg_instance, chat_id: i64, out: *mut bool) -> ntg_result;
    pub fn ntg_mute(handle: *mut ntg_instance, chat_id: i64, out: *mut bool) -> ntg_result;
    pub fn ntg_unmute(handle: *mut ntg_instance, chat_id: i64, out: *mut bool) -> ntg_result;
    pub fn ntg_stop(handle: *mut ntg_instance, chat_id: i64) -> ntg_result;
    pub fn ntg_stop_presentation(handle: *mut ntg_instance, chat_id: i64) -> ntg_result;
    pub fn ntg_get_emojis_fingerprint(
        handle: *mut ntg_instance,
        chat_id: i64,
        out: *mut *mut c_char,
    ) -> ntg_result;
    pub fn ntg_time(
        handle: *mut ntg_instance,
        chat_id: i64,
        mode: ntg_stream_mode,
        out: *mut u64,
    ) -> ntg_result;
    pub fn ntg_get_state(
        handle: *mut ntg_instance,
        chat_id: i64,
        out: *mut ntg_media_state,
    ) -> ntg_result;
    pub fn ntg_get_call_type(
        handle: *mut ntg_instance,
        chat_id: i64,
        out: *mut ntg_call_type,
    ) -> ntg_result;
    pub fn ntg_get_connection_mode(
        handle: *mut ntg_instance,
        chat_id: i64,
        out: *mut ntg_connection_mode,
    ) -> ntg_result;
    pub fn ntg_cpu_usage(out: *mut f64) -> ntg_result;
    pub fn ntg_ping(out: *mut *mut c_char) -> ntg_result;
    pub fn ntg_get_media_devices(out: *mut ntg_media_devices) -> ntg_result;
    pub fn ntg_get_protocol(out: *mut ntg_protocol) -> ntg_result;
    pub fn ntg_enable_glib_loop(enable: bool) -> ntg_result;
    pub fn ntg_send_broadcast_timestamp(
        handle: *mut ntg_instance,
        chat_id: i64,
        timestamp: i64,
    ) -> ntg_result;
    pub fn ntg_send_broadcast_part(
        handle: *mut ntg_instance,
        chat_id: i64,
        segment_id: i64,
        part_id: i32,
        status: ntg_media_segment_part_status,
        quality_update: bool,
        data: *const u8,
        data_len: usize,
    ) -> ntg_result;
    pub fn ntg_send_signaling_data(
        handle: *mut ntg_instance,
        chat_id: i64,
        msg_key: *const u8,
        msg_key_len: usize,
    ) -> ntg_result;
    pub fn ntg_send_external_frame(
        handle: *mut ntg_instance,
        chat_id: i64,
        device: ntg_stream_device,
        data: *const u8,
        data_len: usize,
        frame_data: ntg_frame_data,
    ) -> ntg_result;
    pub fn ntg_update_audio_ssrc_mappings(
        handle: *mut ntg_instance,
        chat_id: i64,
        ssrc_groups: *const ntg_ssrc_mapping,
        ssrc_groups_len: usize,
    ) -> ntg_result;
    pub fn ntg_apply_blocks(
        handle: *mut ntg_instance,
        chat_id: i64,
        subchain: i32,
        next_offset: i32,
        blocks: *const ntg_bytes,
        blocks_len: usize,
        from_short_poll: bool,
    ) -> ntg_result;
    pub fn ntg_finish_subchain_request(
        handle: *mut ntg_instance,
        chat_id: i64,
        subchain: i32,
    ) -> ntg_result;
    pub fn ntg_calls(
        handle: *mut ntg_instance,
        out: *mut *mut ntg_call_info_entry,
        out_len: *mut usize,
    ) -> ntg_result;

    pub fn ntg_on_upgrade_callback(
        handle: *mut ntg_instance,
        callback: ntg_upgrade_callback_cb,
        user_data: *mut c_void,
    ) -> ntg_result;
    pub fn ntg_on_stream_end_callback(
        handle: *mut ntg_instance,
        callback: ntg_stream_end_callback_cb,
        user_data: *mut c_void,
    ) -> ntg_result;
    pub fn ntg_on_connection_change_callback(
        handle: *mut ntg_instance,
        callback: ntg_connection_change_callback_cb,
        user_data: *mut c_void,
    ) -> ntg_result;
    pub fn ntg_on_frames_callback(
        handle: *mut ntg_instance,
        callback: ntg_frames_callback_cb,
        user_data: *mut c_void,
    ) -> ntg_result;
    pub fn ntg_on_signaling_data_callback(
        handle: *mut ntg_instance,
        callback: ntg_signaling_data_callback_cb,
        user_data: *mut c_void,
    ) -> ntg_result;
    pub fn ntg_on_remote_source_change_callback(
        handle: *mut ntg_instance,
        callback: ntg_remote_source_change_callback_cb,
        user_data: *mut c_void,
    ) -> ntg_result;
    pub fn ntg_on_request_broadcast_part_callback(
        handle: *mut ntg_instance,
        callback: ntg_request_broadcast_part_callback_cb,
        user_data: *mut c_void,
    ) -> ntg_result;
    pub fn ntg_on_request_broadcast_timestamp_callback(
        handle: *mut ntg_instance,
        callback: ntg_request_broadcast_timestamp_callback_cb,
        user_data: *mut c_void,
    ) -> ntg_result;
    pub fn ntg_on_request_participants_callback(
        handle: *mut ntg_instance,
        callback: ntg_request_participants_callback_cb,
        user_data: *mut c_void,
    ) -> ntg_result;
    pub fn ntg_on_outbound_block_callback(
        handle: *mut ntg_instance,
        callback: ntg_outbound_block_callback_cb,
        user_data: *mut c_void,
    ) -> ntg_result;
    pub fn ntg_on_subchain_request_callback(
        handle: *mut ntg_instance,
        callback: ntg_subchain_request_callback_cb,
        user_data: *mut c_void,
    ) -> ntg_result;
    pub fn ntg_on_update_emojis_callback(
        handle: *mut ntg_instance,
        callback: ntg_update_emojis_callback_cb,
        user_data: *mut c_void,
    ) -> ntg_result;
}

// ---------------------------------------------------------------------------
// dlopen loader (runtime only — no link-time dependency, LGPL sidecar rule)
// ---------------------------------------------------------------------------

/// Diagnostic failure from [`Loader::load`] / [`Loader::load_default`].
///
/// The call UI keeps its honest "no audio yet" stance until C2b wires the
/// engine; these errors are what C2b surfaces in that UI when the sidecar
/// library is absent or too old.
#[derive(Debug)]
pub enum LoadError {
    /// The library itself could not be opened. `searched` lists every
    /// location that was tried, in order.
    LibraryMissing { searched: Vec<PathBuf> },
    /// The library opened, but a bound `ntg_*` symbol was not exported by it.
    SymbolMissing { symbol: &'static str },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::LibraryMissing { searched } => {
                write!(f, "native call media library not found; searched: ")?;
                let mut first = true;
                for path in searched {
                    if !first {
                        write!(f, ", ")?;
                    }
                    first = false;
                    write!(f, "{}", path.display())?;
                }
                write!(
                    f,
                    " (run scripts/vendor-ntgcalls.sh or set QUILL_NTGCALLS_LIB)"
                )
            }
            LoadError::SymbolMissing { symbol } => write!(
                f,
                "native call media library does not export symbol `{symbol}`; \
                 the vendored library may be older than these bindings expect"
            ),
        }
    }
}

impl std::error::Error for LoadError {}

/// Runtime-loaded ntgcalls API.
///
/// `dlopen`s the LGPLv3 `libntgcalls.so` sidecar (procured unmodified by
/// `scripts/vendor-ntgcalls.sh`) and resolves every bound `ntg_*` symbol.
/// The `lib` field is kept alive so the resolved function pointers stay
/// valid for the `Loader`'s lifetime.
pub struct Loader {
    #[allow(dead_code)]
    lib: libloading::Library,
    pub ntg_set_log_callback: unsafe extern "C" fn(ntg_log_cb, *mut c_void),
    pub ntg_get_version: unsafe extern "C" fn() -> *const c_char,
    pub ntg_last_error: unsafe extern "C" fn() -> *const c_char,
    pub ntg_instance_create: unsafe extern "C" fn() -> *mut ntg_instance,
    pub ntg_instance_destroy: unsafe extern "C" fn(*mut ntg_instance),
    pub ntg_connection_info_free: unsafe extern "C" fn(*mut ntg_connection_info),
    pub ntg_remote_source_free: unsafe extern "C" fn(*mut ntg_remote_source),
    pub ntg_subchain_request_free: unsafe extern "C" fn(*mut ntg_subchain_request),
    pub ntg_audio_description_free: unsafe extern "C" fn(*mut ntg_audio_description),
    pub ntg_call_info_free: unsafe extern "C" fn(*mut ntg_call_info),
    pub ntg_device_info_free: unsafe extern "C" fn(*mut ntg_device_info),
    pub ntg_media_devices_free: unsafe extern "C" fn(*mut ntg_media_devices),
    pub ntg_media_state_free: unsafe extern "C" fn(*mut ntg_media_state),
    pub ntg_video_description_free: unsafe extern "C" fn(*mut ntg_video_description),
    pub ntg_media_description_free: unsafe extern "C" fn(*mut ntg_media_description),
    pub ntg_frame_data_free: unsafe extern "C" fn(*mut ntg_frame_data),
    pub ntg_ssrc_group_free: unsafe extern "C" fn(*mut ntg_ssrc_group),
    pub ntg_frame_free: unsafe extern "C" fn(*mut ntg_frame),
    pub ntg_segment_part_request_free: unsafe extern "C" fn(*mut ntg_segment_part_request),
    pub ntg_participants_request_free: unsafe extern "C" fn(*mut ntg_participants_request),
    pub ntg_ssrc_mapping_free: unsafe extern "C" fn(*mut ntg_ssrc_mapping),
    pub ntg_auth_params_free: unsafe extern "C" fn(*mut ntg_auth_params),
    pub ntg_conference_join_params_free: unsafe extern "C" fn(*mut ntg_conference_join_params),
    pub ntg_dh_config_free: unsafe extern "C" fn(*mut ntg_dh_config),
    pub ntg_protocol_free: unsafe extern "C" fn(*mut ntg_protocol),
    pub ntg_rtc_server_free: unsafe extern "C" fn(*mut ntg_rtc_server),
    pub ntg_call_info_entry_free: unsafe extern "C" fn(*mut ntg_call_info_entry, usize),
    pub ntg_string_free: unsafe extern "C" fn(*mut c_char),
    pub ntg_bytes_free: unsafe extern "C" fn(*mut c_void),
    pub ntg_create_p2p_call: unsafe extern "C" fn(*mut ntg_instance, i64) -> ntg_result,
    pub ntg_init_exchange: unsafe extern "C" fn(
        *mut ntg_instance,
        i64,
        ntg_dh_config,
        *const u8,
        usize,
        *mut *mut u8,
        *mut usize,
    ) -> ntg_result,
    pub ntg_exchange_keys: unsafe extern "C" fn(
        *mut ntg_instance,
        i64,
        *const u8,
        usize,
        i64,
        *mut ntg_auth_params,
    ) -> ntg_result,
    pub ntg_skip_exchange:
        unsafe extern "C" fn(*mut ntg_instance, i64, *const u8, usize, bool) -> ntg_result,
    pub ntg_connect_p2p: unsafe extern "C" fn(
        *mut ntg_instance,
        i64,
        *const ntg_rtc_server,
        usize,
        *const *const c_char,
        usize,
        bool,
        *const c_char,
    ) -> ntg_result,
    pub ntg_create_call:
        unsafe extern "C" fn(*mut ntg_instance, i64, *mut *mut c_char) -> ntg_result,
    pub ntg_init_presentation:
        unsafe extern "C" fn(*mut ntg_instance, i64, *mut *mut c_char) -> ntg_result,
    pub ntg_init_conference: unsafe extern "C" fn(
        *mut ntg_instance,
        i64,
        i64,
        *const u8,
        usize,
        *mut ntg_conference_join_params,
    ) -> ntg_result,
    pub ntg_connect:
        unsafe extern "C" fn(*mut ntg_instance, i64, *const c_char, bool) -> ntg_result,
    pub ntg_add_incoming_video: unsafe extern "C" fn(
        *mut ntg_instance,
        i64,
        i64,
        *const c_char,
        *const ntg_ssrc_group,
        usize,
        *mut u32,
    ) -> ntg_result,
    pub ntg_remove_incoming_video:
        unsafe extern "C" fn(*mut ntg_instance, i64, *const c_char, *mut bool) -> ntg_result,
    pub ntg_set_stream_sources: unsafe extern "C" fn(
        *mut ntg_instance,
        i64,
        ntg_stream_mode,
        ntg_media_description,
    ) -> ntg_result,
    pub ntg_pause: unsafe extern "C" fn(*mut ntg_instance, i64, *mut bool) -> ntg_result,
    pub ntg_resume: unsafe extern "C" fn(*mut ntg_instance, i64, *mut bool) -> ntg_result,
    pub ntg_mute: unsafe extern "C" fn(*mut ntg_instance, i64, *mut bool) -> ntg_result,
    pub ntg_unmute: unsafe extern "C" fn(*mut ntg_instance, i64, *mut bool) -> ntg_result,
    pub ntg_stop: unsafe extern "C" fn(*mut ntg_instance, i64) -> ntg_result,
    pub ntg_stop_presentation: unsafe extern "C" fn(*mut ntg_instance, i64) -> ntg_result,
    pub ntg_get_emojis_fingerprint:
        unsafe extern "C" fn(*mut ntg_instance, i64, *mut *mut c_char) -> ntg_result,
    pub ntg_time:
        unsafe extern "C" fn(*mut ntg_instance, i64, ntg_stream_mode, *mut u64) -> ntg_result,
    pub ntg_get_state:
        unsafe extern "C" fn(*mut ntg_instance, i64, *mut ntg_media_state) -> ntg_result,
    pub ntg_get_call_type:
        unsafe extern "C" fn(*mut ntg_instance, i64, *mut ntg_call_type) -> ntg_result,
    pub ntg_get_connection_mode:
        unsafe extern "C" fn(*mut ntg_instance, i64, *mut ntg_connection_mode) -> ntg_result,
    pub ntg_cpu_usage: unsafe extern "C" fn(*mut f64) -> ntg_result,
    pub ntg_ping: unsafe extern "C" fn(*mut *mut c_char) -> ntg_result,
    pub ntg_get_media_devices: unsafe extern "C" fn(*mut ntg_media_devices) -> ntg_result,
    pub ntg_get_protocol: unsafe extern "C" fn(*mut ntg_protocol) -> ntg_result,
    pub ntg_enable_glib_loop: unsafe extern "C" fn(bool) -> ntg_result,
    pub ntg_send_broadcast_timestamp:
        unsafe extern "C" fn(*mut ntg_instance, i64, i64) -> ntg_result,
    pub ntg_send_broadcast_part: unsafe extern "C" fn(
        *mut ntg_instance,
        i64,
        i64,
        i32,
        ntg_media_segment_part_status,
        bool,
        *const u8,
        usize,
    ) -> ntg_result,
    pub ntg_send_signaling_data:
        unsafe extern "C" fn(*mut ntg_instance, i64, *const u8, usize) -> ntg_result,
    pub ntg_send_external_frame: unsafe extern "C" fn(
        *mut ntg_instance,
        i64,
        ntg_stream_device,
        *const u8,
        usize,
        ntg_frame_data,
    ) -> ntg_result,
    pub ntg_update_audio_ssrc_mappings:
        unsafe extern "C" fn(*mut ntg_instance, i64, *const ntg_ssrc_mapping, usize) -> ntg_result,
    pub ntg_apply_blocks: unsafe extern "C" fn(
        *mut ntg_instance,
        i64,
        i32,
        i32,
        *const ntg_bytes,
        usize,
        bool,
    ) -> ntg_result,
    pub ntg_finish_subchain_request:
        unsafe extern "C" fn(*mut ntg_instance, i64, i32) -> ntg_result,
    pub ntg_calls: unsafe extern "C" fn(
        *mut ntg_instance,
        *mut *mut ntg_call_info_entry,
        *mut usize,
    ) -> ntg_result,
    pub ntg_on_upgrade_callback:
        unsafe extern "C" fn(*mut ntg_instance, ntg_upgrade_callback_cb, *mut c_void) -> ntg_result,
    pub ntg_on_stream_end_callback: unsafe extern "C" fn(
        *mut ntg_instance,
        ntg_stream_end_callback_cb,
        *mut c_void,
    ) -> ntg_result,
    pub ntg_on_connection_change_callback: unsafe extern "C" fn(
        *mut ntg_instance,
        ntg_connection_change_callback_cb,
        *mut c_void,
    ) -> ntg_result,
    pub ntg_on_frames_callback:
        unsafe extern "C" fn(*mut ntg_instance, ntg_frames_callback_cb, *mut c_void) -> ntg_result,
    pub ntg_on_signaling_data_callback: unsafe extern "C" fn(
        *mut ntg_instance,
        ntg_signaling_data_callback_cb,
        *mut c_void,
    ) -> ntg_result,
    pub ntg_on_remote_source_change_callback: unsafe extern "C" fn(
        *mut ntg_instance,
        ntg_remote_source_change_callback_cb,
        *mut c_void,
    ) -> ntg_result,
    pub ntg_on_request_broadcast_part_callback: unsafe extern "C" fn(
        *mut ntg_instance,
        ntg_request_broadcast_part_callback_cb,
        *mut c_void,
    ) -> ntg_result,
    pub ntg_on_request_broadcast_timestamp_callback: unsafe extern "C" fn(
        *mut ntg_instance,
        ntg_request_broadcast_timestamp_callback_cb,
        *mut c_void,
    ) -> ntg_result,
    pub ntg_on_request_participants_callback: unsafe extern "C" fn(
        *mut ntg_instance,
        ntg_request_participants_callback_cb,
        *mut c_void,
    ) -> ntg_result,
    pub ntg_on_outbound_block_callback: unsafe extern "C" fn(
        *mut ntg_instance,
        ntg_outbound_block_callback_cb,
        *mut c_void,
    ) -> ntg_result,
    pub ntg_on_subchain_request_callback: unsafe extern "C" fn(
        *mut ntg_instance,
        ntg_subchain_request_callback_cb,
        *mut c_void,
    ) -> ntg_result,
    pub ntg_on_update_emojis_callback: unsafe extern "C" fn(
        *mut ntg_instance,
        ntg_update_emojis_callback_cb,
        *mut c_void,
    ) -> ntg_result,
}

macro_rules! resolve {
    ($lib:expr, $name:literal, $ty:ty) => {{
        let name_bytes = concat!($name, "\0");
        let symbol: libloading::Symbol<$ty> = unsafe { $lib.get(name_bytes.as_bytes()) }
            .map_err(|_| LoadError::SymbolMissing { symbol: $name })?;
        *symbol
    }};
}

impl Loader {
    /// Open the library at `path` and resolve every bound `ntg_*` symbol.
    ///
    /// Fails with [`LoadError::LibraryMissing`] when the file cannot be
    /// opened, or [`LoadError::SymbolMissing`] naming the first symbol that
    /// did not resolve.
    pub fn load(path: impl AsRef<OsStr>) -> Result<Self, LoadError> {
        let path = path.as_ref();
        let lib =
            unsafe { libloading::Library::new(path) }.map_err(|_| LoadError::LibraryMissing {
                searched: vec![PathBuf::from(path)],
            })?;
        Ok(Self {
            ntg_set_log_callback: resolve!(
                lib,
                "ntg_set_log_callback",
                unsafe extern "C" fn(ntg_log_cb, *mut c_void)
            ),
            ntg_get_version: resolve!(
                lib,
                "ntg_get_version",
                unsafe extern "C" fn() -> *const c_char
            ),
            ntg_last_error: resolve!(
                lib,
                "ntg_last_error",
                unsafe extern "C" fn() -> *const c_char
            ),
            ntg_instance_create: resolve!(
                lib,
                "ntg_instance_create",
                unsafe extern "C" fn() -> *mut ntg_instance
            ),
            ntg_instance_destroy: resolve!(
                lib,
                "ntg_instance_destroy",
                unsafe extern "C" fn(*mut ntg_instance)
            ),
            ntg_connection_info_free: resolve!(
                lib,
                "ntg_connection_info_free",
                unsafe extern "C" fn(*mut ntg_connection_info)
            ),
            ntg_remote_source_free: resolve!(
                lib,
                "ntg_remote_source_free",
                unsafe extern "C" fn(*mut ntg_remote_source)
            ),
            ntg_subchain_request_free: resolve!(
                lib,
                "ntg_subchain_request_free",
                unsafe extern "C" fn(*mut ntg_subchain_request)
            ),
            ntg_audio_description_free: resolve!(
                lib,
                "ntg_audio_description_free",
                unsafe extern "C" fn(*mut ntg_audio_description)
            ),
            ntg_call_info_free: resolve!(
                lib,
                "ntg_call_info_free",
                unsafe extern "C" fn(*mut ntg_call_info)
            ),
            ntg_device_info_free: resolve!(
                lib,
                "ntg_device_info_free",
                unsafe extern "C" fn(*mut ntg_device_info)
            ),
            ntg_media_devices_free: resolve!(
                lib,
                "ntg_media_devices_free",
                unsafe extern "C" fn(*mut ntg_media_devices)
            ),
            ntg_media_state_free: resolve!(
                lib,
                "ntg_media_state_free",
                unsafe extern "C" fn(*mut ntg_media_state)
            ),
            ntg_video_description_free: resolve!(
                lib,
                "ntg_video_description_free",
                unsafe extern "C" fn(*mut ntg_video_description)
            ),
            ntg_media_description_free: resolve!(
                lib,
                "ntg_media_description_free",
                unsafe extern "C" fn(*mut ntg_media_description)
            ),
            ntg_frame_data_free: resolve!(
                lib,
                "ntg_frame_data_free",
                unsafe extern "C" fn(*mut ntg_frame_data)
            ),
            ntg_ssrc_group_free: resolve!(
                lib,
                "ntg_ssrc_group_free",
                unsafe extern "C" fn(*mut ntg_ssrc_group)
            ),
            ntg_frame_free: resolve!(lib, "ntg_frame_free", unsafe extern "C" fn(*mut ntg_frame)),
            ntg_segment_part_request_free: resolve!(
                lib,
                "ntg_segment_part_request_free",
                unsafe extern "C" fn(*mut ntg_segment_part_request)
            ),
            ntg_participants_request_free: resolve!(
                lib,
                "ntg_participants_request_free",
                unsafe extern "C" fn(*mut ntg_participants_request)
            ),
            ntg_ssrc_mapping_free: resolve!(
                lib,
                "ntg_ssrc_mapping_free",
                unsafe extern "C" fn(*mut ntg_ssrc_mapping)
            ),
            ntg_auth_params_free: resolve!(
                lib,
                "ntg_auth_params_free",
                unsafe extern "C" fn(*mut ntg_auth_params)
            ),
            ntg_conference_join_params_free: resolve!(
                lib,
                "ntg_conference_join_params_free",
                unsafe extern "C" fn(*mut ntg_conference_join_params)
            ),
            ntg_dh_config_free: resolve!(
                lib,
                "ntg_dh_config_free",
                unsafe extern "C" fn(*mut ntg_dh_config)
            ),
            ntg_protocol_free: resolve!(
                lib,
                "ntg_protocol_free",
                unsafe extern "C" fn(*mut ntg_protocol)
            ),
            ntg_rtc_server_free: resolve!(
                lib,
                "ntg_rtc_server_free",
                unsafe extern "C" fn(*mut ntg_rtc_server)
            ),
            ntg_call_info_entry_free: resolve!(
                lib,
                "ntg_call_info_entry_free",
                unsafe extern "C" fn(*mut ntg_call_info_entry, usize)
            ),
            ntg_string_free: resolve!(lib, "ntg_string_free", unsafe extern "C" fn(*mut c_char)),
            ntg_bytes_free: resolve!(lib, "ntg_bytes_free", unsafe extern "C" fn(*mut c_void)),
            ntg_create_p2p_call: resolve!(
                lib,
                "ntg_create_p2p_call",
                unsafe extern "C" fn(*mut ntg_instance, i64) -> ntg_result
            ),
            ntg_init_exchange: resolve!(
                lib,
                "ntg_init_exchange",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    i64,
                    ntg_dh_config,
                    *const u8,
                    usize,
                    *mut *mut u8,
                    *mut usize,
                ) -> ntg_result
            ),
            ntg_exchange_keys: resolve!(
                lib,
                "ntg_exchange_keys",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    i64,
                    *const u8,
                    usize,
                    i64,
                    *mut ntg_auth_params,
                ) -> ntg_result
            ),
            ntg_skip_exchange: resolve!(
                lib,
                "ntg_skip_exchange",
                unsafe extern "C" fn(*mut ntg_instance, i64, *const u8, usize, bool) -> ntg_result
            ),
            ntg_connect_p2p: resolve!(
                lib,
                "ntg_connect_p2p",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    i64,
                    *const ntg_rtc_server,
                    usize,
                    *const *const c_char,
                    usize,
                    bool,
                    *const c_char,
                ) -> ntg_result
            ),
            ntg_create_call: resolve!(
                lib,
                "ntg_create_call",
                unsafe extern "C" fn(*mut ntg_instance, i64, *mut *mut c_char) -> ntg_result
            ),
            ntg_init_presentation: resolve!(
                lib,
                "ntg_init_presentation",
                unsafe extern "C" fn(*mut ntg_instance, i64, *mut *mut c_char) -> ntg_result
            ),
            ntg_init_conference: resolve!(
                lib,
                "ntg_init_conference",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    i64,
                    i64,
                    *const u8,
                    usize,
                    *mut ntg_conference_join_params,
                ) -> ntg_result
            ),
            ntg_connect: resolve!(
                lib,
                "ntg_connect",
                unsafe extern "C" fn(*mut ntg_instance, i64, *const c_char, bool) -> ntg_result
            ),
            ntg_add_incoming_video: resolve!(
                lib,
                "ntg_add_incoming_video",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    i64,
                    i64,
                    *const c_char,
                    *const ntg_ssrc_group,
                    usize,
                    *mut u32,
                ) -> ntg_result
            ),
            ntg_remove_incoming_video: resolve!(
                lib,
                "ntg_remove_incoming_video",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    i64,
                    *const c_char,
                    *mut bool,
                ) -> ntg_result
            ),
            ntg_set_stream_sources: resolve!(
                lib,
                "ntg_set_stream_sources",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    i64,
                    ntg_stream_mode,
                    ntg_media_description,
                ) -> ntg_result
            ),
            ntg_pause: resolve!(
                lib,
                "ntg_pause",
                unsafe extern "C" fn(*mut ntg_instance, i64, *mut bool) -> ntg_result
            ),
            ntg_resume: resolve!(
                lib,
                "ntg_resume",
                unsafe extern "C" fn(*mut ntg_instance, i64, *mut bool) -> ntg_result
            ),
            ntg_mute: resolve!(
                lib,
                "ntg_mute",
                unsafe extern "C" fn(*mut ntg_instance, i64, *mut bool) -> ntg_result
            ),
            ntg_unmute: resolve!(
                lib,
                "ntg_unmute",
                unsafe extern "C" fn(*mut ntg_instance, i64, *mut bool) -> ntg_result
            ),
            ntg_stop: resolve!(
                lib,
                "ntg_stop",
                unsafe extern "C" fn(*mut ntg_instance, i64) -> ntg_result
            ),
            ntg_stop_presentation: resolve!(
                lib,
                "ntg_stop_presentation",
                unsafe extern "C" fn(*mut ntg_instance, i64) -> ntg_result
            ),
            ntg_get_emojis_fingerprint: resolve!(
                lib,
                "ntg_get_emojis_fingerprint",
                unsafe extern "C" fn(*mut ntg_instance, i64, *mut *mut c_char) -> ntg_result
            ),
            ntg_time: resolve!(
                lib,
                "ntg_time",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    i64,
                    ntg_stream_mode,
                    *mut u64,
                ) -> ntg_result
            ),
            ntg_get_state: resolve!(
                lib,
                "ntg_get_state",
                unsafe extern "C" fn(*mut ntg_instance, i64, *mut ntg_media_state) -> ntg_result
            ),
            ntg_get_call_type: resolve!(
                lib,
                "ntg_get_call_type",
                unsafe extern "C" fn(*mut ntg_instance, i64, *mut ntg_call_type) -> ntg_result
            ),
            ntg_get_connection_mode: resolve!(
                lib,
                "ntg_get_connection_mode",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    i64,
                    *mut ntg_connection_mode,
                ) -> ntg_result
            ),
            ntg_cpu_usage: resolve!(
                lib,
                "ntg_cpu_usage",
                unsafe extern "C" fn(*mut f64) -> ntg_result
            ),
            ntg_ping: resolve!(
                lib,
                "ntg_ping",
                unsafe extern "C" fn(*mut *mut c_char) -> ntg_result
            ),
            ntg_get_media_devices: resolve!(
                lib,
                "ntg_get_media_devices",
                unsafe extern "C" fn(*mut ntg_media_devices) -> ntg_result
            ),
            ntg_get_protocol: resolve!(
                lib,
                "ntg_get_protocol",
                unsafe extern "C" fn(*mut ntg_protocol) -> ntg_result
            ),
            ntg_enable_glib_loop: resolve!(
                lib,
                "ntg_enable_glib_loop",
                unsafe extern "C" fn(bool) -> ntg_result
            ),
            ntg_send_broadcast_timestamp: resolve!(
                lib,
                "ntg_send_broadcast_timestamp",
                unsafe extern "C" fn(*mut ntg_instance, i64, i64) -> ntg_result
            ),
            ntg_send_broadcast_part: resolve!(
                lib,
                "ntg_send_broadcast_part",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    i64,
                    i64,
                    i32,
                    ntg_media_segment_part_status,
                    bool,
                    *const u8,
                    usize,
                ) -> ntg_result
            ),
            ntg_send_signaling_data: resolve!(
                lib,
                "ntg_send_signaling_data",
                unsafe extern "C" fn(*mut ntg_instance, i64, *const u8, usize) -> ntg_result
            ),
            ntg_send_external_frame: resolve!(
                lib,
                "ntg_send_external_frame",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    i64,
                    ntg_stream_device,
                    *const u8,
                    usize,
                    ntg_frame_data,
                ) -> ntg_result
            ),
            ntg_update_audio_ssrc_mappings: resolve!(
                lib,
                "ntg_update_audio_ssrc_mappings",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    i64,
                    *const ntg_ssrc_mapping,
                    usize,
                ) -> ntg_result
            ),
            ntg_apply_blocks: resolve!(
                lib,
                "ntg_apply_blocks",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    i64,
                    i32,
                    i32,
                    *const ntg_bytes,
                    usize,
                    bool,
                ) -> ntg_result
            ),
            ntg_finish_subchain_request: resolve!(
                lib,
                "ntg_finish_subchain_request",
                unsafe extern "C" fn(*mut ntg_instance, i64, i32) -> ntg_result
            ),
            ntg_calls: resolve!(
                lib,
                "ntg_calls",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    *mut *mut ntg_call_info_entry,
                    *mut usize,
                ) -> ntg_result
            ),
            ntg_on_upgrade_callback: resolve!(
                lib,
                "ntg_on_upgrade_callback",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    ntg_upgrade_callback_cb,
                    *mut c_void,
                ) -> ntg_result
            ),
            ntg_on_stream_end_callback: resolve!(
                lib,
                "ntg_on_stream_end_callback",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    ntg_stream_end_callback_cb,
                    *mut c_void,
                ) -> ntg_result
            ),
            ntg_on_connection_change_callback: resolve!(
                lib,
                "ntg_on_connection_change_callback",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    ntg_connection_change_callback_cb,
                    *mut c_void,
                ) -> ntg_result
            ),
            ntg_on_frames_callback: resolve!(
                lib,
                "ntg_on_frames_callback",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    ntg_frames_callback_cb,
                    *mut c_void,
                ) -> ntg_result
            ),
            ntg_on_signaling_data_callback: resolve!(
                lib,
                "ntg_on_signaling_data_callback",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    ntg_signaling_data_callback_cb,
                    *mut c_void,
                ) -> ntg_result
            ),
            ntg_on_remote_source_change_callback: resolve!(
                lib,
                "ntg_on_remote_source_change_callback",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    ntg_remote_source_change_callback_cb,
                    *mut c_void,
                ) -> ntg_result
            ),
            ntg_on_request_broadcast_part_callback: resolve!(
                lib,
                "ntg_on_request_broadcast_part_callback",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    ntg_request_broadcast_part_callback_cb,
                    *mut c_void,
                ) -> ntg_result
            ),
            ntg_on_request_broadcast_timestamp_callback: resolve!(
                lib,
                "ntg_on_request_broadcast_timestamp_callback",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    ntg_request_broadcast_timestamp_callback_cb,
                    *mut c_void,
                ) -> ntg_result
            ),
            ntg_on_request_participants_callback: resolve!(
                lib,
                "ntg_on_request_participants_callback",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    ntg_request_participants_callback_cb,
                    *mut c_void,
                ) -> ntg_result
            ),
            ntg_on_outbound_block_callback: resolve!(
                lib,
                "ntg_on_outbound_block_callback",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    ntg_outbound_block_callback_cb,
                    *mut c_void,
                ) -> ntg_result
            ),
            ntg_on_subchain_request_callback: resolve!(
                lib,
                "ntg_on_subchain_request_callback",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    ntg_subchain_request_callback_cb,
                    *mut c_void,
                ) -> ntg_result
            ),
            ntg_on_update_emojis_callback: resolve!(
                lib,
                "ntg_on_update_emojis_callback",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    ntg_update_emojis_callback_cb,
                    *mut c_void,
                ) -> ntg_result
            ),
            lib,
        })
    }

    /// Search the standard locations for the sidecar library and load it.
    ///
    /// Order: `QUILL_NTGCALLS_LIB` env var, then
    /// The native library under `vendor/ntgcalls/lib` at any ancestor of the current
    /// executable (covers `cargo run` from the repo), then the bare
    /// native library filename on the system search path.
    pub fn load_default() -> Result<Self, LoadError> {
        let mut searched: Vec<PathBuf> = Vec::new();

        if let Some(env_path) = std::env::var_os("QUILL_NTGCALLS_LIB") {
            let path = PathBuf::from(env_path);
            searched.push(path.clone());
            if let Ok(loader) = Self::load(&path) {
                return Ok(loader);
            }
        }

        if let Ok(exe) = std::env::current_exe() {
            // Linux/Windows package layout: `<dir>/quill` + `<dir>/lib/<native lib>`.
            if let Some(candidate) = exe
                .parent()
                .map(|dir| dir.join("lib").join(library_filename()))
                .filter(|candidate| candidate.exists())
            {
                return Self::load(&candidate);
            }
            for ancestor in exe
                .parent()
                .into_iter()
                .flat_map(std::path::Path::ancestors)
            {
                for directory in [
                    ancestor.join("Frameworks"),
                    ancestor.join("vendor/ntgcalls/lib"),
                    // Windows: the package keeps ntgcalls.dll beside quill.exe; the
                    // upstream zip puts it under lib/Release.
                    #[cfg(windows)]
                    ancestor.to_path_buf(),
                    #[cfg(windows)]
                    ancestor.join("vendor/ntgcalls/lib/Release"),
                ] {
                    let candidate = directory.join(library_filename());
                    if candidate.exists() {
                        return Self::load(&candidate);
                    }
                }
            }
        }

        let system = PathBuf::from(library_filename());
        searched.push(system.clone());
        Self::load(&system).map_err(|_| LoadError::LibraryMissing { searched })
    }
}
