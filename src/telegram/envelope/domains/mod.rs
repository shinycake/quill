//! Per-domain envelope payloads and their parsers.
//!
//! Each domain owns its `<Domain>Payload` enum (wrapped by
//! [`EnvelopePayload`](super::EnvelopePayload)) and the parser for its
//! TDLib type names, so a new update or answer only edits that domain's
//! files. See `docs/decisions/codex-refactor-4.md`.
mod auth;
mod bots;
mod calls;
mod chat_list;
mod chats;
mod common;
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

pub use auth::AuthPayload;
pub use bots::BotsPayload;
pub use calls::CallsPayload;
pub use chat_list::ChatListPayload;
pub use chats::ChatsPayload;
pub use common::CommonPayload;
pub use groups::GroupsPayload;
pub use media::MediaPayload;
pub use messages::MessagesPayload;
pub use payments::PaymentsPayload;
pub use search::SearchPayload;
pub use settings::SettingsPayload;
pub use stickers::StickersPayload;
pub use stories::StoriesPayload;
pub use threads::ThreadsPayload;
pub use users::UsersPayload;

pub(crate) use auth::parse_auth_payload;
pub(crate) use bots::parse_bots_payload;
pub(crate) use calls::parse_calls_payload;
pub(crate) use chat_list::parse_chat_list_payload;
pub(crate) use chats::parse_chats_payload;
pub(crate) use common::parse_common_payload;
pub(crate) use groups::parse_groups_payload;
pub(crate) use media::parse_media_payload;
pub(crate) use messages::parse_messages_payload;
pub(crate) use payments::parse_payments_payload;
pub(crate) use search::parse_search_payload;
pub(crate) use settings::parse_settings_payload;
pub(crate) use stickers::parse_stickers_payload;
pub(crate) use stories::parse_stories_payload;
pub(crate) use threads::parse_threads_payload;
pub(crate) use users::parse_users_payload;
