//! What the chat list is showing, and what belongs to it: the view (the
//! category and the folder tab), the sidebar tab, and the rows laid out for
//! them. Every change of view or tab goes through a transition here, which
//! ends what was made for the old one (the multi-select, a pinned drag, the
//! laid-out rows), so no caller ends them by hand.

use super::*;
use crate::ui::chat_list_state::{ChatListUi, PinStage, SidebarTab};

/// The chat list's view: the category and the folder tab (`None` is Main),
/// one value so they never change apart. Only `ChatListUi::show_view` writes
/// it, and that ends what belongs to the old view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::ui) struct ChatListView {
    filter: ChatListFilter,
    folder: Option<i32>,
}

impl ChatListView {
    /// The category: all, unread, archived or a community's chats.
    pub(in crate::ui) fn filter(self) -> ChatListFilter {
        self.filter
    }

    /// The selected folder tab; `None` is the Main tab.
    pub(in crate::ui) fn folder(self) -> Option<i32> {
        self.folder
    }
}

impl ChatListUi {
    /// Shows another folder or category: the multi-select and the pinned
    /// drag belong to the view they were made in and end with it.
    pub(in crate::ui) fn show_view(&mut self, filter: ChatListFilter, folder: Option<i32>) {
        let next = ChatListView { filter, folder };
        if self.view != next {
            self.selected.clear();
            self.pin.end_drag();
        }
        self.view = next;
    }

    /// Shows another sidebar tab. Contacts and Calls take the place of the
    /// rows, so a pinned drag (whose release can never reach them) ends
    /// unsaved and the laid-out rows go: key stepping and file drags must
    /// not use a list that is not on screen.
    pub(in crate::ui) fn show_tab(&mut self, tab: SidebarTab) {
        if tab != SidebarTab::Chats {
            self.leave_rows();
        }
        self.tab = tab;
    }

    /// The rows are not on screen any more (another tab, the search): the
    /// pinned drag ends unsaved and the laid-out rows are forgotten.
    pub(in crate::ui) fn leave_rows(&mut self) {
        self.pin.end_drag();
        self.items.clear();
    }

    /// A pinned reorder begins (or fails to).
    pub(in crate::ui) fn begin_pin_drag(&mut self, stage: PinStage) {
        self.pin = stage;
    }

    /// A new list pass starts: rows not drawn in it age out of the
    /// animation map. A pass that finds the map in use is skipped.
    pub(in crate::ui) fn begin_row_pass(&self) {
        if let Ok(mut fx) = self.row_fx.try_borrow_mut() {
            fx.begin_pass();
        }
    }

    /// This frame's online dot and badge animation of one row. A map that is
    /// in use (a re-entrant draw) shows the row settled, with no frame asked.
    pub(in crate::ui) fn observe_row(
        &self,
        chat: i64,
        online: bool,
        badge: bool,
        animate: bool,
        now: std::time::Instant,
    ) -> quill::row_fx::RowFx {
        match self.row_fx.try_borrow_mut() {
            Ok(mut fx) => fx.observe(chat, online, badge, animate, now),
            Err(_) => quill::row_fx::RowFx {
                online: f32::from(u8::from(online)),
                badge: f32::from(u8::from(badge)),
                animating: false,
            },
        }
    }
}
