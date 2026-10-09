//! Forum view mode and Saved Messages sublists / tags: the reducer side.
use super::*;
use crate::telegram::envelope::{
    ReactionType, SavedMessagesTag, SavedMessagesTopic, SavedTopicKind,
};

/// Page size of `getSavedMessagesTopicHistory` / `searchSavedMessages`.
pub const SAVED_PAGE: i32 = 50;
/// How many sublists one `loadSavedMessagesTopics` asks for.
pub const SAVED_TOPICS_PAGE: i32 = 50;

impl Session {
    /// `chat.view_as_topics` / `updateChatViewAsTopics`.
    pub fn set_chat_view_as_topics(&mut self, chat_id: i64, view_as_topics: bool) {
        self.chat_view_as_topics.insert(chat_id, view_as_topics);
        self.view_generation.bump();
    }

    /// Whether a forum shows its topic list (the default) or its plain
    /// message history. Saved Messages shows its sublists only when TDLib
    /// said so.
    pub fn chat_views_as_topics(&self, chat_id: ChatId) -> bool {
        let explicit = self.chat_view_as_topics.get(&chat_id.0).copied();
        if self.is_saved_messages(chat_id) {
            return explicit.unwrap_or(false);
        }
        explicit.unwrap_or(true)
    }

    // ---- forum topic extras ------------------------------------------

    /// The pinned topics of a loaded forum, in their current pin order
    /// (`order` descending, which is how TDLib lists them).
    pub fn pinned_forum_topic_ids(&self, chat_id: ChatId) -> Vec<i32> {
        self.ordered_forum_topics(chat_id)
            .into_iter()
            .filter(|topic| topic.is_pinned)
            .map(|topic| topic.forum_topic_id)
            .collect()
    }

    /// The pin order after moving `forum_topic_id` one step up or down
    /// among the pinned topics; `None` when it cannot move.
    pub fn moved_pinned_order(
        &self,
        chat_id: ChatId,
        forum_topic_id: i32,
        up: bool,
    ) -> Option<Vec<i32>> {
        let mut ids = self.pinned_forum_topic_ids(chat_id);
        let at = ids.iter().position(|id| *id == forum_topic_id)?;
        let to = if up { at.checked_sub(1)? } else { at + 1 };
        if to >= ids.len() {
            return None;
        }
        ids.swap(at, to);
        Some(ids)
    }

    /// Reorder the cached pinned topics right away (the server confirms
    /// with `updateForumTopic`).
    pub(crate) fn apply_pinned_forum_order(&mut self, chat_id: ChatId, ids: &[i32]) {
        let Some(topics) = self.forum_topics.get_mut(&chat_id.0) else {
            return;
        };
        let mut orders: Vec<i64> = topics
            .iter()
            .filter(|topic| ids.contains(&topic.forum_topic_id))
            .map(|topic| topic.order)
            .collect();
        orders.sort_unstable_by(|a, b| b.cmp(a));
        for (id, order) in ids.iter().zip(orders) {
            if let Some(topic) = topics.iter_mut().find(|topic| topic.forum_topic_id == *id) {
                topic.order = order;
            }
        }
    }

    /// Zero the mention / reaction badges of one topic after a read-all.
    pub(crate) fn clear_topic_marks(
        &mut self,
        chat_id: ChatId,
        forum_topic_id: i32,
        mentions: bool,
    ) {
        if let Some(topic) = self.forum_topics.get_mut(&chat_id.0).and_then(|topics| {
            topics
                .iter_mut()
                .find(|t| t.forum_topic_id == forum_topic_id)
        }) {
            if mentions {
                topic.unread_mention_count = 0;
            } else {
                topic.unread_reaction_count = 0;
            }
        }
    }

    /// `getForumTopicDefaultIcons` answered.
    pub(crate) fn accept_topic_default_icons(&mut self, stickers: Vec<StickerItem>) {
        self.forum_topic_icons = stickers
            .into_iter()
            .filter(|sticker| sticker.custom_emoji_id.is_some_and(|id| id > 0))
            .collect();
    }

    // ---- Saved Messages sublists -------------------------------------

    /// `updateSavedMessagesTopic`.
    pub(crate) fn apply_saved_topic(&mut self, topic: SavedMessagesTopic) {
        self.saved.topics.insert(topic.id, topic);
        self.view_generation.bump();
    }

    /// `updateSavedMessagesTags` / `getSavedMessagesTags`.
    pub(crate) fn apply_saved_tags(&mut self, topic_id: i64, tags: Vec<SavedMessagesTag>) {
        if topic_id == 0 {
            self.saved.tags = tags;
            self.saved.tags_loaded = true;
        } else {
            self.saved.topic_tags.insert(topic_id, tags);
        }
        self.view_generation.bump();
    }

    /// The name of a sublist: the source chat's title, "My Notes" or
    /// "Author Hidden" (tdesktop `TopBarNameText`).
    pub fn saved_topic_title(&self, topic: &SavedMessagesTopic) -> String {
        match topic.kind {
            SavedTopicKind::MyNotes => "My Notes".to_owned(),
            SavedTopicKind::AuthorHidden => {
                if topic.hidden_sender_name.is_empty() {
                    "Author Hidden".to_owned()
                } else {
                    topic.hidden_sender_name.clone()
                }
            }
            SavedTopicKind::FromChat(chat_id) => self
                .chats
                .get(&chat_id)
                .map(|chat| chat.title.clone())
                .or_else(|| {
                    self.user(chat_id)
                        .map(|user| user.display_name())
                        .filter(|name| !name.is_empty())
                })
                .unwrap_or_else(|| "Unknown chat".to_owned()),
        }
    }

