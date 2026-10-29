//! Phase C2b: 1:1 call-engine boundary and signaling bridge.
#![deny(clippy::indexing_slicing)]

pub mod audio_level;
pub mod capture;
pub mod engine;
pub mod group_call_create;
#[cfg(feature = "ui")]
pub mod level_tap;
pub mod live_video;
pub mod proxy;
pub mod ptt;
pub mod ptt_global;
pub mod rtmp_start;
pub mod share_links;
pub mod speaker_levels;
pub mod stream_audio;
pub mod tile_pin;
