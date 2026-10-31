//! B10: reducer pieces behind the profile and contact panels — the
//! chat-id lists (groups in common, similar channels, personal channel
//! candidates) and the profile photo gallery.
use super::*;
use crate::telegram::envelope::{ChannelMemberStatus, ParsedProfilePhoto};

impl Session {
    /// The chat of a supergroup or channel, for the info panel's title and
    /// header badges.
    pub fn supergroup_chat(&self, supergroup_id: i64) -> Option<&ChatSummary> {
        self.chats.values().find(|chat| {
            matches!(chat.kind, ChatKind::Supergroup { supergroup_id: id, .. } if id == supergroup_id)
        })
    }

    /// The group or channel the info panel shows, held from pruning by the
    /// UI (`ui::history_holds`) so what the panel loaded stays. Usually the
    /// open chat, looked at first so a frame does not search every chat.
    pub fn info_panel_chat(&self) -> Option<ChatId> {
        let target = self.users_state.open_info_panel?;
        let shows = |chat: &&ChatSummary| match (target, &chat.kind) {
            (InfoPanelTarget::Supergroup(id), ChatKind::Supergroup { supergroup_id, .. }) => {
                *supergroup_id == id
            }
            (InfoPanelTarget::BasicGroup(id), ChatKind::BasicGroup { basic_group_id }) => {
                *basic_group_id == id
            }
            _ => false,
        };
        if !matches!(
            target,
            InfoPanelTarget::Supergroup(_) | InfoPanelTarget::BasicGroup(_)
        ) {
            return None;
        }
        self.open_chat
            .and_then(|id| self.chats.get(&id.0))
            .filter(shows)
            .or_else(|| self.chats.values().find(shows))
            .map(|chat| chat.id)
    }

    /// Whether sharing is disabled in the private chat with another person
    /// (`toggleChatHasProtectedContent`); `None` for other chats, bots
    /// and Saved Messages, where it cannot be toggled.
    pub fn private_sharing_disabled(&self, chat_id: ChatId) -> Option<bool> {
        let user_id = self.private_chat_user_id(chat_id)?;
        let other_person = self.my_user_id != Some(user_id) && !self.is_bot_user(user_id);
        other_person.then(|| self.chat_has_protected_content(chat_id))
    }

    /// The private chat with `user_id`, listed or not.
    pub fn private_chat_with(&self, user_id: i64) -> Option<ChatId> {
        self.chats
            .values()
            .find(|chat| matches!(chat.kind, ChatKind::Private { user_id: peer } if peer.0 == user_id))
            .map(|chat| chat.id)
    }

    /// Whether the viewer owns or administers the group or channel.
    pub fn chat_owner_or_admin(&self, chat_id: ChatId) -> bool {
        matches!(
            self.group_own_status(chat_id),
            Some(ChannelMemberStatus::Creator | ChannelMemberStatus::Administrator)
        )
    }

    /// Whether the viewer is known not to be in the chat (left or banned),
    /// which is when tdesktop shows Join Channel.
    pub fn chat_not_joined(&self, chat_id: ChatId) -> bool {
        self.chats.get(&chat_id.0).is_some_and(|chat| {
            matches!(
                chat.my_member_status,
                Some(ChannelMemberStatus::Left | ChannelMemberStatus::Banned)
            )
        })
    }

    /// Whether supergroup `supergroup_id` is a broadcast channel.
    pub fn supergroup_is_channel(&self, supergroup_id: i64) -> bool {
        self.supergroup_chat(supergroup_id).is_some_and(|chat| {
            matches!(
                chat.kind,
                ChatKind::Supergroup {
                    is_channel: true,
                    ..
                }
            )
        })
    }

    /// The `profile_chat_lists` key a pending `GetProfileChats` request
    /// answers: the user id for groups in common, the channel chat id for
    /// similar channels, 0 for the personal channel candidates.
    pub(crate) fn profile_chats_key(pending: &PendingRequest) -> Option<(ProfileChatsKind, i64)> {
        let RequestPurpose::GetProfileChats(kind) = pending.purpose else {
            return None;
        };
        let id = match kind {
            ProfileChatsKind::GroupsInCommon => pending.user_id?,
            ProfileChatsKind::SimilarChats => pending.chat_id?.0,
            ProfileChatsKind::SuitablePersonalChats | ProfileChatsKind::SuitableDiscussionChats => {
                0
            }
        };
        Some((kind, id))
    }

