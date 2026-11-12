//! The emoji panel's asks that carry their own stage: the trending sets of
//! the Emoji Sets box (paged), the `getKeywordEmojis` search and the
//! composer's animated emoji suggestion. Each is one value holding the
//! request that asked, so only its own answer lands.
use crate::ids::RequestId;
use crate::telegram::envelope::{StickerItem, StickerSetInfo};
use std::collections::VecDeque;

/// The trending emoji sets the box shows.
#[derive(Debug, Clone, Default)]
pub struct TrendingPage {
    pub sets: Vec<StickerSetInfo>,
    pub is_premium: bool,
    /// Where the next page starts (the raw count of what was asked so far).
    pub next_offset: i32,
    /// More pages follow the last one asked.
    pub has_more: bool,
}

/// The Trending tab: not asked, a page in flight, answered, or failed.
/// `shown` is the list on screen while a page is out or after it failed.
#[derive(Debug, Clone, Default)]
pub enum TrendingSets {
    #[default]
    Idle,
    /// A page is out from `from` (0 = the first page, which replaces the
    /// list) as `request`.
    Loading {
        from: i32,
        request: RequestId,
        shown: Option<TrendingPage>,
    },
    Ready(TrendingPage),
    /// TDLib said the trending sets changed: the list stays on screen and
    /// is asked again by whoever shows it.
    Stale(TrendingPage),
    Failed {
        error: String,
        shown: Option<TrendingPage>,
    },
}

impl TrendingSets {
    fn page(&self) -> Option<&TrendingPage> {
        match self {
            Self::Idle => None,
            Self::Ready(page) | Self::Stale(page) => Some(page),
            Self::Loading { shown, .. } | Self::Failed { shown, .. } => shown.as_ref(),
        }
    }

    /// A list that is already answered, for the demos and tests.
    pub fn ready(sets: Vec<StickerSetInfo>) -> Self {
        let next_offset = i32::try_from(sets.len()).unwrap_or(i32::MAX);
        Self::Ready(TrendingPage {
            sets,
            is_premium: false,
            next_offset,
            has_more: false,
        })
    }

    /// An ask is due: never asked, or the list went stale.
    pub fn wants_fetch(&self) -> bool {
        matches!(self, Self::Idle | Self::Stale(_))
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Failed { error, .. } => Some(error),
            _ => None,
        }
    }

    /// The sets shown.
    pub fn sets(&self) -> &[StickerSetInfo] {
        self.page().map_or(&[], |page| page.sets.as_slice())
    }

    pub fn sets_mut(&mut self) -> &mut [StickerSetInfo] {
        match self {
            Self::Idle => &mut [],
            Self::Ready(page) | Self::Stale(page) => &mut page.sets,
            Self::Loading { shown, .. } | Self::Failed { shown, .. } => {
                shown.as_mut().map_or(&mut [], |page| &mut page.sets)
            }
        }
    }

    pub fn is_premium(&self) -> bool {
        self.page().is_some_and(|page| page.is_premium)
    }

    pub fn has_more(&self) -> bool {
        self.page().is_some_and(|page| page.has_more)
    }

    /// Where the next page starts.
    pub fn next_offset(&self) -> i32 {
        self.page().map_or(0, |page| page.next_offset)
    }

    /// The page from `from` went out as `request`; what is shown stays.
    pub fn begin(&mut self, from: i32, request: RequestId) {
        let shown = std::mem::take(self).into_shown();
        *self = Self::Loading {
            from,
            request,
            shown,
        };
    }

    fn into_shown(self) -> Option<TrendingPage> {
        match self {
            Self::Idle => None,
            Self::Ready(page) | Self::Stale(page) => Some(page),
            Self::Loading { shown, .. } | Self::Failed { shown, .. } => shown,
        }
    }

    /// The request never left, or was refused: the list says so.
    pub fn fail(&mut self, error: impl Into<String>) {
        let shown = std::mem::take(self).into_shown();
        *self = Self::Failed {
            error: error.into(),
            shown,
        };
    }

    /// The ask is given up (a newer one replaces it, or the account
    /// restarts): back to what was shown.
    pub fn cancel(&mut self) {
        if self.is_loading() {
            *self = match std::mem::take(self).into_shown() {
                Some(page) => Self::Ready(page),
                None => Self::Idle,
            };
        }
    }

    /// The list changed on the server: what is shown stays, and whoever
    /// shows it asks again. An ask in flight predates the change, so it is
    /// no longer waited for.
    pub fn outdate(&mut self) {
        *self = match std::mem::take(self).into_shown() {
            Some(page) => Self::Stale(page),
            None => Self::Idle,
        };
    }

    /// `request` is the page this list waits for.
    pub fn waits_for(&self, asked: Option<RequestId>) -> bool {
        matches!(self, Self::Loading { request: waiting, .. } if Some(*waiting) == asked)
    }

    /// `request` failed: only the page the list waits for fails it.
    pub fn failed_request(&mut self, asked: Option<RequestId>, error: impl Into<String>) {
        if self.waits_for(asked) {
            self.fail(error);
        }
    }

    /// The answer of `asked`: a first page replaces the list, a later page
    /// joins it (sets already listed are not repeated). `total` is the
    /// server's count. `false` when no ask waits for it.
    pub fn answered(
        &mut self,
        asked: Option<RequestId>,
        sets: Vec<StickerSetInfo>,
        total: i32,
        is_premium: bool,
    ) -> bool {
        if !self.waits_for(asked) {
            return false;
        }
        let Self::Loading { from, shown, .. } = std::mem::take(self) else {
            return false;
        };
        let raw_count = i32::try_from(sets.len()).unwrap_or(i32::MAX);
        let mut listed = if from == 0 {
            Vec::new()
        } else {
            shown.map(|page| page.sets).unwrap_or_default()
        };
        for set in sets {
            if !listed.iter().any(|old| old.id == set.id) {
                listed.push(set);
            }
        }
        let next_offset = from.saturating_add(raw_count);
        *self = Self::Ready(TrendingPage {
            sets: listed,
            is_premium,
            next_offset,
            has_more: raw_count > 0 && next_offset < total,
        });
        true
    }
}

