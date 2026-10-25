#![deny(clippy::indexing_slicing)]

mod account_notices;
mod ai_compose;
mod auth;
mod backgrounds;
mod boost_features;
mod boost_slots;
mod bots;
mod calls;
mod chat;
mod chat_action_bar;
mod chat_appearance;
mod chat_boosts;
mod chat_drafts;
mod chat_events;
mod chat_invite;
mod chat_list;
mod chat_members;
mod chat_notifications;
mod chat_photo_animation;
mod chat_themes;
mod communities;
mod domains;
mod envelope_types;
mod forum;
mod gift_card;
mod giveaway;
mod group_profile;
mod inline_queries;
mod json_helpers;
mod keyboards;
mod language;
mod link_info;
mod live_story;
mod message;
mod message_audience;
mod message_audio_video;
mod message_checklist;
mod message_contact;
mod message_content;
mod message_link_preview;
mod message_location;
mod message_media;
mod message_poll;
mod message_reactions;
mod message_sponsored;
mod message_sticker;
mod message_thread;
mod paid_media;
mod payload;
mod payments;
mod poll_option_media;
mod profile_audios;
mod saved;
mod secret_chat;
mod service_action;
mod sessions;
mod statistics;
mod storage;
mod storage_categories;
mod stories;
mod story_insights;
mod updates_sync;
mod users;
mod video_alternatives;
mod web_apps;

#[cfg(test)]
mod account_change_tests;
#[cfg(test)]
mod channel_tests_admin;
#[cfg(test)]
mod channel_tests_geo;
#[cfg(test)]
mod channel_tests_members;
#[cfg(test)]
mod channel_tests_polls;
#[cfg(test)]
mod channel_tests_service;
#[cfg(test)]
mod channel_tests_statistics;
#[cfg(test)]
mod channel_tests_stories;
#[cfg(test)]
mod channel_tests_supergroups;
#[cfg(test)]
mod channel_tests_updates;
#[cfg(test)]
mod channel_tests_users;
#[cfg(test)]
mod notification_sound_tests;
#[cfg(test)]
mod password_state_tests;
#[cfg(test)]
mod privacy_data_tests;
#[cfg(test)]
mod sessions_tests;
#[cfg(test)]
mod storage_statistics_tests;
#[cfg(test)]
mod tests_chats;
#[cfg(test)]
mod tests_core;
#[cfg(test)]
mod tests_entities;
#[cfg(test)]
mod tests_keyboards;
#[cfg(test)]
mod tests_live_location;
#[cfg(test)]
mod tests_media;
#[cfg(test)]
mod tests_messages;
#[cfg(test)]
mod tests_payments;
#[cfg(test)]
mod tests_threads;
#[cfg(test)]
mod tests_view_button;

pub use super::story_areas::{StoryAreaKind, StoryAreaView};
pub use account_notices::*;
pub use ai_compose::*;
pub use auth::*;
pub use backgrounds::*;
pub use boost_features::*;
pub use boost_slots::*;
pub use bots::*;
pub use calls::*;
pub use chat::*;
pub use chat_action_bar::*;
pub use chat_appearance::*;
pub use chat_boosts::*;
pub use chat_drafts::*;
pub use chat_events::*;
pub use chat_invite::*;
pub use chat_list::*;
pub use chat_members::*;
pub use chat_notifications::*;
pub use chat_photo_animation::*;
pub use chat_themes::*;
pub use communities::*;
pub use domains::*;
pub use envelope_types::*;
pub use forum::*;
pub use gift_card::*;
pub use giveaway::*;
pub use group_profile::*;
pub use inline_queries::*;
pub(crate) use json_helpers::*;
pub use keyboards::*;
pub use language::*;
pub use link_info::*;
pub use live_story::*;
pub use message::*;
pub use message_audience::*;
pub use message_audio_video::*;
pub use message_checklist::*;
pub use message_contact::*;
pub use message_content::*;
pub use message_link_preview::*;
pub use message_location::*;
pub use message_media::*;
pub use message_poll::*;
pub use message_reactions::*;
pub use message_sponsored::*;
pub use message_sticker::*;
pub use message_thread::*;
pub(crate) use payload::*;
pub use payments::*;
pub use poll_option_media::*;
pub use profile_audios::*;
pub use saved::*;
pub use secret_chat::*;
pub use service_action::*;
pub use sessions::*;
pub use statistics::*;
pub use storage::*;
pub use storage_categories::*;
pub use stories::*;
pub use story_insights::*;
pub use updates_sync::*;
pub use users::*;
pub use video_alternatives::VideoAlternative;
pub use web_apps::*;
