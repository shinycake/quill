//! Connect driver: supergroups, channels, communities, forums, boosts.
use super::*;
use crate::ids::{ChatId, MessageId, RequestId, TopicId};
use crate::state::{ChatStatisticsFetch, RequestPurpose, RequestRollback, WelcomeMessagesFetch};
use crate::state::{GroupsPurpose, ThreadsPurpose};
use crate::telegram::envelope::{ChatKind, ParsedCommunity};
use crate::telegram::requests::{
    add_chat_welcome_message, boost_chat, create_community, create_forum_topic,
    create_new_basic_group_chat, create_new_supergroup_chat, delete_chat_welcome_message,
    delete_community, delete_forum_topic, edit_chat_welcome_message, edit_forum_topic,
    get_available_chat_boost_slots, get_chat_boost_status, get_chat_history, get_chat_member,
    get_chat_statistics, get_community_full_info, get_forum_topics, get_me, get_supergroup,
    get_supergroup_full_info, join_chat, leave_chat, load_chat_welcome_messages,
    search_chat_messages, set_chat_description, set_chat_member_tag,
    set_chat_message_auto_delete_time, set_chat_photo, set_chat_slow_mode_delay, set_chat_title,
    set_community_name, set_community_permissions, set_community_photo, toggle_forum_topic_closed,
    toggle_forum_topic_pinned, toggle_general_forum_topic_hidden,
    toggle_supergroup_aggressive_anti_spam, toggle_supergroup_is_broadcast_group,
    toggle_supergroup_sign_messages,
};
use crate::telegram::requests_group_stickers::{
    set_supergroup_custom_emoji_sticker_set, set_supergroup_sticker_set,
};

mod boosts;
mod community;
mod forum;
mod supergroup;
mod welcome;

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
        // Broadcast channels need the status for the composer gate;
        // supergroups need the viewer's own restriction (`getChatMember`).
        if !self.session.chats.get(&chat_id.0).is_some_and(|chat| {
            chat.is_channel() || matches!(chat.kind, ChatKind::Supergroup { .. })
        }) {
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
        if self.session.chats.get(&chat_id.0).is_some_and(|chat| {
            if chat.is_channel() {
                chat.my_member_status.is_some()
            } else {
                chat.my_rights_fetched
            }
        }) {
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
        let purpose = RequestPurpose::Groups(GroupsPurpose::CreateSupergroupChannel { is_channel });
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
        let purpose = RequestPurpose::Groups(GroupsPurpose::SetChatMemberTag { user_id });
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
