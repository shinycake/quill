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
use quill::state::SharedMediaTab;
use std::cell::Cell;

/// What identifies the first page of a tab: its chat, the date it was
/// reloaded from and its newest message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PageKey {
    pub chat: i64,
    pub anchor_day: Option<i64>,
    pub newest: i64,
}

pub(crate) struct SharedMediaGridUi {
    scroll: [VirtualListScrollHandle; SharedMediaTab::COUNT],
    page: [Cell<Option<PageKey>>; SharedMediaTab::COUNT],
    /// The month-sectioned list tabs (files, links, voice, music), which
    /// page near their end (`scroll_load`).
    list_scroll: [ScrollHandle; SharedMediaTab::COUNT],
    list_asked: [Asked; SharedMediaTab::COUNT],
}

impl SharedMediaGridUi {
    pub(crate) fn new() -> Self {
        Self {
            scroll: std::array::from_fn(|_| VirtualListScrollHandle::new()),
            page: std::array::from_fn(|_| Cell::new(None)),
            list_scroll: std::array::from_fn(|_| ScrollHandle::new()),
            list_asked: std::array::from_fn(|_| Asked::default()),
        }
    }

    pub(super) fn list_asked(&self, tab: SharedMediaTab) -> Option<&Asked> {
        self.list_asked.get(tab.index())
    }

    pub(super) fn list_scroll(&self, tab: SharedMediaTab) -> Option<&ScrollHandle> {
        self.list_scroll.get(tab.index())
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
