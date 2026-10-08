//! Connect driver: supergroups, channels, communities, forums, boosts.
use super::*;
use crate::ids::{ChatId, MessageId, RequestId, TopicId};
use crate::state::{ChatStatisticsFetch, RequestPurpose, RequestRollback, WelcomeMessagesFetch};
use crate::telegram::envelope::ChatKind;
use crate::telegram::requests::{
    add_chat_welcome_message, boost_chat, create_community, create_forum_topic,
    create_new_basic_group_chat, create_new_supergroup_chat, delete_chat_welcome_message,
    delete_forum_topic, edit_chat_welcome_message, edit_forum_topic,
    get_available_chat_boost_slots, get_chat_boost_status, get_chat_history, get_chat_member,
    get_chat_statistics, get_forum_topics, get_me, get_message_thread_history, get_supergroup,
    get_supergroup_full_info, join_chat, leave_chat, load_chat_welcome_messages,
    load_community_full_info, search_chat_messages, set_chat_description, set_chat_member_tag,
    set_chat_message_auto_delete_time, set_chat_photo, set_chat_slow_mode_delay, set_chat_title,
    set_community_name, toggle_forum_topic_closed, toggle_forum_topic_pinned,
    toggle_general_forum_topic_hidden, toggle_supergroup_aggressive_anti_spam,
    toggle_supergroup_is_broadcast_group, toggle_supergroup_sign_messages,
};
use crate::telegram::requests_group_stickers::{
    set_supergroup_custom_emoji_sticker_set, set_supergroup_sticker_set,
};

impl<S: JsonSender> ConnectDriver<S> {
    pub fn load_group_sticker_choices(&mut self, chat_id: ChatId) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() || !self.session.chat_can_set_sticker_set(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.stickers.failed = false;
        self.session.emoji.failed = false;
        for (purpose, build) in [
            (
                RequestPurpose::GetInstalledStickerSets,
                crate::telegram::requests::get_installed_sticker_sets as fn(RequestId) -> String,
            ),
            (
                RequestPurpose::GetInstalledEmojiSets,
                crate::telegram::requests_emoji::get_installed_emoji_sets
                    as fn(RequestId) -> String,
            ),
        ] {
            if self.session.requests.has_purpose(purpose) {
                continue;
            }
            let extra = self.session.request(purpose, None);
            if let Err(err) = self.sender.send_json(&build(extra)) {
                self.session.requests.take(extra);
                return Err(err);
            }
        }
        Ok(())
    }

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

