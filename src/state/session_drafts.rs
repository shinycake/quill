//! Composer drafts keyed by chat and topic (Telegram Desktop's
//! `Data::DraftKey`): the chat itself, a forum topic, or a reply thread.
use super::*;
use crate::composer::DraftTopic;

impl Session {
    /// The conversation the view in place shows inside `chat_id`: a comment
    /// thread, a forum topic, a channel's direct-messages conversation, a
    /// Saved Messages sublist, or the chat itself. Two of them in one chat
    /// show different messages, so what a view opened for one (a selection,
    /// an AI rewrite) is keyed by this, not by the chat.
    pub fn open_conversation(&self, chat_id: ChatId) -> crate::ids::ConversationKey {
        crate::ids::ConversationKey {
            account: self.account.clone(),
            chat_id,
            topic: self.open_topic_id(chat_id),
        }
    }

    /// The topic half of [`Session::open_conversation`].
    pub fn open_topic_id(&self, chat_id: ChatId) -> crate::ids::TopicId {
        use crate::ids::TopicId;
        // A thread opened inside a forum topic shows its replies only, so
        // it is a conversation of its own, unlike its draft slot.
        if let Some(thread) = self.thread_for_chat(chat_id) {
            return TopicId::Thread {
                message_thread_id: thread.thread_id,
            };
        }
        if let Some(topic) = self.open_direct_topic(chat_id) {
            return TopicId::DirectMessages {
                direct_messages_chat_topic_id: topic,
            };
        }
        if self.open_chat != Some(chat_id) {
            return TopicId::None;
        }
        if let Some(topic) = self.open_topic {
            return TopicId::forum(topic);
        }
        match self.threads.saved.sublist.as_ref() {
            Some(sublist) if self.is_saved_messages(chat_id) => TopicId::SavedMessages {
                saved_messages_topic_id: sublist.topic_id,
            },
            _ => TopicId::None,
        }
    }

    /// The draft slot the composer of `chat_id` writes to right now: the
    /// open reply thread (`messageTopicThread`; inside a forum the thread
    /// stays in its topic, as sends do), else the open forum topic, else
    /// the chat (tdesktop `ChatWidget::resolvedTopicRootId`).
    pub fn open_draft_topic(&self, chat_id: ChatId) -> DraftTopic {
        if let Some(thread) = self.thread_for_chat(chat_id) {
            return match thread.forum_topic_id {
                Some(topic) => DraftTopic::Forum(topic),
                None => DraftTopic::Thread(thread.thread_id),
            };
        }
        if let Some(topic) = self.open_direct_topic(chat_id) {
            return DraftTopic::DirectMessages(topic);
        }
        match self.open_topic {
            Some(topic) if self.open_chat == Some(chat_id) => DraftTopic::Forum(topic),
            _ => DraftTopic::Chat,
        }
    }

