//! Stories, the story tray, albums, archive, posting, viewers and close friends: the `stories` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use super::fetch::next_retry;
use crate::state::fetch::Fetch;
use crate::state::*;
use crate::story_page::StoryPageOps;
use std::time::{Duration, Instant};

/// The close-friends list and the request loading it, in one value so a
/// failure is matched to the `getCloseFriends` that asked (the viewer's
/// single story-page op is overwritten by any later op).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum CloseFriendsList {
    #[default]
    NotLoaded,
    /// `getCloseFriends` `request` is out; `stale` is the last list.
    Loading {
        request: RequestId,
        stale: Option<Vec<i64>>,
    },
    Ready(Vec<i64>),
    /// The load failed; `stale` is the last list, if any.
    Failed {
        stale: Option<Vec<i64>>,
    },
}

impl CloseFriendsList {
    /// The newest list held.
    pub fn ids(&self) -> Option<&Vec<i64>> {
        match self {
            Self::NotLoaded => None,
            Self::Ready(ids) => Some(ids),
            Self::Loading { stale, .. } | Self::Failed { stale } => stale.as_ref(),
        }
    }

    /// The load failed and there is no list to fall back on.
    pub fn failed_without_list(&self) -> bool {
        matches!(self, Self::Failed { stale: None })
    }

    fn take_ids(&mut self) -> Option<Vec<i64>> {
        match std::mem::take(self) {
            Self::NotLoaded => None,
            Self::Ready(ids) => Some(ids),
            Self::Loading { stale, .. } | Self::Failed { stale } => stale,
        }
    }

    /// A load is out.
    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    /// `getCloseFriends` `request` went out.
    pub fn begin_load(&mut self, request: RequestId) {
        let stale = self.take_ids();
        *self = Self::Loading { request, stale };
    }

    /// The answer of `request`: only the load in flight under that id
    /// takes it (a list saved since, or a newer load, is left alone).
    pub fn answered(&mut self, request: Option<RequestId>, ids: Vec<i64>) -> bool {
        if request.is_some()
            && matches!(self, Self::Loading { request: r, .. } if Some(*r) == request)
        {
            *self = Self::Ready(ids);
            return true;
        }
        false
    }

    /// The send of the load failed, or TDLib's error for `request`
    /// arrived: only the load in flight under that id fails.
    pub fn fail_load(&mut self, request: RequestId) {
        if matches!(self, Self::Loading { request: r, .. } if *r == request) {
            let stale = self.take_ids();
            *self = Self::Failed { stale };
        }
    }
}

/// How far `loadActiveStories` got through one story list. TDLib sends
/// `updateChatActiveStories` only for chats above the point loaded so far
/// (`send_update_chat_active_stories` skips `public_order_ == 0`), so a list
/// must page to its 404 before a new story past that point can show.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ActiveStoriesPaging {
    /// The next page may go out.
    #[default]
    Idle,
    /// A page is in flight as `request`; only its answer or error leaves
    /// this stage. `wait` is the wait of the failure the page follows, so
    /// the next failure doubles it.
    Loading {
        request: RequestId,
        wait: Option<Duration>,
    },
    /// After a failure other than 404: no page goes out before `until`,
    /// and `wait` doubles per failure.
    Waiting { until: Instant, wait: Duration },
    /// `loadActiveStories` answered 404: the whole list is loaded.
    Exhausted,
}

impl ActiveStoriesPaging {
    /// Whether the next page may go out at `now`.
    pub fn may_load(&self, now: Instant) -> bool {
        match self {
            Self::Idle => true,
            Self::Waiting { until, .. } => now >= *until,
            Self::Loading { .. } | Self::Exhausted => false,
        }
    }

    /// The clock of a failed page, if one is running.
    pub fn retry(&self) -> Option<(Instant, Duration)> {
        match self {
            Self::Waiting { until, wait } => Some((*until, *wait)),
            _ => None,
        }
    }

    /// The next page went out as `request`.
    pub fn begin(&mut self, request: RequestId) {
        let wait = match self {
            Self::Waiting { wait, .. } => Some(*wait),
            Self::Loading { wait, .. } => *wait,
            Self::Idle | Self::Exhausted => None,
        };
        *self = Self::Loading { request, wait };
    }

    /// The page `request` asked failed with `code`: 404 means everything is
    /// loaded; anything else (a timeout, FLOOD_WAIT, a 500) waits and tries
    /// again. Only the page in flight fails; any other request's error
    /// changes nothing.
    pub fn failed(&mut self, request: Option<RequestId>, code: i32, now: Instant) {
        let Self::Loading {
            request: asked,
            wait,
        } = *self
        else {
            return;
        };
        if request != Some(asked) {
            return;
        }
        *self = if code == 404 {
            Self::Exhausted
        } else {
            let (until, wait) = next_retry(wait.map(|wait| (now, wait)), now);
            Self::Waiting { until, wait }
        };
    }

