//! A channel's direct messages as its admins see them: one conversation per
//! user (tdesktop `Data::SavedMessages` owned by a monoforum, its
//! `SavedSublist`s).
#![deny(clippy::indexing_slicing)]

use std::collections::{BTreeMap, HashMap};

use crate::telegram::envelope::DirectMessagesTopic;

/// The conversations of every administered direct messages chat.
#[derive(Debug, Default)]
pub struct DirectMessagesTopics {
    /// Conversations by chat id, then topic id.
    pub topics: HashMap<i64, BTreeMap<i64, DirectMessagesTopic>>,
    /// The conversation shown in each chat; none = the All view. It stays
    /// after the chat is left, so opening the chat again lands on it
    /// (tdesktop `activeSubsectionThread`).
    pub open: HashMap<i64, i64>,
    /// How loading each chat's conversations stands. No entry: the first
    /// `loadDirectMessagesChatTopics` was not sent yet.
    pub loads: HashMap<i64, TopicsLoad>,
    /// `true` = banned from the channel, by `(channel chat id, user id)`
    /// (tdesktop `kickedValue`). Missing = not asked yet.
    pub kicked: HashMap<(i64, i64), bool>,
    /// Stars a conversation has paid, by `(chat id, topic id)`
    /// (`getDirectMessagesChatTopicRevenue`).
    pub revenue: HashMap<(i64, i64), i64>,
    /// `getDirectMessagesChatTopicHistory`'s `total_count` per open
    /// conversation, for the header's "N messages".
    pub message_counts: HashMap<(i64, i64), i32>,
}

/// How far loading one chat's conversations got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopicsLoad {
    /// A page was asked for (and may have answered).
    Asked,
    /// The last page failed with an error that is not the end of the list.
    /// The preload does not ask again (tdesktop does not either); scrolling
    /// the tabs does.
    Failed,
    /// `loadDirectMessagesChatTopics` answered 404: all are loaded.
    Exhausted,
}

impl DirectMessagesTopics {
    /// Every conversation of `chat_id` is loaded.
    pub fn exhausted(&self, chat_id: i64) -> bool {
        self.loads.get(&chat_id) == Some(&TopicsLoad::Exhausted)
    }

    /// The first page was asked for (the preload happens once).
    pub fn preloaded(&self, chat_id: i64) -> bool {
        self.loads.contains_key(&chat_id)
    }

    /// A page went out.
    pub fn page_asked(&mut self, chat_id: i64) {
        let load = self.loads.entry(chat_id).or_insert(TopicsLoad::Asked);
        if *load == TopicsLoad::Failed {
            *load = TopicsLoad::Asked;
        }
    }

    /// A page failed with `code`: 404 is the end of the list.
    pub fn page_failed(&mut self, chat_id: i64, code: i32) {
        let load = if code == 404 {
            TopicsLoad::Exhausted
        } else {
            TopicsLoad::Failed
        };
        self.loads.insert(chat_id, load);
    }

    /// The conversations of one chat in list order: `order` descending.
    pub fn ordered(&self, chat_id: i64) -> Vec<&DirectMessagesTopic> {
        let mut topics: Vec<&DirectMessagesTopic> = self
            .topics
            .get(&chat_id)
            .map(|topics| topics.values().filter(|t| t.order != 0).collect())
            .unwrap_or_default();
        topics.sort_by(|a, b| b.order.cmp(&a.order).then(a.id.cmp(&b.id)));
        topics
    }

    pub fn get(&self, chat_id: i64, topic_id: i64) -> Option<&DirectMessagesTopic> {
        self.topics.get(&chat_id)?.get(&topic_id)
    }

    pub fn open_topic(&self, chat_id: i64) -> Option<i64> {
        self.open.get(&chat_id).copied()
    }

    /// Conversations with unread messages or marked unread: the chat-list
    /// badge counts these (tdesktop `CountInBadge::Chats`).
    pub fn unread_topics(&self, chat_id: i64) -> usize {
        self.topics.get(&chat_id).map_or(0, |topics| {
            topics
                .values()
                .filter(|t| t.order != 0 && t.is_unread())
                .count()
        })
    }

    /// Unread messages across every conversation (the All view's
    /// jump-down counter, tdesktop `chatListUnreadState().messages`).
    pub fn unread_messages(&self, chat_id: i64) -> i32 {
        self.topics.get(&chat_id).map_or(0, |topics| {
            topics
                .values()
                .map(|t| t.unread_count.max(0))
                .fold(0i32, i32::saturating_add)
        })
    }
}
