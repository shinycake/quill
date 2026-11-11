//! Sticker and GIF panel state types.
use super::*;
use crate::state::fetch::Fetch;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StickerTab {
    #[default]
    Installed,
    Recent,
    Favorites,
    Trending,
    Search,
    Archived,
}

/// What the session's found stickers were asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StickerSearch {
    /// A typed query (`searchStickers` by keyword); empty before any.
    Typed(String),
    /// A category of the sticker strip (`searchStickers` for its emoji,
    /// `getPremiumStickers`).
    Category(crate::telegram::EmojiCategorySource),
}

impl Default for StickerSearch {
    fn default() -> Self {
        Self::Typed(String::new())
    }
}

impl StickerSearch {
    /// The typed query; empty for a category.
    pub fn typed(&self) -> &str {
        match self {
            Self::Typed(query) => query,
            Self::Category(_) => "",
        }
    }
}

/// The selected set's stickers. A failed set is not asked again until
/// another set is selected or the panel opens again, or the per-update pass
/// would resend it at round-trip speed. An answer applies only to the
/// request that asked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StickerSetStage {
    #[default]
    Idle,
    Loading {
        set_id: i64,
        request: RequestId,
    },
    Ready {
        set_id: i64,
    },
    Failed {
        set_id: i64,
    },
}

impl StickerSetStage {
    /// The set this stage is about.
    pub fn set_id(&self) -> Option<i64> {
        match *self {
            Self::Idle => None,
            Self::Loading { set_id, .. } | Self::Ready { set_id } | Self::Failed { set_id } => {
                Some(set_id)
            }
        }
    }

    /// The set whose stickers are in the panel's `stickers`.
    pub fn loaded_id(&self) -> Option<i64> {
        match *self {
            Self::Ready { set_id } => Some(set_id),
            _ => None,
        }
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }
}

/// One fetch per list tab of the sticker panel.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StickerLists {
    pub installed: Fetch<()>,
    pub recent: Fetch<()>,
    pub favorites: Fetch<()>,
    pub trending: Fetch<()>,
    pub archived: Fetch<()>,
    /// The search tab asks two things at once: the matching sets
    /// (`searchStickerSets`) and the matching stickers (`searchStickers`).
    /// Each answer ends only its own half.
    pub search: Fetch<()>,
    pub search_stickers: Fetch<()>,
}

impl StickerLists {
    /// The fetches a tab shows: two for the search tab, one for the rest.
    fn of(&self, tab: StickerTab) -> [&Fetch<()>; 2] {
        match tab {
            StickerTab::Installed => [&self.installed; 2],
            StickerTab::Recent => [&self.recent; 2],
            StickerTab::Favorites => [&self.favorites; 2],
            StickerTab::Trending => [&self.trending; 2],
            StickerTab::Archived => [&self.archived; 2],
            StickerTab::Search => [&self.search, &self.search_stickers],
        }
    }

    /// Whether the tab is still loading: the search tab while either of
    /// its requests is out, so the first answer does not read as "No
    /// stickers found" before the other arrives.
    pub fn tab_loading(&self, tab: StickerTab) -> bool {
        self.of(tab).iter().any(|fetch| fetch.is_loading())
    }

    /// The tab's failure, if any of its fetches failed.
    pub fn tab_error(&self, tab: StickerTab) -> Option<&str> {
        self.of(tab).into_iter().find_map(|fetch| fetch.error())
    }

    /// A new search: both halves start over.
    pub fn reset_search(&mut self) {
        for fetch in [&mut self.search, &mut self.search_stickers] {
            fetch.cancel();
            fetch.clear_error();
        }
    }

