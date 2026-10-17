//! Scroll positions of the shared-media grid tabs, owned by the app so a
//! tab keeps its place across re-renders (the grid is a virtual list, whose
//! scroll handle would otherwise start over every frame).
#![deny(clippy::indexing_slicing)]

use gpui_kit::component::*;
use quill::state::SharedMediaTab;

pub(crate) struct SharedMediaGridUi {
    scroll: [VirtualListScrollHandle; SharedMediaTab::COUNT],
}

impl SharedMediaGridUi {
    pub(crate) fn new() -> Self {
        Self {
            scroll: std::array::from_fn(|_| VirtualListScrollHandle::new()),
        }
    }

    pub(super) fn scroll(&self, tab: SharedMediaTab) -> Option<&VirtualListScrollHandle> {
        self.scroll.get(tab.index())
    }
}
