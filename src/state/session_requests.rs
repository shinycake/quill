//! Outgoing request builders.
use super::*;

impl Session {
    pub fn request(&mut self, purpose: RequestPurpose, chat_id: Option<ChatId>) -> RequestId {
        let view = if matches!(
            purpose,
            RequestPurpose::GetHistory | RequestPurpose::GetHistoryNewer
        ) {
            Some(self.view_generation)
        } else {
            None
        };
        let id = self
            .requests
            .register(self.account_generation, purpose, chat_id, view);
        if matches!(
            purpose,
            RequestPurpose::GetHistory | RequestPurpose::GetHistoryNewer
        ) {
            self.stamp_window_epoch(id, chat_id);
        }
        id
    }

    /// A request whose answer names nothing it is for: it records `key`
    /// when it is registered, so the answer is read against what asked.
    fn request_keyed(
        &mut self,
        purpose: RequestPurpose,
        chat_id: Option<ChatId>,
        key: RequestKey,
    ) -> RequestId {
        let id = self.request(purpose, chat_id);
        if let Some(pending) = self.requests.pending_mut(id) {
            pending.key = key;
        }
        id
    }

    /// A history page asked for `chat_id` belongs to the window it has now.
    pub(crate) fn stamp_window_epoch(&mut self, id: RequestId, chat_id: Option<ChatId>) {
        let epoch = chat_id
            .and_then(|chat| self.histories.get(&chat.0))
            .map_or(0, |history| history.window_epoch);
        if let Some(pending) = self.requests.pending_mut(id) {
            pending.window_epoch = Some(epoch);
        }
    }

