//! Phase C2b: 1:1 call-engine boundary and signaling bridge.
#![deny(clippy::indexing_slicing)]

pub mod audio_level;
pub mod capture;
pub mod engine;
#[cfg(feature = "ui")]
pub mod level_tap;
pub mod proxy;
pub mod ptt;
pub mod ptt_global;
pub mod share_links;
pub mod tile_pin;
