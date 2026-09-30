//! Outgoing request builders.
use super::*;

impl Session {
    pub fn request(&mut self, purpose: RequestPurpose, chat_id: Option<ChatId>) -> RequestId {
        let view = if matches!(purpose, RequestPurpose::GetHistory) {
            Some(self.view_generation)
        } else {
            None
        };
        self.requests
            .register(self.account_generation, purpose, chat_id, view)
    }

    /// Slice bots-games: like `request`, but also stamps the message id for
    /// `GetGameHighScores` correlation (`PendingRequest::around_message_id`
    /// — the pending record has no message field).
    pub fn request_for_message(
        &mut self,
        purpose: RequestPurpose,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> RequestId {
        let extra = self.request(purpose, Some(chat_id));
        if let Some(pending) = self.requests.pending_mut(extra) {
            pending.around_message_id = Some(message_id);
        }
        extra
    }

    /// Parity slice: like `request`, but also stamps the notification
    /// settings scope for `GetScopeNotificationSettings` /
    /// `SetScopeNotificationSettings` correlation (`PendingRequest::scope`).
    pub fn request_for_scope(
        &mut self,
        purpose: RequestPurpose,
        scope: NotificationSettingsScope,
    ) -> RequestId {
        let extra = self.request(purpose, None);
        if let Some(pending) = self.requests.pending_mut(extra) {
            pending.scope = Some(scope);
        }
        extra
    }

    /// Phase 5.1: like `request`, but also stamps the forum topic id for
    /// `GetTopicHistory` correlation (`PendingRequest::forum_topic_id`).
    pub fn request_for_topic(
        &mut self,
        purpose: RequestPurpose,
        chat_id: Option<ChatId>,
        forum_topic_id: i32,
    ) -> RequestId {
        let id = self.request(purpose, chat_id);
        if let Some(pending) = self.requests.pending.get_mut(&id.0) {
            pending.forum_topic_id = Some(forum_topic_id);
        }
        id
    }

    /// Phase 6: like `request`, but stamps the user id for user-scoped
    /// requests (`GetUserFullInfo` from the contacts panel, `AddContact`)
    /// so id-less responses correlate (`PendingRequest::user_id`).
    pub fn request_for_user(&mut self, purpose: RequestPurpose, user_id: i64) -> RequestId {
        let id = self.request(purpose, None);
        if let Some(pending) = self.requests.pending.get_mut(&id.0) {
            pending.user_id = Some(user_id);
        }
        id
    }

    /// Phase 6: like `request`, but stamps the supergroup id for
    /// `GetSupergroupFullInfo` correlation
    /// (`PendingRequest::supergroup_id`).
    pub fn request_for_supergroup(
        &mut self,
        purpose: RequestPurpose,
        supergroup_id: i64,
    ) -> RequestId {
        let id = self.request(purpose, None);
        if let Some(pending) = self.requests.pending.get_mut(&id.0) {
            pending.supergroup_id = Some(supergroup_id);
        }
        id
    }

    /// Slice (communities backend core): like `request`, but stamps the
    /// community id for `LoadCommunityFullInfo` / `SetCommunityName`
    /// correlation (`PendingRequest::community_id`).
    pub fn request_for_community(
        &mut self,
        purpose: RequestPurpose,
        community_id: i64,
    ) -> RequestId {
        let id = self.request(purpose, None);
        if let Some(pending) = self.requests.pending.get_mut(&id.0) {
            pending.community_id = Some(community_id);
        }
        id
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
        let id = self.request(purpose, Some(chat_id));
        if let Some(pending) = self.requests.pending.get_mut(&id.0) {
            pending.story_id = Some(story_id);
        }
        id
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
        let id = self.request(purpose, Some(chat_id));
        if let Some(pending) = self.requests.pending.get_mut(&id.0) {
            pending.story_ids = story_ids;
            pending.story_album_id = story_album_id;
        }
        id
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
        let id = self.request(purpose, None);
        if let Some(pending) = self.requests.pending.get_mut(&id.0) {
            pending.secret_chat_id = Some(secret_chat_id);
        }
        id
    }

    /// Parity slice: like `request`, but stamps the folder id for
    /// folder-scoped requests (`GetChatFolder`, `EditChatFolder`,
    /// `DeleteChatFolder`, `LoadFolderChats`) so responses correlate
    /// (`PendingRequest::folder_id`).
    pub fn request_for_folder(&mut self, purpose: RequestPurpose, folder_id: i32) -> RequestId {
        let id = self.request(purpose, None);
        if let Some(pending) = self.requests.pending.get_mut(&id.0) {
            pending.folder_id = Some(folder_id);
        }
        id
    }
}
