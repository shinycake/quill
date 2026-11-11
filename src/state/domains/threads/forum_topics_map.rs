//! The loaded topics of each forum, with each forum's counted unread kept
//! as its topics change, so the badge reads one number per forum instead
//! of walking every topic.
use crate::state::*;

/// One forum's topic list: loaded (with its counted unread), or the load
/// failed. A forum with neither has no entry; it is asked for.
#[derive(Debug)]
enum TopicList {
    Loaded {
        topics: Vec<ForumTopic>,
        unread: i32,
    },
    Failed,
}

#[derive(Debug, Default)]
pub struct ForumTopicsMap {
    lists: HashMap<i64, TopicList>,
}

impl ForumTopicsMap {
    /// The loaded topics of `chat_id`.
    pub fn get(&self, chat_id: &i64) -> Option<&Vec<ForumTopic>> {
        match self.lists.get(chat_id)? {
            TopicList::Loaded { topics, .. } => Some(topics),
            TopicList::Failed => None,
        }
    }

    pub fn contains_key(&self, chat_id: &i64) -> bool {
        self.get(chat_id).is_some()
    }

    /// Every forum's loaded topics.
    pub fn values(&self) -> impl Iterator<Item = &Vec<ForumTopic>> {
        self.lists.values().filter_map(|list| match list {
            TopicList::Loaded { topics, .. } => Some(topics),
            TopicList::Failed => None,
        })
    }

    /// The forums with loaded topics.
    pub fn keys(&self) -> impl Iterator<Item = &i64> {
        self.lists.iter().filter_map(|(chat_id, list)| match list {
            TopicList::Loaded { .. } => Some(chat_id),
            TopicList::Failed => None,
        })
    }

    /// Whether the topics of `chat_id` could not be loaded: its rows read
    /// "No chats" and do not ask again.
    pub fn failed(&self, chat_id: &i64) -> bool {
        matches!(self.lists.get(chat_id), Some(TopicList::Failed))
    }

    /// The load of `chat_id` failed. Loaded topics stay.
    pub fn fail(&mut self, chat_id: i64) {
        self.lists.entry(chat_id).or_insert(TopicList::Failed);
    }

    /// Forget a failure so the forum is asked for again.
    pub fn clear_failure(&mut self, chat_id: &i64) {
        if self.failed(chat_id) {
            self.lists.remove(chat_id);
        }
    }

    pub fn insert(&mut self, chat_id: i64, topics: Vec<ForumTopic>) -> Option<Vec<ForumTopic>> {
        let unread = counted_unread(&topics);
        match self
            .lists
            .insert(chat_id, TopicList::Loaded { topics, unread })
        {
            Some(TopicList::Loaded { topics, .. }) => Some(topics),
            _ => None,
        }
    }

    /// Forget the forum's topics and any failure.
    pub fn remove(&mut self, chat_id: &i64) -> Option<Vec<ForumTopic>> {
        match self.lists.remove(chat_id) {
            Some(TopicList::Loaded { topics, .. }) => Some(topics),
            _ => None,
        }
    }

    pub fn clear(&mut self) {
        self.lists.clear();
    }

    /// Change `chat_id`'s loaded topics; `None` when none are loaded.
    pub fn update<R>(
        &mut self,
        chat_id: i64,
        change: impl FnOnce(&mut Vec<ForumTopic>) -> R,
    ) -> Option<R> {
        let Some(TopicList::Loaded { topics, unread }) = self.lists.get_mut(&chat_id) else {
            return None;
        };
        let result = change(topics);
        *unread = counted_unread(topics);
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
        match self.lists.get(&chat_id) {
            Some(TopicList::Loaded { unread, .. }) => *unread,
            _ => 0,
        }
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
