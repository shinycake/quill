//! Raw FFI bindings for libquill_calls: Telegram's tgcalls call engine (the
//! one Telegram Desktop ships) behind the plain C API in
//! `native/quill-calls/include/quill_calls.h`.
//!
//! **License (LGPL-3.0 sidecar).** tgcalls is LGPL-3.0, so the library is
//! never linked into Quill: `scripts/build-tgcalls.sh` builds it and
//! [`Loader`] opens it at runtime, which lets users rebuild and replace it.
//!
//! Every declaration mirrors `quill_calls.h`; [`QC_API_VERSION`] must match
//! the library's `qc_api_version()` or [`Loader::load`] refuses it.

#![allow(non_camel_case_types)]

use std::ffi::{c_char, c_void};
use std::fmt;
use std::path::PathBuf;

mod loader;
pub use loader::*;

pub const QC_API_VERSION: u32 = 3;

pub const QC_LOG_VERBOSE: i32 = 0;
pub const QC_LOG_INFO: i32 = 1;
pub const QC_LOG_WARNING: i32 = 2;
pub const QC_LOG_ERROR: i32 = 3;

pub const QC_AUDIO_RATE: u32 = 48_000;
pub const QC_AUDIO_FRAME_SAMPLES: usize = 480;

pub const QC_VIDEO_INACTIVE: i32 = 0;
pub const QC_VIDEO_PAUSED: i32 = 1;
pub const QC_VIDEO_ACTIVE: i32 = 2;

pub const QC_P2P_WAIT_INIT: i32 = 0;
pub const QC_P2P_WAIT_INIT_ACK: i32 = 1;
pub const QC_P2P_ESTABLISHED: i32 = 2;
pub const QC_P2P_FAILED: i32 = 3;
pub const QC_P2P_RECONNECTING: i32 = 4;

pub const QC_GROUP_MODE_NONE: i32 = 0;
pub const QC_GROUP_MODE_RTC: i32 = 1;
pub const QC_GROUP_MODE_BROADCAST: i32 = 2;

pub const QC_GROUP_QUALITY_THUMBNAIL: i32 = 0;
pub const QC_GROUP_QUALITY_MEDIUM: i32 = 1;
pub const QC_GROUP_QUALITY_FULL: i32 = 2;

pub const QC_BROADCAST_PART_SUCCESS: i32 = 0;
pub const QC_BROADCAST_PART_NOT_READY: i32 = 1;
pub const QC_BROADCAST_PART_RESYNC_NEEDED: i32 = 2;

pub type qc_log_fn =
    Option<unsafe extern "C" fn(user: *mut c_void, level: i32, line: *const c_char)>;
pub type qc_audio_out_fn =
    Option<unsafe extern "C" fn(user: *mut c_void, samples: *const i16, count: usize)>;
pub type qc_video_frame_fn =
    Option<unsafe extern "C" fn(user: *mut c_void, frame: *const qc_video_frame)>;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct qc_video_frame {
    pub width: i32,
    pub height: i32,
    pub rotation: i32,
    pub timestamp_us: i64,
    pub y: *const u8,
    pub u: *const u8,
    pub v: *const u8,
    pub stride_y: i32,
    pub stride_u: i32,
    pub stride_v: i32,
}

#[repr(C)]
pub struct qc_video_capture {
    _private: [u8; 0],
}

#[repr(C)]
pub struct qc_p2p {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct qc_p2p_callbacks {
    pub user: *mut c_void,
    pub state: Option<unsafe extern "C" fn(user: *mut c_void, state: i32)>,
    pub signal_bars: Option<unsafe extern "C" fn(user: *mut c_void, bars: i32)>,
    pub remote_media_state:
        Option<unsafe extern "C" fn(user: *mut c_void, audio_active: i32, video_state: i32)>,
    pub remote_battery_low: Option<unsafe extern "C" fn(user: *mut c_void, is_low: i32)>,
    pub signaling: Option<unsafe extern "C" fn(user: *mut c_void, data: *const u8, length: usize)>,
    pub audio_out: qc_audio_out_fn,
    pub remote_video: qc_video_frame_fn,
}

#[repr(C)]
pub struct qc_group {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct qc_group_level {
    pub ssrc: u32,
    pub level: f32,
    pub voice: i32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct qc_group_media_channel {
    pub audio_ssrc: u32,
    pub user_id: i64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct qc_group_broadcast_part {
    pub timestamp_ms: i64,
    pub duration_ms: i64,
    pub is_video: i32,
    pub video_channel: i32,
    pub video_quality: i32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct qc_group_callbacks {
    pub user: *mut c_void,
    pub network_state:
        Option<unsafe extern "C" fn(user: *mut c_void, connected: i32, transitioning_to_rtc: i32)>,
    pub audio_levels: Option<
        unsafe extern "C" fn(user: *mut c_void, levels: *const qc_group_level, count: usize),
    >,
    pub audio_out: qc_audio_out_fn,
    pub incoming_video: Option<
        unsafe extern "C" fn(
            user: *mut c_void,
            endpoint: *const c_char,
            frame: *const qc_video_frame,
        ),
    >,
    pub request_media_channels: Option<
        unsafe extern "C" fn(user: *mut c_void, request_id: u64, ssrcs: *const u32, count: usize),
    >,
    pub request_current_time: Option<unsafe extern "C" fn(user: *mut c_void, request_id: u64)>,
    pub request_broadcast_part: Option<
        unsafe extern "C" fn(
            user: *mut c_void,
            request_id: u64,
            part: *const qc_group_broadcast_part,
        ),
    >,
    pub request_cancelled: Option<unsafe extern "C" fn(user: *mut c_void, request_id: u64)>,
}

/// Native sidecar name, using Rust's platform library naming convention.
pub fn library_filename() -> String {
    format!(
        "{}quill_calls{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    )
}

/// Why [`Loader::load`] or [`Loader::load_default`] failed.
#[derive(Debug)]
pub enum LoadError {
    /// The library could not be opened. `searched` lists every location
    /// tried, in order.
    LibraryMissing { searched: Vec<PathBuf> },
    /// The library opened but does not export a bound `qc_*` symbol.
    SymbolMissing { symbol: &'static str },
    /// The library speaks another version of `quill_calls.h`.
    ApiMismatch { library: u32 },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::LibraryMissing { searched } => {
                write!(f, "call engine library not found; searched: ")?;
                for (index, path) in searched.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", path.display())?;
                }
                write!(f, " (run scripts/build-tgcalls.sh or set QUILL_CALLS_LIB)")
            }
            LoadError::SymbolMissing { symbol } => write!(
                f,
                "call engine library does not export `{symbol}`; it is older than these bindings"
            ),
            LoadError::ApiMismatch { library } => write!(
                f,
                "call engine library speaks API version {library}, Quill needs {QC_API_VERSION}; \
                 rebuild it with scripts/build-tgcalls.sh"
            ),
        }
    }
}

impl std::error::Error for LoadError {}