/// Queries kept: every open window's panel asks for its own.
const KEYWORD_KEPT: usize = 8;

/// Where one query's `getKeywordEmojis` stands.
#[derive(Debug, Clone, PartialEq, Eq)]
enum KeywordStage {
    /// The search is out as `request`; only its answer or error leaves.
    Asking {
        request: RequestId,
    },
    Ready {
        emojis: Vec<String>,
    },
    /// The search never left or was refused: the panel shows the catalog's
    /// matches only, and does not ask this query again.
    Failed,
}

/// The panels' `getKeywordEmojis` searches, one stage per query. Two windows'
/// panels search different queries at once, so each reads its own through
/// [`KeywordEmojis::for_query`], and a search for one query never replaces
/// another's.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeywordEmojis {
    /// Oldest first; at most [`KEYWORD_KEPT`].
    entries: VecDeque<(String, KeywordStage)>,
}

impl KeywordEmojis {
    fn stage(&self, query: &str) -> Option<&KeywordStage> {
        let query = query.trim();
        self.entries
            .iter()
            .find_map(|(asked, stage)| (asked == query).then_some(stage))
    }

    /// Nothing is left to ask for `query`: it is empty, or its search is
    /// out, answered or refused.
    pub fn knows(&self, query: &str) -> bool {
        query.trim().is_empty() || self.stage(query).is_some()
    }

    /// The search for `query` is out.
    pub fn asking(&self, query: &str) -> bool {
        matches!(self.stage(query), Some(KeywordStage::Asking { .. }))
    }

    /// The matches for `query`; none when it has no answer.
    pub fn for_query(&self, query: &str) -> &[String] {
        match self.stage(query) {
            Some(KeywordStage::Ready { emojis }) => emojis,
            _ => &[],
        }
    }

    /// The query was cleared: the searches that failed are forgotten, so
    /// typing the same words again asks again.
    pub fn cleared(&mut self) {
        self.forgive_failures();
    }

    /// The connection is back, or the field was cleared: refused searches
    /// are asked again.
    pub fn forgive_failures(&mut self) {
        self.entries
            .retain(|(_, stage)| !matches!(stage, KeywordStage::Failed));
    }

