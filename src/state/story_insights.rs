//! Story statistics, public forwards and public story search state.
use super::*;
use crate::telegram::envelope::{
    FoundStoriesView, PublicForwardView, PublicForwardsView, StoryStatisticsView,
};
use crate::telegram::requests_story_insights::StoryLocationAddress;

/// Fetch state of the `getStoryStatistics` answer.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum StoryStatsFetch {
    #[default]
    Loading,
    Loaded(StoryStatisticsView),
    Failed(String),
}

/// One page request of a paged list: idle, waiting for its own answer
/// (`None` between starting the list and sending the request), or failed
/// with the text the panel draws.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum PageFetch {
    #[default]
    Idle,
    Loading(Option<RequestId>),
    Failed(String),
}

impl PageFetch {
    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading(_))
    }

    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Failed(message) => Some(message),
            _ => None,
        }
    }

    /// Whether `id` is the request this page waits for.
    pub(crate) fn waits_for(&self, id: RequestId) -> bool {
        matches!(self, Self::Loading(Some(waiting)) if *waiting == id)
    }
}

/// Statistics plus the public forwards list of one story.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StoryInsightsState {
    pub chat_id: i64,
    pub story_id: i32,
    pub statistics: StoryStatsFetch,
    pub forwards: Vec<PublicForwardView>,
    pub forwards_total: i32,
    pub forwards_next_offset: String,
    /// Where the forwards page request stands.
    pub forwards_page: ForwardsPage,
}

/// The public-forwards request of one insights panel. A page is applied
/// only when the answer carries the id it is `Loading` for, and a first
/// page replaces the rows instead of extending them, so a page asked for
/// twice never shows twice.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ForwardsPage {
    /// The first page was never asked for.
    #[default]
    NotAsked,
    /// `request` asks for the page after `offset` (empty: the first).
    Loading { request: RequestId, offset: String },
    /// The last page landed; more come while `forwards_next_offset` is set.
    Ready,
    /// The last page failed; scrolling or Retry asks again.
    Failed(String),
}

impl StoryInsightsState {
    pub fn forwards_loading(&self) -> bool {
        matches!(self.forwards_page, ForwardsPage::Loading { .. })
    }

    pub fn forwards_error(&self) -> Option<&str> {
        match &self.forwards_page {
            ForwardsPage::Failed(error) => Some(error),
            _ => None,
        }
    }
}

/// What a public story search looks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorySearchQuery {
    /// A hashtag or cashtag, stored with its leading `#` or `$`.
    Tag(String),
    Location {
        address: StoryLocationAddress,
        label: String,
    },
    Venue {
        provider: String,
        venue_id: String,
        label: String,
    },
}

impl StorySearchQuery {
    /// Heading of the results panel.
    pub fn label(&self) -> &str {
        match self {
            Self::Tag(tag) => tag,
            Self::Location { label, .. } | Self::Venue { label, .. } => label,
        }
    }
}

/// One public story search and the pages received so far.
#[derive(Debug, Clone, PartialEq)]
pub struct StorySearchState {
    pub query: StorySearchQuery,
    /// `(poster chat id, story id)` of each hit, in result order.
    pub stories: Vec<(i64, i32)>,
    pub total_count: i32,
    pub next_offset: String,
    /// The page request this search waits for. Only its answer or error
    /// fills the search: an earlier search's request still in flight
    /// answers into nothing.
    pub page: PageFetch,
}

/// The hashtag or cashtag a chat-search box holds, when the whole text is
/// one (`#sunset`, `$TON`). Returns it with its sigil.
pub fn story_search_tag(text: &str) -> Option<String> {
    let text = text.trim();
    let mut chars = text.chars();
    let sigil = chars.next().filter(|c| matches!(c, '#' | '$'))?;
    let rest = chars.as_str();
    (!rest.is_empty() && rest.chars().all(|c| c.is_alphanumeric() || c == '_'))
        .then(|| format!("{sigil}{rest}"))
}

/// "Private Shares" in tdesktop's story statistics: forwards that are not
/// among the public ones (`forward_count - public total`, never negative).
pub fn private_share_count(forward_count: i32, public_forwards: i32) -> i32 {
    (forward_count - public_forwards).max(0)
}

impl Session {
    /// Start tracking insights for a story. The same story keeps what it
    /// has; another story starts clean.
    pub fn begin_story_insights(&mut self, chat_id: i64, story_id: i32) {
        let same = self
            .stories
            .insights
            .as_ref()
            .is_some_and(|state| state.chat_id == chat_id && state.story_id == story_id);
        if !same {
            self.stories.insights = Some(StoryInsightsState {
                chat_id,
                story_id,
                ..Default::default()
            });
        }
    }

    pub fn clear_story_insights(&mut self) {
        self.stories.insights = None;
    }

    /// The driver sent the forwards page after `offset` as `request` for
    /// the story's insights.
    pub fn story_forwards_loading(
        &mut self,
        chat_id: i64,
        story_id: i32,
        request: RequestId,
        offset: &str,
    ) {
        if let Some(state) = self.stories.insights.as_mut()
            && state.chat_id == chat_id
            && state.story_id == story_id
        {
            state.forwards_page = ForwardsPage::Loading {
                request,
                offset: offset.to_string(),
            };
        }
    }

