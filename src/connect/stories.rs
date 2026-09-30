//! Connect driver: stories.
use super::*;
use crate::ids::{ChatId, RequestId};
use crate::state::LiveStoryJoinIntent;
use crate::state::RequestPurpose;
use crate::story_composer::{StoryMediaKind, StoryPrivacy};
use crate::story_page::{StoryPageOp, StoryPageOpState, story_page_op_label};
use crate::telegram::envelope::StoryContentView;
use crate::telegram::requests::{
    activate_story_stealth_mode as activate_story_stealth_mode_request,
    can_post_story as can_post_story_request, close_story, delete_story,
    edit_story as edit_story_request, edit_story_cover as edit_story_cover_request,
    get_chat_active_stories, get_chats_to_post_stories as get_chats_to_post_stories_request,
    get_story, get_story_available_reactions,
    get_story_interactions as get_story_interactions_request, load_active_stories, open_story,
    post_story as post_story_request, report_story as report_story_request, send_text_story_reply,
    set_story_custom_emoji_reaction as set_story_custom_emoji_reaction_request,
    set_story_privacy_settings as set_story_privacy_settings_request, set_story_reaction,
};
use crate::telegram::requests_story::{
    add_story_album_stories, create_story_album, delete_story_album, get_chat_archived_stories,
    get_chat_posted_to_chat_page_stories, get_chat_story_albums, get_story_album_stories,
    remove_story_album_stories, reorder_story_album_stories, reorder_story_albums,
    set_chat_pinned_stories, set_story_album_name,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// Phase 9.1: `loadActiveStories(storyListMain)` once per Ready. The
    /// loaded stories arrive as `updateChatActiveStories` updates and feed
    /// the story tray above the chat list.
    pub fn maybe_load_active_stories(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() || self.session.stories_active_loaded {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::LoadActiveStories)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::LoadActiveStories, None);
        match self.sender.send_json(&load_active_stories(extra)) {
            Ok(()) => {
                self.session.stories_active_loaded = true;
                Ok(Some(extra))
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 9.1: refresh one chat's active stories (`getChatActiveStories`).
    /// The response is `chatActiveStories`, handled like the update.
    pub fn get_chat_active_stories(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetChatActiveStories, chat_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetChatActiveStories, Some(chat_id));
        match self
            .sender
            .send_json(&get_chat_active_stories(extra, chat_id))
        {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// stories-live-play: join the group call behind a live story
    /// (`storyContentLive.group_call_id`, `schema/td_api.tl:6662`).
    /// Two-step, reusing the C3a machinery: `getGroupCall` (via the
    /// existing `fetch_group_call`) creates the unjoined tracker, and the
    /// ingest pump issues `join_video_chat` once the tracker exists
    /// (`pending_live_story_join`). Refuses while a 1:1 call is active,
    /// while another group call is tracked, or while a live-story join is
    /// already pending — the group-call overlay's Join button stays the
    /// manual fallback either way.
    pub fn join_live_story(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let group_call_id = match self.session.stories.get(&(chat_id.0, story_id)) {
            Some(story) => match story.content {
                StoryContentView::Live { group_call_id, .. } => group_call_id,
                _ => return Err(ConnectSendError::InvalidRequest),
            },
            None => return Err(ConnectSendError::InvalidRequest),
        };
        if self.session.active_call.is_some() || self.session.active_group_call.is_some() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.pending_live_story_join.is_some() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.fetch_group_call(group_call_id)?;
        self.session.pending_live_story_join = Some(LiveStoryJoinIntent {
            group_call_id,
            request: extra,
        });
        Ok(extra)
    }

    /// Phase 9.1: fetch one story's full content (`getStory`). Deduped by
    /// the story cache + in-flight per-story requests; the `story` response
    /// is authoritative on `(poster_chat_id, id)`.
    pub fn get_story(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.stories.contains_key(&(chat_id.0, story_id))
            || self.session.requests.has_purpose_for_story(
                RequestPurpose::GetStory,
                chat_id,
                story_id,
            )
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request_for_story(RequestPurpose::GetStory, chat_id, story_id);
        match self.sender.send_json(&get_story(extra, chat_id, story_id)) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 9.1: `openStory` — the user opened a story for viewing.
    /// Fire-and-forget; the `ok` answer needs no handling.
    pub fn open_story(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::OpenStory, Some(chat_id));
        match self.sender.send_json(&open_story(extra, chat_id, story_id)) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 9.1: `closeStory` — the user closed a story. Fire-and-forget.
    pub fn close_story(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::CloseStory, Some(chat_id));
        match self
            .sender
            .send_json(&close_story(extra, chat_id, story_id))
        {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 9.2: `getStoryAvailableReactions` (row_size 10, within the
    /// schema's 5–25 range). The `availableReactions` response feeds the
    /// story viewer's reaction picker. Deduped while a request is
    /// in-flight; cached afterwards (`Session::story_available_reactions`).
    pub fn get_story_available_reactions(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.story_available_reactions.is_some()
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::GetStoryAvailableReactions)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetStoryAvailableReactions, None);
        match self
            .sender
            .send_json(&get_story_available_reactions(extra, 10))
        {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 9.2: `setStoryReaction` — set (or with `None`, remove) the
    /// user's emoji reaction on a story. Gates: the story must be cached
    /// and must not be live (`setStoryReaction` is not supported for live
    /// stories, TDLib 1.8.67 `schema/td_api.tl:13809`). The reaction shows
    /// up via the follow-up `updateStory` (`story.chosen_reaction_type`).
    pub fn set_story_reaction(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
        emoji: Option<&str>,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let story = self
            .session
            .stories
            .get(&(chat_id.0, story_id))
            .ok_or(ConnectSendError::InvalidRequest)?;
        if matches!(story.content, StoryContentView::Live { .. }) {
            return Err(ConnectSendError::InvalidRequest);
        }
        if emoji.is_some_and(|emoji| emoji.trim().is_empty()) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = if emoji.is_some() {
            RequestPurpose::SetStoryReaction
        } else {
            RequestPurpose::RemoveStoryReaction
        };
        let extra = self.session.request_for_story(purpose, chat_id, story_id);
        match self
            .sender
            .send_json(&set_story_reaction(extra, chat_id, story_id, emoji))
        {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 9.2+: `setStoryReaction` with a `reactionTypeCustomEmoji`.
    /// Same gates as the emoji path (chats path, story cached, not live);
    /// the id must be positive. Premium enforcement is server-side —
    /// TDLib rejects non-Premium callers and the error surfaces on the
    /// in-flight request; the picker gates on
    /// `availableReaction.needs_premium`. Reuses
    /// `RequestPurpose::SetStoryReaction`: the `ok` is ignored either way
    /// (the truth arrives via `updateStory` → `chosen_reaction_type`).
    pub fn set_story_custom_emoji_reaction(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
        custom_emoji_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let story = self
            .session
            .stories
            .get(&(chat_id.0, story_id))
            .ok_or(ConnectSendError::InvalidRequest)?;
        if matches!(story.content, StoryContentView::Live { .. }) {
            return Err(ConnectSendError::InvalidRequest);
        }
        if custom_emoji_id <= 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra =
            self.session
                .request_for_story(RequestPurpose::SetStoryReaction, chat_id, story_id);
        match self
            .sender
            .send_json(&set_story_custom_emoji_reaction_request(
                extra,
                chat_id,
                story_id,
                custom_emoji_id,
            )) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 9.2: `deleteStory` — delete a story posted by the current
    /// user. Gated on `story.can_be_deleted`. The deletion lands as
    /// `updateStoryDeleted`.
    pub fn delete_story(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let deletable = self
            .session
            .stories
            .get(&(chat_id.0, story_id))
            .is_some_and(|story| story.can_be_deleted);
        if !deletable {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request_for_story(RequestPurpose::DeleteStory, chat_id, story_id);
        match self
            .sender
            .send_json(&delete_story(extra, chat_id, story_id))
        {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Phase 9.7: send a story-page request; on transport failure take the
    /// pending request back and mark the page op failed (the UI surfaces
    /// it; the driver never silently swallows it).
    fn send_story_page(
        &mut self,
        purpose: RequestPurpose,
        extra: RequestId,
        json: &str,
    ) -> Result<RequestId, ConnectSendError> {
        match self.sender.send_json(json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.story_page_op = Some(StoryPageOp {
                    label: story_page_op_label(purpose),
                    state: StoryPageOpState::Failed("could not send".to_string()),
                });
                Err(err)
            }
        }
    }

    /// Phase 9.7: the chat must exist for story-page requests.
    fn story_page_chat(&self, chat_id: ChatId) -> bool {
        self.chats_path_active() && self.session.chats.contains_key(&chat_id.0)
    }

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
            self.session.chat_page_stories.remove(&chat_id.0);
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
