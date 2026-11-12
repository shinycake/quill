//! Outgoing request builders.
use super::*;

impl Session {
    /// The newest request this session registered so far.
    pub fn newest_request(&self) -> RequestId {
        self.requests.newest()
    }

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

    /// A request whose answer names nothing it is for: it records
    /// `correlation` when it is registered, so the answer is read against
    /// what asked.
    fn request_correlated(
        &mut self,
        purpose: RequestPurpose,
        chat_id: Option<ChatId>,
        correlation: Correlation,
    ) -> RequestId {
        let id = self.request(purpose, chat_id);
        self.requests.correlate(id, correlation);
        id
    }

    /// A history page asked for `chat_id` belongs to the window it has now.
    pub(crate) fn stamp_window_epoch(&mut self, id: RequestId, chat_id: Option<ChatId>) {
        let window_epoch = chat_id
            .and_then(|chat| self.histories.get(&chat.0))
            .map_or(0, |history| history.window_epoch);
        let from = self
            .requests
            .get(id)
            .and_then(PendingRequest::around_message_id);
        self.requests
            .correlate(id, Correlation::HistoryPage { window_epoch, from });
    }

    /// Slice bots-games: like `request`, but also records the message id for
    /// `GetGameHighScores` correlation (`Correlation::Message`). `GetHistory`
    /// records its `from_message_id` in its `Correlation::HistoryPage`.
    pub fn request_for_message(
        &mut self,
        purpose: RequestPurpose,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> RequestId {
        let extra = self.request(purpose, Some(chat_id));
        let correlation = match self.requests.get(extra).map(|p| &p.correlation) {
            Some(Correlation::HistoryPage { window_epoch, .. }) => Correlation::HistoryPage {
                window_epoch: *window_epoch,
                from: Some(message_id),
            },
            _ => Correlation::Message(message_id),
        };
        self.requests.correlate(extra, correlation);
        extra
    }

    /// Parity slice: like `request`, but also stamps the notification
    /// settings scope for `GetScopeNotificationSettings` /
    /// `SetScopeNotificationSettings` correlation (`Correlation::Scope`).
    pub fn request_for_scope(
        &mut self,
        purpose: RequestPurpose,
        scope: NotificationSettingsScope,
    ) -> RequestId {
        self.request_correlated(purpose, None, Correlation::Scope(scope))
    }

    /// Phase 5.1: like `request`, but also stamps the forum topic id for
    /// `GetTopicHistory` correlation (`Correlation::ForumTopic`).
    pub fn request_for_topic(
        &mut self,
        purpose: RequestPurpose,
        chat_id: Option<ChatId>,
        forum_topic_id: i32,
    ) -> RequestId {
        self.request_correlated(purpose, chat_id, Correlation::ForumTopic(forum_topic_id))
    }

    /// Phase 6: like `request`, but stamps the user id for user-scoped
    /// requests (`GetUserFullInfo` from the contacts panel, `AddContact`)
    /// so id-less responses correlate (`Correlation::user_id`).
    pub fn request_for_user(&mut self, purpose: RequestPurpose, user_id: i64) -> RequestId {
        self.request_correlated(purpose, None, Correlation::User(user_id))
    }

    /// Like [`Self::request_for_user`] for a request that also names its chat.
    pub fn request_for_user_in_chat(
        &mut self,
        purpose: RequestPurpose,
        chat_id: ChatId,
        user_id: i64,
    ) -> RequestId {
        self.request_correlated(purpose, Some(chat_id), Correlation::User(user_id))
    }

    /// Phase 6: like `request`, but stamps the supergroup id for
    /// `GetSupergroupFullInfo` correlation
    /// (`Correlation::supergroup_id`).
    pub fn request_for_supergroup(
        &mut self,
        purpose: RequestPurpose,
        supergroup_id: i64,
    ) -> RequestId {
        self.request_correlated(purpose, None, Correlation::Supergroup(supergroup_id))
    }

    /// Slice (communities backend core): like `request`, but stamps the
    /// community id for `GetCommunityFullInfo` / `SetCommunityName`
    /// correlation (`Correlation::community_id`).
    pub fn request_for_community(
        &mut self,
        purpose: RequestPurpose,
        community_id: i64,
    ) -> RequestId {
        self.request_correlated(purpose, None, Correlation::Community(community_id))
    }

    /// Phase 9.1: like `request`, but stamps the chat id and story id for
    /// `GetStory` correlation and per-story in-flight dedupe
    /// (`Correlation::story_id`).
    pub fn request_for_story(
        &mut self,
        purpose: RequestPurpose,
        chat_id: ChatId,
        story_id: i32,
    ) -> RequestId {
        self.request_correlated(purpose, Some(chat_id), Correlation::Story(story_id))
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
        self.request_correlated(
            purpose,
            Some(chat_id),
            Correlation::StoryAlbum {
                story_ids,
                album_id: story_album_id,
            },
        )
    }

    /// Like `request`, but stamps the user ids a `setCloseFriends` sent
    /// (`Correlation::user_ids`); its `ok` carries none.
    pub fn request_for_user_ids(&mut self, purpose: RequestPurpose, user_ids: &[i64]) -> RequestId {
        self.request_correlated(purpose, None, Correlation::UserIds(user_ids.to_vec()))
    }

    /// Like `request`, but stamps the invite link a link-scoped request
    /// acts on (`Correlation::invite_link`); its `ok` carries none.
    pub fn request_for_invite_link(
        &mut self,
        purpose: RequestPurpose,
        chat_id: ChatId,
        invite_link: &str,
    ) -> RequestId {
        self.request_correlated(
            purpose,
            Some(chat_id),
            Correlation::InviteLink(invite_link.to_owned()),
        )
    }

    /// Like `request`, but stamps the string id of what it acts on
    /// (`Correlation::subject_id`: the terms `acceptTermsOfService`
    /// accepts, the gift `sellGift` sells); its `ok` carries none.
    pub fn request_for_subject(&mut self, purpose: RequestPurpose, subject_id: &str) -> RequestId {
        self.request_for_subject_in(purpose, None, subject_id)
    }

    /// Like [`Self::request_for_subject`] for a request that may name a chat.
    pub fn request_for_subject_in(
        &mut self,
        purpose: RequestPurpose,
        chat_id: Option<ChatId>,
        subject_id: &str,
    ) -> RequestId {
        self.request_correlated(
            purpose,
            chat_id,
            Correlation::Subject(subject_id.to_owned()),
        )
    }

    /// Phase B1: like `request`, but stamps the secret chat id for
    /// `GetSecretChat` / `CloseSecretChat` correlation
    /// (`Correlation::secret_chat_id`) — the `secretChat` response and
    /// the `ok` carry no chat id of their own.
    pub fn request_for_secret_chat(
        &mut self,
        purpose: RequestPurpose,
        secret_chat_id: i32,
    ) -> RequestId {
        self.request_correlated(purpose, None, Correlation::SecretChat(secret_chat_id))
    }

    /// A page the shared-media gallery's Stories tab asks for
    /// (`Correlation::GalleryTab`); the stories page asks for the same
    /// purpose without it.
    pub fn request_for_gallery_tab(
        &mut self,
        purpose: RequestPurpose,
        chat_id: ChatId,
    ) -> RequestId {
        self.request_correlated(purpose, Some(chat_id), Correlation::GalleryTab)
    }

    /// Parity slice: like `request`, but stamps the folder id for
    /// folder-scoped requests (`GetChatFolder`, `EditChatFolder`,
    /// `DeleteChatFolder`, `LoadFolderChats`) so responses correlate
    /// (`Correlation::folder_id`).
    pub fn request_for_folder(&mut self, purpose: RequestPurpose, folder_id: i32) -> RequestId {
        self.request_correlated(purpose, None, Correlation::Folder(folder_id))
    }
}
