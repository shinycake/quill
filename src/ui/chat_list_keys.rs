//! Keyboard navigation of the chat list and of its search results.
//!
//! Telegram Desktop (`Dialogs::InnerWidget::processKeyDispatch`,
//! `selectSkip`, `selectSkipPage`, `chooseRow` in
//! dialogs/dialogs_inner_widget.cpp): with the chat list focused, Up and
//! Down move the selected row, Page Up and Page Down move it by a page,
//! Home and End jump to the ends, and Enter opens it (the first row when
//! none is selected, `Widget::submit`). The selected row is the one the
//! pointer selects too (`_selected`), painted with the hover fill; a mouse
//! move over the list takes the selection back. With the search field
//! focused, Up, Down and the page keys move through the results and Enter
//! opens the selected one; Home and End stay with the field.
//!
//! Escape follows the same file and history/history_widget.cpp: in the chat
//! it closes the chat and focuses the list (`HistoryWidget::escape` →
//! `MainWidget::handleHistoryBack`); in the list it leaves the archive, then
//! goes back to the open chat (`Widget::escape` → `dialogsCancelled`), and
//! with no chat open returns to the first folder.
#![deny(clippy::indexing_slicing)]

use super::app::{ChatListFilter, QuillApp};
use super::chat_row::{ChatListItem, chat_row_height};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

actions!(
    quill_ui,
    [
        /// Chat list or search field focused: select the row above.
        ChatListUp,
        /// Chat list or search field focused: select the row below.
        ChatListDown,
        /// Chat list or search field focused: select a page up.
        ChatListPageUp,
        /// Chat list or search field focused: select a page down.
        ChatListPageDown,
        /// Chat list focused: select the first row.
        ChatListHome,
        /// Chat list focused: select the last row.
        ChatListEnd,
        /// Chat list focused: open the selected row (the first when none is).
        ChatListOpen,
    ]
);

/// Key context of the chat list column (the sidebar that tracks
/// `QuillApp::focus_sidebar`).
pub(super) const CHAT_LIST_CONTEXT: &str = "QuillChatList";
/// Key context of the wrapper around the chat list's search field.
pub(super) const CHAT_SEARCH_CONTEXT: &str = "QuillChatSearch";
/// The search field itself. These bindings tie with the field's own
/// `Input` bindings on depth, so they are installed after `gpui_kit::init`
/// and win; each handler propagates when it has nothing to do, which lets
/// the field's binding run next.
const CHAT_SEARCH_INPUT_CONTEXT: &str = "QuillChatSearch > Input";

/// The chat list's keys. Not user-rebindable, like tdesktop's (they are
/// widget keys there, not `Shortcuts` commands).
pub(super) fn chat_list_bindings() -> Vec<KeyBinding> {
    let list = Some(CHAT_LIST_CONTEXT);
    let search = Some(CHAT_SEARCH_INPUT_CONTEXT);
    vec![
        KeyBinding::new("up", ChatListUp, list),
        KeyBinding::new("down", ChatListDown, list),
        KeyBinding::new("pageup", ChatListPageUp, list),
        KeyBinding::new("pagedown", ChatListPageDown, list),
        KeyBinding::new("home", ChatListHome, list),
        KeyBinding::new("end", ChatListEnd, list),
        KeyBinding::new("enter", ChatListOpen, list),
        // The field keeps Home and End for its caret, and its own Enter
        // (`PressEnter`) opens the selected result.
        KeyBinding::new("up", ChatListUp, search),
        KeyBinding::new("down", ChatListDown, search),
        KeyBinding::new("pageup", ChatListPageUp, search),
        KeyBinding::new("pagedown", ChatListPageDown, search),
    ]
}

/// One navigation key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NavKey {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
}

/// A selectable row of the chat list, by identity (the list reorders under
/// the selection as messages arrive).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum ListRow {
    /// The "Archived chats" row or its collapsed bar.
    Archive,
    Chat(ChatId),
}

/// A selectable search result, by identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum SearchRow {
    Chat(ChatId),
    PublicChat(ChatId),
    Message(ChatId, MessageId),
}

