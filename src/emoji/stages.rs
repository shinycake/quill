//! The emoji panel's asks that carry their own stage: the trending sets of
//! the Emoji Sets box (paged), the `getKeywordEmojis` search and the
//! composer's animated emoji suggestion. Each is one value holding the
//! request that asked, so only its own answer lands.
use crate::ids::RequestId;
use crate::telegram::envelope::{StickerItem, StickerSetInfo};

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
    /// list); `request` is set once it went out.
    Loading {
        from: i32,
        request: Option<RequestId>,
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

    /// A page is about to be asked from `from`; what is shown stays.
    pub fn begin(&mut self, from: i32) {
        let shown = std::mem::take(self).into_shown();
        *self = Self::Loading {
            from,
            request: None,
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

    /// The page went out as `asked`.
    pub fn sent(&mut self, asked: RequestId) {
        if let Self::Loading { request, .. } = self {
            *request = Some(asked);
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
        matches!(self, Self::Loading { request: Some(waiting), .. } if Some(*waiting) == asked)
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

/// The panel's `getKeywordEmojis` search: one list for the session, with
/// the query it was asked for. Two windows' panels share it, so each reads
/// it through [`KeywordEmojis::for_query`]: a list asked for another
/// window's query is none of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeywordEmojis {
    /// No search, or an empty query was the last.
    Idle {
        query: String,
    },
    /// The search for `query` is out as `request`.
    Asking {
        query: String,
        request: RequestId,
    },
    Ready {
        query: String,
        emojis: Vec<String>,
    },
    /// The search never left or was refused: the panel shows the
    /// catalog's matches only, and does not ask this query again.
    Failed {
        query: String,
    },
}

impl Default for KeywordEmojis {
    fn default() -> Self {
        Self::Idle {
            query: String::new(),
        }
    }
}

impl KeywordEmojis {
    /// The trimmed query the list is for (the last one asked).
    pub fn query(&self) -> &str {
        match self {
            Self::Idle { query }
            | Self::Asking { query, .. }
            | Self::Ready { query, .. }
            | Self::Failed { query } => query,
        }
    }

    /// The matches for `query`; none when the list is another query's.
    pub fn for_query(&self, query: &str) -> &[String] {
        match self {
            Self::Ready {
                query: asked,
                emojis,
            } if asked == query.trim() => emojis,
            _ => &[],
        }
    }

    /// `request` is the search this list waits for.
    pub fn waits_for(&self, asked: Option<RequestId>) -> bool {
        matches!(self, Self::Asking { request, .. } if Some(*request) == asked)
    }

    /// The answer of `asked`, taken only by the search that waits for it.
    pub fn answered(&mut self, asked: Option<RequestId>, emojis: Vec<String>) {
        if let Self::Asking { query, request } = self
            && Some(*request) == asked
        {
            let query = std::mem::take(query);
            *self = Self::Ready { query, emojis };
        }
    }

    /// `asked` was refused: only the search that waits for it fails.
    pub fn failed_request(&mut self, asked: Option<RequestId>) {
        if let Self::Asking { query, request } = self
            && Some(*request) == asked
        {
            let query = std::mem::take(query);
            *self = Self::Failed { query };
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
}

impl AnimatedSuggestion {
    /// The emoji asked for or answered (the dedupe key).
    pub fn emoji_for(&self) -> Option<&str> {
        match self {
            Self::Idle => None,
            Self::Asking { emoji, .. } | Self::Ready { emoji, .. } => Some(emoji),
        }
    }

    /// The animated sticker to suggest.
    pub fn sticker(&self) -> Option<&StickerItem> {
        match self {
            Self::Ready { sticker, .. } => sticker.as_ref(),
            _ => None,
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
}