    /// The page `request` asked answered: the next one may go out.
    pub fn answered(&mut self, request: Option<RequestId>) {
        if matches!(self, Self::Loading { request: asked, .. } if Some(*asked) == request) {
            *self = Self::Idle;
        }
    }
}

/// The story picker's `getCustomEmojiStickers` fetch after failures.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum CustomEmojiFetch {
    #[default]
    Idle,
    /// The fetch is out as `request` (after `failures` failures in a row);
    /// only its own answer or error leaves the stage, and no second fetch
    /// goes out meanwhile.
    Asking { request: RequestId, failures: u32 },
    /// It failed `failures` times in a row: no new fetch before `until`,
    /// and `wait` doubles per failure. An answer ends it.
    Backoff {
        failures: u32,
        until: Instant,
        wait: Duration,
    },
}

impl CustomEmojiFetch {
    /// How many fetches failed in a row. The story viewer's warm-up record
    /// carries it, so a failure makes the next tick fetch again.
    pub fn failures(&self) -> u32 {
        match self {
            Self::Idle => 0,
            Self::Asking { failures, .. } | Self::Backoff { failures, .. } => *failures,
        }
    }

    pub fn waiting(&self, now: Instant) -> bool {
        matches!(self, Self::Backoff { until, .. } if now < *until)
    }

    /// A fetch is out.
    pub fn is_asking(&self) -> bool {
        matches!(self, Self::Asking { .. })
    }

    /// A fetch went out as `request`.
    pub fn asked(&mut self, request: RequestId) {
        *self = Self::Asking {
            request,
            failures: self.failures(),
        };
    }

    /// `request` answered: the stage returns to idle with no failures. The
    /// answer of any other request leaves a fetch in flight alone.
    pub fn answered(&mut self, request: Option<RequestId>) {
        if let Self::Asking {
            request: waiting, ..
        } = self
            && request != Some(*waiting)
        {
            return;
        }
        *self = Self::Idle;
    }

    /// `request` failed at `now`. Another request's error leaves a fetch in
    /// flight alone.
    pub fn failed(&mut self, request: Option<RequestId>, now: Instant) {
        if let Self::Asking {
            request: waiting, ..
        } = self
            && request != Some(*waiting)
        {
            return;
        }
        let previous = match self {
            Self::Backoff { until, wait, .. } => Some((*until, *wait)),
            Self::Idle | Self::Asking { .. } => None,
        };
        let (until, wait) = next_retry(previous, now);
        *self = Self::Backoff {
            failures: self.failures().saturating_add(1),
            until,
            wait,
        };
    }
}

/// The story asks in flight, by what they ask for.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StoryAsks {
    chats: HashMap<i64, RequestId>,
    stories: HashMap<(i64, i32), RequestId>,
}

impl StoryAsks {
    /// The active-stories ask for `chat_id` is still out.
    pub fn chat_out(&self, chat_id: i64, requests: &crate::state::RequestRegistry) -> bool {
        self.chats
            .get(&chat_id)
            .is_some_and(|id| requests.get(*id).is_some())
    }

    /// The `getStory` for `story` is still out.
    pub fn story_out(&self, story: (i64, i32), requests: &crate::state::RequestRegistry) -> bool {
        self.stories
            .get(&story)
            .is_some_and(|id| requests.get(*id).is_some())
    }

    /// The active-stories ask for `chat_id` went out as `request`.
    pub fn chat_sent(
        &mut self,
        chat_id: i64,
        request: RequestId,
        requests: &crate::state::RequestRegistry,
    ) {
        self.chats.retain(|_, id| requests.get(*id).is_some());
        self.chats.insert(chat_id, request);
    }

    /// The `getStory` for `story` went out as `request`.
    pub fn story_sent(
        &mut self,
        story: (i64, i32),
        request: RequestId,
        requests: &crate::state::RequestRegistry,
    ) {
        self.stories.retain(|_, id| requests.get(*id).is_some());
        self.stories.insert(story, request);
    }
}

