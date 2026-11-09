//! Where in a chat a send that finishes later goes: the forum topic, the
//! channel direct-messages conversation and the comments thread open when
//! the user tapped Send. A recording is finished off the UI thread, and by
//! the time the file is ready the user may have opened another topic,
//! thread or chat; the send uses what was fixed at the tap, never what is
//! open then.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ids::TopicId;
use crate::telegram::requests::{route_into_direct_topic, route_into_thread};

/// A chat's send destination below the chat, fixed at Send.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SendPlace {
    /// The forum topic or direct-messages conversation (`topic_id`).
    pub topic: Option<TopicId>,
    /// The open comments or reply thread: its root, and the forum topic it
    /// lives in.
    pub thread: Option<(i64, Option<i32>)>,
}

impl<S: JsonSender> ConnectDriver<S> {
    /// Where a send from `chat_id`'s composer goes now.
    pub fn send_place(&self, chat_id: ChatId) -> SendPlace {
        SendPlace {
            topic: self.send_topic(chat_id),
            thread: self
                .session
                .thread_send_target(chat_id)
                .map(|(thread_id, _)| (thread_id, self.session.thread_forum_topic(chat_id))),
        }
    }

    /// `place`'s forum topic is closed (TDLib would refuse the send).
    pub(crate) fn place_is_closed(&self, chat_id: ChatId, place: &SendPlace) -> bool {
        let Some(topic_id) = place.topic.and_then(TopicId::forum_topic_id) else {
            return false;
        };
        self.session
            .threads
            .forum_topics
            .get(&chat_id.0)
            .and_then(|topics| topics.iter().find(|t| t.forum_topic_id == topic_id))
            .is_some_and(|topic| topic.is_closed)
    }

    /// [`ConnectDriver::thread_routed`] for `place` instead of what is open:
    /// a reply in a channel's direct messages All view goes to its
    /// message's conversation, and a send in a thread replies to its root.
    pub(crate) fn place_routed(&self, chat_id: ChatId, place: &SendPlace, json: String) -> String {
        let in_conversation = matches!(place.topic, Some(TopicId::DirectMessages { .. }));
        let json = if self.session.is_direct_messages_admin(chat_id) && !in_conversation {
            route_into_direct_topic(&json, |message_id| {
                self.session
                    .direct_topic_of_message(chat_id, MessageId(message_id))
                    .map(|topic| TopicId::DirectMessages {
                        direct_messages_chat_topic_id: topic,
                    })
            })
        } else {
            json
        };
        match place.thread {
            Some((thread_id, forum_topic)) => route_into_thread(&json, thread_id, forum_topic),
            None => json,
        }
    }
}
