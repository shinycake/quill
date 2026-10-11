//! Composer drafts keyed by chat and topic (Telegram Desktop's
//! `Data::DraftKey`): the chat itself, a forum topic, or a reply thread.
use super::*;
use crate::composer::DraftTopic;

impl Session {
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
        let has_topics = self.chat_has_topics(chat_id);
        match topic {
            DraftTopic::Chat => !has_topics,
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
        }
    }

    /// Composer text changed since the last persisted draft: remote
    /// updates for this slot must not replace it (schema comment on
    /// `updateChatDraftMessage`).
    pub fn mark_draft_dirty(&mut self, chat_id: ChatId, topic: DraftTopic) {
        self.messages.draft_dirty.insert(topic.key(chat_id.0));
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
        self.write_draft_slot(chat_id, topic, draft);
    }

    fn write_draft_slot(&mut self, chat_id: ChatId, topic: DraftTopic, draft: Option<ChatDraft>) {
        match topic {
            DraftTopic::Chat => {
                if let Some(chat) = self.chats.get_mut(&chat_id.0) {
                    chat.draft = draft;
                }
            }
            DraftTopic::Forum(topic_id) => {
                if let Some(topic) = self
                    .threads
                    .forum_topics
                    .get_mut(&chat_id.0)
                    .and_then(|topics| topics.iter_mut().find(|t| t.forum_topic_id == topic_id))
                {
                    topic.draft = draft;
                }
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

    /// The message a draft replies to, looked up where the composer is:
    /// the chat history, the open thread, or the open topic's history.
    pub fn draft_reply_preview(&self, chat_id: ChatId, message_id: MessageId) -> Option<String> {
        let row = self
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .or_else(|| {
                self.threads
                    .thread
                    .as_ref()
                    .filter(|thread| thread.chat_id == chat_id)
                    .and_then(|thread| thread.history.messages.get(&message_id.0))
            })
            .or_else(|| {
                let topic = self.open_topic?;
                self.threads
                    .topic_histories
                    .get(&(chat_id.0, topic))
                    .and_then(|history| history.messages.get(&message_id.0))
            })?;
        Some(effective_preview(row))
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
        let text = draft.text.replace('\n', " ").trim().to_string();
        let reply = draft.reply_to_message_id.is_some();
        (!text.is_empty() || reply).then_some(DraftPreview { text, reply })
    }
}