    /// The list a request loads; `None` for the requests that change
    /// something (those report a note when they fail).
    pub fn for_purpose(&mut self, purpose: RequestPurpose) -> Option<&mut Fetch<()>> {
        match purpose {
            RequestPurpose::GetInstalledStickerSets => Some(&mut self.installed),
            RequestPurpose::GetRecentStickers => Some(&mut self.recent),
            RequestPurpose::GetFavoriteStickers => Some(&mut self.favorites),
            RequestPurpose::GetTrendingStickerSets => Some(&mut self.trending),
            RequestPurpose::GetArchivedStickerSets => Some(&mut self.archived),
            RequestPurpose::SearchStickerSets => Some(&mut self.search),
            RequestPurpose::SearchStickers => Some(&mut self.search_stickers),
            _ => None,
        }
    }
}

/// Composer sticker panel (Unigram `StickerDrawerViewModel` installed regular sets).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StickerPanel {
    pub open: bool,
    pub tab: StickerTab,
    pub sets: Vec<StickerSetInfo>,
    pub archived: Vec<StickerSetInfo>,
    pub archived_offset: i64,
    pub archived_next_offset: i64,
    pub archived_has_more: bool,
    pub selected_set_id: Option<i64>,
    pub stickers: Vec<StickerItem>,
    /// The selected set's `getStickerSet`: asked, answered or failed.
    pub set: StickerSetStage,
    /// Each list tab's own fetch: a failure of one list shows on that tab
    /// only, and a failed mutation is a note, not a list failure.
    pub lists: StickerLists,
    /// The latest batch of set changes (`manage_sticker_sets`).
    pub batch: StickerBatch,
    /// Slice S8: trending sets (`getTrendingStickerSets`) + premium-row flag.
    pub trending: Vec<StickerSetInfo>,
    pub trending_total: usize,
    pub trending_offset: usize,
    pub trending_next_offset: usize,
    pub trending_is_premium: bool,
    /// `getGreetingStickers`: hello stickers for empty private chats.
    /// Asked once per session from `Idle`; a failure stays `Failed`, so a
    /// redraw never asks again.
    pub greeting: Fetch<Vec<StickerItem>>,
    /// The "Attached Stickers" lookup of a photo or video
    /// (`getAttachedStickerSets`).
    pub attached: AttachedStickers,
    /// Slice S8: favorite stickers (`getFavoriteStickers`).
    pub favorites: Vec<StickerItem>,
    /// Slice S8: recent stickers (`getRecentStickers`).
    pub recent: Vec<StickerItem>,
    /// Slice S8: `searchStickerSets` / `searchStickers` results.
    pub found_sets: Vec<StickerSetInfo>,
    /// What `found_stickers` are (being) found for: a typed query or a
    /// category. One list per session; a window shows it only while it is
    /// for what that window asks.
    pub searched: StickerSearch,
    pub search_offset: usize,
    pub search_has_more: bool,
    pub found_stickers: Vec<StickerItem>,
    /// Slice S12: sticker suggestions for the composer's trailing emoji
    /// (`searchStickers` answers under `SuggestStickers`). `suggest_for`
    /// is the emoji they were requested for.
    pub suggestions: Vec<StickerItem>,
    pub suggest_for: SuggestFor,
}

/// Saved GIFs (`getSavedAnimations`). tdesktop Gifs tab / Unigram animation drawer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GifPanel {
    pub open: bool,
    pub animations: Vec<AnimationItem>,
    /// The saved list (`getSavedAnimations`): `Idle` until asked, `Loading`
    /// (with `Some` when a refresh runs over an answer), `Ready`, `Outdated`
    /// (`updateSavedAnimations` arrived: refreshed even if the panel is
    /// closed) or `Failed` (also after a failed save or remove, which
    /// leaves the list in place). The list itself is `animations`.
    pub saved: Fetch<()>,
    /// Which list the panel holds: the saved GIFs, or a search and how it
    /// stands. Leaving the search leaves its stage with it.
    pub list: GifList,
    pub search_bot_username: String,
    pub search_bot_user_id: Option<i64>,
    /// Slice S9: `updateAnimationSearchParameters` (schema 1.8.67, line
    /// 11064) — the upstream animation-search provider name and its
    /// suggested search emojis.
    pub search_provider: String,
    pub provider_emojis: Vec<String>,
    /// The searches answered lately, so a window's GIF tab shows its own
    /// search while another window's is the one in `search_results`.
    pub searches_kept: GifSearches,
}

