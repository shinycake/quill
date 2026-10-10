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
    get_story_custom_emoji_stickers as get_story_custom_emoji_stickers_request,
    get_story_interactions as get_story_interactions_request, load_active_stories, open_story,
    post_story as post_story_request, report_story as report_story_request, send_text_story_reply,
    set_story_custom_emoji_reaction as set_story_custom_emoji_reaction_request,
    set_story_privacy_settings as set_story_privacy_settings_request, set_story_reaction,
};
use crate::telegram::requests_story::{
    add_story_album_stories, create_story_album, delete_story_album, get_chat_archived_stories,
    get_chat_posted_to_chat_page_stories, get_chat_story_albums,
    get_close_friends as get_close_friends_request, get_story_album_stories,
    remove_story_album_stories, reorder_story_album_stories, reorder_story_albums,
    set_chat_active_stories_list, set_chat_pinned_stories,
    set_close_friends as set_close_friends_request, set_story_album_name,
    toggle_story_is_posted_to_chat_page,
};

mod albums;
mod posting;

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

    /// Phase 9.2+: `getCustomEmojiStickers` for custom-emoji reaction
    /// visuals (picker options and the chosen reaction badge). Dedupes
    /// against the cache; one in-flight request at a time. The viewer
    /// tick calls this while open — it is a no-op when there is nothing
    /// new to fetch.
    pub fn maybe_fetch_story_custom_emoji_stickers(
        &mut self,
        ids: &[i64],
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::GetStoryCustomEmojiStickers)
        {
            return Ok(None);
        }
        let ids: Vec<i64> = ids
            .iter()
            .copied()
            .filter(|id| *id > 0 && !self.session.story_custom_emoji_stickers.contains_key(id))
            .collect();
        if ids.is_empty() {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetStoryCustomEmojiStickers, None);
        match self
            .sender
            .send_json(&get_story_custom_emoji_stickers_request(extra, &ids))
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
}
