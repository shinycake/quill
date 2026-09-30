//! Live TDLib connect gate: credentials + tdjson → setTdlibParameters → auth updates.
//! Never logs api_hash, phone numbers, or codes.
mod auth;
mod bots;
mod calls;
mod chat_list;
mod composer;
mod connect_flow;
mod contacts;
mod core;
mod group_calls;
mod groups;
mod live;
mod media;
mod message_actions;
mod messages;
mod moderation;
mod payments;
mod polls;
mod profile;
mod search;
mod secret_chats;
mod sender;
mod settings;
mod stickers;
mod stories;
mod types;
mod typing;

pub use connect_flow::*;
pub use live::*;
pub use sender::*;
pub use types::*;

pub(crate) use connect_flow::group_video_sources;
pub(crate) use sender::{SignalingOutbox, TransportOutbox, VideoStateOutbox};
pub(crate) use types::{OutgoingTyping, PendingDraft, ToggleSessionKind};

#[cfg(test)]
mod tests;