/// The selection state (a `ChatListUi` field).
#[derive(Default)]
pub(crate) struct ChatListKeys {
    /// The row the keys selected, painted until the mouse moves over the
    /// list.
    list: Option<ListRow>,
    /// The result the keys selected, painted until the mouse moves over
    /// the results.
    search: Option<SearchRow>,
    /// The row and result under the pointer: the mouse's selection, which
    /// the keys move from and Enter opens.
    list_mouse: Option<ListRow>,
    search_mouse: Option<SearchRow>,
    /// The results in the order the last frame drew them.
    search_order: RefCell<Vec<SearchRow>>,
    /// Where the last frame laid each result out (window coordinates; the
    /// first result of a section includes its heading).
    search_bounds: Rc<RefCell<HashMap<SearchRow, Bounds<Pixels>>>>,
}

impl ChatListKeys {
    /// The keys put the selection on `row`.
    pub(super) fn list_highlighted(&self, row: ListRow) -> bool {
        self.list == Some(row)
    }

    pub(super) fn search_highlighted(&self, row: SearchRow) -> bool {
        self.search == Some(row)
    }

    /// The selection Enter opens: the keys', else the pointer's, while it
    /// is still among the results.
    pub(super) fn search_selection(&self) -> Option<SearchRow> {
        self.search
            .or(self.search_mouse)
            .filter(|row| self.search_order.borrow().contains(row))
    }

    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn search_highlight(&self) -> Option<SearchRow> {
        self.search
    }

    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn search_rows(&self) -> Vec<SearchRow> {
        self.search_order.borrow().clone()
    }

    /// The search results are about to be drawn: start their order and
    /// bounds afresh.
    pub(super) fn begin_search_rows(&self) {
        self.search_order.borrow_mut().clear();
        self.search_bounds.borrow_mut().clear();
    }

    pub(super) fn push_search_row(&self, row: SearchRow) {
        self.search_order.borrow_mut().push(row);
    }

    /// A prepaint listener for one section of results: `rows` are its
    /// results in order, drawn after the section heading (child 0).
    pub(super) fn section_bounds_recorder(
        &self,
        rows: Vec<SearchRow>,
    ) -> impl Fn(Vec<Bounds<Pixels>>, &mut Window, &mut App) + 'static {
        let store = self.search_bounds.clone();
        move |bounds, _, _| {
            let heading = bounds.first().copied();
            let Ok(mut store) = store.try_borrow_mut() else {
                return;
            };
            for (index, (row, row_bounds)) in rows.iter().zip(bounds.iter().skip(1)).enumerate() {
                // tdesktop reveals a section's heading with its first row
                // (`scrollToFilteredSelected`, `searchedBarHeight`).
                let reveal = match heading {
                    Some(heading) if index == 0 => heading.union(row_bounds),
                    _ => *row_bounds,
                };
                store.insert(*row, reveal);
            }
        }
    }
}

/// Where `key` moves the selection among `len` rows, or `None` when it
/// stays. `page` is how many rows fit the viewport (tdesktop: viewport
/// height / row height). `filtered` is the search state, where a page key
/// skips like the arrows (`selectSkipPage` → `selectSkip`).
pub(super) fn step(
    key: NavKey,
    current: Option<usize>,
    len: usize,
    page: usize,
    filtered: bool,
) -> Option<usize> {
    let last = len.checked_sub(1)?;
    // `selectSkip`: nothing selected selects the first row, whatever the
    // direction; otherwise the move clamps at both ends.
    let skip = |back: bool, by: usize| match current {
        None => 0,
        Some(cur) => {
            let cur = cur.min(last);
            if back {
                cur.saturating_sub(by)
            } else {
                cur.saturating_add(by).min(last)
            }
        }
    };
    match key {
        NavKey::Up => Some(skip(true, 1)),
        NavKey::Down => Some(skip(false, 1)),
        NavKey::Home => Some(skip(true, len)),
        NavKey::End => Some(skip(false, len)),
        NavKey::PageUp if filtered => Some(skip(true, page)),
        NavKey::PageDown if filtered => Some(skip(false, page)),
        // The default list's `selectSkipPage`: nothing selected selects the
        // first row going down (and nothing going up); the downward walk
        // counts the row it starts on, so it lands `page - 1` rows on.
        NavKey::PageDown => {
            let from = current.unwrap_or(0).min(last);
            Some(from.saturating_add(page.saturating_sub(1)).min(last))
        }
        NavKey::PageUp => current.map(|cur| cur.min(last).saturating_sub(page)),
    }
}

