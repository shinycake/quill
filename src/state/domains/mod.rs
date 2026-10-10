//! Per-domain request purposes and reducer handlers.
//!
//! Each domain owns its `<Domain>Purpose` enum (wrapped by
//! [`RequestPurpose`](super::RequestPurpose)) and the code that reacts
//! to its answers and errors, so a feature in one domain only edits
//! that domain's files. See `docs/decisions/codex-refactor-4.md`.
mod auth;
mod bots;
mod calls;
mod chat_list;
mod chats;
mod groups;
mod media;
mod messages;
mod payments;
mod search;
mod settings;
mod stickers;
mod stories;
mod threads;
mod users;

pub use auth::*;
pub use bots::*;
pub use calls::*;
pub use chat_list::*;
pub use chats::*;
pub use groups::*;
pub use media::*;
pub use messages::*;
pub use payments::*;
pub use search::*;
pub use settings::*;
pub use stickers::*;
pub use stories::*;
pub use threads::*;
pub use users::*;
