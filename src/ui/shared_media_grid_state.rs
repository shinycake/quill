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

/// What identifies the list a tab shows: its chat, the date it was
/// reloaded from and the search it answers. Not its items: a deleted or a
/// new newest item is the same list, and keeps its place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PageKey {
    pub chat: i64,
    pub anchor_day: Option<i64>,
    /// The search query, hashed (empty for the plain list).
    pub query: u64,
    /// The tab's stamp: a gallery closed and opened again, or a fetch that
    /// starts the list over, is a new page even for the same chat.
    pub generation: u64,
}

impl PageKey {
    pub(crate) fn of(chat: Option<quill::ids::ChatId>, state: &SharedMediaTabState) -> Self {
        use std::hash::{Hash, Hasher};
        let mut query = std::collections::hash_map::DefaultHasher::new();
        state.query.hash(&mut query);
        Self {
            chat: chat.map_or(0, |chat| chat.0),
            anchor_day: state.anchor_day,
            query: query.finish(),
            generation: state.generation,
        }
    }
}

/// A grid tab's place: its scroll and the page it belongs to, one value.
struct GridTab {
    scroll: VirtualListScrollHandle,
    page: Cell<Option<PageKey>>,
}

/// A month-sectioned list tab's place (files, links, voice, music): its
/// scroll, its paging mark (`scroll_load`) and the page both belong to,
/// one value, so another chat's list starts at the top and asks for its
/// next page afresh.
struct ListTab {
    scroll: ScrollHandle,
    asked: Asked,
    page: Cell<Option<PageKey>>,
}

impl ListTab {
    fn for_page(&self, key: PageKey) -> (&ScrollHandle, &Asked) {
        if self.page.get() != Some(key) {
            self.scroll.set_offset(Point::new(px(0.), px(0.)));
            self.asked.reset();
            self.page.set(Some(key));
        }
        (&self.scroll, &self.asked)
    }
}

pub(crate) struct SharedMediaGridUi {
    grid: [GridTab; SharedMediaTab::COUNT],
    list: [ListTab; SharedMediaTab::COUNT],
}

impl crate::ui::on_screen::Farewell for SharedMediaGridUi {}

impl SharedMediaGridUi {
    pub(crate) fn new() -> Self {
        Self {
            grid: std::array::from_fn(|_| GridTab {
                scroll: VirtualListScrollHandle::new(),
                page: Cell::new(None),
            }),
            list: std::array::from_fn(|_| ListTab {
                scroll: ScrollHandle::new(),
                asked: Asked::default(),
                page: Cell::new(None),
            }),
        }
    }

    /// A list tab's scroll handle and paging mark for the page `key`, both
    /// back at the start when the tab last drew another page.
    pub(super) fn list_scroll(
        &self,
        tab: SharedMediaTab,
        key: PageKey,
    ) -> Option<(&ScrollHandle, &Asked)> {
        Some(self.list.get(tab.index())?.for_page(key))
    }

    /// The handle of `tab` for the page `key`, back at the top when the tab
    /// last drew another page.
    pub(super) fn scroll(
        &self,
        tab: SharedMediaTab,
        key: PageKey,
    ) -> Option<&VirtualListScrollHandle> {
        let tab = self.grid.get(tab.index())?;
        if tab.page.get() != Some(key) {
            tab.scroll.set_offset(Point::new(px(0.), px(0.)));
            tab.page.set(Some(key));
        }
        Some(&tab.scroll)
    }

    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn scroll_down(&self, tab: SharedMediaTab, by: f32) {
        if let Some(tab) = self.grid.get(tab.index()) {
            tab.scroll.set_offset(Point::new(px(0.), px(-by)));
        }
    }

    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn offset_y(&self, tab: SharedMediaTab) -> Option<gpui_kit::Pixels> {
        self.grid.get(tab.index()).map(|tab| tab.scroll.offset().y)
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
            query: 0,
            generation: 0,
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

    /// Closing the gallery and opening the same chat again restamps its
    /// tabs: the list starts at the top and asks for its paging afresh.
    #[test]
    fn a_reopened_gallery_of_the_same_chat_is_a_new_page() {
        use quill::state::SharedMediaTabState;
        let ui = SharedMediaGridUi::new();
        let tab = SharedMediaTab::Files;
        let mut state = SharedMediaTabState::default();
        let chat = Some(quill::ids::ChatId(1));
        let Some((scroll, asked)) = ui.list_scroll(tab, PageKey::of(chat, &state)) else {
            panic!("the Files tab has a list");
        };
        scroll.set_offset(Point::new(px(0.), px(-300.)));
        assert!(asked.allow(20, false));
        state.generation += 1;
        let Some((scroll, asked)) = ui.list_scroll(tab, PageKey::of(chat, &state)) else {
            panic!("the Files tab has a list");
        };
        assert_eq!(scroll.offset().y, px(0.));
        assert!(asked.allow(20, false), "the paging mark was reset");
    }

    /// Deleting the newest file is the same list: it keeps its place. A
    /// search is another list.
    #[test]
    fn the_same_list_keeps_its_place_when_its_newest_item_goes() {
        use quill::state::SharedMediaTabState;
        let ui = SharedMediaGridUi::new();
        let tab = SharedMediaTab::Files;
        let mut state = SharedMediaTabState::default();
        let chat = Some(quill::ids::ChatId(1));
        let Some((scroll, _)) = ui.list_scroll(tab, PageKey::of(chat, &state)) else {
            panic!("the Files tab has a list");
        };
        scroll.set_offset(Point::new(px(0.), px(-300.)));
        state.items.clear();
        state.total_count -= 1;
        let Some((scroll, _)) = ui.list_scroll(tab, PageKey::of(chat, &state)) else {
            panic!("the Files tab has a list");
        };
        assert_eq!(scroll.offset().y, px(-300.), "the place stays");
        state.query = "report".into();
        let Some((scroll, _)) = ui.list_scroll(tab, PageKey::of(chat, &state)) else {
            panic!("the Files tab has a list");
        };
        assert_eq!(scroll.offset().y, px(0.), "a search starts at the top");
    }
}
