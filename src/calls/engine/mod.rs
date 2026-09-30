//! Phase C2b: call-engine abstraction plus the runtime-loaded ntgcalls adapter.
mod api;
mod mock;
mod native;
mod ntgcalls;
mod types;

pub use api::*;
pub use mock::*;
pub use native::*;
pub use ntgcalls::*;
pub use types::*;

pub(crate) use native::{
    append_devices, audio_description, c_string_pointer_array, c_strings, connection_trampoline,
    frames_trampoline, native_input, native_string, remote_source_trampoline, signaling_trampoline,
};
pub(crate) use types::{
    CallMediaConfig, CallbackShared, GroupCallMedia, NativeRtcServers, retained_call_media,
};
