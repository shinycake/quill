//! Statistics beyond the chat overview: async and zoomed graphs
//! (`getStatisticalGraph`), message statistics with their public
//! forwards, previews of recent posts, and which page the statistics
//! panel shows (tdesktop `info/statistics/`).
use super::*;
use crate::telegram::envelope::{
    MessageStatisticsView, ParsedMessage, PublicForwardsView, StatisticalGraph,
};

/// Which page the statistics panel shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatsPage {
    /// The chat's statistics.
    #[default]
    Overview,
    /// "Message Statistic" for a post of the chat.
    Message(i64),
    /// "Story Statistic" for a story of the chat.
    Story(i32),
}

/// One `getStatisticalGraph` call: the chat, the graph's token and the
/// zoom point (0 for an async graph).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GraphKey {
    pub chat_id: i64,
    pub token: String,
    pub x: i64,
}

/// Fetch state of one `getStatisticalGraph` answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphFetch {
    Loading,
    Loaded(StatisticalGraph),
    Failed(String),
}

/// Fetch state of a `getMessageStatistics` answer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum MessageStatsFetch {
    #[default]
    Loading,
    Loaded(MessageStatisticsView),
    Failed(String),
}

/// Pages of public forwards received so far.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PublicForwardsList {
    pub forwards: Vec<crate::telegram::envelope::PublicForwardView>,
    pub total: i32,
    pub next_offset: String,
    pub loading: bool,
    pub error: Option<String>,
}

impl PublicForwardsList {
    fn accept(&mut self, view: PublicForwardsView) {
        self.loading = false;
        self.error = None;
        self.total = view.total_count;
        self.next_offset = view.next_offset;
        self.forwards.extend(view.forwards);
    }
}

/// Statistics and public forwards of one channel post.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MessageStatsState {
    pub chat_id: i64,
    pub message_id: i64,
    pub statistics: MessageStatsFetch,
    pub forwards: PublicForwardsList,
}

/// What a recent-post row shows of a post that is not in the loaded
/// history: its one-line preview and date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatsPostPreview {
    pub text: String,
    pub date: i32,
    pub views: i32,
    pub forwards: i32,
}

/// tdesktop shows ten recent posts first and thirty more per "Show more".
pub const RECENT_POSTS_FIRST: usize = 10;
pub const RECENT_POSTS_PAGE: usize = 30;

/// The statistics panel's state beyond `GroupsState::chat_statistics`.
#[derive(Debug, Default)]
pub struct StatsState {
    /// The page the panel shows, keyed by chat.
    pages: HashMap<i64, StatsPage>,
    pub graphs: HashMap<GraphKey, GraphFetch>,
    graph_requests: HashMap<RequestId, GraphKey>,
    /// Graphs zoomed into one day: graph token → the zoom point.
    pub zoomed: HashMap<String, i64>,
    /// The post whose statistics are open (one at a time).
    pub message: Option<MessageStatsState>,
    /// Fetched previews of recent posts, keyed by (chat id, message id).
    pub posts: HashMap<(i64, i64), Option<StatsPostPreview>>,
    /// How many recent posts are listed, keyed by chat.
    pub recent_shown: HashMap<i64, usize>,
}

impl StatsState {
    pub fn page(&self, chat_id: i64) -> StatsPage {
        self.pages.get(&chat_id).copied().unwrap_or_default()
    }

    pub fn set_page(&mut self, chat_id: i64, page: StatsPage) {
        if page == StatsPage::Overview {
            self.pages.remove(&chat_id);
        } else {
            self.pages.insert(chat_id, page);
        }
    }

    /// The panel was opened afresh: it starts on the overview with the
    /// first page of recent posts and no zoomed graph.
    pub fn reset(&mut self, chat_id: i64) {
        self.pages.remove(&chat_id);
        self.recent_shown.remove(&chat_id);
        self.zoomed.clear();
    }

    /// How many recent posts the overview lists.
    pub fn recent_posts_shown(&self, chat_id: i64, total: usize) -> usize {
        self.recent_shown
            .get(&chat_id)
            .copied()
            .unwrap_or(RECENT_POSTS_FIRST)
            .min(total)
    }

    /// "Show more": thirty more recent posts.
    pub fn show_more_recent_posts(&mut self, chat_id: i64, total: usize) {
        let shown = self.recent_posts_shown(chat_id, total);
        self.recent_shown
            .insert(chat_id, (shown + RECENT_POSTS_PAGE).min(total));
    }

    pub fn graph(&self, chat_id: i64, token: &str, x: i64) -> Option<&GraphFetch> {
        self.graphs.get(&GraphKey {
            chat_id,
            token: token.to_string(),
            x,
        })
    }

    /// Record a sent `getStatisticalGraph`.
    pub(crate) fn graph_requested(&mut self, extra: RequestId, key: GraphKey) {
        self.graphs.insert(key.clone(), GraphFetch::Loading);
        self.graph_requests.insert(extra, key);
    }

