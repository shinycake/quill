//! Connect driver: story statistics, public forwards and public story search.
use super::*;
use crate::ids::{ChatId, RequestId};
use crate::state::{RequestPurpose, StorySearchQuery, StoryStatsFetch};
use crate::telegram::requests_story_insights::{
    get_story_public_forwards, get_story_statistics, search_public_stories_by_location,
    search_public_stories_by_tag, search_public_stories_by_venue,
};

/// Public forwards and search hits are fetched in pages of this size.
const PAGE: i32 = 30;

impl<S: JsonSender> ConnectDriver<S> {
    /// `getStoryStatistics` plus the first `getStoryPublicForwards` page.
    /// The schema allows the statistics call only when
    /// `story.can_get_statistics`; anything else is refused.
    pub fn fetch_story_insights(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
        is_dark: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let can_get = self
            .session
            .stories
            .get(&(chat_id.0, story_id))
            .is_some_and(|story| story.can_get_statistics);
        if !can_get {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.requests.has_purpose_for_story(
            RequestPurpose::GetStoryStatistics,
            chat_id,
            story_id,
        ) {
            return Ok(None);
        }
        self.session.clear_story_insights();
        self.session.begin_story_insights(chat_id.0, story_id);
        let extra =
            self.session
                .request_for_story(RequestPurpose::GetStoryStatistics, chat_id, story_id);
        if let Err(err) = self
            .sender
            .send_json(&get_story_statistics(extra, chat_id, story_id, is_dark))
        {
            self.session.requests.take(extra);
            self.session.clear_story_insights();
            return Err(err);
        }
        let _ = self.fetch_story_public_forwards(chat_id, story_id, "");
        Ok(Some(extra))
    }

    /// One `getStoryPublicForwards` page; `offset` is the previous page's
    /// `next_offset` (empty for the first).
    pub fn fetch_story_public_forwards(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
        offset: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.requests.has_purpose_for_story(
            RequestPurpose::GetStoryPublicForwards,
            chat_id,
            story_id,
        ) {
            return Ok(None);
        }
        let extra = self.session.request_for_story(
            RequestPurpose::GetStoryPublicForwards,
            chat_id,
            story_id,
        );
        match self.sender.send_json(&get_story_public_forwards(
            extra, chat_id, story_id, offset, PAGE,
        )) {
            Ok(()) => {
                if let Some(state) = self.session.story_insights.as_mut()
                    && state.chat_id == chat_id.0
                    && state.story_id == story_id
                {
                    state.forwards_loading = true;
                }
                Ok(Some(extra))
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Re-run a failed statistics load.
    pub fn retry_story_statistics(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
        is_dark: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if self
            .session
            .story_insights
            .as_ref()
            .is_some_and(|state| matches!(state.statistics, StoryStatsFetch::Failed(_)))
        {
            self.session.clear_story_insights();
        }
        self.fetch_story_insights(chat_id, story_id, is_dark)
    }

    /// Start a public story search (first page).
    pub fn search_public_stories(
        &mut self,
        query: StorySearchQuery,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if let StorySearchQuery::Tag(tag) = &query
            && tag.trim_start_matches(['#', '$']).is_empty()
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.begin_story_search(query);
        self.send_story_search_page("")
    }

    /// The next page of the open search.
    pub fn load_more_found_stories(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        let Some(search) = self.session.story_search.as_ref() else {
            return Ok(None);
        };
        if search.loading || search.next_offset.is_empty() {
            return Ok(None);
        }
        let offset = search.next_offset.clone();
        self.session.story_search_page_requested();
        self.send_story_search_page(&offset)
    }

    fn send_story_search_page(
        &mut self,
        offset: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let Some(query) = self
            .session
            .story_search
            .as_ref()
            .map(|search| search.query.clone())
        else {
            return Ok(None);
        };
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::SearchPublicStories)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::SearchPublicStories, None);
        let json = match &query {
            StorySearchQuery::Tag(tag) => search_public_stories_by_tag(extra, 0, tag, offset, PAGE),
            StorySearchQuery::Location { address, .. } => {
                search_public_stories_by_location(extra, address, offset, PAGE)
            }
            StorySearchQuery::Venue {
                provider, venue_id, ..
            } => search_public_stories_by_venue(extra, provider, venue_id, offset, PAGE),
        };
        match self.sender.send_json(&json) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.clear_story_search();
                Err(err)
            }
        }
    }
}
