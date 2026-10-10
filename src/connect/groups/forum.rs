//! Connect driver: forum topics (list, selection, history, create / edit / toggle / delete).
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
    /// Phase 5.1: `getSupergroup` for a non-channel supergroup whose forum
    /// status is still unknown. Fires once (deduped by cache + in-flight
    /// purpose); the `supergroup` response and `updateSupergroup` both
    /// populate `ChatSummary::is_forum`. No-op for channels, non-supergroups,
    /// and already-resolved chats.
    pub(crate) fn maybe_fetch_supergroup_forum(
        &mut self,
        chat_id: ChatId,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        let supergroup_id = match self.session.chats.get(&chat_id.0) {
            Some(chat)
                if matches!(
                    chat.kind,
                    ChatKind::Supergroup {
                        is_channel: false,
                        ..
                    }
                ) && chat.is_forum.is_none() =>
            {
                match chat.kind {
                    ChatKind::Supergroup { supergroup_id, .. } => supergroup_id,
                    _ => return Ok(()),
                }
            }
            _ => return Ok(()),
        };
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetSupergroup, chat_id)
        {
            return Ok(());
        }
        let extra = self
            .session
            .request(RequestPurpose::GetSupergroup, Some(chat_id));
        if let Err(err) = self.sender.send_json(&get_supergroup(extra, supergroup_id)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(())
    }

    /// Phase 5.1: `getForumTopics` (first page) for a known forum supergroup.
    /// Fires once per chat (deduped by cache + in-flight purpose). No-op
    /// until `is_forum` resolves true.
    /// Slice G2: force a `getForumTopics` refresh (the manage dialog
    /// calls this after a mutation so the list shows the new state;
    /// the state layer already drops the cache on confirmed
    /// create/delete).
    pub fn refresh_forum_topics(&mut self, chat_id: ChatId) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.forum_topics.remove(&chat_id.0);
        self.maybe_fetch_forum_topics(chat_id)
    }

    pub(crate) fn maybe_fetch_forum_topics(
        &mut self,
        chat_id: ChatId,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        // Subsection tabs: bots with topics answer `getForumTopics` too.
        if !self.session.chat_has_topics(chat_id) {
            return Ok(());
        }
        if self.session.forum_topics.contains_key(&chat_id.0) {
            return Ok(());
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetForumTopics, chat_id)
        {
            return Ok(());
        }
        let extra = self
            .session
            .request(RequestPurpose::GetForumTopics, Some(chat_id));
        if let Err(err) = self.sender.send_json(&get_forum_topics(
            extra,
            chat_id,
            "",
            0,
            MessageId(0),
            0,
            FORUM_TOPICS_LIMIT,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(())
    }

    /// Phase 5.1: select a forum topic. The topic's history is fetched with
    /// `searchChatMessages` (`topic_id = messageTopicForum`, empty query)
    /// and rendered by the same history component as chat history.
    pub fn select_topic(
        &mut self,
        forum_topic_id: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat_id) = self.session.open_chat else {
            return Err(ConnectSendError::InvalidRequest);
        };
        // A topic already in the loaded list is selectable whatever the
        // chat kind (bots with topics, forums).
        let known_topic = self
            .session
            .forum_topics
            .get(&chat_id.0)
            .is_some_and(|topics| topics.iter().any(|t| t.forum_topic_id == forum_topic_id));
        if !known_topic && !self.session.chat_has_topics(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.select_topic(chat_id, forum_topic_id);
        self.fetch_topic_history()
    }

    /// Phase 5.1: leave the topic view, back to the forum's topic list.
    pub fn deselect_topic(&mut self) {
        self.session.deselect_topic();
    }

    /// Phase 5.1: page the open topic's history (`searchChatMessages` with
    /// `topic_id`). First page starts at `from_message_id` 0; later pages
    /// continue from the response's `next_from_message_id`.
    pub fn fetch_topic_history(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat_id) = self.session.open_chat else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let Some(forum_topic_id) = self.session.open_topic else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let key = (chat_id.0, forum_topic_id);
        if self
            .session
            .topic_histories
            .get(&key)
            .is_some_and(|h| h.loaded_complete)
        {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetTopicHistory, chat_id)
        {
            return Ok(None);
        }
        let from = self
            .session
            .topic_histories
            .get(&key)
            .map(|h| h.next_from_message_id)
            .unwrap_or(MessageId(0));
        let extra = self.session.request_for_topic(
            RequestPurpose::GetTopicHistory,
            Some(chat_id),
            forum_topic_id,
        );
        let topic = TopicId::Forum {
            forum_topic_id: forum_topic_id as i64,
        };
        match self.sender.send_json(&search_chat_messages(
            extra,
            chat_id,
            &topic,
            "",
            from,
            0,
            TOPIC_HISTORY_PAGE_SIZE,
            None,
        )) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Slice G2: supergroup id for a non-channel supergroup chat —
    /// `None` for everything else (channels, basic groups, unknowns).
    fn forum_supergroup(&self, chat_id: ChatId) -> Option<i64> {
        self.session
            .chats
            .get(&chat_id.0)
            .and_then(|chat| match chat.kind {
                ChatKind::Supergroup {
                    supergroup_id,
                    is_channel: false,
                } => Some(supergroup_id),
                _ => None,
            })
    }

    /// Slice G2: gate shared by every forum-topic mutation — requires
    /// the viewer to hold `can_manage_topics` in a non-channel
    /// supergroup.
    /// Subsection tabs: a bot chat with topics passes too — pin / unpin
    /// and delete work there (schema 1.8.67, lines 12725 / 12736).
    pub(crate) fn forum_topic_gate(&self, chat_id: ChatId) -> bool {
        (self.forum_supergroup(chat_id).is_some() && self.session.chat_can_manage_topics(chat_id))
            || self.session.bot_topics(chat_id).is_some()
    }

    /// Slice G2: `createForumTopic` (schema 1.8.67, line 12665).
    /// Answers `forumTopicInfo`; the cached topic list is refetched on
    /// success. Returns `Err` for an empty name.
    pub fn create_forum_topic(
        &mut self,
        chat_id: ChatId,
        name: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if name.trim().is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.forum_topic_gate(chat_id) {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::CreateForumTopic)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::CreateForumTopic, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&create_forum_topic(extra, chat_id, name.trim()))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G2: `editForumTopic` (schema 1.8.67, line 12674) — renames
    /// the topic. Answers `ok`; the topic list is refetched on success.
    pub fn edit_forum_topic(
        &mut self,
        chat_id: ChatId,
        forum_topic_id: i32,
        name: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if name.trim().is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.forum_topic_gate(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::Threads(ThreadsPurpose::EditForumTopic { forum_topic_id });
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self.sender.send_json(&edit_forum_topic(
            extra,
            chat_id,
            forum_topic_id,
            name.trim(),
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G2: `toggleForumTopicIsClosed` (schema 1.8.67, line 12713).
    /// Answers `ok`; the topic list is refetched on success.
    pub fn toggle_forum_topic_closed(
        &mut self,
        chat_id: ChatId,
        forum_topic_id: i32,
        closed: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.toggle_forum_topic_flag(
            chat_id,
            forum_topic_id,
            RequestPurpose::Threads(ThreadsPurpose::ToggleForumTopicClosed { forum_topic_id }),
            closed,
        )
    }

    /// Slice G2: `toggleForumTopicIsPinned` (schema 1.8.67, line 12725).
    /// Answers `ok`; the topic list is refetched on success.
    pub fn toggle_forum_topic_pinned(
        &mut self,
        chat_id: ChatId,
        forum_topic_id: i32,
        pinned: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.toggle_forum_topic_flag(
            chat_id,
            forum_topic_id,
            RequestPurpose::Threads(ThreadsPurpose::ToggleForumTopicPinned { forum_topic_id }),
            pinned,
        )
    }

    /// Slice G2: shared sender for the two boolean forum-topic toggles.
    fn toggle_forum_topic_flag(
        &mut self,
        chat_id: ChatId,
        forum_topic_id: i32,
        purpose: RequestPurpose,
        flag: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.forum_topic_gate(chat_id) {
            return Ok(None);
        }
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        let sent = if matches!(
            purpose,
            RequestPurpose::Threads(ThreadsPurpose::ToggleForumTopicClosed { .. })
        ) {
            self.sender.send_json(&toggle_forum_topic_closed(
                extra,
                chat_id,
                forum_topic_id,
                flag,
            ))
        } else {
            self.sender.send_json(&toggle_forum_topic_pinned(
                extra,
                chat_id,
                forum_topic_id,
                flag,
            ))
        };
        if let Err(err) = sent {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G2: `deleteForumTopic` (schema 1.8.67, line 12736).
    /// Answers `ok`; the topic list is refetched on success.
    pub fn delete_forum_topic(
        &mut self,
        chat_id: ChatId,
        forum_topic_id: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.forum_topic_gate(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::Threads(ThreadsPurpose::DeleteForumTopic { forum_topic_id });
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&delete_forum_topic(extra, chat_id, forum_topic_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G2: `toggleGeneralForumTopicIsHidden` (schema 1.8.67, line
    /// 12718). Answers `ok`; the topic list is refetched on success.
    pub fn toggle_general_forum_topic_hidden(
        &mut self,
        chat_id: ChatId,
        hidden: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.forum_topic_gate(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::ToggleGeneralForumTopicHidden;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&toggle_general_forum_topic_hidden(extra, chat_id, hidden))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }
}
