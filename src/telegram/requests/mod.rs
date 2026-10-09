//! TDLib request builders, one domain per module.
//! Pure code-motion split of the former `requests.rs`; all public paths unchanged.

mod auth;
mod backgrounds;
mod bots;
mod calls;
mod chat_list;
mod chats;
mod checklists;
mod contacts;
mod folders;
mod forum_saved;
mod group_admin;
mod group_calls;
mod groups;
mod invite_admin;
mod media;
mod message_menu;
mod messages;
mod misc;
mod payments;
mod poll_extras;
mod polls;
mod privacy;
mod profile_panels;
mod proxy;
mod secret_chats;
mod send_as;
mod share_content;
mod stickers;
mod stories;
#[cfg(test)]
mod tests_auth;
#[cfg(test)]
mod tests_calls;
#[cfg(test)]
mod tests_chat_list;
#[cfg(test)]
mod tests_chats;
#[cfg(test)]
mod tests_group_admin;
#[cfg(test)]
mod tests_groups;
#[cfg(test)]
mod tests_media;
#[cfg(test)]
mod tests_message_menu;
#[cfg(test)]
mod tests_messages;
#[cfg(test)]
mod tests_misc;
#[cfg(test)]
mod tests_polls;
#[cfg(test)]
mod tests_profile_panels;
#[cfg(test)]
mod tests_send_as;
#[cfg(test)]
mod tests_stickers;
#[cfg(test)]
mod tests_stories;
#[cfg(test)]
mod tests_threads;
mod translate;
mod users;

pub use auth::*;
pub use backgrounds::*;
pub use bots::*;
pub use calls::*;
pub use chat_list::*;
pub use chats::*;
pub use checklists::*;
pub use contacts::*;
pub use folders::*;
pub use forum_saved::*;
pub use group_admin::*;
pub use group_calls::*;
pub use groups::*;
pub use invite_admin::*;
pub use media::*;
pub use message_menu::*;
pub use messages::*;
pub(crate) use messages::{
    formatted_caption, message_topic_value, self_destruct_type_value, send_reply_value,
};
pub use misc::*;
pub use payments::*;
pub use poll_extras::*;
pub use polls::*;
pub use privacy::*;
pub use profile_panels::*;
pub use proxy::*;
pub use secret_chats::*;
pub use send_as::*;
pub use share_content::*;
pub use stickers::*;
pub use stories::*;
pub use translate::*;
pub use users::*;