/// GIF searches answered lately, by query: each window's GIF tab asks for
/// its own section (tdesktop has a `GifsListWidget` per window), but the
/// session pages one search at a time.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GifSearches(Vec<GifSearchKept>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GifSearchKept {
    pub query: String,
    pub results: Vec<AnimationItem>,
}

impl GifSearches {
    /// Enough for every open window's GIF tab.
    pub const KEPT: usize = 8;

    /// The GIFs found for `query`, when it was answered lately.
    pub fn get(&self, query: &str) -> Option<&[AnimationItem]> {
        self.0
            .iter()
            .find(|kept| kept.query == query)
            .map(|kept| kept.results.as_slice())
    }

    /// `query` was answered with `results` (every page so far).
    pub fn keep(&mut self, query: &str, results: &[AnimationItem]) {
        self.0.retain(|kept| kept.query != query);
        self.0.push(GifSearchKept {
            query: query.to_string(),
            results: results.to_vec(),
        });
        let over = self.0.len().saturating_sub(Self::KEPT);
        self.0.drain(..over);
    }
}

/// Which list a GIF panel holds. A search is a stage of its own: for one
/// query, asked (`Loading`), answered (`Ready`, the results are in
/// `search_results`), or refused (`Failed`). Its pages are matched to the
/// request that asked, so a late page of an earlier search changes nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum GifList {
    #[default]
    Saved,
    Search(GifSearch),
}

/// A GIF search (Slice S9: the animation search bot, `getInlineQueryResults`;
/// schema 1.8.67, lines 6483, 13019): its query, where it stands (the page
/// request it waits for is in `fetch`), and what it found. A new search
/// replaces all of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GifSearch {
    pub query: String,
    pub fetch: Fetch<()>,
    /// The chat of the window that asked (the inline query's chat); paging
    /// keeps it.
    pub chat: Option<crate::ids::ChatId>,
    /// First page replaces; later pages append, deduped by file id.
    pub results: Vec<AnimationItem>,
    /// `next_offset` of the last page ("" = exhausted).
    pub next_offset: String,
    /// The offset the page in flight (or the last answered) was asked from.
    pub offset: String,
}

const GIF_FAILED: &str = "gif request failed";

impl GifPanel {
    /// The saved list was answered, or failed after being asked (it counts
    /// as loaded so a failure does not ask again by itself).
    pub fn loaded(&self) -> bool {
        match &self.saved {
            Fetch::Idle => false,
            Fetch::Ready(()) => true,
            Fetch::Outdated(stale) | Fetch::Loading { stale, .. } | Fetch::Failed { stale, .. } => {
                stale.is_some()
            }
        }
    }

    pub fn loading(&self) -> bool {
        self.saved.is_loading()
    }

    pub fn failed(&self) -> bool {
        self.saved.error().is_some()
    }

    pub fn search_mode(&self) -> bool {
        matches!(self.list, GifList::Search(_))
    }

    pub fn search_loading(&self) -> bool {
        matches!(&self.list, GifList::Search(search) if search.fetch.is_loading())
    }

    pub fn search_failed(&self) -> bool {
        matches!(&self.list, GifList::Search(search) if search.fetch.error().is_some())
    }

    /// The query of the search the panel holds ("" when it shows the saved
    /// GIFs).
    pub fn search_query(&self) -> &str {
        match &self.list {
            GifList::Search(search) => &search.query,
            GifList::Saved => "",
        }
    }

    /// `getSavedAnimations` went out.
    pub fn saved_asked(&mut self) {
        self.saved.begin();
    }

    /// `getSavedAnimations` answered. An `updateSavedAnimations` that came
    /// while the request was out marks the list out of date: the answer may
    /// predate the change, so it is shown (the list does not blank) and the
    /// mark stays, so the list is asked for again.
    pub fn saved_answered(&mut self, animations: Vec<AnimationItem>) {
        self.animations = animations;
        if !self.saved.answer(()) {
            self.saved = Fetch::Outdated(Some(()));
        }
    }