pub struct StoriesState {
    /// Phase 9.1: active stories per chat from `updateChatActiveStories` /
    /// `getChatActiveStories` (TDLib 1.8.67, `schema/td_api.tl:6776-6783`),
    /// keyed by chat id. Entries whose `list` is not `Main` (archived or
    /// not shown in any story list) are dropped on insert.
    pub tray: HashMap<i64, ChatActiveStoriesView>,
    /// Phase 9.1: full story objects from `getStory` (and `updateStory`
    /// updates), keyed by `(poster_chat_id, story_id)`. The viewer
    /// prefetches every story in a tray entry before opening.
    pub stories: HashMap<(i64, i32), ParsedStory>,
    /// Phase 9.2+: custom-emoji reactions the story picker can offer —
    /// `getStoryAvailableReactions` response (`availableReactions`,
    /// `schema/td_api.tl:13802`). Emoji, custom-emoji, and paid rows.
    pub available_reactions: Fetch<Vec<StoryAvailableReactionView>>,
    /// Phase 9.2+: sticker visuals for the picker's custom-emoji
    /// reactions — the `getCustomEmojiStickers` response, keyed by
    /// sticker id (= custom emoji id).
    pub custom_emoji_stickers: HashMap<i64, StickerItem>,
    /// `getCustomEmojiStickers` for the story picker after failures.
    pub custom_emoji_fetch: CustomEmojiFetch,
    /// Phase 9.2: poster chat ids whose active stories the driver should
    /// refresh with `getChatActiveStories`. Filled by the reducer on
    /// `updateStoryPostSucceeded` (a story posted from another client goes
    /// live — e.g. our own) and drained by the UI each render, like
    /// `pending_story_open`.
    pub tray_refresh: HashSet<i64>,
    /// Phase 9.3: story-posting round-trip state — `canPostStory`
    /// eligibility plus the `postStory` pending/succeeded/failed outcome
    /// the composer renders.
    pub post: StoryPostState,
    /// Phase 9.5: paginated viewers list for the story currently open in
    /// the viewer (`getStoryInteractions` pages, `Session::story_viewers`
    /// accumulates them). `None` when the panel is closed or the viewer
    /// moved to a different story.
    pub viewers: Option<StoryViewersState>,
    /// Statistics and public forwards of the story open in the viewer.
    pub insights: Option<StoryInsightsState>,
    /// The public story search (hashtag, location or venue) and its pages.
    pub search: Option<StorySearchState>,
    /// Phase 9.5: the in-progress `reportStory` flow for the story open in
    /// the viewer — the reason picker and the optional details step.
    /// `None` when no report is in flight.
    pub report: Option<StoryReportFlow>,
    /// Phase 9.5: story stealth-mode state from `updateStoryStealthMode`
    /// (TDLib 1.8.67, `schema/td_api.tl:10919`); 0/0 = disabled, no
    /// cooldown — the schema exposes no getter, so this only ever
    /// reflects updates TDLib has pushed.
    pub stealth: StoryStealthMode,
    /// The latest `activateStoryStealthMode` the server refused, with the
    /// request it answers: the Stealth Mode box that asked shows why.
    pub stealth_refusal: Option<(RequestId, String)>,
    pub stealth_periods: crate::story_stealth::StealthPeriods,
    /// Phase 9.5: chat ids from the last `getChatsToPostStories` answer —
    /// the composer's "post as" picker (channels/supergroups where the
    /// user has the `can_post_stories` admin right) — and why it failed.
    /// Its own value: the composer's eligibility check
    /// (`StoryPostState::check`) is another question.
    pub post_as: Fetch<Vec<i64>>,
    /// Phase 9.5: posted-story management round-trip state (edit /
    /// cover / privacy) rendered as one status line.
    pub manage: StoryManageState,
    /// Phase 9.7: `getChatStoryAlbums` results per chat (`storyAlbum` rows;
    /// covers dropped in the parser — name-only list).
    pub albums: HashMap<i64, Vec<ParsedStoryAlbum>>,
    /// Phase 9.7: `(chat_id, album_id)` → story ids of an opened album
    /// (`getStoryAlbumStories` pages accumulate; stories live in
    /// `Session::stories`).
    pub album_stories: HashMap<(i64, i32), Vec<i32>>,
    /// Phase 9.7: `getChatPostedToChatPageStories` results per chat —
    /// story ids, `pinned_story_ids` (first page only), and the server
    /// total for the "Load more" gate.
    pub chat_page_stories: HashMap<i64, ChatPageStories>,
    /// Phase 9.7: `getChatArchivedStories` pages per chat (accumulated;
    /// `next_from_story_id` is the smallest loaded id, `None` until the
    /// first page lands).
    pub archived_stories: HashMap<i64, ArchivedStories>,
    /// Phase 9.7: honest status of the latest album/pin mutation on the
    /// story page (`Sending` at send time, `Succeeded` / `Failed` when
    /// the TDLib answer lands). The story page renders it as its status
    /// line.
    pub page_ops: StoryPageOps,
    /// B14: the close-friends list (`getCloseFriends` / `setCloseFriends`);
    /// `None` until loaded.
    pub close_friends: CloseFriendsList,
    /// Paging of the main story list.
    pub main_paging: ActiveStoriesPaging,
    /// Paging of the hidden story list.
    pub hidden_paging: ActiveStoriesPaging,
    /// Where each story's `getStory` stands: asked for a reply strip or
    /// card, gone (a forwarded-story card then says "This story has
    /// expired", tdesktop `NoStory::Deleted`), or waiting out a failure
    /// that may pass. A user opening the story drops its wait and asks at
    /// once.
    pub story_fetch: StoryFetches,
    /// The `getChatActiveStories` asked for each chat and the `getStory`
    /// asked for each story: an ask is out while its own request is still
    /// registered (an answer, an error or a sweep removes it), so a second
    /// ask for the same chat or story waits instead of going out twice.
    pub asks: StoryAsks,
    /// The live story the viewer is watching.
    pub live: Option<LiveStoryState>,
    /// Paid live story comment levels from `updateGroupCallMessageLevels`,
    /// lowest first; empty until TDLib sends them.
    pub live_levels: Vec<crate::telegram::envelope::GroupCallMessageLevel>,
    /// `paid_group_call_message_star_count_max`, once known.
    pub live_max_stars: Option<i64>,
    /// The bursts the live story Stars effect plays.
    pub live_bursts: LiveBursts,
}

