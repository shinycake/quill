//! The paged sticker set lists of the sticker panel: the archived sets and
//! the trending sets, each one value with its paging (split out of
//! `sticker_gif_types.rs` for the file-size limit).
use super::*;

/// The archived sets and how far their paging got, one value: a restart
/// leaves both at once.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArchivedSets {
    pub sets: Vec<StickerSetInfo>,
    pub page: ArchivedPage,
}

/// Where the archived sets' paging stands; the next ask's offset is in the
/// variant.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ArchivedPage {
    /// The first page is not in (not asked, or being asked): its answer
    /// replaces the list.
    #[default]
    First,
    /// The last page was full: the next one starts after set `next`.
    More { next: i64 },
    /// The last page was short, or made no progress.
    End,
}

/// The trending sets, the premium-row flag and the paging of the list, one
/// value: the offset of the page asked rides in the stage, so a restart
/// leaves everything at once and only the page asked last lands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrendingSets {
    pub sets: Vec<StickerSetInfo>,
    pub is_premium: bool,
    pub page: TrendingPage,
}

/// Where the trending sets' paging stands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TrendingPage {
    /// The first page is not in: its answer replaces the list.
    #[default]
    First,
    /// The page from set `from` is out as `request`. Leaves with its
    /// answer or its error; a page asked later replaces it.
    Asking { request: RequestId, from: usize },
    /// More sets follow: the next page starts at `next`.
    More { next: usize },
    /// The last page reached the server's total.
    End,
}

impl TrendingSets {
    /// The next page's offset, when there is one.
    pub fn next_offset(&self) -> Option<usize> {
        match self.page {
            TrendingPage::More { next } => Some(next),
            _ => None,
        }
    }

    /// More sets exist than are loaded.
    pub fn has_more(&self) -> bool {
        matches!(self.page, TrendingPage::More { .. })
    }

    /// A page went out as `request`, from set `from` (0: the first page).
    pub fn ask(&mut self, request: RequestId, from: usize) {
        self.page = TrendingPage::Asking { request, from };
    }

    /// `request` answered: only the page asked last lands. The first page
    /// replaces the list, a later one adds the sets it does not have yet.
    /// Whether it landed.
    pub fn accept(
        &mut self,
        request: Option<RequestId>,
        page: Vec<StickerSetInfo>,
        is_premium: bool,
        total: usize,
    ) -> bool {
        let TrendingPage::Asking {
            request: asked,
            from,
        } = self.page
        else {
            return false;
        };
        if Some(asked) != request {
            return false;
        }
        let next = from + page.len();
        // An empty page ends the list whatever the server's total says.
        let total = if page.is_empty() { next } else { total };
        if from == 0 {
            self.sets = page;
        } else {
            let mut ids: std::collections::HashSet<_> =
                self.sets.iter().map(|set| set.id).collect();
            self.sets
                .extend(page.into_iter().filter(|set| ids.insert(set.id)));
        }
        self.is_premium = is_premium;
        self.page = if next < total {
            TrendingPage::More { next }
        } else {
            TrendingPage::End
        };
        true
    }

    /// `request` failed: the page can be asked again.
    pub fn failed(&mut self, request: Option<RequestId>) {
        if let TrendingPage::Asking {
            request: asked,
            from,
        } = self.page
            && Some(asked) == request
        {
            self.page = if from == 0 {
                TrendingPage::First
            } else {
                TrendingPage::More { next: from }
            };
        }
    }
}

impl ArchivedSets {
    /// The next page's offset, when there is one.
    pub fn next_offset(&self) -> Option<i64> {
        match self.page {
            ArchivedPage::More { next } => Some(next),
            ArchivedPage::First | ArchivedPage::End => None,
        }
    }

    /// A page answered: the first replaces the list, a later one adds the
    /// sets it does not have yet.
    pub fn accept(&mut self, page: Vec<StickerSetInfo>) {
        let asked = match self.page {
            ArchivedPage::First => {
                self.sets.clear();
                None
            }
            ArchivedPage::More { next } => Some(next),
            ArchivedPage::End => return,
        };
        let last = page.last().map_or(0, |set| set.id);
        self.page = if page.len() == 100 && last > 0 && Some(last) != asked {
            ArchivedPage::More { next: last }
        } else {
            ArchivedPage::End
        };
        let mut ids: std::collections::HashSet<_> = self.sets.iter().map(|set| set.id).collect();
        self.sets
            .extend(page.into_iter().filter(|set| ids.insert(set.id)));
    }
}