    /// Forget a request that could not be sent.
    pub(crate) fn graph_unsent(&mut self, extra: RequestId) {
        if let Some(key) = self.graph_requests.remove(&extra) {
            self.graphs.remove(&key);
        }
    }

    pub(crate) fn accept_graph(&mut self, extra: RequestId, graph: StatisticalGraph) {
        if let Some(key) = self.graph_requests.remove(&extra) {
            self.graphs.insert(key, GraphFetch::Loaded(graph));
        }
    }

    pub(crate) fn fail_graph(&mut self, extra: RequestId, message: String) {
        if let Some(key) = self.graph_requests.remove(&extra) {
            self.graphs.insert(key, GraphFetch::Failed(message));
        }
    }

    /// Start tracking a post's statistics; the same post keeps what it
    /// has.
    pub fn begin_message(&mut self, chat_id: i64, message_id: i64) {
        let same = self
            .message
            .as_ref()
            .is_some_and(|m| m.chat_id == chat_id && m.message_id == message_id);
        if !same {
            self.message = Some(MessageStatsState {
                chat_id,
                message_id,
                ..Default::default()
            });
        }
    }

    pub fn message_for(&self, chat_id: i64, message_id: i64) -> Option<&MessageStatsState> {
        self.message
            .as_ref()
            .filter(|m| m.chat_id == chat_id && m.message_id == message_id)
    }

    fn message_mut(&mut self, chat_id: i64, message_id: i64) -> Option<&mut MessageStatsState> {
        self.message
            .as_mut()
            .filter(|m| m.chat_id == chat_id && m.message_id == message_id)
    }

    pub(crate) fn forwards_requested(&mut self, chat_id: i64, message_id: i64) {
        if let Some(state) = self.message_mut(chat_id, message_id) {
            state.forwards.loading = true;
            state.forwards.error = None;
        }
    }

    pub(crate) fn accept_message_statistics(
        &mut self,
        chat_id: i64,
        message_id: i64,
        statistics: MessageStatisticsView,
    ) {
        if let Some(state) = self.message_mut(chat_id, message_id) {
            state.statistics = MessageStatsFetch::Loaded(statistics);
        }
    }

    pub(crate) fn accept_message_forwards(
        &mut self,
        chat_id: i64,
        message_id: i64,
        view: PublicForwardsView,
    ) {
        if let Some(state) = self.message_mut(chat_id, message_id) {
            state.forwards.accept(view);
        }
    }

    pub(crate) fn fail_message_statistics(&mut self, chat_id: i64, message_id: i64, error: String) {
        if let Some(state) = self.message_mut(chat_id, message_id) {
            state.statistics = MessageStatsFetch::Failed(error);
        }
    }

    pub(crate) fn fail_message_forwards(&mut self, chat_id: i64, message_id: i64, error: String) {
        if let Some(state) = self.message_mut(chat_id, message_id) {
            state.forwards.loading = false;
            state.forwards.error = Some(error);
        }
    }

    pub(crate) fn accept_post(&mut self, chat_id: i64, message_id: i64, message: &ParsedMessage) {
        let info = message.interaction_info.as_ref();
        self.posts.insert(
            (chat_id, message_id),
            Some(StatsPostPreview {
                text: message.content.preview(),
                date: message.date,
                views: info.map_or(0, |i| i.view_count),
                forwards: info.map_or(0, |i| i.forward_count),
            }),
        );
    }

    /// A post that could not be fetched (deleted): its row is skipped.
    pub(crate) fn post_missing(&mut self, chat_id: i64, message_id: i64) {
        self.posts.insert((chat_id, message_id), None);
    }
}

/// What is known about a post a statistics row shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostLookup {
    Ready(StatsPostPreview),
    /// Fetched and gone (deleted): the row is skipped.
    Missing,
    /// Not loaded yet: fetch it.
    Unknown,
}

impl Session {
    /// A post's preview: from the loaded history when it is there, else
    /// from a `getMessage` fetched for the statistics.
    pub fn stats_post(&self, chat_id: i64, message_id: i64) -> PostLookup {
        if let Some(message) = self
            .histories
            .get(&chat_id)
            .and_then(|history| history.messages.get(&message_id))
        {
            let info = message.interaction_info.as_ref();
            return PostLookup::Ready(StatsPostPreview {
                text: message.content.preview(),
                date: message.date,
                views: info.map_or(0, |i| i.view_count),
                forwards: info.map_or(0, |i| i.forward_count),
            });
        }
        match self.groups.stats.posts.get(&(chat_id, message_id)) {
            Some(Some(preview)) => PostLookup::Ready(preview.clone()),
            Some(None) => PostLookup::Missing,
            None => PostLookup::Unknown,
        }
    }

    /// Open the statistics of `message_id` in `chat_id`'s panel.
    pub fn open_message_stats_page(&mut self, chat_id: i64, message_id: i64) {
        self.groups
            .stats
            .set_page(chat_id, StatsPage::Message(message_id));
        self.groups.stats.begin_message(chat_id, message_id);
    }
}
