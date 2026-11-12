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

    /// The `begin` ask for `chat_id` went out as `request`: only its
    /// answer or error acts.
    pub fn sent(&mut self, chat_id: i64, request: RequestId) {
        if let Some(list) = self.lists.get_mut(&chat_id) {
            list.sent(request);
        }
    }

    /// The topics changed on the server: what is held stays until the
    /// refetch replaces it, and an ask in flight (it predates the change)
    /// is no longer waited for. The chat list asks again.
    pub fn outdate(&mut self, chat_id: i64) {
        if let Some(list) = self.lists.get_mut(&chat_id) {
            list.outdate();
        }
    }

    /// A first page is owed: never asked, or outdated by a change.
    pub fn wants_fetch(&self, chat_id: i64) -> bool {
        self.lists.get(&chat_id).is_none_or(Fetch::wants_fetch)
    }

    /// `request` answered with `chat_id`'s first page: only the ask that
    /// waits for it takes it.
    pub fn answered(
        &mut self,
        chat_id: i64,
        request: Option<RequestId>,
        topics: Vec<ForumTopic>,
    ) -> bool {
        let unread = counted_unread(&topics);
        // An ask other than the one waited for, or one a change outdated or
        // a reset forgot, is stale and lands nowhere.
        let Some(list) = self.lists.get_mut(&chat_id) else {
            return false;
        };
        let taken = list.answered(request, topics);
        if taken {
            self.unread.insert(chat_id, unread);
        }
        taken
    }

    /// `request` was refused: only the ask that waits for it fails.
    pub fn failed_request(
        &mut self,
        chat_id: i64,
        request: Option<RequestId>,
        error: impl Into<String>,
    ) -> bool {
        // Only the ask in flight fails: another ask's refusal, an outdated
        // one's, or one against a list that is loaded or never asked is
        // stale and changes nothing.
        self.lists
            .get_mut(&chat_id)
            .is_some_and(|list| list.failed(request, error, false))
    }

    /// A failed ask for `chat_id` is forgotten (the held list, if any,
    /// stays), so the next ask goes out: opening the chat retries it.
    pub fn clear_failure(&mut self, chat_id: i64) {
        if let Some(list) = self.lists.get_mut(&chat_id) {
            list.clear_error();
        }
    }

    /// `getForumTopics` for `chat_id` never left.
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

impl Session {
    /// `getForumTopics` for `chat_id` as the driver sends it: the request is
    /// registered and the topic list waits for exactly that request. The
    /// driver's own sender records the same after its send went out; this
    /// is the one a fixture or demo that plays the answer uses.
    pub fn ask_forum_topics(&mut self, chat_id: ChatId) -> RequestId {
        let request = self.request(RequestPurpose::GetForumTopics, Some(chat_id));
        self.threads.forum_topics.begin(chat_id.0);
        self.threads.forum_topics.sent(chat_id.0, request);
        request
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
        map.sent(5, RequestId(1));
        assert!(map.asked(5) && !map.failed(5) && !map.contains_key(&5));
        assert!(map.answered(5, Some(RequestId(1)), Vec::new()));
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