    /// Slice bots-games: like `request`, but also stamps the message id for
    /// `GetGameHighScores` correlation (`PendingRequest::around_message_id`
    /// — the pending record has no message field). `GetHistory` stamps its
    /// `from_message_id` the same way.
    pub fn request_for_message(
        &mut self,
        purpose: RequestPurpose,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> RequestId {
        self.request_keyed(purpose, Some(chat_id), RequestKey::Message(message_id))
    }

    /// Parity slice: like `request`, but also stamps the notification
    /// settings scope for `GetScopeNotificationSettings` /
    /// `SetScopeNotificationSettings` correlation (`PendingRequest::scope`).
    pub fn request_for_scope(
        &mut self,
        purpose: RequestPurpose,
        scope: NotificationSettingsScope,
    ) -> RequestId {
        self.request_keyed(purpose, None, RequestKey::Scope(scope))
    }

    /// Phase 5.1: like `request`, but also stamps the forum topic id for
    /// `GetTopicHistory` correlation (`PendingRequest::forum_topic_id`).
    pub fn request_for_topic(
        &mut self,
        purpose: RequestPurpose,
        chat_id: Option<ChatId>,
        forum_topic_id: i32,
    ) -> RequestId {
        self.request_keyed(purpose, chat_id, RequestKey::Topic(forum_topic_id))
    }

    /// Phase 6: like `request`, but stamps the user id for user-scoped
    /// requests (`GetUserFullInfo` from the contacts panel, `AddContact`)
    /// so id-less responses correlate (`PendingRequest::user_id`).
    pub fn request_for_user(&mut self, purpose: RequestPurpose, user_id: i64) -> RequestId {
        self.request_keyed(purpose, None, RequestKey::User(user_id))
    }

    /// Like [`Self::request_for_user`] for a request that also names its chat.
    pub fn request_for_user_in_chat(
        &mut self,
        purpose: RequestPurpose,
        chat_id: ChatId,
        user_id: i64,
    ) -> RequestId {
        self.request_keyed(purpose, Some(chat_id), RequestKey::User(user_id))
    }

    /// Phase 6: like `request`, but stamps the supergroup id for
    /// `GetSupergroupFullInfo` correlation
    /// (`PendingRequest::supergroup_id`).
    pub fn request_for_supergroup(
        &mut self,
        purpose: RequestPurpose,
        supergroup_id: i64,
    ) -> RequestId {
        self.request_keyed(purpose, None, RequestKey::Supergroup(supergroup_id))
    }

    /// Slice (communities backend core): like `request`, but stamps the
    /// community id for `GetCommunityFullInfo` / `SetCommunityName`
    /// correlation (`PendingRequest::community_id`).
    pub fn request_for_community(
        &mut self,
        purpose: RequestPurpose,
        community_id: i64,
    ) -> RequestId {
        self.request_keyed(purpose, None, RequestKey::Community(community_id))
    }

    /// Phase 9.1: like `request`, but stamps the chat id and story id for
    /// `GetStory` correlation and per-story in-flight dedupe
    /// (`PendingRequest::story_id`).
    pub fn request_for_story(
        &mut self,
        purpose: RequestPurpose,
        chat_id: ChatId,
        story_id: i32,
    ) -> RequestId {
        self.request_keyed(purpose, Some(chat_id), RequestKey::Story(story_id))
    }

    /// Phase 9.7: like `request`, but stamps the sent id lists for album
    /// requests whose `ok` response carries no payload —
    /// `PendingRequest::story_ids` for `SetChatPinnedStories` (the new
    /// pinned list) and `ReorderStoryAlbums` (the new album order), and
    /// `PendingRequest::story_album_id` for `DeleteStoryAlbum`.
    pub fn request_for_story_album(
        &mut self,
        purpose: RequestPurpose,
        chat_id: ChatId,
        story_ids: Option<Vec<i32>>,
        story_album_id: Option<i32>,
    ) -> RequestId {
        let key = match (story_ids, story_album_id) {
            (Some(ids), _) => RequestKey::StoryList(ids),
            (None, Some(album)) => RequestKey::StoryAlbum(album),
            (None, None) => RequestKey::None,
        };
        self.request_keyed(purpose, Some(chat_id), key)
    }

    /// Like `request`, but stamps the user ids a `setCloseFriends` sent
    /// (`PendingRequest::user_ids`); its `ok` carries none.
    pub fn request_for_user_ids(&mut self, purpose: RequestPurpose, user_ids: &[i64]) -> RequestId {
        self.request_keyed(purpose, None, RequestKey::UserIds(user_ids.to_vec()))
    }

    /// Like `request`, but stamps the invite link a link-scoped request
    /// acts on (`PendingRequest::invite_link`); its `ok` carries none.
    pub fn request_for_invite_link(
        &mut self,
        purpose: RequestPurpose,
        chat_id: ChatId,
        invite_link: &str,
    ) -> RequestId {
        self.request_keyed(
            purpose,
            Some(chat_id),
            RequestKey::InviteLink(invite_link.to_owned()),
        )
    }

    /// Like `request`, but stamps the string id of what it acts on
    /// (`PendingRequest::subject_id`: the terms `acceptTermsOfService`
    /// accepts, the gift `sellGift` sells); its `ok` carries none.
    pub fn request_for_subject(&mut self, purpose: RequestPurpose, subject_id: &str) -> RequestId {
        self.request_keyed(purpose, None, RequestKey::Subject(subject_id.to_owned()))
    }

    /// Like [`Self::request_for_subject`] for a request that may name a chat.
    pub fn request_for_subject_in(
        &mut self,
        purpose: RequestPurpose,
        chat_id: Option<ChatId>,
        subject_id: &str,
    ) -> RequestId {
        self.request_keyed(purpose, chat_id, RequestKey::Subject(subject_id.to_owned()))
    }

    /// Phase B1: like `request`, but stamps the secret chat id for
    /// `GetSecretChat` / `CloseSecretChat` correlation
    /// (`PendingRequest::secret_chat_id`) — the `secretChat` response and
    /// the `ok` carry no chat id of their own.
    pub fn request_for_secret_chat(
        &mut self,
        purpose: RequestPurpose,
        secret_chat_id: i32,
    ) -> RequestId {
        self.request_keyed(purpose, None, RequestKey::SecretChat(secret_chat_id))
    }

    /// Parity slice: like `request`, but stamps the folder id for
    /// folder-scoped requests (`GetChatFolder`, `EditChatFolder`,
    /// `DeleteChatFolder`, `LoadFolderChats`) so responses correlate
    /// (`PendingRequest::folder_id`).
    pub fn request_for_folder(&mut self, purpose: RequestPurpose, folder_id: i32) -> RequestId {
        self.request_keyed(purpose, None, RequestKey::Folder(folder_id))
    }
}
