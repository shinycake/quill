//! Connect driver: close friends, replies, interactions, reports, stealth mode, posting and editing.
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
    /// B14: `getCloseFriends`. Deduped while in flight.
    pub fn get_close_friends(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::GetCloseFriends;
        if self.session.requests.has_purpose(purpose) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, None);
        self.session
            .begin_story_page_check(story_page_op_label(purpose));
        let json = get_close_friends_request(extra);
        self.send_story_page(purpose, extra, &json).map(Some)
    }

    /// B14: `setCloseFriends` with the full new list; the ids are staged
    /// in `Session::close_friends_pending` and applied on `ok`.
    pub fn set_close_friends(&mut self, user_ids: &[i64]) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::SetCloseFriends;
        let extra = self.session.request(purpose, None);
        self.session.close_friends_pending = Some(user_ids.to_vec());
        self.session
            .begin_story_page_op(story_page_op_label(purpose));
        let json = set_close_friends_request(extra, user_ids);
        let sent = self.send_story_page(purpose, extra, &json);
        if sent.is_err() {
            self.session.close_friends_pending = None;
        }
        sent
    }

    /// B14: `setChatActiveStoriesList` — hide (`archive`) or unhide a
    /// peer's stories.
    pub fn set_chat_active_stories_list(
        &mut self,
        chat_id: ChatId,
        archive: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.story_page_chat(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::SetChatActiveStoriesList;
        let extra = self.session.request(purpose, Some(chat_id));
        self.session
            .begin_story_page_op(story_page_op_label(purpose));
        let json = set_chat_active_stories_list(extra, chat_id, archive);
        self.send_story_page(purpose, extra, &json)
    }

    /// B14: `toggleStoryIsPostedToChatPage`, gated on
    /// `story.can_toggle_is_posted_to_chat_page`.
    pub fn toggle_story_is_posted_to_chat_page(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
        posted: bool,
    ) -> Result<RequestId, ConnectSendError> {
        let allowed = self
            .session
            .stories
            .get(&(chat_id.0, story_id))
            .is_some_and(|story| story.can_toggle_is_posted_to_chat_page);
        if !self.story_page_chat(chat_id) || !allowed {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = RequestPurpose::ToggleStoryIsPostedToChatPage;
        let extra = self.session.request_for_story(purpose, chat_id, story_id);
        self.session
            .begin_story_page_op(story_page_op_label(purpose));
        let json = toggle_story_is_posted_to_chat_page(extra, chat_id, story_id, posted);
        self.send_story_page(purpose, extra, &json)
    }

    /// Phase 9.2: reply to a story — `sendMessage` to the poster chat with
    /// `inputMessageReplyToStory`. Gated on `story.can_be_replied` and a
    /// non-empty message sent to a supported chat.
    pub fn send_story_reply(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
        text: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let repliable = self
            .session
            .stories
            .get(&(chat_id.0, story_id))
            .is_some_and(|story| story.can_be_replied);
        if !repliable || text.trim().is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported());
        if !supported {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra =
            self.session
                .request_for_story(RequestPurpose::SendStoryReply, chat_id, story_id);
        let json = send_text_story_reply(extra, chat_id, chat_id, story_id, text);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 9.5: `getStoryInteractions` — one page of an own story's
    /// viewers. Gated on the cached story and its
    /// `can_get_interactions` flag (the schema comment at
    /// `td_api.tl:6732` names this function as what the flag allows).
    /// Deduped per story while a fetch is in flight; the UI passes the
    /// previous page's `next_offset` for pagination.
    pub fn get_story_interactions(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
        offset: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let can_get = self
            .session
            .stories
            .get(&(chat_id.0, story_id))
            .is_some_and(|story| story.can_get_interactions);
        if !can_get {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.requests.has_purpose_for_story(
            RequestPurpose::GetStoryInteractions,
            chat_id,
            story_id,
        ) {
            return Ok(None);
        }
        let extra =
            self.session
                .request_for_story(RequestPurpose::GetStoryInteractions, chat_id, story_id);
        let json = get_story_interactions_request(extra, story_id, "", offset, 50);
        match self.sender.send_json(&json) {
            Ok(()) => {
                if let Some(state) = self.session.story_viewers.as_mut()
                    && state.chat_id == chat_id.0
                    && state.story_id == story_id
                {
                    state.loading = true;
                }
                Ok(Some(extra))
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 9.5: `reportStory` — one step of the report flow. The UI
    /// starts with empty `option_id`/`text`; the
    /// `reportStoryResultOptionRequired` / `reportStoryResultTextRequired`
    /// answers tell it what to send next. Gated on the cached story;
    /// own stories (deletable) are not reportable.
    pub fn report_story(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
        option_id: &str,
        text: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let reportable = self
            .session
            .stories
            .get(&(chat_id.0, story_id))
            .is_some_and(|story| !story.can_be_deleted);
        if !reportable {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request_for_story(RequestPurpose::ReportStory, chat_id, story_id);
        let json = report_story_request(extra, chat_id, story_id, option_id, text);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 9.5: `activateStoryStealthMode` — hides the current user's
    /// story views (Premium only; the server decides). The state arrives
    /// as `updateStoryStealthMode`; a refused call surfaces as
    /// `Session::story_stealth_error`. Deduped while in flight.
    pub fn activate_story_stealth_mode(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::ActivateStoryStealthMode)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::ActivateStoryStealthMode, None);
        match self
            .sender
            .send_json(&activate_story_stealth_mode_request(extra))
        {
            Ok(()) => {
                self.session.story_stealth_error = None;
                Ok(Some(extra))
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 9.3: `canPostStory` eligibility check (TDLib 1.8.67,
    /// `schema/td_api.tl:13702`) for the given chat — the user's own
    /// story chat or an eligible channel / supergroup from
    /// `getChatsToPostStories`. The composer calls this before every
    /// post; the answer lands in `Session::story_post.eligibility`.
    /// Deduped per chat while a check is in flight.
    pub fn check_can_post_story(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.my_user_id.is_none() {
            return Err(ConnectSendError::InvalidRequest);
        };
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::CheckCanPostStory, chat_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::CheckCanPostStory, Some(chat_id));
        match self
            .sender
            .send_json(&can_post_story_request(extra, chat_id))
        {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 9.3: `postStory` (TDLib 1.8.67, `schema/td_api.tl:13715`) —
    /// posts the composer's photo/video with caption + privacy on
    /// `chat_id` — the current user's own story chat, or a channel /
    /// supergroup from `getChatsToPostStories` (post-as-channel, 9.5).
    /// `kind` must be detected and the file must exist; `SelectedUsers`
    /// needs at least one user. Phase 9.4: `active_period` must be one
    /// of the schema-legal values (21600 / 43200 / 86400 / 172800 —
    /// `td_api.tl:13715` comment); anything else is rejected before
    /// sending. Phase 9.5: `from_story` carries a repost source
    /// (`storyFullId`, `td_api.tl:6766`). The `story` response and
    /// `updateStoryPostSucceeded` / `updateStoryPostFailed` drive
    /// `Session::story_post.outcome`.
    #[allow(clippy::too_many_arguments)] // mirrors requests::post_story, one arg per schema field
    pub fn post_story(
        &mut self,
        chat_id: ChatId,
        kind: StoryMediaKind,
        path: &str,
        caption: &str,
        privacy: StoryPrivacy,
        user_ids: &[i64],
        areas: serde_json::Value,
        active_period: i32,
        from_story: Option<(i64, i32)>,
        is_posted_to_chat_page: bool,
        protect_content: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.my_user_id.is_none() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if kind == StoryMediaKind::Unknown || !std::path::Path::new(path).is_file() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if privacy == StoryPrivacy::SelectedUsers && user_ids.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !matches!(active_period, 21600 | 43200 | 86400 | 172800) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::PostStory, Some(chat_id));
        let json = post_story_request(
            extra,
            chat_id,
            kind,
            path,
            caption,
            privacy.settings_json(user_ids),
            areas,
            active_period,
            from_story,
            is_posted_to_chat_page,
            protect_content,
        );
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 9.5: `getChatsToPostStories` (TDLib 1.8.67,
    /// `schema/td_api.tl:13698`) — channels/supergroups where the user
    /// may post stories; stored in `Session::story_post_as_chats`.
    /// Deduped while a fetch is in flight.
    pub fn get_chats_to_post_stories(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::GetChatsToPostStories)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetChatsToPostStories, None);
        match self
            .sender
            .send_json(&get_chats_to_post_stories_request(extra))
        {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 9.5: `editStory` (TDLib 1.8.67, `schema/td_api.tl:13732`).
    /// Gated on the cached story's `can_be_edited`; `None` fields keep
    /// the current value. Sets `Session::story_manage` pending/error.
    pub fn edit_story(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
        content: Option<serde_json::Value>,
        areas: Option<serde_json::Value>,
        caption: Option<&str>,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let editable = self
            .session
            .stories
            .get(&(chat_id.0, story_id))
            .is_some_and(|story| story.can_be_edited);
        if !editable {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request_for_story(RequestPurpose::EditStory, chat_id, story_id);
        self.session.story_manage.pending = true;
        self.session.story_manage.error = None;
        match self.sender.send_json(&edit_story_request(
            extra, chat_id, story_id, content, areas, caption,
        )) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.story_manage.pending = false;
                Err(err)
            }
        }
    }

    /// Phase 9.5: `editStoryCover` (TDLib 1.8.67, `schema/td_api.tl:13738`).
    /// Gated on the cached story's `can_be_edited`.
    pub fn edit_story_cover(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
        cover_frame_timestamp: f64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let editable = self
            .session
            .stories
            .get(&(chat_id.0, story_id))
            .is_some_and(|story| story.can_be_edited);
        if !editable {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra =
            self.session
                .request_for_story(RequestPurpose::EditStoryCover, chat_id, story_id);
        self.session.story_manage.pending = true;
        self.session.story_manage.error = None;
        match self.sender.send_json(&edit_story_cover_request(
            extra,
            chat_id,
            story_id,
            cover_frame_timestamp,
        )) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.story_manage.pending = false;
                Err(err)
            }
        }
    }

    /// Phase 9.5: `setStoryPrivacySettings` (TDLib 1.8.67,
    /// `schema/td_api.tl:13743`). Gated on the cached story's
    /// `can_set_privacy_settings`.
    pub fn set_story_privacy_settings(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
        privacy_settings: serde_json::Value,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let settable = self
            .session
            .stories
            .get(&(chat_id.0, story_id))
            .is_some_and(|story| story.can_set_privacy_settings);
        if !settable {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request_for_story(
            RequestPurpose::SetStoryPrivacySettings,
            chat_id,
            story_id,
        );
        self.session.story_manage.pending = true;
        self.session.story_manage.error = None;
        match self.sender.send_json(&set_story_privacy_settings_request(
            extra,
            story_id,
            privacy_settings,
        )) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.story_manage.pending = false;
                Err(err)
            }
        }
    }
}