    fn put(&mut self, query: &str, stage: KeywordStage) {
        let query = query.trim().to_string();
        self.entries.retain(|(asked, _)| *asked != query);
        if self.entries.len() >= KEYWORD_KEPT {
            self.entries.pop_front();
        }
        self.entries.push_back((query, stage));
    }

    /// The search for `query` went out as `request`.
    pub fn asked(&mut self, query: &str, request: RequestId) {
        self.put(query, KeywordStage::Asking { request });
    }

    /// The search for `query` could not be sent.
    pub fn unsent(&mut self, query: &str) {
        self.put(query, KeywordStage::Failed);
    }

    /// `request` is a search this list waits for.
    pub fn waits_for(&self, asked: Option<RequestId>) -> bool {
        asked.is_some_and(|asked| {
            self.entries.iter().any(
                |(_, stage)| matches!(stage, KeywordStage::Asking { request } if *request == asked),
            )
        })
    }

    fn asking_entry(&mut self, asked: Option<RequestId>) -> Option<&mut KeywordStage> {
        let asked = asked?;
        self.entries.iter_mut().find_map(|(_, stage)| {
            matches!(stage, KeywordStage::Asking { request } if *request == asked).then_some(stage)
        })
    }

    /// The answer of `asked`, taken only by the search that waits for it.
    pub fn answered(&mut self, asked: Option<RequestId>, emojis: Vec<String>) {
        if let Some(stage) = self.asking_entry(asked) {
            *stage = KeywordStage::Ready { emojis };
        }
    }

    /// `asked` was refused: only the search that waits for it fails.
    pub fn failed_request(&mut self, asked: Option<RequestId>) {
        if let Some(stage) = self.asking_entry(asked) {
            *stage = KeywordStage::Failed;
        }
    }
}

/// The composer's animated emoji suggestion (`getAnimatedEmoji` for the
/// emoji the text ends with).
#[derive(Debug, Clone, Default)]
pub enum AnimatedSuggestion {
    #[default]
    Idle,
    /// Asked for `emoji` as `request`.
    Asking { emoji: String, request: RequestId },
    /// Answered for `emoji`; `sticker` is `None` when it has no animation.
    Ready {
        emoji: String,
        sticker: Option<StickerItem>,
    },
    /// The ask for `emoji` failed. Nothing is suggested, and the same emoji
    /// is not asked for again on every keystroke; another emoji, or a text
    /// that ends in none, leaves it.
    Failed { emoji: String },
}

impl AnimatedSuggestion {
    /// The emoji asked for or answered (the dedupe key).
    pub fn emoji_for(&self) -> Option<&str> {
        match self {
            Self::Idle => None,
            Self::Asking { emoji, .. } | Self::Ready { emoji, .. } | Self::Failed { emoji } => {
                Some(emoji)
            }
        }
    }

    /// The animated sticker to suggest.
    pub fn sticker(&self) -> Option<&StickerItem> {
        match self {
            Self::Ready { sticker, .. } => sticker.as_ref(),
            Self::Idle | Self::Asking { .. } | Self::Failed { .. } => None,
        }
    }

    pub fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }

    /// `request` is the ask this suggestion waits for.
    pub fn waits_for(&self, asked: Option<RequestId>) -> bool {
        matches!(self, Self::Asking { request, .. } if Some(*request) == asked)
    }

    /// The answer of `asked`, taken only by the ask that waits for it.
    pub fn answered(&mut self, asked: Option<RequestId>, sticker: Option<StickerItem>) -> bool {
        if let Self::Asking { emoji, request } = self
            && Some(*request) == asked
        {
            let emoji = std::mem::take(emoji);
            *self = Self::Ready { emoji, sticker };
            return true;
        }
        false
    }

    /// `asked` failed: only the ask that waits for it leaves `Asking`.
    pub fn failed_request(&mut self, asked: Option<RequestId>) -> bool {
        if let Self::Asking { emoji, request } = self
            && Some(*request) == asked
        {
            let emoji = std::mem::take(emoji);
            *self = Self::Failed { emoji };
            return true;
        }
        false
    }
}