    /// Whether drafts are kept for `topic` of `chat_id`. Telegram Desktop
    /// saves a cloud draft wherever the compose field is shown
    /// (`ApiWrap::saveCurrentDraftToCloud` runs for the active thread,
    /// whatever the peer), and TDLib refuses `setChatDraftMessage` where
    /// the user cannot write (`can_send_message`), so the gate is
    /// [`ChatSummary::can_post`]: users, bots, groups, channels the user
    /// posts in, and secret chats once ready (TDLib keeps those drafts
    /// locally). A chat split into topics drafts per topic only; a plain
    /// chat has no topic drafts.
    pub fn accepts_composer_draft(&self, chat_id: ChatId, topic: DraftTopic) -> bool {
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return false;
        };
        if !chat.can_post() {
            return false;
        }
        // Admins of a channel's direct messages write in one conversation
        // at a time; TDLib wants its topic on every draft.
        if self.is_direct_messages_admin(chat_id) {
            return matches!(topic, DraftTopic::DirectMessages(_));
        }
        let has_topics = self.chat_has_topics(chat_id);
        match topic {
            DraftTopic::Chat => !has_topics,
            DraftTopic::DirectMessages(_) => false,
            DraftTopic::Forum(_) => has_topics,
            DraftTopic::Thread(thread_id) => !has_topics && thread_id != 0,
        }
    }

    /// The stored draft of `topic` in `chat_id`, if any.
    pub fn draft_for(&self, chat_id: ChatId, topic: DraftTopic) -> Option<&ChatDraft> {
        match topic {
            DraftTopic::Chat => self.chats.get(&chat_id.0)?.draft.as_ref(),
            DraftTopic::Forum(topic_id) => self
                .threads
                .forum_topics
                .get(&chat_id.0)?
                .iter()
                .find(|topic| topic.forum_topic_id == topic_id)?
                .draft
                .as_ref(),
            DraftTopic::Thread(thread_id) => self
                .threads
                .thread
                .as_ref()
                .filter(|thread| thread.chat_id == chat_id && thread.thread_id == thread_id)?
                .draft
                .as_ref(),
            DraftTopic::DirectMessages(topic_id) => {
                self.threads.direct.get(chat_id.0, topic_id)?.draft.as_ref()
            }
        }
    }

    /// Composer text changed since the last persisted draft: remote
    /// updates for this slot must not replace it (schema comment on
    /// `updateChatDraftMessage`).
    pub fn mark_draft_dirty(&mut self, chat_id: ChatId, topic: DraftTopic) {
        self.messages.draft_dirty.insert(topic.key(chat_id.0));
    }

    /// The composer left the slot with its text unsaved (the save failed
    /// for good, or was dropped): remote drafts may replace it again.
    pub fn clear_draft_dirty(&mut self, chat_id: ChatId, topic: DraftTopic) {
        self.messages.draft_dirty.remove(&topic.key(chat_id.0));
    }

    pub fn draft_is_dirty(&self, chat_id: ChatId, topic: DraftTopic) -> bool {
        self.messages.draft_dirty.contains(&topic.key(chat_id.0))
    }

    /// Local persist (after `setChatDraftMessage`, or the demo path).
    /// Clears the dirty bit.
    pub fn store_composer_draft(
        &mut self,
        chat_id: ChatId,
        topic: DraftTopic,
        draft: Option<ChatDraft>,
    ) {
        self.write_draft_slot(chat_id, topic, draft);
        self.messages.draft_dirty.remove(&topic.key(chat_id.0));
    }

    /// A draft from TDLib (`chat.draft_message`, `updateChatDraftMessage`,
    /// `forumTopic` / `updateForumTopic`, `messageThreadInfo`). Skipped
    /// while the composer has unsaved changes for that slot.
    pub(crate) fn set_remote_draft(
        &mut self,
        chat_id: ChatId,
        topic: DraftTopic,
        draft: Option<ChatDraft>,
    ) {
        if self.draft_is_dirty(chat_id, topic) {
            return;
        }
        if topic == DraftTopic::Chat {
            let terms = draft.as_ref().and_then(|draft| draft.suggested_post);
            self.payments.suggested_posts.sync_draft(chat_id, terms);
        }
        if self.draft_for(chat_id, topic) != draft.as_ref() {
            self.messages.remote_draft_revision =
                self.messages.remote_draft_revision.wrapping_add(1);
        }
        self.write_draft_slot(chat_id, topic, draft);
    }

    /// Bumped each time a remote draft replaced a stored one
    /// ([`Self::set_remote_draft`]); composers showing a slot re-read its
    /// draft when this moves.
    pub fn remote_draft_revision(&self) -> u64 {
        self.messages.remote_draft_revision
    }

    fn write_draft_slot(&mut self, chat_id: ChatId, topic: DraftTopic, draft: Option<ChatDraft>) {
        match topic {
            DraftTopic::Chat => {
                if let Some(chat) = self.chats.get_mut(&chat_id.0) {
                    chat.draft = draft;
                }
            }
            DraftTopic::Forum(topic_id) => {
                self.threads
                    .forum_topics
                    .update_topic(chat_id.0, topic_id, |topic| topic.draft = draft);
            }
            DraftTopic::Thread(thread_id) => {
                if let Some(thread) = self
                    .threads
                    .thread
                    .as_mut()
                    .filter(|thread| thread.chat_id == chat_id && thread.thread_id == thread_id)
                {
                    thread.draft = draft;
                }
            }
            DraftTopic::DirectMessages(topic_id) => {
                if let Some(topic) = self
                    .threads
                    .direct
                    .topics
                    .get_mut(&chat_id.0)
                    .and_then(|topics| topics.get_mut(&topic_id))
                {
                    topic.draft = draft;
                }
            }
        }
    }

    /// A fresh topic list from TDLib replaces the cached one; topics whose
    /// composer has unsaved changes keep the draft the composer started
    /// from (the save in flight overwrites it).
    pub(crate) fn keep_dirty_topic_drafts(&self, chat_id: ChatId, topics: &mut [ForumTopic]) {
        for topic in topics {
            let slot = DraftTopic::Forum(topic.forum_topic_id);
            if self.draft_is_dirty(chat_id, slot) {
                topic.draft = self.draft_for(chat_id, slot).cloned();
            }
        }
    }

    /// The slot a sent message clears the draft of: its forum topic, its
    /// reply thread, or the chat.
    pub(crate) fn draft_topic_of_message(
        topic_id: Option<i32>,
        thread_id: Option<i64>,
    ) -> DraftTopic {
        match (topic_id, thread_id) {
            (Some(topic), _) => DraftTopic::Forum(topic),
            (None, Some(thread)) => DraftTopic::Thread(thread),
            (None, None) => DraftTopic::Chat,
        }
    }
}

impl ForumTopic {
    /// The draft line a topic row shows in place of its last message, as
    /// Telegram Desktop decides it for a thread row (`dialogs_layout.cpp`,
    /// `cloudDraft`): only when the topic has nothing unread. A reply-only
    /// draft shows too.
    pub fn draft_preview(&self) -> Option<DraftPreview> {
        let draft = self.draft.as_ref()?;
        if self.unread_count > 0 {
            return None;
        }
        let text = draft.preview_text().replace('\n', " ").trim().to_string();
        let reply = draft.reply_to_message_id.is_some();
        (!text.is_empty() || reply).then_some(DraftPreview { text, reply })
    }
}