    /// Parity slice: `getSupergroup` for the channel/supergroup header's
    /// @username (and `is_forum` for non-channels). Fires once per
    /// supergroup — deduped by the `supergroup_usernames` cache (which
    /// spontaneous `updateSupergroup` updates also fill) and the in-flight
    /// `GetSupergroup` purpose, so it never doubles
    /// `maybe_fetch_supergroup_forum`'s request.
    pub(crate) fn maybe_fetch_supergroup_profile(
        &mut self,
        chat_id: ChatId,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        let supergroup_id = match self.session.chats.get(&chat_id.0) {
            Some(chat) => match chat.kind {
                ChatKind::Supergroup { supergroup_id, .. }
                    if !self
                        .session
                        .supergroup_usernames
                        .contains_key(&supergroup_id) =>
                {
                    supergroup_id
                }
                _ => return Ok(()),
            },
            None => return Ok(()),
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

    /// Parity slice: `getSupergroupFullInfo` for the channel/supergroup
    /// header (description snippet, subscriber/member count, linked
    /// discussion group). Deduped by the cache + in-flight purpose inside
    /// `fetch_supergroup_full_info`.
    pub(crate) fn maybe_fetch_supergroup_full_info_for_header(
        &mut self,
        chat_id: ChatId,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        let supergroup_id = match self.session.chats.get(&chat_id.0) {
            Some(chat) => match chat.kind {
                ChatKind::Supergroup { supergroup_id, .. } => supergroup_id,
                _ => return Ok(()),
            },
            None => return Ok(()),
        };
        self.fetch_supergroup_full_info(supergroup_id).map(|_| ())
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

    /// Own membership probe for the open broadcast channel: `getMe` once, then
    /// `getChatMember`. Drives the composer gate and the join/leave affordance
    /// (`ChannelMemberStatus`). No-op for non-channels.
    pub(crate) fn maybe_probe_channel_membership(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        let Some(chat_id) = self.session.open_chat else {
            return Ok(());
        };
        if !self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.is_channel())
        {
            return Ok(());
        }
        let Some(my_id) = self.session.my_user_id else {
            if self.session.requests.has_purpose(RequestPurpose::GetMe) {
                return Ok(());
            }
            let extra = self.session.request(RequestPurpose::GetMe, None);
            return self
                .sender
                .send_json(&get_me(extra))
                .map(|_| ())
                .inspect_err(|_| {
                    self.session.requests.take(extra);
                });
        };
        if self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.my_member_status.is_some())
        {
            return Ok(());
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetChatMember, chat_id)
        {
            return Ok(());
        }
        let extra = self
            .session
            .request(RequestPurpose::GetChatMember, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&get_chat_member(extra, chat_id, my_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(())
    }

    /// `joinChat` for a public channel. Own membership updates arrive via
    /// `updateChatMember`; the response also flips status optimistically.
    pub fn join_channel(&mut self, chat_id: ChatId) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::JoinChat, Some(chat_id));
        if let Err(err) = self.sender.send_json(&join_chat(extra, chat_id)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(())
    }

    /// `leaveChat` for a channel. Own membership updates arrive via
    /// `updateChatMember`; the `ok` response flips status optimistically.
    pub fn leave_channel(&mut self, chat_id: ChatId) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::LeaveChat, Some(chat_id));
        if let Err(err) = self.sender.send_json(&leave_chat(extra, chat_id)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(())
    }

    /// Phase B4: `setChatMessageAutoDeleteTime` (TDLib 1.8.67,
    /// `schema/td_api.tl:13454`) — the chat-level auto-delete or
    /// self-destruct (secret chats) timer. Value rule from the schema
    /// comment, enforced here (defense in depth — TDLib would 400 an
    /// out-of-rule value): secret chats accept any non-negative second
    /// value; other chats need 0 or a multiple of 86400 up to
    /// 365 * 86400. The new value arrives as
    /// `updateChatMessageAutoDeleteTime` (plus the service message in
    /// history); there is no optimistic state change.
    pub fn set_chat_message_auto_delete_time(
        &mut self,
        chat_id: ChatId,
        message_auto_delete_time: i32,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat) = self.session.chats.get(&chat_id.0) else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if !chat.supported() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let is_secret = matches!(chat.kind, ChatKind::Secret { .. });
        let valid = if message_auto_delete_time < 0 {
            false
        } else if is_secret {
            true
        } else {
            message_auto_delete_time == 0
                || (message_auto_delete_time % 86_400 == 0
                    && message_auto_delete_time <= 365 * 86_400)
        };
        if !valid {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SetChatMessageAutoDeleteTime, Some(chat_id));
        let json = set_chat_message_auto_delete_time(extra, chat_id.0, message_auto_delete_time);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 6: `getSupergroupFullInfo` for the group info panel. Deduped
    /// by cache + in-flight supergroup id; the id-less response correlates
    /// via `PendingRequest::supergroup_id`.
    pub fn fetch_supergroup_full_info(
        &mut self,
        supergroup_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .supergroup_full_infos
            .contains_key(&supergroup_id)
            || self
                .session
                .requests
                .has_purpose_for_supergroup(RequestPurpose::GetSupergroupFullInfo, supergroup_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request_for_supergroup(RequestPurpose::GetSupergroupFullInfo, supergroup_id);
        if let Err(err) = self
            .sender
            .send_json(&get_supergroup_full_info(extra, supergroup_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase D2: `getChatStatistics` (TDLib 1.8.67, line 15760). Sent only
    /// when `supergroupFullInfo.can_get_statistics` is true for the chat's
    /// supergroup (the schema gates the method on it). Idempotent: a
    /// cached `Loaded` result is kept until an explicit refresh clears it,
    /// and no second request goes out while one is in flight. Returns
    /// `Ok(None)` when nothing was sent.
    pub fn fetch_chat_statistics(
        &mut self,
        chat_id: ChatId,
        is_dark: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let can_get = self
            .session
            .chats
            .get(&chat_id.0)
            .and_then(|chat| match chat.kind {
                ChatKind::Supergroup { supergroup_id, .. } => {
                    self.session.supergroup_full_infos.get(&supergroup_id)
                }
                _ => None,
            })
            .is_some_and(|info| info.can_get_statistics);
        if !can_get {
            return Ok(None);
        }
        if matches!(
            self.session.chat_statistics.get(&chat_id.0),
            Some(ChatStatisticsFetch::Loading | ChatStatisticsFetch::Loaded(_))
        ) || self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetChatStatistics, chat_id)
        {
            return Ok(None);
        }
        self.session
            .chat_statistics
            .insert(chat_id.0, ChatStatisticsFetch::Loading);
        let extra = self
            .session
            .request(RequestPurpose::GetChatStatistics, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&get_chat_statistics(extra, chat_id.0, is_dark))
        {
            self.session.requests.take(extra);
            self.session.chat_statistics.remove(&chat_id.0);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase D2: explicit refresh of `getChatStatistics` — clears the
    /// cached result and re-sends (the plain fetch keeps `Loaded`).
    pub fn refresh_chat_statistics(
        &mut self,
        chat_id: ChatId,
        is_dark: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.session.chat_statistics.remove(&chat_id.0);
        self.fetch_chat_statistics(chat_id, is_dark)
    }

    /// Slice G1: `createNewBasicGroupChat` (schema 1.8.67, line 13327).
    /// The response is `createdBasicGroupChat`; the new chat itself
    /// arrives as `updateNewChat`.
    pub fn create_basic_group(
        &mut self,
        title: &str,
        user_ids: &[i64],
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::CreateBasicGroup;
        if self.session.requests.has_purpose(purpose) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, None);
        if let Err(err) = self
            .sender
            .send_json(&create_new_basic_group_chat(extra, user_ids, title))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G1: `createNewSupergroupChat` (schema 1.8.67, line 13337).
    /// `is_channel` selects channel vs. supergroup; the new chat arrives
    /// as `updateNewChat`.
    pub fn create_supergroup_channel(
        &mut self,
        title: &str,
        is_channel: bool,
        description: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::CreateSupergroupChannel { is_channel };
        if self.session.requests.has_purpose(purpose) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, None);
        if let Err(err) = self.sender.send_json(&create_new_supergroup_chat(
            extra,
            title,
            is_channel,
            description,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G1: `toggleSupergroupIsBroadcastGroup` (schema 1.8.67, line
    /// 15221). One-way upgrade of a supergroup to a broadcast group —
    /// the constructor takes no boolean and the schema offers no reverse.
    /// Requires owner privileges (line 15220). Applied optimistically;
    /// `updateSupergroup` confirms.
    pub fn upgrade_to_broadcast_group(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supergroup_id = match self.session.chats.get(&chat_id.0) {
            Some(chat) => match chat.kind {
                ChatKind::Supergroup {
                    supergroup_id,
                    is_channel: false,
                } => supergroup_id,
                _ => return Ok(None),
            },
            None => return Ok(None),
        };
        if !self.session.chat_is_owner(chat_id)
            || self
                .session
                .supergroup_is_broadcast
                .get(&supergroup_id)
                .is_some_and(|b| *b)
        {
            return Ok(None);
        }
        let purpose = RequestPurpose::ToggleBroadcastGroup;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&toggle_supergroup_is_broadcast_group(extra, supergroup_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        // Optimistic: the toggle is one-way, so a sent request means the
        // group becomes a broadcast group barring a TDLib error.
        self.session
            .supergroup_is_broadcast
            .insert(supergroup_id, true);
        Ok(Some(extra))
    }

    /// Slice: group/channel title edit — `setChatTitle` (schema 1.8.67,
    /// line 13430). Basic groups, supergroups and channels, gated on
    /// `can_change_info`. Title length is validated client-side (1–128
    /// characters per the schema); invalid input is refused with
    /// `InvalidRequest` before anything is sent. Not optimistic — the
    /// server's `updateChatTitle` carries the new title; a TDLib error is
    /// surfaced to the caller unchanged.
    pub fn set_group_title(
        &mut self,
        chat_id: ChatId,
        title: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let len = title.chars().count();
        if len == 0 || len > 128 {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.group_info_edit_allowed(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::SetChatTitle;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        let json = set_chat_title(extra, chat_id, title);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Slice: group/channel description edit — `setChatDescription`
    /// (schema 1.8.67, line 13533). Same gating as title; description is
    /// 0–255 characters, empty clears it. Not optimistic and not
    /// refetched: TDLib has no `updateChatDescription` broadcast, so the
    /// new description arrives on the next `getSupergroupFullInfo` /
    /// `getBasicGroupFullInfo` pull; errors are surfaced unchanged.
    pub fn set_group_description(
        &mut self,
        chat_id: ChatId,
        description: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if description.chars().count() > 255 {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.group_info_edit_allowed(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::SetChatDescription;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        let json = set_chat_description(extra, chat_id, description);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Slice: group/channel photo edit — `setChatPhoto` (schema 1.8.67,
    /// line 13435). Same gating as title. `photo_path` is a local JPEG
    /// file (`inputChatPhotoStatic` / `inputFileLocal`); `None` deletes
    /// the photo (a null top-level `photo` per the schema — "pass null to
    /// delete the chat photo"). Not optimistic — the server's
    /// `updateChatPhoto` carries the new photo; errors are surfaced
    /// unchanged.
    pub fn set_group_photo(
        &mut self,
        chat_id: ChatId,
        photo_path: Option<&str>,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.group_info_edit_allowed(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::SetChatPhoto;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        let photo_json = match photo_path {
            Some(path) => serde_json::json!({
                "@type": "inputChatPhotoStatic",
                "photo": { "@type": "inputFileLocal", "path": path },
            }),
            None => serde_json::Value::Null,
        };
        let json = set_chat_photo(extra, chat_id, photo_json);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Shared gate for the three group/channel info-edit drivers
    /// (`setChatTitle` / `setChatDescription` / `setChatPhoto`, all
    /// schema-limited to basic groups, supergroups and channels). Basic
    /// groups are democratic — every member may change the title,
    /// photo and description (telegram.org/blog/supergroups: "Everyone
    /// can invite new members and change the group's name and photo"),
    /// so no right is required there. Supergroups and channels need
    /// the `can_change_info` right: creator, an admin with the right
    /// (`Session::chat_can_change_info`), or a plain member with the
    /// default `permissions.can_change_info` permission (schema 1.8.67,
    /// line 1066 — "True, if the user can change the chat title, photo,
    /// and other settings"), mirroring `Session::chat_can_add_members`.
    fn group_info_edit_allowed(&self, chat_id: ChatId) -> bool {
        let Some(chat) = self.session.chats.get(&chat_id.0) else {
            return false;
        };
        match chat.kind {
            ChatKind::BasicGroup { .. } => true,
            ChatKind::Supergroup { .. } => {
                self.session.chat_can_change_info(chat_id)
                    || chat.permissions.as_ref().is_some_and(|p| p.can_change_info)
            }
            _ => false,
        }
    }

    /// Slice (communities backend core): `createCommunity` (schema 1.8.67,
    /// line 11806) — creates a community from an owned chat (owned basic
    /// group / supergroup / channel, or a chat with an owned bot; basic
    /// groups are auto-upgraded to supergroups). Empty names are refused
    /// client-side (`Err(InvalidRequest)`); the chat must exist or the
    /// call is refused with `Ok(None)`. The response is `communityId`
    /// (the update `updateCommunity` is guaranteed to arrive first); the
    /// ingest chain resolves the id into `loadCommunityFullInfo`.
    pub fn create_community(
        &mut self,
        chat_id: ChatId,
        name: &str,
        is_chat_hidden: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if name.trim().is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.chats.contains_key(&chat_id.0) {
            return Ok(None);
        }
        let purpose = RequestPurpose::CreateCommunity;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) =
            self.sender
                .send_json(&create_community(extra, name, chat_id.0, is_chat_hidden))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice (communities backend core): `loadCommunityFullInfo` (schema
    /// 1.8.67, line 11799). No-op when the pack is already cached or a
    /// fetch is in flight (deduped per community id); the response is
    /// `ok` and the pack arrives as `updateCommunityFullInfo`.
    pub fn load_community_full_info(
        &mut self,
        community_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::LoadCommunityFullInfo;
        if self
            .session
            .community_full_infos
            .contains_key(&community_id)
            || self
                .session
                .requests
                .has_purpose_for_community(purpose, community_id)
        {
            return Ok(None);
        }
        let extra = self.session.request_for_community(purpose, community_id);
        if let Err(err) = self
            .sender
            .send_json(&load_community_full_info(extra, community_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice (communities backend core): `setCommunityName` (schema 1.8.67,
    /// line 11811). Empty names are refused client-side
    /// (`Err(InvalidRequest)`). Not optimistic — the new name arrives via
    /// `updateCommunity`; on `ok` the state drops the cached full-info
    /// pack and the ingest refetches it (the welcome-message-mutation
    /// pattern). In-flight dedupe is per community id.
    pub fn set_community_name(
        &mut self,
        community_id: i64,
        name: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if name.trim().is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::SetCommunityName;
        if self
            .session
            .requests
            .has_purpose_for_community(purpose, community_id)
        {
            return Ok(None);
        }
        let extra = self.session.request_for_community(purpose, community_id);
        if let Err(err) = self
            .sender
            .send_json(&set_community_name(extra, community_id, name))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G2: `toggleSupergroupSignMessages` (schema 1.8.67, line
    /// 15175). Channels only; gated on `can_change_info` (creator or an
    /// admin with the right, like Telegram X's `ProfileController`).
    /// Optimistic — the error arm rolls back via `RequestRollback`.
    pub fn toggle_sign_messages(
        &mut self,
        chat_id: ChatId,
        sign_messages: bool,
        show_message_sender: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supergroup_id = match self.session.chats.get(&chat_id.0) {
            Some(chat) => match chat.kind {
                ChatKind::Supergroup {
                    supergroup_id,
                    is_channel: true,
                } => supergroup_id,
                _ => return Ok(None),
            },
            None => return Ok(None),
        };
        if !self.session.chat_can_change_info(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::ToggleSupergroupSignMessages;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self.sender.send_json(&toggle_supergroup_sign_messages(
            extra,
            supergroup_id,
            sign_messages,
            show_message_sender,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        // Optimistic: the previous flags ride on the pending entry so
        // the error arm can roll back.
        let previous_sign = self
            .session
            .supergroup_sign_messages
            .get(&supergroup_id)
            .copied();
        let previous_show = self
            .session
            .supergroup_show_message_sender
            .get(&supergroup_id)
            .copied();
        self.session
            .supergroup_sign_messages
            .insert(supergroup_id, sign_messages);
        self.session
            .supergroup_show_message_sender
            .insert(supergroup_id, sign_messages && show_message_sender);
        if let Some(pending) = self.session.requests.pending_mut(extra) {
            pending.rollback = Some(RequestRollback::SignMessages {
                supergroup_id,
                previous_sign,
                previous_show,
            });
        }
        Ok(Some(extra))
    }

    /// Slice G2: `toggleSupergroupHasAggressiveAntiSpamEnabled` (schema
    /// 1.8.67, line 15212). Non-channel supergroups only; the schema
    /// requires `supergroupFullInfo.can_toggle_aggressive_anti_spam`.
    /// Optimistic — the error arm rolls back via `RequestRollback`.
    pub fn toggle_aggressive_anti_spam(
        &mut self,
        chat_id: ChatId,
        enabled: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supergroup_id = match self.session.chats.get(&chat_id.0) {
            Some(chat) => match chat.kind {
                ChatKind::Supergroup {
                    supergroup_id,
                    is_channel: false,
                } => supergroup_id,
                _ => return Ok(None),
            },
            None => return Ok(None),
        };
        if !self.session.chat_can_toggle_anti_spam(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::ToggleSupergroupAggressiveAntiSpam;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&toggle_supergroup_aggressive_anti_spam(
                extra,
                supergroup_id,
                enabled,
            ))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        // Optimistic: the previous flag rides on the pending entry so
        // the error arm can roll back.
        let previous = self
            .session
            .supergroup_anti_spam_enabled
            .get(&supergroup_id)
            .copied();
        self.session
            .supergroup_anti_spam_enabled
            .insert(supergroup_id, enabled);
        if let Some(pending) = self.session.requests.pending_mut(extra) {
            pending.rollback = Some(RequestRollback::AntiSpam {
                supergroup_id,
                previous,
            });
        }
        Ok(Some(extra))
    }

    /// Slice S11: `setSupergroupStickerSet` (schema 1.8.67, line 15154).
    /// Gated on `supergroupFullInfo.can_set_sticker_set` (fail closed while
    /// the full info is unfetched). `sticker_set_id` 0 removes the group
    /// sticker set per the schema; negative ids are refused client-side.
    /// Not optimistic — `updateSupergroupFullInfo` carries the confirmed
    /// `sticker_set_id` back. In-flight dedup per chat.
    pub fn set_supergroup_sticker_set(
        &mut self,
        chat_id: ChatId,
        sticker_set_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if sticker_set_id < 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(supergroup_id) =
            self.session
                .chats
                .get(&chat_id.0)
                .and_then(|chat| match chat.kind {
                    ChatKind::Supergroup { supergroup_id, .. } => Some(supergroup_id),
                    _ => None,
                })
        else {
            return Ok(None);
        };
        if !self.session.chat_can_set_sticker_set(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::SetSupergroupStickerSet;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self.sender.send_json(&set_supergroup_sticker_set(
            extra,
            supergroup_id,
            sticker_set_id,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice S11: `setSupergroupCustomEmojiStickerSet` (schema 1.8.67,
    /// line 15159). Same gating as the regular group sticker set;
    /// `custom_emoji_sticker_set_id` 0 removes it per the schema.
    pub fn set_supergroup_custom_emoji_sticker_set(
        &mut self,
        chat_id: ChatId,
        custom_emoji_sticker_set_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if custom_emoji_sticker_set_id < 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(supergroup_id) =
            self.session
                .chats
                .get(&chat_id.0)
                .and_then(|chat| match chat.kind {
                    ChatKind::Supergroup { supergroup_id, .. } => Some(supergroup_id),
                    _ => None,
                })
        else {
            return Ok(None);
        };
        if !self.session.chat_can_set_sticker_set(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::SetSupergroupCustomEmojiStickerSet;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&set_supergroup_custom_emoji_sticker_set(
                extra,
                supergroup_id,
                custom_emoji_sticker_set_id,
            ))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
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
    fn forum_topic_gate(&self, chat_id: ChatId) -> bool {
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
        let purpose = RequestPurpose::EditForumTopic { forum_topic_id };
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
            RequestPurpose::ToggleForumTopicClosed { forum_topic_id },
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
            RequestPurpose::ToggleForumTopicPinned { forum_topic_id },
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
        let sent = if matches!(purpose, RequestPurpose::ToggleForumTopicClosed { .. }) {
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
        let purpose = RequestPurpose::DeleteForumTopic { forum_topic_id };
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

    /// Slice G2: `getMessageThreadHistory` (schema 1.8.67, line 11839)
    /// for the channel-comments viewer — the first page of the comment
    /// thread under a channel post. Deduped per channel post while one
    /// is in flight.
    pub fn fetch_message_thread_history(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::GetMessageThreadHistory {
            message_id: message_id.0,
        };
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self.sender.send_json(&get_message_thread_history(
            extra,
            chat_id,
            message_id,
            MessageId(0),
            50,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice CL: `getChatHistory` (schema 1.8.67, line 11829) for the
    /// chat-list peek preview — the most recent messages of a chat the
    /// user has not opened. Deduped per chat while one is in flight; the
    /// `messages` answer lands in `Session::chat_preview_fetch`. Read-only:
    /// no `openChat`, so nothing is marked read.
    pub fn fetch_chat_preview_history(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::GetChatPreview;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        self.session.chat_preview_fetch = None;
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self.sender.send_json(&get_chat_history(
            extra,
            chat_id,
            MessageId(0),
            0,
            PREVIEW_HISTORY_LIMIT,
            false,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G2: `getChatBoostStatus` (schema 1.8.67, line 13917) —
    /// cached per chat (level + boost count) for the boost dialog.
    pub fn fetch_chat_boost_status(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !matches!(
            self.session.chats.get(&chat_id.0).map(|chat| &chat.kind),
            Some(ChatKind::Supergroup { .. })
        ) {
            return Ok(None);
        }
        if self.session.chat_boost_status.contains_key(&chat_id.0)
            || self
                .session
                .requests
                .has_purpose_for_chat(RequestPurpose::GetChatBoostStatus, chat_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetChatBoostStatus, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&get_chat_boost_status(extra, chat_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G2: boost the chat. Sends `getAvailableChatBoostSlots`
    /// (schema 1.8.67, line 13914); the driver chains `boostChat` with
    /// the first slot once the answer arrives (`maybe_continue_boost`).
    /// Deduped while an intent or either request is in flight.
    pub fn request_chat_boost(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !matches!(
            self.session.chats.get(&chat_id.0).map(|chat| &chat.kind),
            Some(ChatKind::Supergroup { .. })
        ) {
            return Ok(None);
        }
        if self.session.boost_intent == Some(chat_id.0)
            || self
                .session
                .requests
                .has_purpose_for_chat(RequestPurpose::GetBoostSlotsForBoost, chat_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetBoostSlotsForBoost, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&get_available_chat_boost_slots(extra))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        self.session.boost_intent = Some(chat_id.0);
        Ok(Some(extra))
    }

    /// Slice G2: `boostChat` chain — once the slots answer for a pending
    /// boost intent arrives, send `boostChat` with the first slot id.
    /// An empty slot list (or a failed slots request) just drops the
    /// intent; `boostChat` errors are reported by the reducer.
    pub(crate) fn maybe_continue_boost(&mut self) -> Result<(), ConnectSendError> {
        let Some(chat_id) = self.session.boost_intent else {
            return Ok(());
        };
        let Some(slots) = self.session.boost_slots_by_chat.remove(&chat_id) else {
            return Ok(());
        };
        self.session.boost_intent = None;
        let Some(slot_id) = slots.into_iter().next() else {
            return Ok(());
        };
        let chat = ChatId(chat_id);
        let extra = self.session.request(RequestPurpose::BoostChat, Some(chat));
        if let Err(err) = self.sender.send_json(&boost_chat(extra, chat, &[slot_id])) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(())
    }

    /// Slice G2: `loadChatWelcomeMessages` (schema 1.8.67, line 12630).
    /// The pack also arrives spontaneously as `updateChatWelcomeMessages`;
    /// deduped on a cached pack or an in-flight fetch.
    pub fn load_chat_welcome_messages(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !matches!(
            self.session.chats.get(&chat_id.0).map(|chat| &chat.kind),
            Some(ChatKind::Supergroup { .. })
        ) {
            return Ok(None);
        }
        if self.session.welcome_messages.contains_key(&chat_id.0)
            || self
                .session
                .requests
                .has_purpose_for_chat(RequestPurpose::LoadChatWelcomeMessages, chat_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::LoadChatWelcomeMessages, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&load_chat_welcome_messages(extra, chat_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        self.session
            .welcome_message_fetches
            .insert(chat_id.0, WelcomeMessagesFetch::Loading);
        Ok(Some(extra))
    }

    /// Slice G2: shared gate for welcome-message mutations — requires
    /// `can_send_welcome_messages` (creator or an admin with the right)
    /// in a supergroup or channel.
    fn welcome_mutation_gate(&self, chat_id: ChatId) -> bool {
        matches!(
            self.session.chats.get(&chat_id.0).map(|chat| &chat.kind),
            Some(ChatKind::Supergroup { .. })
        ) && self.session.chat_can_send_welcome_messages(chat_id)
    }

    /// Slice G2: `addChatWelcomeMessage` (schema 1.8.67, line 12639).
    /// Answers `ok`; the pack is refetched on success.
    pub fn add_chat_welcome_message(
        &mut self,
        chat_id: ChatId,
        text: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if text.trim().is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.welcome_mutation_gate(chat_id) {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::AddChatWelcomeMessage)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::AddChatWelcomeMessage, Some(chat_id));
        if let Err(err) =
            self.sender
                .send_json(&add_chat_welcome_message(extra, chat_id, text.trim()))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G2: `editChatWelcomeMessage` (schema 1.8.67, line 12646).
    /// Answers `ok`; the pack is refetched on success.
    pub fn edit_chat_welcome_message(
        &mut self,
        chat_id: ChatId,
        welcome_message_id: i32,
        text: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if text.trim().is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.welcome_mutation_gate(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::EditChatWelcomeMessage { welcome_message_id };
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self.sender.send_json(&edit_chat_welcome_message(
            extra,
            chat_id,
            welcome_message_id,
            text.trim(),
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G2: `deleteChatWelcomeMessage` (schema 1.8.67, line 12651).
    /// Answers `ok`; the pack is refetched on success.
    pub fn delete_chat_welcome_message(
        &mut self,
        chat_id: ChatId,
        welcome_message_id: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.welcome_mutation_gate(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::DeleteChatWelcomeMessage { welcome_message_id };
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self.sender.send_json(&delete_chat_welcome_message(
            extra,
            chat_id,
            welcome_message_id,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G1: `setChatMemberTag` (schema 1.8.67, line 13598) — the
    /// admin custom-title setter (0-16 characters, no emoji; Telegram
    /// X `EditRightsController` enforces the same limits client-side).
    /// Basic groups and supergroups only, not channels. Gated on the
    /// caller being able to manage tags: owner/creator, or an admin
    /// with `can_manage_tags` (changing your own tag is allowed for
    /// any admin).
    pub fn set_chat_member_tag(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
        tag: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if tag.chars().count() > 16 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let me = self.session.my_user_id;
        let allowed = self.session.chats.get(&chat_id.0).is_some_and(|chat| {
            !matches!(
                chat.kind,
                ChatKind::Supergroup {
                    is_channel: true,
                    ..
                }
            )
        }) && (Some(user_id) == me
            || self.session.chat_is_owner(chat_id)
            || self.session.chat_can_manage_tags(chat_id));
        if !allowed {
            return Ok(None);
        }
        let purpose = RequestPurpose::SetChatMemberTag { user_id };
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&set_chat_member_tag(extra, chat_id, user_id, tag))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase A1: forced `getSupergroupFullInfo` refresh for the slow-mode
    /// gate. Unlike `fetch_supergroup_full_info` it ignores the "already
    /// fetched" cache: the schema (1.8.67, line 2759) warns no
    /// `updateSupergroupFullInfo` fires when only
    /// `slow_mode_delay_expires_in` changes, so a blocked send attempt
    /// re-reads the server value. In-flight requests are still deduped.
    pub fn refresh_supergroup_full_info(
        &mut self,
        supergroup_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .requests
            .has_purpose_for_supergroup(RequestPurpose::GetSupergroupFullInfo, supergroup_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request_for_supergroup(RequestPurpose::GetSupergroupFullInfo, supergroup_id);
        if let Err(err) = self
            .sender
            .send_json(&get_supergroup_full_info(extra, supergroup_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase A1: `setChatSlowModeDelay` (TDLib 1.8.67, line 13551) — the
    /// admin slow-mode control. `slow_mode_delay` must be one of 0, 5, 10,
    /// 30, 60, 300, 900, 3600. The new delay arrives via
    /// `updateSupergroupFullInfo`; `ok`/errors resolve through the pending
    /// request like other fire-and-forget setters.
    pub fn set_chat_slow_mode_delay(
        &mut self,
        chat_id: ChatId,
        slow_mode_delay: i32,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SetChatSlowModeDelay, Some(chat_id));
        if let Err(err) =
            self.sender
                .send_json(&set_chat_slow_mode_delay(extra, chat_id, slow_mode_delay))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }
}
