//! A channel's direct messages as its admins see them: one conversation per
//! user (tdesktop `Data::SavedMessages` owned by a monoforum, its
//! `SavedSublist`s).
#![deny(clippy::indexing_slicing)]

use std::collections::{BTreeMap, HashMap};

use crate::ids::RequestId;
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
    /// Whether the user is banned from the channel (`true`), by
    /// `(channel chat id, user id)` (tdesktop `kickedValue`): asked, known,
    /// or failed. Missing = not asked yet.
    pub kicked: HashMap<(i64, i64), crate::state::fetch::Fetch<bool>>,
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
    /// A page is in flight as `request`; only its answer or error leaves
    /// this stage.
    Loading { request: RequestId },
    /// The last page answered; the next one may be asked.
    Loaded,
    /// The last page failed with an error that is not the end of the list.
    /// The update-driven preload does not ask again; opening the chat or
    /// scrolling the tabs does.
    Failed,
    /// `loadDirectMessagesChatTopics` answered 404: all are loaded.
    Exhausted,
}

impl DirectMessagesTopics {
    /// Every conversation of `chat_id` is loaded.
    pub fn exhausted(&self, chat_id: i64) -> bool {
        self.loads.get(&chat_id) == Some(&TopicsLoad::Exhausted)
    }

    /// A page of `chat_id` is in flight.
    pub fn loading(&self, chat_id: i64) -> bool {
        matches!(self.loads.get(&chat_id), Some(TopicsLoad::Loading { .. }))
    }

    /// The first page went out and did not fail (or the list ended). A
    /// failed first page is not loaded: opening the chat asks again.
    pub fn preloaded(&self, chat_id: i64) -> bool {
        matches!(
            self.loads.get(&chat_id),
            Some(TopicsLoad::Loading { .. } | TopicsLoad::Loaded | TopicsLoad::Exhausted)
        )
    }

    /// No page was ever asked for: the update-driven preload sends the
    /// first one (a failed page is retried on open, not on every update).
    pub fn never_asked(&self, chat_id: i64) -> bool {
        !self.loads.contains_key(&chat_id)
    }

    /// A page went out as `request`.
    pub fn page_asked(&mut self, chat_id: i64, request: RequestId) {
        self.loads.insert(chat_id, TopicsLoad::Loading { request });
    }

    /// The page `request` answered: the next one may be asked.
    pub fn page_answered(&mut self, chat_id: i64, request: Option<RequestId>) {
        if self.waits_for(chat_id, request) {
            self.loads.insert(chat_id, TopicsLoad::Loaded);
        }
    }

    /// The page `request` failed with `code`: 404 is the end of the list.
    /// `false` when `request` is not the page in flight (an older page's
    /// error changes nothing).
    pub fn page_failed(&mut self, chat_id: i64, request: Option<RequestId>, code: i32) -> bool {
        if !self.waits_for(chat_id, request) {
            return false;
        }
        let load = if code == 404 {
            TopicsLoad::Exhausted
        } else {
            TopicsLoad::Failed
        };
        self.loads.insert(chat_id, load);
        true
    }

    fn waits_for(&self, chat_id: i64, request: Option<RequestId>) -> bool {
        matches!(
            self.loads.get(&chat_id),
            Some(TopicsLoad::Loading { request: asked }) if Some(*asked) == request
        )
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
