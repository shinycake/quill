//! Stories, the story tray, albums, archive, posting, viewers and close friends: the `stories` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::state::*;
use std::time::{Duration, Instant};

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
    pub available_reactions: Option<Vec<StoryAvailableReactionView>>,
    /// Phase 9.2+: sticker visuals for the picker's custom-emoji
    /// reactions — the `getCustomEmojiStickers` response, keyed by
    /// sticker id (= custom emoji id).
    pub custom_emoji_stickers: HashMap<i64, StickerItem>,
    /// `getCustomEmojiStickers` for the story picker failed: no new fetch
    /// before this instant, and the wait before it, which doubles per
    /// failure. An answer clears it.
    pub custom_emoji_retry: Option<(Instant, Duration)>,
    /// How many of those fetches failed. The story viewer's warm-up record
    /// carries it, so a failure makes the next tick fetch again.
    pub custom_emoji_failures: u32,
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
    pub stealth_periods: crate::story_stealth::StealthPeriods,
    /// Phase 9.5: chat ids from the last `getChatsToPostStories` answer —
    /// the composer's "post as" picker (channels/supergroups where the
    /// user has the `can_post_stories` admin right).
    pub post_as_chats: Vec<i64>,
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
    pub page_op: Option<StoryPageOp>,
    /// B14: the close-friends list (`getCloseFriends` / `setCloseFriends`);
    /// `None` until loaded.
    pub close_friends: Option<Vec<i64>>,
    /// B14: ids sent by an in-flight `setCloseFriends`, applied on `ok`.
    pub close_friends_pending: Option<Vec<i64>>,
    /// Phase 9.1: `loadActiveStories(storyListMain)` was issued. A retry is
    /// allowed (the flag is reset) if the attempt failed.
    pub stories_active_loaded: bool,
    /// `loadActiveStories` answered 404: the whole main tray is loaded and
    /// no more pages are asked for.
    pub stories_active_exhausted: bool,
    /// After any other `loadActiveStories` failure: no page goes out before
    /// this instant, and the wait before it, which doubles per failure.
    pub stories_active_retry: Option<(Instant, Duration)>,
    /// Stories `getStory` failed for with an error that may pass (a
    /// timeout, FLOOD_WAIT, a 500): no prefetch asks again before the
    /// instant, and the wait before it, which doubles per failure. A user
    /// opening the story drops its entry and asks at once.
    pub story_retry: HashMap<(i64, i32), (Instant, Duration)>,
    /// Stories replied to that `getStory` was sent for, so one is not
    /// requested again on every refresh. A failure that may pass takes the
    /// story out again, to be asked for after its wait.
    pub reply_attempted: HashSet<(i64, i32)>,
    /// Stories `getStory` answered with an error (deleted, expired or not
    /// reachable): a forwarded-story card then says "This story has
    /// expired" (tdesktop `NoStory::Deleted`).
    pub missing: HashSet<(i64, i32)>,
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
    pub(crate) fn new() -> Self {
        Self {
            tray: HashMap::new(),
            stories: HashMap::new(),
            available_reactions: None,
            custom_emoji_stickers: HashMap::new(),
            tray_refresh: HashSet::new(),
            post: StoryPostState::default(),
            viewers: None,
            insights: None,
            search: None,
            report: None,
            stealth: StoryStealthMode::default(),
            stealth_periods: Default::default(),
            post_as_chats: Vec::new(),
            manage: StoryManageState::default(),
            albums: HashMap::new(),
            album_stories: HashMap::new(),
            chat_page_stories: HashMap::new(),
            archived_stories: HashMap::new(),
            page_op: None,
            close_friends: None,
            close_friends_pending: None,
            stories_active_loaded: false,
            stories_active_exhausted: false,
            stories_active_retry: None,
            custom_emoji_retry: None,
            custom_emoji_failures: 0,
            story_retry: HashMap::new(),
            reply_attempted: HashSet::new(),
            missing: HashSet::new(),
            live: None,
            live_levels: Vec::new(),
            live_max_stars: None,
            live_bursts: LiveBursts::Unasked,
        }
    }
}

/// The first wait after a failed `loadActiveStories` or `getStory`; each
/// later failure doubles it, up to [`STORIES_RETRY_MAX`].
pub const STORIES_RETRY_FIRST: Duration = Duration::from_secs(5);
pub const STORIES_RETRY_MAX: Duration = Duration::from_secs(5 * 60);

/// The retry clock after one more failure at `now`.
fn next_retry(previous: Option<(Instant, Duration)>, now: Instant) -> (Instant, Duration) {
    let wait = previous.map_or(STORIES_RETRY_FIRST, |(_, wait)| {
        wait.saturating_mul(2).min(STORIES_RETRY_MAX)
    });
    (now.checked_add(wait).unwrap_or(now), wait)
}

impl StoriesState {
    /// Allow one more `loadActiveStories` page, which the driver otherwise
    /// sends once per Ready. `false` once the main tray is fully loaded, or
    /// while waiting out a failed page.
    pub fn rearm_active_stories_load(&mut self, now: Instant) -> bool {
        if self.stories_active_exhausted
            || self.stories_active_retry.is_some_and(|(at, _)| now < at)
        {
            return false;
        }
        self.stories_active_loaded = false;
        true
    }

    /// `loadActiveStories` failed with `code`: 404 means everything is
    /// loaded; anything else (a timeout, FLOOD_WAIT, a 500) waits and tries
    /// again.
    pub fn active_stories_failed(&mut self, code: i32, now: Instant) {
        if code == 404 {
            self.stories_active_exhausted = true;
            return;
        }
        self.stories_active_retry = Some(next_retry(self.stories_active_retry, now));
    }

    /// `getStory` for `story` failed with an error that may pass. A card or
    /// reply header showing it asks again once the wait is over.
    pub fn story_fetch_failed(&mut self, story: (i64, i32), now: Instant) {
        let previous = self.story_retry.get(&story).copied();
        self.story_retry.insert(story, next_retry(previous, now));
        self.reply_attempted.remove(&story);
    }

    /// The story picker's `getCustomEmojiStickers` failed at `now`.
    pub fn custom_emoji_fetch_failed(&mut self, now: Instant) {
        self.custom_emoji_retry = Some(next_retry(self.custom_emoji_retry, now));
        self.custom_emoji_failures = self.custom_emoji_failures.saturating_add(1);
    }

    /// Whether the next picker fetch still waits out a failed one.
    pub fn custom_emoji_fetch_waiting(&self, now: Instant) -> bool {
        self.custom_emoji_retry.is_some_and(|(at, _)| now < at)
    }

    /// Whether a prefetch of `story` still waits out a failed `getStory`.
    pub fn story_fetch_waiting(&self, story: (i64, i32), now: Instant) -> bool {
        self.story_retry
            .get(&story)
            .is_some_and(|(at, _)| now < *at)
    }

    /// A new Ready pages the tray from the start, and the stories that
    /// failed before it are asked for again at once.
    pub fn reset_active_stories_paging(&mut self) {
        self.stories_active_loaded = false;
        self.stories_active_exhausted = false;
        self.stories_active_retry = None;
        self.story_retry.clear();
    }
}
