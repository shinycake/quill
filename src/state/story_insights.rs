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

/// Statistics plus the public forwards list of one story.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StoryInsightsState {
    pub chat_id: i64,
    pub story_id: i32,
    pub statistics: StoryStatsFetch,
    pub forwards: Vec<PublicForwardView>,
    pub forwards_total: i32,
    pub forwards_next_offset: String,
    pub forwards_loading: bool,
    pub forwards_error: Option<String>,
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
    pub loading: bool,
    pub error: Option<String>,
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
            .story_insights
            .as_ref()
            .is_some_and(|state| state.chat_id == chat_id && state.story_id == story_id);
        if !same {
            self.story_insights = Some(StoryInsightsState {
                chat_id,
                story_id,
                ..Default::default()
            });
        }
    }

    pub fn clear_story_insights(&mut self) {
        self.story_insights = None;
    }

    fn insights_for(&mut self, pending: &PendingRequest) -> Option<&mut StoryInsightsState> {
        let (chat_id, story_id) = (pending.chat_id?, pending.story_id?);
        self.story_insights
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
        if let Some(state) = self.insights_for(pending) {
            state.forwards_loading = false;
            state.forwards_error = None;
            state.forwards_total = view.total_count;
            state.forwards_next_offset = view.next_offset;
            state.forwards.extend(view.forwards);
        }
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
        } else {
            state.forwards_loading = false;
            state.forwards_error = Some(message);
        }
    }

    /// Start a new public story search; results of an earlier search go.
    pub fn begin_story_search(&mut self, query: StorySearchQuery) {
        self.story_search = Some(StorySearchState {
            query,
            stories: Vec::new(),
            total_count: 0,
            next_offset: String::new(),
            loading: true,
            error: None,
        });
    }

    /// Mark the next page of the open search as requested.
    pub fn story_search_page_requested(&mut self) {
        if let Some(search) = self.story_search.as_mut() {
            search.loading = true;
            search.error = None;
        }
    }

    pub fn clear_story_search(&mut self) {
        self.story_search = None;
    }

    pub(crate) fn accept_found_stories(&mut self, found: FoundStoriesView) {
        self.remember_files(&found.files);
        let keys: Vec<(i64, i32)> = found
            .stories
            .iter()
            .map(|story| (story.poster_chat_id, story.id))
            .collect();
        for story in found.stories {
            self.stories.insert((story.poster_chat_id, story.id), story);
        }
        let Some(search) = self.story_search.as_mut() else {
            return;
        };
        search.loading = false;
        search.error = None;
        search.total_count = found.total_count;
        search.next_offset = found.next_offset;
        for key in keys {
            if !search.stories.contains(&key) {
                search.stories.push(key);
            }
        }
    }

    pub(crate) fn fail_story_search(&mut self, message: String) {
        if let Some(search) = self.story_search.as_mut() {
            search.loading = false;
            search.error = Some(message);
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