    /// `updateSavedAnimations` arrived (another device or a save changed
    /// the list): it is asked for again, also with the panel closed.
    pub fn saved_outdated(&mut self) {
        self.saved.outdate();
    }

    /// The saved list should be asked for now: never asked, or out of date.
    pub fn saved_wanted(&self) -> bool {
        !self.loaded() || self.saved.is_outdated()
    }

    /// `getSavedAnimations` failed, or could not be sent: counted as loaded,
    /// so nothing asks again until the panel is opened. A list marked out
    /// of date stays marked.
    pub fn saved_failed(&mut self) {
        if self.saved.is_outdated() {
            return;
        }
        self.saved = Fetch::Failed {
            error: GIF_FAILED.to_string(),
            stale: Some(()),
        };
    }

    /// A save or remove failed, or a request could not be sent: the list
    /// stays as it was, and a fetch in flight goes on (its answer is the
    /// list's).
    pub fn saved_mutation_failed(&mut self) {
        if !self.saved.is_loading() {
            self.saved.fail(GIF_FAILED);
        }
    }

    /// A new request starts clean: a failure no longer shows.
    pub fn saved_failure_cleared(&mut self) {
        self.saved = match std::mem::take(&mut self.saved) {
            Fetch::Failed { stale, .. } => stale.map_or(Fetch::Idle, Fetch::Ready),
            other => other,
        };
    }

    /// The panel was opened after a failure: it asks again from scratch.
    pub fn saved_retry(&mut self) {
        if self.failed() {
            self.saved = Fetch::Idle;
        }
    }

    /// The list is out of date: the next ask fetches it again.
    pub fn saved_forgotten(&mut self) {
        self.saved = match std::mem::take(&mut self.saved) {
            Fetch::Idle | Fetch::Ready(()) | Fetch::Outdated(_) => Fetch::Idle,
            Fetch::Loading { request, .. } => Fetch::Loading {
                stale: None,
                request,
            },
            Fetch::Failed { error, .. } => Fetch::Failed { error, stale: None },
        };
    }

    /// A search for `query` starts, for the window of `chat`: the panel holds
    /// its (not yet answered) results, and nothing of an earlier search.
    pub fn search_begun(&mut self, query: String, chat: Option<crate::ids::ChatId>) {
        self.list = GifList::Search(GifSearch {
            query,
            fetch: Fetch::Loading {
                stale: None,
                request: None,
            },
            chat,
            results: Vec::new(),
            next_offset: String::new(),
            offset: String::new(),
        });
    }

    /// The panel goes back to the saved GIFs.
    pub fn search_left(&mut self) {
        self.list = GifList::Saved;
    }

    /// The search waits for a page again, asked from `offset` (a retry, or
    /// the first after the bot is known); the request that answers it is
    /// named by [`Self::search_sent`].
    pub fn search_asked(&mut self, offset: &str) {
        if let GifList::Search(search) = &mut self.list {
            search.fetch.begin();
            search.offset = offset.to_string();
        }
    }

    /// The search starts over (the search bot changed): what it found is
    /// gone and its first page is asked again.
    pub fn search_restarted(&mut self) {
        if let GifList::Search(search) = &mut self.list {
            search.results.clear();
            search.next_offset.clear();
        }
        self.search_asked("");
    }

    /// The page request `request` went out for the search.
    pub fn search_sent(&mut self, request: crate::ids::RequestId) {
        if let GifList::Search(search) = &mut self.list {
            search.fetch.sent(request);
        }
    }

    /// Whether `request` is the page the search waits for.
    pub fn search_waits_for(&self, request: Option<crate::ids::RequestId>) -> bool {
        matches!(&self.list, GifList::Search(search)
            if request.is_some_and(|request| search.fetch.waits_for(request)))
    }

