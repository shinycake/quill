//! Where the Trending tab of the Emoji Sets box stands: asking a page,
//! holding the next page's offset, at the end, or failed. One value (not a
//! fetch beside four loose offset fields), so a page that fails or is
//! dropped leaves the offset it asked at in the stage that names it.
use crate::ids::RequestId;

/// Sets asked per page (`getTrendingStickerSets` limit).
pub const TRENDING_PAGE: i32 = 100;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum TrendingPaging {
    /// Nothing asked, or the list was dropped for a refetch.
    #[default]
    Idle,
    /// A page starting at `offset` is out as `request`. Only its answer or
    /// error leaves this stage; the first page (`offset` 0) replaces the
    /// list, later ones append.
    Asking { request: RequestId, offset: i32 },
    /// Pages are in; the next one starts at `next`.
    More { next: i32 },
    /// Every page is in.
    End,
    /// The page starting at `retry_at` failed; asking it again is a retry
    /// (`retry_at` 0 reopens the tab).
    Failed { error: String, retry_at: i32 },
}

impl TrendingPaging {
    pub fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }

    /// The error line of a failed page.
    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Failed { error, .. } => Some(error),
            _ => None,
        }
    }

    /// Where a "load more" asks: the next page, or the one that failed
    /// after the first.
    pub fn next_page(&self) -> Option<i32> {
        match self {
            Self::More { next } => Some(*next),
            Self::Failed { retry_at, .. } if *retry_at > 0 => Some(*retry_at),
            Self::Idle | Self::Asking { .. } | Self::End | Self::Failed { .. } => None,
        }
    }

    /// A page starting at `offset` went out as `request`.
    pub fn asked(&mut self, request: RequestId, offset: i32) {
        *self = Self::Asking { request, offset };
    }

    /// The page starting at `offset` never left.
    pub fn send_failed(&mut self, offset: i32, error: impl Into<String>) {
        *self = Self::Failed {
            error: error.into(),
            retry_at: offset,
        };
    }

    /// `request` answered with `count` sets of `total`: the offset its page
    /// started at when it is the one asked for (`None`, no change, for any
    /// other request).
    pub fn answered(&mut self, request: Option<RequestId>, count: i32, total: i32) -> Option<i32> {
        let Self::Asking {
            request: asked,
            offset,
        } = *self
        else {
            return None;
        };
        if request != Some(asked) {
            return None;
        }
        let next = offset.saturating_add(count);
        *self = if count > 0 && next < total {
            Self::More { next }
        } else {
            Self::End
        };
        Some(offset)
    }

    /// `request` failed: its page can be asked again.
    pub fn failed(&mut self, request: Option<RequestId>, error: impl Into<String>) {
        if let Self::Asking {
            request: asked,
            offset,
        } = *self
            && request == Some(asked)
        {
            self.send_failed(offset, error);
        }
    }
}

impl super::EmojiPanel {
    /// The error line of the shown tab's list.
    pub fn tab_error(&self) -> Option<&str> {
        match self.tab {
            super::EmojiSetTab::Installed => self.lists.installed.error(),
            super::EmojiSetTab::Trending => self.trending.error(),
            super::EmojiSetTab::Search => self.lists.search.error(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TrendingPaging;
    use crate::ids::RequestId;

    #[test]
    fn a_page_is_taken_only_by_its_own_request_and_a_failure_keeps_its_offset() {
        let mut paging = TrendingPaging::default();
        paging.asked(RequestId(1), 0);
        assert_eq!(paging.answered(Some(RequestId(9)), 2, 5), None);
        assert_eq!(paging.answered(Some(RequestId(1)), 2, 5), Some(0));
        assert_eq!(paging.next_page(), Some(2));
        paging.asked(RequestId(2), 2);
        paging.failed(Some(RequestId(7)), "other");
        assert!(matches!(paging, TrendingPaging::Asking { .. }));
        paging.failed(Some(RequestId(2)), "refused");
        assert_eq!(paging.error(), Some("refused"));
        assert_eq!(paging.next_page(), Some(2), "retry asks the failed page");
        paging.asked(RequestId(3), 2);
        assert_eq!(paging.answered(Some(RequestId(3)), 3, 5), Some(2));
        assert_eq!(paging, TrendingPaging::End);
        assert_eq!(paging.next_page(), None);
    }

    #[test]
    fn a_failed_first_page_has_nothing_to_load_more_from() {
        let mut paging = TrendingPaging::default();
        paging.send_failed(0, "refused");
        assert_eq!(paging.next_page(), None);
        assert!(!paging.is_idle());
    }
}
