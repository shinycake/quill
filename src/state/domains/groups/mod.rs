//! The groups domain: groups and channels: members, admin rights, invite links, join requests, boosts, communities.
mod apply;
mod error;
mod purpose;
mod state;

pub use purpose::*;

pub use state::*;
