//! Connect driver: story albums, archived and pinned profile stories.
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
    /// Phase 9.7: `getChatStoryAlbums` (schema 1.8.67, line 13850).
    /// Deduped while a fetch is in flight; the answer replaces the chat's
    /// album list.
    pub fn get_chat_story_albums(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.story_page_chat(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetChatStoryAlbums, chat_id)
        {
            return Ok(None);
        }
        let purpose = RequestPurpose::GetChatStoryAlbums;
        let extra = self.session.request(purpose, Some(chat_id));
        self.session
            .begin_story_page_check(story_page_op_label(purpose));
        let json = get_chat_story_albums(extra, chat_id);
        self.send_story_page(purpose, extra, &json).map(Some)
    }

    /// Phase 9.7: `getStoryAlbumStories` (schema 1.8.67, line 13857).
    /// `offset` 0 starts from the first album story; the reducer
    /// accumulates pages.
    pub fn get_story_album_stories(
        &mut self,
        chat_id: ChatId,
        story_album_id: i32,
        offset: i32,
        limit: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.story_page_chat(chat_id) || offset < 0 || limit <= 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.requests.has_purpose_for_story_album(
            RequestPurpose::GetStoryAlbumStories,
            chat_id,
            story_album_id,
        ) {
            return Ok(None);
        }
        let purpose = RequestPurpose::GetStoryAlbumStories;
        let extra =
            self.session
                .request_for_story_album(purpose, chat_id, None, Some(story_album_id));
        self.session
            .begin_story_page_check(story_page_op_label(purpose));
        let json = get_story_album_stories(extra, chat_id, story_album_id, offset, limit);
        self.send_story_page(purpose, extra, &json).map(Some)
    }

    /// Phase 9.7: `createStoryAlbum` (schema 1.8.67, line 13863). The name
    /// is 1-12 characters per the schema comment; the answer is the new
    /// `storyAlbum`.
    pub fn create_story_album(
        &mut self,
        story_poster_chat_id: ChatId,
        name: &str,
        story_ids: &[i32],
    ) -> Result<RequestId, ConnectSendError> {
        if !self.story_page_chat(story_poster_chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let name_len = name.chars().count();
        if !(1..=12).contains(&name_len) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::CreateStoryAlbum;
        let extra = self.session.request(purpose, Some(story_poster_chat_id));
        self.session
            .begin_story_page_op(story_page_op_label(purpose));
        let json = create_story_album(extra, story_poster_chat_id, name, story_ids);
        self.send_story_page(purpose, extra, &json)
    }

    /// Phase 9.7: `reorderStoryAlbums` (schema 1.8.67, line 13868). The
    /// sent order rides on `PendingRequest::story_ids`; the `ok` answer
    /// applies it to the cached list.
    pub fn reorder_story_albums(
        &mut self,
        chat_id: ChatId,
        story_album_ids: &[i32],
    ) -> Result<RequestId, ConnectSendError> {
        if !self.story_page_chat(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::ReorderStoryAlbums;
        let extra = self.session.request_for_story_album(
            purpose,
            chat_id,
            Some(story_album_ids.to_vec()),
            None,
        );
        self.session
            .begin_story_page_op(story_page_op_label(purpose));
        let json = reorder_story_albums(extra, chat_id, story_album_ids);
        self.send_story_page(purpose, extra, &json)
    }

    /// Phase 9.7: `deleteStoryAlbum` (schema 1.8.67, line 13873). The
    /// album id rides on `PendingRequest::story_album_id`; the `ok`
    /// answer drops the album from the cached list.
    pub fn delete_story_album(
        &mut self,
        chat_id: ChatId,
        story_album_id: i32,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.story_page_chat(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::DeleteStoryAlbum;
        let extra =
            self.session
                .request_for_story_album(purpose, chat_id, None, Some(story_album_id));
        self.session
            .begin_story_page_op(story_page_op_label(purpose));
        let json = delete_story_album(extra, chat_id, story_album_id);
        self.send_story_page(purpose, extra, &json)
    }

    /// Phase 9.7: `setStoryAlbumName` (schema 1.8.67, line 13879). The
    /// name is 1-12 characters per the schema comment; the answer is the
    /// changed `storyAlbum`.
    pub fn set_story_album_name(
        &mut self,
        chat_id: ChatId,
        story_album_id: i32,
        name: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.story_page_chat(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let name_len = name.chars().count();
        if !(1..=12).contains(&name_len) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::SetStoryAlbumName;
        let extra = self.session.request(purpose, Some(chat_id));
        self.session
            .begin_story_page_op(story_page_op_label(purpose));
        let json = set_story_album_name(extra, chat_id, story_album_id, name);
        self.send_story_page(purpose, extra, &json)
    }

    /// Phase 9.7: `addStoryAlbumStories` (schema 1.8.67, line 13887).
    /// At least one story id (schema: "1-getOption(...) identifiers").
    /// Stories are added to the beginning; the answer is the changed
    /// `storyAlbum`.
    pub fn add_story_album_stories(
        &mut self,
        chat_id: ChatId,
        story_album_id: i32,
        story_ids: &[i32],
    ) -> Result<RequestId, ConnectSendError> {
        if !self.story_page_chat(chat_id) || story_ids.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::AddStoryAlbumStories;
        let extra = self.session.request(purpose, Some(chat_id));
        self.session
            .begin_story_page_op(story_page_op_label(purpose));
        let json = add_story_album_stories(extra, chat_id, story_album_id, story_ids);
        self.send_story_page(purpose, extra, &json)
    }

    /// Phase 9.7: `removeStoryAlbumStories` (schema 1.8.67, line 13894).
    /// The answer is the changed `storyAlbum`.
    pub fn remove_story_album_stories(
        &mut self,
        chat_id: ChatId,
        story_album_id: i32,
        story_ids: &[i32],
    ) -> Result<RequestId, ConnectSendError> {
        if !self.story_page_chat(chat_id) || story_ids.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::RemoveStoryAlbumStories;
        let extra = self.session.request(purpose, Some(chat_id));
        self.session
            .begin_story_page_op(story_page_op_label(purpose));
        let json = remove_story_album_stories(extra, chat_id, story_album_id, story_ids);
        self.send_story_page(purpose, extra, &json)
    }

    /// Phase 9.7: `reorderStoryAlbumStories` (schema 1.8.67, line 13901).
    /// The listed stories move to the beginning of the album; the answer
    /// is the changed `storyAlbum`.
    pub fn reorder_story_album_stories(
        &mut self,
        chat_id: ChatId,
        story_album_id: i32,
        story_ids: &[i32],
    ) -> Result<RequestId, ConnectSendError> {
        if !self.story_page_chat(chat_id) || story_ids.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::ReorderStoryAlbumStories;
        let extra = self.session.request(purpose, Some(chat_id));
        self.session
            .begin_story_page_op(story_page_op_label(purpose));
        let json = reorder_story_album_stories(extra, chat_id, story_album_id, story_ids);
        self.send_story_page(purpose, extra, &json)
    }

    /// Phase 9.7: `getChatArchivedStories` (schema 1.8.67, line 13784).
    /// `from_story_id` 0 starts from the newest; the reducer accumulates
    /// pages and tracks the smallest loaded id as the next cursor.
    pub fn get_chat_archived_stories(
        &mut self,
        chat_id: ChatId,
        from_story_id: i32,
        limit: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.story_page_chat(chat_id) || from_story_id < 0 || limit <= 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetChatArchivedStories, chat_id)
        {
            return Ok(None);
        }
        let purpose = RequestPurpose::GetChatArchivedStories;
        let extra = self.session.request(purpose, Some(chat_id));
        self.session
            .begin_story_page_check(story_page_op_label(purpose));
        let json = get_chat_archived_stories(extra, chat_id, from_story_id, limit);
        self.send_story_page(purpose, extra, &json).map(Some)
    }

    /// Phase 9.7: `getChatPostedToChatPageStories` (schema 1.8.67, line
    /// 13776). The first page (`from_story_id == 0`) also carries
    /// `pinned_story_ids`; the reducer accumulates pages.
    pub fn get_chat_posted_to_chat_page_stories(
        &mut self,
        chat_id: ChatId,
        from_story_id: i32,
        limit: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.story_page_chat(chat_id) || from_story_id < 0 || limit <= 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetChatPostedToChatPageStories, chat_id)
        {
            return Ok(None);
        }
        let purpose = RequestPurpose::GetChatPostedToChatPageStories;
        let extra = self.session.request(purpose, Some(chat_id));
        self.session
            .begin_story_page_check(story_page_op_label(purpose));
        let json = get_chat_posted_to_chat_page_stories(extra, chat_id, from_story_id, limit);
        let sent = self.send_story_page(purpose, extra, &json).map(Some);
        // Phase 9.7: a fresh first page restarts the list — the server only
        // sends `pinned_story_ids` on the first page (schema 1.8.67, line
        // 6747), so stale pins must not survive a refetch. Restart only once
        // the request actually left: a transport failure must not wipe the
        // already-loaded list.
        if sent.is_ok() && from_story_id == 0 {
            self.session.stories.chat_page_stories.remove(&chat_id.0);
        }
        sent
    }

    /// Phase 9.7: `setChatPinnedStories` (schema 1.8.67, line 13789).
    /// The full new pinned list (not a delta); the sent ids ride on
    /// `PendingRequest::story_ids` so the `ok` answer applies them.
    pub fn set_chat_pinned_stories(
        &mut self,
        chat_id: ChatId,
        story_ids: &[i32],
    ) -> Result<RequestId, ConnectSendError> {
        if !self.story_page_chat(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::SetChatPinnedStories;
        let extra =
            self.session
                .request_for_story_album(purpose, chat_id, Some(story_ids.to_vec()), None);
        self.session
            .begin_story_page_op(story_page_op_label(purpose));
        let json = set_chat_pinned_stories(extra, chat_id, story_ids);
        self.send_story_page(purpose, extra, &json)
    }
}
