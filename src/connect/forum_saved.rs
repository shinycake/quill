//! Connect driver: forum view mode / topic extras and Saved Messages
//! sublists and tags (batch B16).
use super::*;
use crate::ids::{ChatId, FileId, RequestId};
use crate::state::ThreadsPurpose;
use crate::state::{RequestPurpose, SAVED_PAGE, SAVED_TOPICS_PAGE};
use crate::telegram::envelope::ReactionType;
use crate::telegram::requests::{TOPIC_ICON_COLORS, valid_topic_icon_color};
use crate::telegram::requests::{
    create_forum_topic_with_icon, delete_saved_messages_topic_history, edit_forum_topic_with_icon,
    get_forum_topic, get_forum_topic_default_icons, get_forum_topic_link, get_saved_messages_tags,
    get_saved_messages_topic_history, load_saved_messages_topics, read_all_forum_topic_mentions,
    read_all_forum_topic_reactions, search_saved_messages, set_pinned_forum_topics,
    set_saved_messages_tag_label, toggle_chat_view_as_topics, toggle_saved_messages_topic_pinned,
    unpin_all_forum_topic_messages,
};

type Sent = Result<Option<RequestId>, ConnectSendError>;

impl<S: JsonSender> ConnectDriver<S> {
    /// Send one request built from its id; the pending entry is dropped
    /// again when the send fails.
    fn send_purpose(
        &mut self,
        purpose: RequestPurpose,
        chat_id: Option<ChatId>,
        build: impl FnOnce(RequestId) -> String,
    ) -> Sent {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(purpose, chat_id);
        if let Err(err) = self.sender.send_json(&build(extra)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    // ---- forums --------------------------------------------------------

    /// "View as Topics" / "View as Messages" (`toggleChatViewAsTopics`).
    /// The new value arrives with `updateChatViewAsTopics`.
    pub fn toggle_view_as_topics(&mut self, chat_id: ChatId, view_as_topics: bool) -> Sent {
        if !self.session.chats.contains_key(&chat_id.0) {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::ToggleChatViewAsTopics, chat_id)
        {
            return Ok(None);
        }
        self.send_purpose(
            RequestPurpose::ToggleChatViewAsTopics,
            Some(chat_id),
            |extra| toggle_chat_view_as_topics(extra, chat_id, view_as_topics),
        )
    }

    /// `getForumTopicDefaultIcons` once; the answer fills
    /// `Session::forum_topic_icons`.
    pub fn load_forum_topic_icons(&mut self) -> Sent {
        if !self.session.forum_topic_icons.is_empty()
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::GetForumTopicDefaultIcons)
        {
            return Ok(None);
        }
        self.send_purpose(RequestPurpose::GetForumTopicDefaultIcons, None, |extra| {
            get_forum_topic_default_icons(extra)
        })
    }

    /// Download the picker's icon thumbnails that are not local yet.
    pub fn download_forum_topic_icons(&mut self) {
        let files: Vec<FileId> = self
            .session
            .forum_topic_icons
            .iter()
            .filter_map(|sticker| sticker.display_file_id())
            .collect();
        for file in files {
            let _ = self.download_file(file, THUMB_DOWNLOAD_PRIORITY);
        }
    }

    /// "Copy Topic Link": the `messageLink` answer lands in
    /// `Session::message_link_result`, which the UI copies.
    pub fn copy_forum_topic_link(&mut self, chat_id: ChatId, forum_topic_id: i32) -> Sent {
        if !self.session.chat_has_topics(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.send_purpose(RequestPurpose::GetForumTopicLink, Some(chat_id), |extra| {
            get_forum_topic_link(extra, chat_id, forum_topic_id)
        })
    }

    /// Move a pinned topic one step (`setPinnedForumTopics`); the cache is
    /// reordered at once.
    pub fn move_pinned_forum_topic(
        &mut self,
        chat_id: ChatId,
        forum_topic_id: i32,
        up: bool,
    ) -> Sent {
        if !self.session.chat_can_manage_topics(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(order) = self.session.moved_pinned_order(chat_id, forum_topic_id, up) else {
            return Ok(None);
        };
        let sent = self.send_purpose(
            RequestPurpose::SetPinnedForumTopics,
            Some(chat_id),
            |extra| set_pinned_forum_topics(extra, chat_id, &order),
        )?;
        self.session.apply_pinned_forum_order(chat_id, &order);
        Ok(sent)
    }

    /// "Mark all mentions as read" in one topic.
    pub fn read_all_forum_topic_mentions(&mut self, chat_id: ChatId, forum_topic_id: i32) -> Sent {
        let purpose =
            RequestPurpose::Threads(ThreadsPurpose::ReadAllForumTopicMentions { forum_topic_id });
        let sent = self.send_purpose(purpose, Some(chat_id), |extra| {
            read_all_forum_topic_mentions(extra, chat_id, forum_topic_id)
        })?;
        self.refetch_topic(chat_id, forum_topic_id);
        Ok(sent)
    }

    /// "Read all reactions" in one topic.
    pub fn read_all_forum_topic_reactions(&mut self, chat_id: ChatId, forum_topic_id: i32) -> Sent {
        let purpose =
            RequestPurpose::Threads(ThreadsPurpose::ReadAllForumTopicReactions { forum_topic_id });
        let sent = self.send_purpose(purpose, Some(chat_id), |extra| {
            read_all_forum_topic_reactions(extra, chat_id, forum_topic_id)
        })?;
        self.refetch_topic(chat_id, forum_topic_id);
        Ok(sent)
    }

    /// "Unpin all messages" in one topic.
    pub fn unpin_all_forum_topic_messages(&mut self, chat_id: ChatId, forum_topic_id: i32) -> Sent {
        let purpose =
            RequestPurpose::Threads(ThreadsPurpose::UnpinAllForumTopicMessages { forum_topic_id });
        self.send_purpose(purpose, Some(chat_id), |extra| {
            unpin_all_forum_topic_messages(extra, chat_id, forum_topic_id)
        })
    }

    /// `getForumTopic` so the badges come from TDLib again.
    fn refetch_topic(&mut self, chat_id: ChatId, forum_topic_id: i32) {
        let purpose = RequestPurpose::Threads(ThreadsPurpose::GetForumTopic { forum_topic_id });
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return;
        }
        let _ = self.send_purpose(purpose, Some(chat_id), |extra| {
            get_forum_topic(extra, chat_id, forum_topic_id)
        });
    }

    /// Create a topic with a chosen icon (`createForumTopic`).
    pub fn create_forum_topic_with_icon(
        &mut self,
        chat_id: ChatId,
        name: &str,
        color: i32,
        custom_emoji_id: i64,
    ) -> Sent {
        if name.trim().is_empty() || !valid_topic_icon_color(color) || custom_emoji_id < 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.chat_can_manage_topics(chat_id)
            && self.session.bot_topics(chat_id).is_none()
        {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::CreateForumTopic)
        {
            return Ok(None);
        }
        self.send_purpose(RequestPurpose::CreateForumTopic, Some(chat_id), |extra| {
            create_forum_topic_with_icon(extra, chat_id, name.trim(), color, custom_emoji_id)
        })
    }

    /// Rename a topic and change its custom emoji (`editForumTopic`).
    pub fn edit_forum_topic_with_icon(
        &mut self,
        chat_id: ChatId,
        forum_topic_id: i32,
        name: &str,
        icon_custom_emoji_id: i64,
    ) -> Sent {
        if name.trim().is_empty() || icon_custom_emoji_id < 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::Threads(ThreadsPurpose::EditForumTopic { forum_topic_id });
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        self.send_purpose(purpose, Some(chat_id), |extra| {
            edit_forum_topic_with_icon(
                extra,
                chat_id,
                forum_topic_id,
                name.trim(),
                icon_custom_emoji_id,
            )
        })
    }

    // ---- Saved Messages sublists ----------------------------------------

    /// Ask for more sublists (`loadSavedMessagesTopics`); deduped, and
    /// silent once TDLib said everything is loaded.
    pub fn load_saved_topics(&mut self) -> Sent {
        if self.session.saved.topics_exhausted
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::LoadSavedMessagesTopics)
        {
            return Ok(None);
        }
        self.session.saved.topics_requested = true;
        self.send_purpose(RequestPurpose::LoadSavedMessagesTopics, None, |extra| {
            load_saved_messages_topics(extra, SAVED_TOPICS_PAGE)
        })
    }

    /// `getSavedMessagesTags` for all of Saved Messages (once; updates keep
    /// it fresh) or for one sublist.
    pub fn load_saved_tags(&mut self, topic_id: i64) -> Sent {
        let purpose = RequestPurpose::GetSavedMessagesTags { topic_id };
        if self.session.requests.has_purpose(purpose) {
            return Ok(None);
        }
        self.send_purpose(purpose, None, |extra| {
            get_saved_messages_tags(extra, topic_id)
        })
    }

    /// Enter a sublist: reset its view, load its first page and its tags.
    pub fn open_saved_sublist(&mut self, topic_id: i64) -> Sent {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.open_saved_sublist(topic_id);
        let _ = self.load_saved_tags(topic_id);
        self.fetch_saved_sublist_history()
    }

    /// Back from a sublist (or a tag filter) to the sublist list.
    pub fn close_saved_sublist(&mut self) {
        self.session.close_saved_sublist();
    }

    /// The next page of the open sublist.
    pub fn fetch_saved_sublist_history(&mut self) -> Sent {
        let Some(view) = self.session.saved.sublist.as_ref() else {
            return Ok(None);
        };
        if view.history.loaded_complete {
            return Ok(None);
        }
        let (topic_id, from) = (view.topic_id, view.history.next_from_message_id.0);
        let purpose = RequestPurpose::GetSavedMessagesTopicHistory { topic_id };
        if self.session.requests.has_purpose(purpose) {
            return Ok(None);
        }
        self.send_purpose(purpose, None, |extra| {
            get_saved_messages_topic_history(extra, topic_id, from, 0, SAVED_PAGE)
        })
    }

    /// Pin or unpin a sublist (`toggleSavedMessagesTopicIsPinned`); the new
    /// order arrives with `updateSavedMessagesTopic`.
    pub fn toggle_saved_topic_pinned(&mut self, topic_id: i64, pinned: bool) -> Sent {
        let purpose =
            RequestPurpose::Threads(ThreadsPurpose::ToggleSavedMessagesTopicPinned { topic_id });
        if self.session.requests.has_purpose(purpose) {
            return Ok(None);
        }
        self.send_purpose(purpose, None, |extra| {
            toggle_saved_messages_topic_pinned(extra, topic_id, pinned)
        })
    }

    /// Delete everything saved from one chat
    /// (`deleteSavedMessagesTopicHistory`). The UI confirms first.
    pub fn delete_saved_topic_history(&mut self, topic_id: i64) -> Sent {
        let purpose =
            RequestPurpose::Threads(ThreadsPurpose::DeleteSavedMessagesTopicHistory { topic_id });
        if self.session.requests.has_purpose(purpose) {
            return Ok(None);
        }
        self.send_purpose(purpose, None, |extra| {
            delete_saved_messages_topic_history(extra, topic_id)
        })
    }

    // ---- Saved Messages tags --------------------------------------------

    /// "Add Name" / "Edit Name" on a tag (`setSavedMessagesTagLabel`,
    /// Premium only). An empty label removes the name.
    pub fn set_saved_tag_label(&mut self, tag: &ReactionType, label: &str) -> Sent {
        let premium = self
            .session
            .my_user_id
            .and_then(|id| self.session.user(id))
            .is_some_and(|user| user.is_premium);
        if !premium {
            return Err(ConnectSendError::InvalidRequest);
        }
        let label = crate::state::clean_tag_label(label);
        let json = tag.to_tdlib_json();
        let sent = self.send_purpose(RequestPurpose::SetSavedMessagesTagLabel, None, |extra| {
            set_saved_messages_tag_label(extra, &json, &label)
        })?;
        // The label shows right away; `updateSavedMessagesTags` confirms.
        if let Some(entry) = self
            .session
            .saved
            .tags
            .iter_mut()
            .find(|entry| entry.tag == *tag)
        {
            entry.label = label;
        }
        Ok(sent)
    }

    /// "Filter by Tag": search Saved Messages (or the open sublist) for
    /// messages carrying `tag`.
    pub fn filter_saved_by_tag(&mut self, tag: ReactionType) -> Sent {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let topic_id = self
            .session
            .saved
            .sublist
            .as_ref()
            .map_or(0, |view| view.topic_id);
        self.session.begin_saved_tag_search(topic_id, tag);
        self.fetch_saved_tag_page()
    }

    /// The next page of the tag filter.
    pub fn fetch_saved_tag_page(&mut self) -> Sent {
        let Some(search) = self.session.saved.tag_search.as_ref() else {
            return Ok(None);
        };
        if search.history.loaded_complete {
            return Ok(None);
        }
        let topic_id = search.topic_id;
        let from = search.history.next_from_message_id.0;
        let tag = search.tag.to_tdlib_json();
        let purpose = RequestPurpose::SearchSavedMessages { topic_id };
        if self.session.requests.has_purpose(purpose) {
            return Ok(None);
        }
        self.send_purpose(purpose, None, |extra| {
            search_saved_messages(extra, topic_id, Some(&tag), "", from, SAVED_PAGE)
        })
    }

    /// Drop the tag filter ("All" in the tags bar).
    pub fn clear_saved_tag_filter(&mut self) {
        self.session.clear_saved_tag_search();
    }

    /// Colors a new topic icon may take (`TOPIC_ICON_COLORS`).
    pub fn topic_icon_colors() -> &'static [i32] {
        &TOPIC_ICON_COLORS
    }
}
