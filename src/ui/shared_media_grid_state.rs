//! Scroll positions of the shared-media grid tabs, owned by the app so a
//! tab keeps its place across re-renders (the grid is a virtual list, whose
//! scroll handle would otherwise start over every frame).
//!
//! A position belongs to the page it was made on: when the tab shows a
//! different first page (another chat, a date jump, Show latest) the handle
//! goes back to the top. Load more only appends, so it keeps the position.
#![deny(clippy::indexing_slicing)]

use super::scroll_load::Asked;
use gpui_kit::component::*;
use gpui_kit::{Point, ScrollHandle, px};
use quill::state::{SharedMediaTab, SharedMediaTabState};
use std::cell::Cell;

/// What identifies the first page of a tab: its chat, the date it was
/// reloaded from and its newest message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PageKey {
    pub chat: i64,
    pub anchor_day: Option<i64>,
    pub newest: i64,
}

impl PageKey {
    pub(crate) fn of(chat: Option<quill::ids::ChatId>, state: &SharedMediaTabState) -> Self {
        Self {
            chat: chat.map_or(0, |chat| chat.0),
            anchor_day: state.anchor_day,
            newest: state.items.first().map_or(0, |item| item.message_id.0),
        }
    }
}

pub(crate) struct SharedMediaGridUi {
    scroll: [VirtualListScrollHandle; SharedMediaTab::COUNT],
    page: [Cell<Option<PageKey>>; SharedMediaTab::COUNT],
    /// The month-sectioned list tabs (files, links, voice, music), which
    /// page near their end (`scroll_load`).
    list_scroll: [ScrollHandle; SharedMediaTab::COUNT],
    list_asked: [Asked; SharedMediaTab::COUNT],
    /// The page each list tab's scroll and "asked" belong to, as `page`
    /// for the grid: another chat's list starts at the top and asks for
    /// its next page afresh.
    list_page: [Cell<Option<PageKey>>; SharedMediaTab::COUNT],
}

impl crate::ui::on_screen::Farewell for SharedMediaGridUi {}

impl SharedMediaGridUi {
    pub(crate) fn new() -> Self {
        Self {
            scroll: std::array::from_fn(|_| VirtualListScrollHandle::new()),
            page: std::array::from_fn(|_| Cell::new(None)),
            list_scroll: std::array::from_fn(|_| ScrollHandle::new()),
            list_asked: std::array::from_fn(|_| Asked::default()),
            list_page: std::array::from_fn(|_| Cell::new(None)),
        }
    }

    /// A list tab's scroll handle and paging mark for the page `key`, both
    /// back at the start when the tab last drew another page.
    pub(super) fn list_scroll(
        &self,
        tab: SharedMediaTab,
        key: PageKey,
    ) -> Option<(&ScrollHandle, &Asked)> {
        let scroll = self.list_scroll.get(tab.index())?;
        let asked = self.list_asked.get(tab.index())?;
        let page = self.list_page.get(tab.index())?;
        if page.get() != Some(key) {
            scroll.set_offset(Point::new(px(0.), px(0.)));
            asked.reset();
            page.set(Some(key));
        }
        Some((scroll, asked))
    }

    /// The handle of `tab` for the page `key`, back at the top when the tab
    /// last drew another page.
    pub(super) fn scroll(
        &self,
        tab: SharedMediaTab,
        key: PageKey,
    ) -> Option<&VirtualListScrollHandle> {
        let handle = self.scroll.get(tab.index())?;
        let page = self.page.get(tab.index())?;
        if page.get() != Some(key) {
            handle.set_offset(Point::new(px(0.), px(0.)));
            page.set(Some(key));
        }
        Some(handle)
    }

    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn scroll_down(&self, tab: SharedMediaTab, by: f32) {
        if let Some(handle) = self.scroll.get(tab.index()) {
            handle.set_offset(Point::new(px(0.), px(-by)));
        }
    }

    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn offset_y(&self, tab: SharedMediaTab) -> Option<gpui_kit::Pixels> {
        self.scroll.get(tab.index()).map(|handle| handle.offset().y)
    }
}

#[cfg(test)]
mod tests {
    use super::{PageKey, SharedMediaGridUi};
    use gpui_kit::{Point, px};
    use quill::state::SharedMediaTab;

    fn key(chat: i64) -> PageKey {
        PageKey {
            chat,
            anchor_day: None,
            newest: 100,
        }
    }

    /// Another chat's Files list starts at the top and asks for its next
    /// page at the same length the last chat's had asked at.
    #[test]
    fn a_list_tab_starts_over_on_another_chats_page() {
        let ui = SharedMediaGridUi::new();
        let tab = SharedMediaTab::Files;
        let Some((scroll, asked)) = ui.list_scroll(tab, key(1)) else {
            panic!("the Files tab has a list");
        };
        scroll.set_offset(Point::new(px(0.), px(-300.)));
        assert!(asked.allow(20, false), "the first ask at 20 items");
        assert!(!asked.allow(20, false), "asked already");
        let Some((scroll, asked)) = ui.list_scroll(tab, key(1)) else {
            panic!("the Files tab has a list");
        };
        assert_eq!(
            scroll.offset().y,
            px(-300.),
            "the same page keeps its place"
        );
        assert!(!asked.allow(20, false));
        let Some((scroll, asked)) = ui.list_scroll(tab, key(2)) else {
            panic!("the Files tab has a list");
        };
        assert_eq!(
            scroll.offset().y,
            px(0.),
            "another chat's list is at the top"
        );
        assert!(asked.allow(20, false), "and asks again");
    }
}
