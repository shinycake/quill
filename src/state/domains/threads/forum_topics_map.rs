//! The loaded topics of each forum, with each forum's counted unread kept
//! as its topics change, so the badge reads one number per forum instead
//! of walking every topic.
use crate::state::fetch::Fetch;
use crate::state::*;

/// Each forum's topic list as one value: not asked, in flight, answered or
/// failed (the previous answer stays while a refetch runs or fails). The
/// chat list asks for a forum once; a failure reads "No chats" and is not
/// asked again.
#[derive(Debug, Default)]
pub struct ForumTopicsMap {
    lists: HashMap<i64, Fetch<Vec<ForumTopic>>>,
    unread: HashMap<i64, i32>,
}

impl ForumTopicsMap {
    /// The topics held for `chat_id` (the last answer, also while a
    /// refetch runs or after one failed).
    pub fn get(&self, chat_id: &i64) -> Option<&Vec<ForumTopic>> {
        self.lists.get(chat_id).and_then(Fetch::data)
    }

    /// Topics are held for `chat_id`.
    pub fn contains_key(&self, chat_id: &i64) -> bool {
        self.get(chat_id).is_some()
    }

    /// The forums whose topics are held.
    pub fn keys(&self) -> impl Iterator<Item = &i64> {
        self.lists
            .iter()
            .filter(|(_, list)| list.data().is_some())
            .map(|(chat_id, _)| chat_id)
    }

    /// The topic lists held, one per forum.
    pub fn values(&self) -> impl Iterator<Item = &Vec<ForumTopic>> {
        self.lists.values().filter_map(Fetch::data)
    }

    /// The first page was asked for (answered, in flight or failed).
    pub fn asked(&self, chat_id: i64) -> bool {
        self.lists.get(&chat_id).is_some_and(|list| !list.is_idle())
    }

    /// The request for `chat_id`'s topics failed.
    pub fn failed(&self, chat_id: i64) -> bool {
        self.lists
            .get(&chat_id)
            .is_some_and(|list| list.error().is_some())
    }

    /// `getForumTopics` for `chat_id` went out.
    pub fn begin(&mut self, chat_id: i64) {
        self.lists.entry(chat_id).or_default().begin();
    }

    /// `getForumTopics` for `chat_id` failed.
    pub fn fail(&mut self, chat_id: i64, error: impl Into<String>) {
        self.lists.entry(chat_id).or_default().fail(error);
    }

    /// The answer: the first page of `chat_id`'s topics.
    pub fn insert(&mut self, chat_id: i64, topics: Vec<ForumTopic>) {
        self.unread.insert(chat_id, counted_unread(&topics));
        self.lists.entry(chat_id).or_default().succeed(topics);
    }

    /// Forget `chat_id`'s list, asked or not: the chat list asks again.
    pub fn remove(&mut self, chat_id: &i64) {
        self.unread.remove(chat_id);
        self.lists.remove(chat_id);
    }

    pub fn clear(&mut self) {
        self.unread.clear();
        self.lists.clear();
    }

    /// Change `chat_id`'s loaded topics; `None` when none are loaded.
    pub fn update<R>(
        &mut self,
        chat_id: i64,
        change: impl FnOnce(&mut Vec<ForumTopic>) -> R,
    ) -> Option<R> {
        let topics = self.lists.get_mut(&chat_id)?.data_mut()?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_forum_list_is_asked_answered_or_failed_as_one_value() {
        let mut map = ForumTopicsMap::default();
        assert!(!map.asked(5) && !map.failed(5) && !map.contains_key(&5));
        map.begin(5);
        assert!(map.asked(5) && !map.failed(5) && !map.contains_key(&5));
        map.insert(5, Vec::new());
        assert!(map.asked(5) && map.contains_key(&5));
        // A failed refetch keeps the answer and does not ask again.
        map.begin(5);
        map.fail(5, "flood");
        assert!(map.failed(5) && map.asked(5) && map.contains_key(&5));
        // A failed first ask is final: "No chats", never asked again.
        map.fail(6, "invalid");
        assert!(map.failed(6) && map.asked(6) && !map.contains_key(&6));
        // Forgetting a forum lets the chat list ask again.
        map.remove(&6);
        assert!(!map.asked(6) && !map.failed(6));
        map.clear();
        assert!(!map.asked(5));
    }
}
