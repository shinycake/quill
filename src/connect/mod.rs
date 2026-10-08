//! Live TDLib connect gate: credentials + tdjson → setTdlibParameters → auth updates.
//! Never logs api_hash, phone numbers, or codes.
mod account_hygiene;
mod auth;
mod bots;
mod calls;
mod chat_list;
mod composer;
mod connect_flow;
mod contacts;
mod core;
mod deep_links;
mod emoji_sets;
mod gifs;
mod group_calls;
mod groups;
mod live;
mod marketplace;
mod media;
mod media_library;
pub use media_library::REACTION_STRIP_SIZE;
mod message_actions;
mod message_menu;
mod messages;
mod moderation;
mod payments;
mod polls;
mod profile;
mod proxy;
mod search;
mod secret_chats;
mod sender;
mod settings;
mod stickers;
mod stories;
mod subsection_tabs;
mod threads;
mod types;
mod typing;

pub use connect_flow::*;
pub use deep_links::detect_deep_link_arg;
pub use live::*;
pub use message_menu::{ADDED_REACTIONS_PAGE, ModerationChoice};
pub use sender::*;
pub use types::*;

pub(crate) use connect_flow::group_video_sources;
pub(crate) use deep_links::{parse_deep_link_action, tg_query_params};
pub(crate) use sender::{SignalingOutbox, TransportOutbox, VideoStateOutbox};
pub(crate) use types::{OutgoingTyping, PendingDraft, ToggleSessionKind};

#[cfg(test)]
mod tests;
