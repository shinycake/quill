//! The call-engine abstraction, the tgcalls engine (`libquill_calls`,
//! loaded at runtime) and the mock engine tests drive.
mod api;
mod conference;
mod external;
mod mock;
mod stream;
mod stream_feeder;
mod tgcalls;
mod types;

pub use api::*;
pub use conference::*;
pub use external::*;
pub use mock::*;
pub use stream::*;
pub use stream_feeder::*;
pub use tgcalls::{TgcallsEngine, preload_tgcalls};
pub use types::*;

/// Group-call and stream frames go to live feeds as planes.
use crate::calls::live_video::{LiveFrame, LiveKey};

pub(crate) use types::{CallMediaConfig, CallbackShared, retained_call_media};