/// How many rows of `row` height fit `viewport` (`pixels / _st->height`).
pub(super) fn rows_per_page(viewport: Pixels, row: Pixels) -> usize {
    let row = f32::from(row);
    let viewport = f32::from(viewport);
    if row.is_nan() || row <= 0.0 || !viewport.is_finite() || viewport <= 0.0 {
        return 0;
    }
    // Truncating like tdesktop's integer division; `as` saturates.
    (viewport / row).floor() as usize
}

/// The selectable rows of the chat list with their item index, top to
/// bottom (the stories strip is not a row).
fn list_rows(items: &[ChatListItem]) -> Vec<(usize, ListRow)> {
    items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| match item {
            ChatListItem::Chat { id, .. } => Some((index, ListRow::Chat(*id))),
            ChatListItem::ArchiveRow { .. } | ChatListItem::ArchiveBar => {
                Some((index, ListRow::Archive))
            }
            ChatListItem::StoryStrip => None,
        })
        .collect()
}

/// Which list the keys drive.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    List,
    Search,
}

impl QuillApp {
    /// The list the keys drive now, or `None` when the chat list does not
    /// have the keyboard (the key then goes to the next binding).
    fn chat_list_key_scope(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<Scope> {
        if self.shortcut_blocked(window, cx)
            || self.chat_list.contacts_tab_open
            || self.chat_list.calls_tab_open
        {
            return None;
        }
        let list_focused = self.focus_sidebar.is_focused(window);
        if self.search_is_open() {
            let field_focused = self
                .search_ui
                .input
                .read(cx)
                .focus_handle(cx)
                .is_focused(window);
            return (list_focused || field_focused).then_some(Scope::Search);
        }
        list_focused.then_some(Scope::List)
    }

    /// Up, Down, Page Up, Page Down, Home or End in the chat list.
    pub(super) fn chat_list_nav(
        &mut self,
        key: NavKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.chat_list_key_scope(window, cx) {
            Some(Scope::List) => self.step_list_selection(key, cx),
            Some(Scope::Search) => self.step_search_selection(key, cx),
            None => cx.propagate(),
        }
    }

    /// Enter in the chat list: open the selected row, or select and open
    /// the first one.
    pub(super) fn chat_list_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.chat_list_key_scope(window, cx) {
            Some(Scope::List) => self.open_list_selection(window, cx),
            Some(Scope::Search) => self.activate_first_search_result(window, cx),
            None => cx.propagate(),
        }
    }

    fn step_list_selection(&mut self, key: NavKey, cx: &mut Context<Self>) {
        let rows = list_rows(&self.chat_list.items);
        let keys = &self.chat_list.keys;
        let current = keys
            .list
            .or(keys.list_mouse)
            .and_then(|row| rows.iter().position(|(_, candidate)| *candidate == row));
        let page = rows_per_page(
            self.chat_list.scroll.bounds().size.height,
            chat_row_height(&[], self.appearance.preview_lines),
        );
        let Some(&(item, row)) =
            step(key, current, rows.len(), page, false).and_then(|next| rows.get(next))
        else {
            return;
        };
        self.chat_list.keys.list = Some(row);
        // tdesktop `scrollToDefaultSelected`: the least scroll that shows
        // the row.
        self.chat_list
            .scroll
            .scroll_to_item(item, ScrollStrategy::Nearest);
        self.notify_sidebar(cx);
    }

    /// Put the keyboard selection on `row` and scroll it into view when the
    /// list shows it.
    fn select_list_row(&mut self, row: ListRow) {
        self.chat_list.keys.list = Some(row);
        if let Some((item, _)) = list_rows(&self.chat_list.items)
            .into_iter()
            .find(|(_, candidate)| *candidate == row)
        {
            self.chat_list
                .scroll
                .scroll_to_item(item, ScrollStrategy::Nearest);
        }
    }