impl StoriesState {
    /// The account is going away: the story-page calls and management
    /// calls on their way will never be answered.
    pub fn settle_account(&mut self) {
        self.page_ops.clear();
        self.manage.forget_in_flight();
    }

    pub(crate) fn new() -> Self {
        Self {
            tray: HashMap::new(),
            stories: HashMap::new(),
            available_reactions: Fetch::Idle,
            custom_emoji_stickers: HashMap::new(),
            tray_refresh: HashSet::new(),
            post: StoryPostState::default(),
            viewers: None,
            insights: None,
            search: None,
            report: None,
            stealth: StoryStealthMode::default(),
            stealth_refusal: None,
            stealth_periods: Default::default(),
            post_as: Fetch::Idle,
            manage: StoryManageState::default(),
            albums: HashMap::new(),
            album_stories: HashMap::new(),
            chat_page_stories: HashMap::new(),
            archived_stories: HashMap::new(),
            page_ops: StoryPageOps::default(),
            close_friends: CloseFriendsList::NotLoaded,
            main_paging: ActiveStoriesPaging::default(),
            hidden_paging: ActiveStoriesPaging::default(),
            custom_emoji_fetch: CustomEmojiFetch::Idle,
            story_fetch: StoryFetches::default(),
            asks: StoryAsks::default(),
            live: None,
            live_levels: Vec::new(),
            live_max_stars: None,
            live_bursts: LiveBursts::Unasked,
        }
    }
}

impl StoriesState {
    pub fn paging(&self, list: StoryListView) -> &ActiveStoriesPaging {
        match list {
            StoryListView::Main => &self.main_paging,
            StoryListView::Archive => &self.hidden_paging,
        }
    }

    pub fn paging_mut(&mut self, list: StoryListView) -> &mut ActiveStoriesPaging {
        match list {
            StoryListView::Main => &mut self.main_paging,
            StoryListView::Archive => &mut self.hidden_paging,
        }
    }

    /// `getStory` for `story` failed with an error that may pass. A card or
    /// reply header showing it asks again once the wait is over.
    pub fn story_fetch_failed(&mut self, story: (i64, i32), now: Instant) {
        self.story_fetch.failed(story, now);
    }

    /// `story` arrived (an answer or `updateStory`): whatever said it was
    /// gone or waiting no longer applies.
    pub fn story_arrived(&mut self, story: (i64, i32)) {
        self.story_fetch.arrived(story);
    }

    /// The poster chat became known. TDLib refuses `getStory` with a 400
    /// while it does not know the chat, which looks the same as a story that
    /// is gone, so the stories of this poster marked missing are asked for
    /// again.
    pub fn poster_chat_known(&mut self, chat_id: i64) {
        self.story_fetch.poster_known(chat_id);
    }

    /// The story picker's `getCustomEmojiStickers` failed at `now`.
    pub fn custom_emoji_fetch_failed(&mut self, request: Option<RequestId>, now: Instant) {
        self.custom_emoji_fetch.failed(request, now);
    }

    /// Whether the next picker fetch still waits out a failed one.
    pub fn custom_emoji_fetch_waiting(&self, now: Instant) -> bool {
        self.custom_emoji_fetch.waiting(now)
    }

    /// Whether a prefetch of `story` still waits out a failed `getStory`.
    pub fn story_fetch_waiting(&self, story: (i64, i32), now: Instant) -> bool {
        self.story_fetch.waiting(story, now)
    }

    /// A new Ready pages both lists from the start, and the stories that
    /// failed before it are asked for again at once.
    pub fn reset_active_stories_paging(&mut self) {
        self.main_paging = ActiveStoriesPaging::default();
        self.hidden_paging = ActiveStoriesPaging::default();
        self.story_fetch.forget_outcomes();
    }
}