    fn insights_for(&mut self, pending: &PendingRequest) -> Option<&mut StoryInsightsState> {
        let (chat_id, story_id) = (pending.chat_id?, pending.story_id?);
        self.stories
            .insights
            .as_mut()
            .filter(|state| state.chat_id == chat_id.0 && state.story_id == story_id)
    }

    pub(crate) fn accept_story_statistics(
        &mut self,
        pending: &PendingRequest,
        statistics: StoryStatisticsView,
    ) {
        if let Some(state) = self.insights_for(pending) {
            state.statistics = StoryStatsFetch::Loaded(statistics);
        }
    }

    pub(crate) fn accept_public_forwards(
        &mut self,
        pending: &PendingRequest,
        view: PublicForwardsView,
    ) {
        let Some(state) = self.insights_for(pending) else {
            return;
        };
        let ForwardsPage::Loading { request, offset } = &state.forwards_page else {
            return;
        };
        if *request != pending.id {
            return;
        }
        if offset.is_empty() {
            state.forwards.clear();
        }
        state.forwards_page = ForwardsPage::Ready;
        state.forwards_total = view.total_count;
        state.forwards_next_offset = view.next_offset;
        state.forwards.extend(view.forwards);
    }

    /// A TDLib error on one of the two insight requests.
    pub(crate) fn fail_story_insights(
        &mut self,
        pending: &PendingRequest,
        purpose: RequestPurpose,
        message: String,
    ) {
        let Some(state) = self.insights_for(pending) else {
            return;
        };
        if purpose == RequestPurpose::GetStoryStatistics {
            state.statistics = StoryStatsFetch::Failed(message);
        } else if matches!(
            &state.forwards_page,
            ForwardsPage::Loading { request, .. } if *request == pending.id
        ) {
            state.forwards_page = ForwardsPage::Failed(message);
        }
    }

    /// Start a new public story search; results of an earlier search go.
    pub fn begin_story_search(&mut self, query: StorySearchQuery) {
        self.stories.search = Some(StorySearchState {
            query,
            stories: Vec::new(),
            total_count: 0,
            next_offset: String::new(),
            page: PageFetch::Loading(None),
        });
    }

    /// The open search sent `request` for its next page.
    pub fn story_search_sent(&mut self, request: RequestId) {
        if let Some(search) = self.stories.search.as_mut() {
            search.page = PageFetch::Loading(Some(request));
        }
    }

    /// The open search, when `pending` is the page request it waits for.
    fn search_waiting_for(&mut self, pending: &PendingRequest) -> Option<&mut StorySearchState> {
        self.stories
            .search
            .as_mut()
            .filter(|search| search.page.waits_for(pending.id))
    }

    /// Mark the next page of the open search as requested.
    pub fn story_search_page_requested(&mut self) {
        if let Some(search) = self.stories.search.as_mut() {
            search.page = PageFetch::Loading(None);
        }
    }

    /// The open search could not send its page: the pages stay and the
    /// panel offers the retry.
    pub fn story_search_send_failed(&mut self, message: String) {
        if let Some(search) = self.stories.search.as_mut() {
            search.page = PageFetch::Failed(message);
        }
    }

    pub fn clear_story_search(&mut self) {
        self.stories.search = None;
    }

    pub(crate) fn accept_found_stories(
        &mut self,
        pending: &PendingRequest,
        found: FoundStoriesView,
    ) {
        self.remember_files(&found.files);
        let keys: Vec<(i64, i32)> = found
            .stories
            .iter()
            .map(|story| (story.poster_chat_id, story.id))
            .collect();
        for story in found.stories {
            self.stories
                .stories
                .insert((story.poster_chat_id, story.id), story);
        }
        let Some(search) = self.search_waiting_for(pending) else {
            return;
        };
        search.page = PageFetch::Idle;
        search.total_count = found.total_count;
        search.next_offset = found.next_offset;
        for key in keys {
            if !search.stories.contains(&key) {
                search.stories.push(key);
            }
        }
    }

    pub(crate) fn fail_story_search(&mut self, pending: &PendingRequest, message: String) {
        if let Some(search) = self.search_waiting_for(pending) {
            search.page = PageFetch::Failed(message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{private_share_count, story_search_tag};

    #[test]
    fn private_shares_never_go_negative() {
        assert_eq!(private_share_count(10, 4), 6);
        assert_eq!(private_share_count(3, 5), 0);
    }

    #[test]
    fn tag_is_recognised_only_when_it_is_the_whole_text() {
        assert_eq!(story_search_tag(" #sunset "), Some("#sunset".into()));
        assert_eq!(story_search_tag("$TON"), Some("$TON".into()));
        assert_eq!(story_search_tag("#"), None);
        assert_eq!(story_search_tag("sunset"), None);
        assert_eq!(story_search_tag("#two words"), None);
        assert_eq!(story_search_tag("#a-b"), None);
    }
}