    /// A search page answered. Only the page the search waits for lands
    /// (`false` for an earlier search's, or one that was left): the first
    /// page replaces the results, later pages append them.
    pub fn search_page_answered(
        &mut self,
        request: Option<crate::ids::RequestId>,
        animations: Vec<AnimationItem>,
        next_offset: String,
        first_page: bool,
    ) -> bool {
        let GifList::Search(search) = &mut self.list else {
            return false;
        };
        if !request.is_some_and(|request| search.fetch.waits_for(request)) {
            return false;
        }
        search.fetch.succeed(());
        if first_page {
            search.results = animations;
        } else {
            for item in animations {
                if !search.results.iter().any(|r| r.file_id == item.file_id) {
                    search.results.push(item);
                }
            }
        }
        search.next_offset = if !next_offset.is_empty() && next_offset == search.offset {
            String::new()
        } else {
            next_offset
        };
        true
    }

    /// The search failed, or could not be sent.
    pub fn search_failed_now(&mut self) {
        if let GifList::Search(search) = &mut self.list {
            search.fetch.fail(GIF_FAILED);
        }
    }

    /// A page request of the search failed: only the one the search waits
    /// for fails it.
    pub fn search_request_failed(&mut self, request: Option<crate::ids::RequestId>) {
        if self.search_waits_for(request) {
            self.search_failed_now();
        }
    }

    /// A new search request starts clean: a failure no longer shows.
    pub fn search_failure_cleared(&mut self) {
        if let GifList::Search(search) = &mut self.list
            && search.fetch.error().is_some()
        {
            search.fetch = Fetch::Idle;
        }
    }

    /// What the search found so far; empty while the panel shows the saved
    /// GIFs.
    pub fn search_results(&self) -> &[AnimationItem] {
        match &self.list {
            GifList::Search(search) => &search.results,
            GifList::Saved => &[],
        }
    }

    /// The offset of the next search page; empty when there is none.
    pub fn search_next_offset(&self) -> &str {
        match &self.list {
            GifList::Search(search) => &search.next_offset,
            GifList::Saved => "",
        }
    }

    /// The chat of the window that asked the search.
    pub fn search_chat(&self) -> Option<crate::ids::ChatId> {
        match &self.list {
            GifList::Search(search) => search.chat,
            GifList::Saved => None,
        }
    }

    pub fn visible_animations(&self) -> &[AnimationItem] {
        if self.search_mode() {
            self.search_results()
        } else {
            &self.animations
        }
    }

    pub fn close(&mut self) {
        self.open = false;
    }
}

/// The latest batch of sticker set changes (`manage_sticker_sets`): one
/// value from the first request to the last answer. Progress of a finished
/// batch stays readable until the next batch replaces it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum StickerBatch {
    #[default]
    Idle,
    /// Changes still out; `pending` are the sets they are for.
    Running {
        total: usize,
        completed: usize,
        failed: usize,
        pending: Vec<i64>,
    },
    /// Every change answered.
    Done {
        total: usize,
        completed: usize,
        failed: usize,
    },
}

impl StickerBatch {
    /// A batch for `set_ids` starts, replacing the last one.
    pub fn begin(&mut self, set_ids: Vec<i64>) {
        *self = Self::Running {
            total: set_ids.len(),
            completed: 0,
            failed: 0,
            pending: set_ids,
        };
    }

    /// The change of `set_id` answered (`success`) or failed. A set that is
    /// not pending (a stale answer, a change made outside a batch) changes
    /// nothing.
    pub fn finish(&mut self, set_id: i64, success: bool) {
        let Self::Running {
            total,
            completed,
            failed,
            pending,
        } = self
        else {
            return;
        };
        let Some(index) = pending.iter().position(|id| *id == set_id) else {
            return;
        };
        pending.remove(index);
        if success {
            *completed += 1;
        } else {
            *failed += 1;
        }
        if pending.is_empty() {
            *self = Self::Done {
                total: *total,
                completed: *completed,
                failed: *failed,
            };
        }
    }

    pub fn total(&self) -> usize {
        match self {
            Self::Idle => 0,
            Self::Running { total, .. } | Self::Done { total, .. } => *total,
        }
    }

