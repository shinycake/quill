//! Connect driver: the open forum topic's newer side, once its window
//! stopped short of the latest message (`TopicHistory::has_newer`).
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
    /// Load the page after the open topic's newest loaded message. A
    /// negative offset returns `-offset` messages newer than `from` plus
    /// `from` itself (schema `searchChatMessages`).
    pub fn fetch_topic_history_newer(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let (Some(chat_id), Some(topic)) = (self.session.open_chat, self.session.open_topic) else {
            return Ok(None);
        };
        let Some(from) = self
            .session
            .threads
            .topic_histories
            .get(&(chat_id.0, topic))
            .filter(|history| history.has_newer && !history.newer_failed)
            .and_then(|history| history.newest_loaded_id())
        else {
            return Ok(None);
        };
        let purpose = RequestPurpose::Threads(ThreadsPurpose::GetTopicHistoryNewer);
        if self
            .session
            .topic_page_in_flight(chat_id, topic, WindowEnd::Newest)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request_for_topic(purpose, Some(chat_id), topic);
        let topic_id = TopicId::Forum {
            forum_topic_id: i64::from(topic),
        };
        if let Err(err) = self.sender.send_json(&search_chat_messages(
            extra,
            chat_id,
            &topic_id,
            "",
            from,
            -(TOPIC_HISTORY_PAGE_SIZE - 1),
            TOPIC_HISTORY_PAGE_SIZE,
            None,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// The jump-to-latest button in a topic: a window short of the latest
    /// message is replaced by the latest page (the UI scrolls to the end
    /// once it lands). `Ok(None)` when the window already ends at the
    /// latest: the UI only scrolls then.
    pub fn topic_jump_to_latest(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        let (Some(chat_id), Some(topic)) = (self.session.open_chat, self.session.open_topic) else {
            return Ok(None);
        };
        let short = self
            .session
            .threads
            .topic_histories
            .get(&(chat_id.0, topic))
            .is_some_and(|history| history.has_newer);
        if !short || !self.session.reset_topic_window(chat_id, topic) {
            return Ok(None);
        }
        self.fetch_topic_history()
    }
}