    fn open_list_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rows = list_rows(&self.chat_list.items);
        let keys = &self.chat_list.keys;
        let selected = keys
            .list
            .or(keys.list_mouse)
            .filter(|row| rows.iter().any(|(_, candidate)| candidate == row));
        let row = match selected {
            Some(row) => row,
            None => {
                // `Widget::submit`: `selectSkip(1)`, then `chooseRow`.
                let Some(&(item, first)) = rows.first() else {
                    return;
                };
                self.chat_list.keys.list = Some(first);
                self.chat_list
                    .scroll
                    .scroll_to_item(item, ScrollStrategy::Nearest);
                first
            }
        };
        // The selection stays where it is (tdesktop keeps `_selected`), so
        // Escape from the chat comes back to it.
        match row {
            ListRow::Archive => self.open_archive_folder(cx),
            ListRow::Chat(id) if self.selecting_chats() => {
                self.toggle_chat_selected(id, cx);
            }
            ListRow::Chat(id) => {
                self.select_listed_chat(id, window, cx);
                self.focus_opened_chat(window, cx);
            }
        }
        self.notify_sidebar(cx);
    }

    /// `HistoryWidget::setInnerFocus`: a chat opened from the keyboard hands
    /// the keys to its composer. A chat without one leaves them in the list.
    fn focus_opened_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let open = self.session().is_some_and(|s| s.open_chat.is_some());
        if open && self.composer_available(self.pane_mode()) {
            self.composer
                .update(cx, |input, cx| input.focus(window, cx));
        }
    }

    fn step_search_selection(&mut self, key: NavKey, cx: &mut Context<Self>) {
        let order = self.chat_list.keys.search_order.borrow().clone();
        let keys = &self.chat_list.keys;
        let current = keys
            .search
            .or(keys.search_mouse)
            .and_then(|row| order.iter().position(|candidate| *candidate == row));
        let row_height = order
            .first()
            .and_then(|row| keys.search_bounds.borrow().get(row).map(|b| b.size.height))
            .unwrap_or_else(|| chat_row_height(&[], self.appearance.preview_lines));
        let page = rows_per_page(
            self.search_ui.results_scroll.bounds().size.height,
            row_height,
        );
        let Some(&row) =
            step(key, current, order.len(), page, true).and_then(|next| order.get(next))
        else {
            return;
        };
        self.chat_list.keys.search = Some(row);
        self.reveal_search_row(row);
        self.notify_sidebar(cx);
    }

    /// Scroll the results the least that shows `row` (tdesktop
    /// `scrollToFilteredSelected`).
    fn reveal_search_row(&self, row: SearchRow) {
        let Some(bounds) = self
            .chat_list
            .keys
            .search_bounds
            .borrow()
            .get(&row)
            .copied()
        else {
            return;
        };
        let handle = &self.search_ui.results_scroll;
        let viewport = handle.bounds();
        let mut offset = handle.offset();
        if bounds.top() < viewport.top() {
            offset.y += viewport.top() - bounds.top();
        } else if bounds.bottom() > viewport.bottom() {
            offset.y -= bounds.bottom() - viewport.bottom();
        } else {
            return;
        }
        let floor = -handle.max_offset().y;
        if offset.y > px(0.) {
            offset.y = px(0.);
        } else if offset.y < floor {
            offset.y = floor;
        }
        handle.set_offset(offset);
    }

    /// The search result `row` was chosen by key: open it as a click would.
    pub(super) fn open_search_row(
        &mut self,
        row: SearchRow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.chat_list.keys.search = None;
        self.chat_list.keys.search_mouse = None;
        match row {
            SearchRow::Chat(id) | SearchRow::PublicChat(id) => {
                self.select_search_chat(id, window, cx)
            }
            SearchRow::Message(chat_id, message_id) => {
                self.select_search_message(chat_id, message_id, window, cx);
            }
        }
        self.focus_opened_chat(window, cx);
    }

    /// Close the open chat and give the list the keyboard (Escape in the
    /// chat, and the one-column back button).
    pub(super) fn back_to_chat_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(chat) = self.session().and_then(|s| s.open_chat) {
            // The closed chat's row becomes the selection (tdesktop
            // keeps `_selected` on it), the one state the list paints
            // and the next key steps from.
            self.select_list_row(ListRow::Chat(chat));
            self.leave_open_chat(window, cx);
            window.focus(&self.focus_sidebar, cx);
        }
    }

    /// Escape with no layer open.
    pub(super) fn escape_to_chat_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.shortcut_blocked(window, cx) || self.windows.scope.is_some() {
            return;
        }
        let open = self.session().and_then(|s| s.open_chat);
        if !self.focus_sidebar.contains_focused(window, cx) {
            // `HistoryWidget::escape` → `handleHistoryBack`: close the chat
            // (`clearSectionStack`) and give the list the keyboard.
            self.back_to_chat_list(window, cx);
            return;
        }
        // `Widget::escape`: leave the opened folder (Quill's archive view)
        // or community, else go back to the open chat, else to the first
        // folder.
        if self.chat_list.filter == ChatListFilter::Archived {
            self.set_chat_list_view(ChatListFilter::All, self.folders.tab);
            cx.notify();
        } else if matches!(self.chat_list.filter, ChatListFilter::Community(_)) {
            self.exit_community_chat_list_mode(cx);
        } else if open.is_some() {
            self.focus_opened_chat(window, cx);
        } else if self.folders.tab.is_some() {
            self.open_folder_tab(None, cx);
        }
    }

    /// Paint a list row the keys selected, and track the pointer's row.
    pub(super) fn chat_list_nav_row<E>(
        &self,
        row: E,
        key: ListRow,
        open: bool,
        cx: &mut Context<Self>,
    ) -> E
    where
        E: IntoElement + Styled + StatefulInteractiveElement,
    {
        let highlighted = self.chat_list.keys.list_highlighted(key);
        row.when(highlighted, |row| row.bg(nav_fill(open, cx.theme())))
            .hover_listener_mode(HoverListenerMode::InputModalityIndependent)
            .on_hover(cx.listener(move |this, hovered: &bool, _, _| {
                let keys = &mut this.chat_list.keys;
                if *hovered {
                    keys.list_mouse = Some(key);
                } else if keys.list_mouse == Some(key) {
                    keys.list_mouse = None;
                }
            }))
    }

    /// Paint a search result the keys selected, and track the pointer's.
    pub(super) fn chat_search_nav_row(
        &self,
        row: Stateful<Div>,
        key: SearchRow,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let highlighted = self.chat_list.keys.search_highlighted(key);
        self.chat_list.keys.push_search_row(key);
        row.when(highlighted, |row| row.bg(nav_fill(false, cx.theme())))
            .hover_listener_mode(HoverListenerMode::InputModalityIndependent)
            .on_hover(cx.listener(move |this, hovered: &bool, _, _| {
                let keys = &mut this.chat_list.keys;
                if *hovered {
                    keys.search_mouse = Some(key);
                } else if keys.search_mouse == Some(key) {
                    keys.search_mouse = None;
                }
            }))
    }

    /// A mouse move over the list or the results hands the selection back
    /// to the pointer (tdesktop `mouseMoveEvent` → `selectByMouse`).
    pub(super) fn chat_list_mouse_moved(&mut self, cx: &mut Context<Self>) {
        let keys = &mut self.chat_list.keys;
        if keys.list.is_some() || keys.search.is_some() {
            keys.list = None;
            keys.search = None;
            self.notify_sidebar(cx);
        }
    }

    /// The chat list column's key context and handlers.
    pub(super) fn chat_list_key_handlers(
        &self,
        list: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        list.key_context(CHAT_LIST_CONTEXT)
            .on_action(cx.listener(|this, _: &ChatListUp, window, cx| {
                this.chat_list_nav(NavKey::Up, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ChatListDown, window, cx| {
                this.chat_list_nav(NavKey::Down, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ChatListPageUp, window, cx| {
                this.chat_list_nav(NavKey::PageUp, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ChatListPageDown, window, cx| {
                this.chat_list_nav(NavKey::PageDown, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ChatListHome, window, cx| {
                this.chat_list_nav(NavKey::Home, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ChatListEnd, window, cx| {
                this.chat_list_nav(NavKey::End, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ChatListOpen, window, cx| {
                this.chat_list_open(window, cx);
            }))
    }

    /// Whether the list column has the keyboard and the last input came
    /// from it: the list (or the results) then shows the focus ring.
    pub(super) fn chat_list_focus_visible(&self, window: &Window) -> bool {
        self.focus_sidebar.is_focused(window) && window.last_input_was_keyboard()
    }
}

/// The selected row's fill: the hover fill of a row, or of the open chat's
/// row (tdesktop paints the keyboard selection as the pointer's).
fn nav_fill(open: bool, theme: &Theme) -> Hsla {
    if open {
        super::chat_row::open_row_hover_fill(theme)
    } else {
        super::pressable::hover_fill(theme)
    }
}

#[cfg(test)]
mod tests {
    use super::{NavKey, rows_per_page, step};
    use gpui_kit::px;

    #[test]
    fn arrows_select_the_first_row_when_nothing_is_selected() {
        assert_eq!(step(NavKey::Down, None, 5, 3, false), Some(0));
        assert_eq!(step(NavKey::Up, None, 5, 3, false), Some(0));
        assert_eq!(step(NavKey::Up, None, 0, 3, false), None);
    }

    #[test]
    fn arrows_clamp_at_both_ends() {
        assert_eq!(step(NavKey::Down, Some(1), 5, 3, false), Some(2));
        assert_eq!(step(NavKey::Down, Some(4), 5, 3, false), Some(4));
        assert_eq!(step(NavKey::Up, Some(0), 5, 3, false), Some(0));
        assert_eq!(step(NavKey::Up, Some(3), 5, 3, false), Some(2));
        // A stale index past the end clamps first.
        assert_eq!(step(NavKey::Up, Some(9), 5, 3, false), Some(3));
    }

    #[test]
    fn home_and_end_jump_to_the_ends() {
        assert_eq!(step(NavKey::Home, Some(3), 5, 3, false), Some(0));
        assert_eq!(step(NavKey::End, Some(1), 5, 3, true), Some(4));
        // `selectSkip` with nothing selected picks the first row.
        assert_eq!(step(NavKey::End, None, 5, 3, false), Some(0));
    }

    #[test]
    fn page_keys_in_the_list_follow_select_skip_page() {
        // Down counts the starting row; up does not.
        assert_eq!(step(NavKey::PageDown, Some(2), 50, 10, false), Some(11));
        assert_eq!(step(NavKey::PageDown, None, 50, 10, false), Some(9));
        assert_eq!(step(NavKey::PageDown, Some(45), 50, 10, false), Some(49));
        assert_eq!(step(NavKey::PageUp, Some(20), 50, 10, false), Some(10));
        assert_eq!(step(NavKey::PageUp, Some(4), 50, 10, false), Some(0));
        assert_eq!(step(NavKey::PageUp, None, 50, 10, false), None);
        assert_eq!(step(NavKey::PageDown, Some(3), 50, 0, false), Some(3));
    }

    #[test]
    fn page_keys_in_search_skip_like_the_arrows() {
        assert_eq!(step(NavKey::PageDown, Some(2), 50, 10, true), Some(12));
        assert_eq!(step(NavKey::PageDown, None, 50, 10, true), Some(0));
        assert_eq!(step(NavKey::PageUp, Some(5), 50, 10, true), Some(0));
    }

    #[test]
    fn rows_per_page_truncates_and_survives_zero() {
        assert_eq!(rows_per_page(px(640.), px(62.)), 10);
        assert_eq!(rows_per_page(px(640.), px(0.)), 0);
        assert_eq!(rows_per_page(px(0.), px(62.)), 0);
        assert_eq!(rows_per_page(px(f32::NAN), px(62.)), 0);
    }
}