    /// Enter a sublist (the history loads next).
    pub fn open_saved_sublist(&mut self, topic_id: i64) {
        self.saved.tag_search = None;
        self.saved.sublist = Some(SavedSublistView {
            topic_id,
            history: TopicHistory::default(),
        });
        self.view_generation.bump();
    }

    /// Leave the sublist and any tag filter, back to the sublist list.
    pub fn close_saved_sublist(&mut self) {
        self.saved.close_views();
        self.view_generation.bump();
    }

    /// Start (or replace) a tag filter over `topic_id` (0 = everything).
    pub fn begin_saved_tag_search(&mut self, topic_id: i64, tag: ReactionType) {
        self.saved.tag_search = Some(SavedTagSearch {
            topic_id,
            tag,
            history: TopicHistory::default(),
            loaded: false,
        });
        self.view_generation.bump();
    }

    /// Clear the tag filter ("All" in the tags bar).
    pub fn clear_saved_tag_search(&mut self) {
        self.saved.tag_search = None;
        self.view_generation.bump();
    }

    /// One `getSavedMessagesTopicHistory` page.
    pub(crate) fn apply_saved_topic_history(
        &mut self,
        messages: Vec<ParsedMessage>,
        pending: Option<&PendingRequest>,
    ) {
        let Some(RequestPurpose::GetSavedMessagesTopicHistory { topic_id }) =
            pending.map(|p| p.purpose)
        else {
            return;
        };
        for message in &messages {
            self.remember_files(&message.files);
        }
        let Some(view) = self
            .saved
            .sublist
            .as_mut()
            .filter(|view| view.topic_id == topic_id)
        else {
            return;
        };
        let empty = messages.is_empty();
        let mut oldest: Option<i64> = None;
        let mut added = 0usize;
        for message in messages {
            let id = message.id.0;
            oldest = Some(oldest.map_or(id, |old| old.min(id)));
            if view
                .history
                .messages
                .insert(id, history_message(message, false))
                .is_none()
            {
                added += 1;
            }
        }
        if empty || added == 0 {
            view.history.loaded_complete = true;
        }
        if let Some(oldest) = oldest {
            view.history.next_from_message_id = MessageId(oldest);
        }
        view.history.total_count = view.history.messages.len() as i32;
        self.view_generation.bump();
    }

    /// One `searchSavedMessages` page (`foundChatMessages`).
    pub(crate) fn apply_saved_tag_page(
        &mut self,
        messages: Vec<ParsedMessage>,
        total_count: i32,
        next_from_message_id: MessageId,
        pending: Option<&PendingRequest>,
    ) {
        let Some(RequestPurpose::SearchSavedMessages { topic_id }) = pending.map(|p| p.purpose)
        else {
            return;
        };
        for message in &messages {
            self.remember_files(&message.files);
        }
        let Some(search) = self
            .saved
            .tag_search
            .as_mut()
            .filter(|search| search.topic_id == topic_id)
        else {
            return;
        };
        let empty = messages.is_empty();
        for message in messages {
            search
                .history
                .messages
                .insert(message.id.0, history_message(message, false));
        }
        if next_from_message_id.0 == 0 || empty {
            search.history.loaded_complete = true;
        }
        search.history.next_from_message_id = next_from_message_id;
        search.history.total_count = total_count.max(search.history.messages.len() as i32);
        search.loaded = true;
        self.view_generation.bump();
    }

    /// `deleteSavedMessagesTopicHistory` succeeded: the sublist is gone.
    pub(crate) fn remove_saved_topic(&mut self, topic_id: i64) {
        self.saved.topics.remove(&topic_id);
        self.saved.topic_tags.remove(&topic_id);
        if self
            .saved
            .sublist
            .as_ref()
            .is_some_and(|view| view.topic_id == topic_id)
        {
            self.saved.close_views();
        }
        self.view_generation.bump();
    }

    /// Rows removed by `updateDeleteMessages` leave the sublist and the
    /// tag filter too.
    pub(crate) fn saved_remove_messages(&mut self, chat_id: ChatId, ids: &[MessageId]) {
        if !self.is_saved_messages(chat_id) {
            return;
        }
        if let Some(view) = self.saved.sublist.as_mut() {
            for id in ids {
                view.history.messages.remove(&id.0);
            }
        }
        if let Some(search) = self.saved.tag_search.as_mut() {
            for id in ids {
                search.history.messages.remove(&id.0);
            }
        }
    }

    /// The tags the bar offers for the current context: the open sublist's
    /// own tags (named from the global list) or all tags.
    pub fn saved_tag_choices(&self) -> Vec<SavedMessagesTag> {
        let Some(view) = &self.saved.sublist else {
            return self.saved.tags.clone();
        };
        self.saved
            .topic_tags
            .get(&view.topic_id)
            .map(|tags| {
                tags.iter()
                    .map(|tag| {
                        let mut tag = tag.clone();
                        if tag.label.is_empty()
                            && let Some(named) =
                                self.saved.tags.iter().find(|other| other.tag == tag.tag)
                        {
                            tag.label = named.label.clone();
                        }
                        tag
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}
