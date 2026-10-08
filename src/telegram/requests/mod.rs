//! TDLib request builders, one domain per module.
//! Pure code-motion split of the former `requests.rs`; all public paths unchanged.

mod auth;
mod bots;
mod calls;
mod chat_list;
mod chats;
mod contacts;
mod folders;
mod group_calls;
mod groups;
mod media;
mod message_menu;
mod messages;
mod misc;
mod payments;
mod polls;
mod privacy;
mod secret_chats;
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
mod tests_stickers;
#[cfg(test)]
mod tests_stories;
#[cfg(test)]
mod tests_threads;
mod users;

pub use auth::*;
pub use bots::*;
pub use calls::*;
pub use chat_list::*;
pub use chats::*;
pub use contacts::*;
pub use folders::*;
pub use group_calls::*;
pub use groups::*;
pub use media::*;
pub use message_menu::*;
pub use messages::*;
pub(crate) use messages::{
    formatted_caption, message_topic_value, self_destruct_type_value, send_reply_value,
};
pub use misc::*;
pub use payments::*;
pub use polls::*;
pub use privacy::*;
pub use secret_chats::*;
pub use stickers::*;
pub use stories::*;
pub use users::*;
