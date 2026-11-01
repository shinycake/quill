//! The loaded topics of each forum, with each forum's counted unread kept
//! as its topics change, so the badge reads one number per forum instead
//! of walking every topic.
use crate::state::*;
use std::ops::Deref;

#[derive(Debug, Default)]
pub struct ForumTopicsMap {
    topics: HashMap<i64, Vec<ForumTopic>>,
    unread: HashMap<i64, i32>,
}

impl Deref for ForumTopicsMap {
    type Target = HashMap<i64, Vec<ForumTopic>>;

    fn deref(&self) -> &Self::Target {
        &self.topics
    }
}

impl ForumTopicsMap {
    pub fn insert(&mut self, chat_id: i64, topics: Vec<ForumTopic>) -> Option<Vec<ForumTopic>> {
        self.unread.insert(chat_id, counted_unread(&topics));
        self.topics.insert(chat_id, topics)
    }

    pub fn remove(&mut self, chat_id: &i64) -> Option<Vec<ForumTopic>> {
        self.unread.remove(chat_id);
        self.topics.remove(chat_id)
    }

    pub fn clear(&mut self) {
        self.unread.clear();
        self.topics.clear();
    }

    /// Change `chat_id`'s loaded topics; `None` when none are loaded.
    pub fn update<R>(
        &mut self,
        chat_id: i64,
        change: impl FnOnce(&mut Vec<ForumTopic>) -> R,
    ) -> Option<R> {
        let topics = self.topics.get_mut(&chat_id)?;
        let result = change(topics);
        self.unread.insert(chat_id, counted_unread(topics));
        Some(result)
    }

    /// Change one loaded topic; `None` when it is not loaded.
    pub fn update_topic<R>(
        &mut self,
        chat_id: i64,
        forum_topic_id: i32,
        change: impl FnOnce(&mut ForumTopic) -> R,
    ) -> Option<R> {
        self.update(chat_id, |topics| {
            topics
                .iter_mut()
                .find(|topic| topic.forum_topic_id == forum_topic_id)
                .map(change)
        })
        .flatten()
    }

    /// The sum of [`topic_unread_count`] over `chat_id`'s loaded topics.
    pub fn unread(&self, chat_id: i64) -> i32 {
        self.unread.get(&chat_id).copied().unwrap_or(0)
    }
}

/// The topic's unread count when it can be trusted: only a topic with a
/// read position has a meaningful server count (see `Session::topic_badge`).
pub fn topic_unread_count(topic: &ForumTopic) -> Option<i32> {
    (topic.last_read_inbox_message_id > 1).then_some(topic.unread_count)
}

fn counted_unread(topics: &[ForumTopic]) -> i32 {
    topics
        .iter()
        .filter_map(topic_unread_count)
        .map(|count| count.max(0))
        .fold(0, i32::saturating_add)
}