    pub fn completed(&self) -> usize {
        match self {
            Self::Idle => 0,
            Self::Running { completed, .. } | Self::Done { completed, .. } => *completed,
        }
    }

    pub fn failed(&self) -> usize {
        match self {
            Self::Idle => 0,
            Self::Running { failed, .. } | Self::Done { failed, .. } => *failed,
        }
    }

    /// The sets whose changes are still out.
    pub fn pending(&self) -> &[i64] {
        match self {
            Self::Running { pending, .. } => pending,
            _ => &[],
        }
    }
}

/// The "Attached Stickers" lookup of a photo or video
/// (`getAttachedStickerSets`): asked as one request, then answered with the
/// first set (`None`: the media has none) until the box takes it. An answer
/// or error of an earlier request is dropped.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AttachedStickers {
    #[default]
    Idle,
    Asking(RequestId),
    Answered(Option<i64>),
}

impl AttachedStickers {
    /// The lookup goes out as `request`.
    pub fn asked(&mut self, request: RequestId) {
        *self = Self::Asking(request);
    }

    /// `request` answered with `first`: taken only while it is the lookup
    /// waited for.
    pub fn answered(&mut self, request: Option<RequestId>, first: Option<i64>) -> bool {
        if request.is_some_and(|request| *self == Self::Asking(request)) {
            *self = Self::Answered(first);
            return true;
        }
        false
    }

    /// `request` failed (or never went out): only the lookup waited for
    /// ends. `true` when it was.
    pub fn failed(&mut self, request: Option<RequestId>) -> bool {
        if request.is_some_and(|request| *self == Self::Asking(request)) {
            *self = Self::Idle;
            return true;
        }
        false
    }

    /// The answer, once.
    pub fn take_answer(&mut self) -> Option<Option<i64>> {
        match *self {
            Self::Answered(first) => {
                *self = Self::Idle;
                Some(first)
            }
            _ => None,
        }
    }
}

/// The emoji the composer sticker suggestions are for, and whether they
/// wait for the installed sets (`InstalledOnly` filters by them).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum SuggestFor {
    #[default]
    Nothing,
    /// `getInstalledStickerSets` is out; the emoji is asked when it lands.
    WaitingForSets(String),
    /// `searchStickers` for this emoji went out (or answered): only the
    /// answer of `request` lands, and its error returns the stage to
    /// `Nothing`, so the next keystroke asks again.
    Asked { emoji: String, request: RequestId },
}

impl SuggestFor {
    /// The emoji the suggestions are for.
    pub fn emoji(&self) -> Option<&str> {
        match self {
            Self::Nothing => None,
            Self::WaitingForSets(emoji) | Self::Asked { emoji, .. } => Some(emoji),
        }
    }

    /// Whether `request` is the `searchStickers` the suggestions wait for.
    pub fn waits_for(&self, request: Option<RequestId>) -> bool {
        matches!(self, Self::Asked { request: asked, .. } if Some(*asked) == request)
    }

    pub fn is_nothing(&self) -> bool {
        matches!(self, Self::Nothing)
    }

    pub fn waiting_for_sets(&self) -> bool {
        matches!(self, Self::WaitingForSets(_))
    }
}

impl StickerPanel {
    /// The installed sets were answered once (a refetch keeps them); an
    /// invalidated list is not.
    pub fn installed_loaded(&self) -> bool {
        self.lists.installed.data().is_some()
    }

    pub fn visible_stickers(&self) -> &[StickerItem] {
        match self.tab {
            StickerTab::Installed | StickerTab::Trending => &self.stickers,
            StickerTab::Archived if self.selected_set_id.is_some() => &self.stickers,
            StickerTab::Archived => &[],
            StickerTab::Recent => &self.recent,
            StickerTab::Favorites => &self.favorites,
            StickerTab::Search if self.selected_set_id.is_some() => &self.stickers,
            StickerTab::Search => &self.found_stickers,
        }
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn selected_needs_load(&self) -> Option<i64> {
        let id = self.selected_set_id?;
        if self.set.set_id() == Some(id) {
            None
        } else {
            Some(id)
        }
    }
}
