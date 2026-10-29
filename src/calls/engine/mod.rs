//! Phase C2b: call-engine abstraction plus the runtime-loaded ntgcalls adapter.
mod api;
mod external;
mod mock;
mod native;
mod ntgcalls;
mod stream;
mod stream_feeder;
mod tgcalls;
mod types;

pub use api::*;
pub use external::*;
pub use mock::*;
pub use native::*;
pub use ntgcalls::*;
pub use stream::*;
pub use stream_feeder::*;
pub use tgcalls::{TgcallsEngine, preload_tgcalls};
pub use types::*;

/// Group-call and stream frames go to live feeds as planes.
use crate::calls::live_video::{LiveFrame, LiveKey, LiveRotation};

pub(crate) use native::{
    append_devices, audio_description, broadcast_part_trampoline, broadcast_timestamp_trampoline,
    c_string_pointer_array, c_strings, connection_trampoline, frames_trampoline, native_input,
    native_string, remote_source_trampoline, signaling_trampoline,
};
pub(crate) use types::{
    CallMediaConfig, CallbackShared, GroupCallMedia, NativeRtcServers, retained_call_media,
};