    /// A `chats` answer to one of our profile list fetches.
    pub(crate) fn apply_profile_chats(&mut self, chat_ids: &[ChatId], pending: &PendingRequest) {
        let Some(key) = Self::profile_chats_key(pending) else {
            return;
        };
        let mut ids: Vec<i64> = chat_ids.iter().map(|id| id.0).collect();
        if key.0 == ProfileChatsKind::GroupsInCommon {
            let paging = self
                .users_state
                .common_groups_paging
                .entry(key.1)
                .or_default();
            paging.exhausted = ids.len() < GROUPS_IN_COMMON_PAGE;
            // A follow-up page extends the list; the first page replaces it.
            if std::mem::take(&mut paging.loading_more)
                && let Some(ProfileChatsFetch::Loaded(shown)) =
                    self.users_state.profile_chat_lists.get(&key)
            {
                let mut all = shown.clone();
                ids.retain(|id| !all.contains(id));
                all.extend(ids);
                ids = all;
            }
        }
        self.users_state
            .profile_chat_lists
            .insert(key, ProfileChatsFetch::Loaded(ids));
    }

    /// Whether more groups in common can be asked for, and whether that
    /// page is on its way.
    pub fn common_groups_paging(&self, user_id: i64) -> CommonGroupsPaging {
        self.users_state
            .common_groups_paging
            .get(&user_id)
            .copied()
            .unwrap_or_default()
    }

    /// A refused list or gallery fetch: keep the reason so the panel can
    /// show a Retry row instead of spinning forever.
    pub(crate) fn fail_profile_fetch(&mut self, pending: &PendingRequest, reason: String) {
        if let Some(key) = Self::profile_chats_key(pending) {
            // A failed follow-up page keeps the groups already listed and
            // ends the list: tdesktop's `loadMoreRows` has no failure
            // handler, so its request id stays set and it never asks
            // again. (TDLib's "Wrong offset_chat_id" would only repeat.)
            if key.0 == ProfileChatsKind::GroupsInCommon
                && let Some(paging) = self.users_state.common_groups_paging.get_mut(&key.1)
                && std::mem::take(&mut paging.loading_more)
            {
                paging.exhausted = true;
                return;
            }
            self.users_state
                .profile_chat_lists
                .insert(key, ProfileChatsFetch::Failed(reason));
        } else if pending.purpose == RequestPurpose::GetUserProfilePhotos
            && let Some(user_id) = pending.user_id
        {
            // A failed later page keeps the photos already listed.
            if let Some(ProfilePhotosFetch::Loaded { loading_more, .. }) =
                self.users_state.user_profile_photos.get_mut(&user_id)
                && std::mem::take(loading_more)
            {
                return;
            }
            self.users_state
                .user_profile_photos
                .insert(user_id, ProfilePhotosFetch::Failed(reason));
        } else if pending.purpose == RequestPurpose::GetChatPhotoHistory
            && let Some(chat_id) = pending.chat_id
        {
            if let Some(ChatPhotosFetch::Loaded { loading_more, .. }) =
                self.users_state.chat_photo_history.get_mut(&chat_id.0)
                && std::mem::take(loading_more)
            {
                return;
            }
            self.users_state
                .chat_photo_history
                .insert(chat_id.0, ChatPhotosFetch::Failed(reason));
        }
    }

    /// A `chatPhotos` answer: cache every size file and the gallery. A
    /// later page (`loading_more`) extends the list.
    pub(crate) fn apply_profile_photos(
        &mut self,
        total_count: i32,
        photos: Vec<ParsedProfilePhoto>,
        pending: &PendingRequest,
    ) {
        let Some(user_id) = pending.user_id else {
            return;
        };
        let mut page = Vec::with_capacity(photos.len());
        for photo in photos {
            page.push(ProfilePhoto::from_parsed(&photo));
            for file in photo.files {
                self.upsert_file(file, false);
            }
        }
        let mut gallery = match self.users_state.user_profile_photos.remove(&user_id) {
            Some(ProfilePhotosFetch::Loaded {
                photos,
                loading_more: true,
                ..
            }) => photos,
            _ => Vec::new(),
        };
        page.retain(|photo| gallery.iter().all(|shown| shown.id != photo.id));
        // An empty later page means the count was stale: stop there.
        let total_count = if page.is_empty() && !gallery.is_empty() {
            i32::try_from(gallery.len()).unwrap_or(i32::MAX)
        } else {
            total_count
        };
        gallery.extend(page);
        self.users_state.user_profile_photos.insert(
            user_id,
            ProfilePhotosFetch::Loaded {
                total_count,
                photos: gallery,
                loading_more: false,
            },
        );
    }

    /// The chat ids of a loaded profile list; empty while loading.
    pub fn profile_chat_list(&self, kind: ProfileChatsKind, id: i64) -> &[i64] {
        match self.users_state.profile_chat_lists.get(&(kind, id)) {
            Some(ProfileChatsFetch::Loaded(ids)) => ids,
            _ => &[],
        }
    }
}
