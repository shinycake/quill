//! Stories, the story tray, albums, archive, posting, viewers and close friends: the `stories` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::state::*;

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
    /// Phase 9.5: last `activateStoryStealthMode` error (e.g. Premium
    /// required), cleared when a new activation is sent.
    pub stealth_error: Option<String>,
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
    /// Stories replied to that `getStory` was already asked for, so a
    /// deleted one is not requested again on every refresh.
    pub reply_attempted: HashSet<(i64, i32)>,
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
            stealth_error: None,
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
            reply_attempted: HashSet::new(),
        }
    }
}
