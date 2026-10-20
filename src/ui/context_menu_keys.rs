//! Keyboard side of the context menus (archive row, story tile, playlist
//! track, folder tab, message, chat row): one item builder with its focus handle and
//! focus ring, and Up/Down that move between the menu's own items.
//!
//! Telegram Desktop (`Ui::Menu::handleKeyPress`, lib_ui/ui/widgets/menu/
//! menu.cpp) moves the selection with Up and Down, wraps at both ends and
//! skips disabled actions; Enter, Return and Space run the selected action
//! (menu_action.cpp) and Escape hides the menu (popup_menu.cpp). The menu
//! takes the arrows whenever it is open, wherever the focus was. Tab is not
//! part of it there; here the focus trap on the overlay keeps it inside.
//!
//! Which item is selected is GPUI's focus. Each enabled item owns a focus
//! handle kept in [`ContextMenuItems`], in the order the menu draws them, so
//! Up and Down step through that list and nothing else; disabled items have
//! no handle and are skipped. With no enabled item the menu frame keeps the
//! focus.
//!
//! Submenus: `PopupMenu::_activeSubmenu` is set whenever a submenu is shown,
//! by hover or by key (popup_menu.cpp:540), and while it is set
//! `handleKeyPress` hands every key to the submenu first
//! (popup_menu.cpp:577-590). Here `ChatMenuState::submenu` is that state, and
//! with it open the keys go to the submenu whichever row has the focus: Down
//! and Up enter and walk its rows (with none selected, Down takes the first and
//! Up the last, `Menu::handleKeyPress`), Left or Escape close only the submenu
//! and focus the parent row. With it closed, Right, or Enter, on a row with a
//! submenu opens it and focuses its first item. Left stays "close" and Right
//! stays "open" in right-to-left layouts too (owner decision, not mirrored).
//!
//! The arrows are not in the shortcut list and cannot be rebound: the
//! keystroke interceptor (`app_demo.rs`, via `context_menu_key`) handles
//! them, because they must work while the focus is still outside the menu.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::menu_states::ChatMenuSubmenu;
use gpui_kit::assets::IconName;
use gpui_kit::component::*;
use gpui_kit::*;
use std::cell::{Cell, RefCell};

/// Width of the focus ring drawn inside an item's edge.
const RING_WIDTH: f32 = 2.;

/// Focus handles of the enabled items of the open menu. Handles are kept by
/// claim position so they survive re-renders; `begin` runs at the top of
/// every frame and each enabled item drawn after it claims the next slot,
/// with a sort key that says where the row sits in the menu (menus that sort
/// their rows after building them pass the row's order).
#[derive(Default)]
pub(super) struct ContextMenuItems {
    handles: RefCell<Vec<FocusHandle>>,
    keys: RefCell<Vec<u32>>,
    /// Rows drawn this frame that own a submenu, with the submenu each opens.
    parents: RefCell<Vec<(u32, ChatMenuSubmenu)>>,
    /// Key of the item to focus once this frame has drawn it.
    focus_pending: Cell<Option<u32>>,
}

/// Sort key of a submenu row: just after its parent row (`order`), in the
/// order the submenu draws them.
///
/// The low byte holds `1 + index`, so a submenu has room for
/// [`MAX_SUBMENU_ROWS`] rows; later rows share the last slot and keep
/// their drawing order (the sort is stable) instead of spilling into the
/// next row's keys.
pub(super) fn submenu_key(order: u8, index: usize) -> u32 {
    let slot = index.min(MAX_SUBMENU_ROWS - 1) as u32;
    (u32::from(order) << 8) + 1 + slot
}

/// Whether a sort key belongs to a submenu row (top-level keys have a zero
/// low byte).
fn is_submenu_key(key: u32) -> bool {
    key & 0xff != 0
}

/// The parent row's key of a submenu row key.
fn parent_key(key: u32) -> u32 {
    key & !0xff
}

/// Submenu rows that get a distinct sort key under one parent row.
pub(super) const MAX_SUBMENU_ROWS: usize = 255;

/// Sort key of a top-level row with the given order.
pub(super) fn row_key(order: u8) -> u32 {
    u32::from(order) << 8
}

impl ContextMenuItems {
    /// A new frame starts: no item has been drawn yet. The handles are
    /// retained across frames; only the claims reset.
    pub(super) fn begin(&self) {
        self.keys.borrow_mut().clear();
        self.parents.borrow_mut().clear();
    }

    /// The row at `row_key(order)` drawn this frame opens `submenu`.
    pub(super) fn mark_submenu_parent(&self, order: u8, submenu: ChatMenuSubmenu) {
        self.parents.borrow_mut().push((row_key(order), submenu));
    }

    /// The handle for the next enabled item drawn this frame, which sorts
    /// at `key` among the menu's items.
    fn claim(&self, key: u32, cx: &App) -> FocusHandle {
        let slot = {
            let mut keys = self.keys.borrow_mut();
            let slot = keys.len();
            keys.push(key);
            slot
        };
        let existing = self.handles.borrow().get(slot).cloned();
        if let Some(handle) = existing {
            return handle;
        }
        let handle = cx.focus_handle().tab_stop(true);
        self.handles.borrow_mut().push(handle.clone());
        handle
    }

    /// How many enabled items were drawn this frame.
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn enabled_count(&self) -> usize {
        self.keys.borrow().len()
    }

    /// Asks the end of the next frame to focus the item drawn at `key`.
    fn focus_next_frame(&self, key: u32) {
        self.focus_pending.set(Some(key));
    }

    /// The end of a frame: focus the item asked for by `focus_next_frame`,
    /// if this frame drew it. One attempt, so a submenu with no rows cannot
    /// leave a stale request behind.
    pub(super) fn apply_pending_focus(&self, window: &mut Window, cx: &mut App) {
        let Some(key) = self.focus_pending.take() else {
            return;
        };
        if let Some(handle) = self.handle_for(key) {
            window.focus(&handle, cx);
        }
    }

    /// This frame's items as `(key, handle)`, in key order. Guards are
    /// dropped before the caller touches the window.
    fn live(&self) -> Vec<(u32, FocusHandle)> {
        let mut live: Vec<(u32, FocusHandle)> = {
            let handles = self.handles.borrow();
            let keys = self.keys.borrow();
            keys.iter()
                .zip(handles.iter())
                .map(|(key, handle)| (*key, handle.clone()))
                .collect()
        };
        live.sort_by_key(|(key, _)| *key);
        live
    }

    /// The handle of the item drawn at `key`, if any.
    fn handle_for(&self, key: u32) -> Option<FocusHandle> {
        self.live()
            .into_iter()
            .find(|(candidate, _)| *candidate == key)
            .map(|(_, handle)| handle)
    }

    /// The sort key of the focused item.
    fn focused_key(&self, window: &Window) -> Option<u32> {
        self.live()
            .into_iter()
            .find(|(_, handle)| handle.is_focused(window))
            .map(|(key, _)| key)
    }

    /// The submenu the row at `key` opens.
    fn submenu_of(&self, key: u32) -> Option<ChatMenuSubmenu> {
        self.parents
            .borrow()
            .iter()
            .find(|(parent, _)| *parent == key)
            .map(|(_, submenu)| *submenu)
    }

    /// The sort key of the row that owns `submenu`, if drawn this frame.
    fn parent_of(&self, submenu: ChatMenuSubmenu) -> Option<u32> {
        self.parents
            .borrow()
            .iter()
            .find(|(_, candidate)| *candidate == submenu)
            .map(|(parent, _)| *parent)
    }

    /// The handle after (or before) the focused one, wrapping; with none of
    /// the group focused, the first (or last). `inside` names the parent row
    /// of an open submenu, whose rows are then the group; otherwise the
    /// group is the menu's own rows. `None` when the group is empty.
    pub(super) fn step(
        &self,
        forward: bool,
        inside: Option<u32>,
        window: &Window,
    ) -> Option<FocusHandle> {
        let live = self.live();
        let group: Vec<&(u32, FocusHandle)> = live
            .iter()
            .filter(|(key, _)| match inside {
                Some(parent) => is_submenu_key(*key) && parent_key(*key) == parent,
                None => !is_submenu_key(*key),
            })
            .collect();
        let count = group.len();
        if count == 0 {
            return None;
        }
        let next = match group
            .iter()
            .position(|(_, handle)| handle.is_focused(window))
        {
            Some(at) if forward => (at + 1) % count,
            Some(at) => (at + count - 1) % count,
            None if forward => 0,
            None => count.saturating_sub(1),
        };
        group.get(next).map(|(_, handle)| handle.clone())
    }
}

fn focus_ring(cx: &App) -> BoxShadow {
    BoxShadow {
        color: cx.theme().ring,
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(RING_WIDTH),
        inset: true,
    }
}

/// Makes a hand-built menu row reachable by keyboard: it joins the open
/// menu's items at `key`, so Up, Down and Tab reach it and Enter or Space
/// run its `on_click`, and shows the shared focus ring while focused.
pub(super) trait MenuKeyTarget: Sized {
    fn menu_key_target(self, key: u32, items: &ContextMenuItems, cx: &App) -> Self;
}

impl<T: StatefulInteractiveElement> MenuKeyTarget for T {
    fn menu_key_target(self, key: u32, items: &ContextMenuItems, cx: &App) -> Self {
        let ring = focus_ring(cx);
        self.track_focus(&items.claim(key, cx))
            .focus_visible(|style| style.shadow(vec![ring]))
    }
}

/// One row of a context menu: icon and label in `color`, a hover fill, a
/// focus handle from `items` (so Up, Down and Tab reach it and Enter or Space
/// run it) and, while keyboard-focused, a ring inside its edge in the theme's
/// focus colour. The ring is an inset shadow, so it takes no room and hover
/// (a fill) and focus (a ring) read differently. A disabled row is dimmed and
/// gets no handle. The caller adds `on_click`.
pub(super) fn context_menu_item(
    id: impl Into<ElementId>,
    icon: IconName,
    label: &'static str,
    color: Hsla,
    enabled: bool,
    items: &ContextMenuItems,
    cx: &App,
) -> Stateful<Div> {
    let hover = cx.theme().accent;
    let item = div()
        .id(id)
        .flex()
        .items_center()
        .gap_3()
        .px_3()
        .py_1p5()
        .rounded_md()
        .text_sm()
        .text_color(color)
        .role(Role::MenuItem)
        .aria_label(label)
        .child(Icon::new(icon).size(px(16.)).text_color(color))
        .child(label);
    if enabled {
        let position = items.keys.borrow().len();
        let key = row_key(u8::try_from(position).unwrap_or(u8::MAX));
        item.cursor_pointer()
            .hover(|style| style.bg(hover))
            .menu_key_target(key, items, cx)
    } else {
        item.opacity(0.5)
    }
}

impl QuillApp {
    /// Whether any context menu is showing. The one list the focus hand-off,
    /// the keystroke interceptor and the Escape stack all read.
    pub(super) fn context_menu_open(&self) -> bool {
        self.message_ui.menu.is_some()
            || self.chat_list.menu.is_some()
            || self.chat_list.archive_menu.is_some()
            || self.chat_list.global.story_menu.is_some()
            || self.playback.playlist_menu.is_some()
            || self.folders.tab_menu.is_some()
    }

    /// Close every context menu. Returns whether one was open.
    pub(super) fn dismiss_context_menus(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.context_menu_open() {
            return false;
        }
        self.playback.playlist_menu = None;
        self.message_ui.menu = None;
        self.chat_list.menu = None;
        self.chat_list.archive_menu = None;
        self.chat_list.global.story_menu = None;
        self.folders.tab_menu = None;
        cx.notify();
        true
    }

    /// The keystroke interceptor's hook: with a context menu open, Escape
    /// closes it and Up/Down move its focus. Returns whether the key was
    /// taken. The arrows are swallowed whenever a menu is open, even one with
    /// no items and with the focus moved back to the composer; they never
    /// reach the chat list under it.
    pub(super) fn context_menu_key(
        &mut self,
        key: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.context_menu_open() {
            return false;
        }
        if self.submenu_key_press(key, window, cx) {
            return true;
        }
        if key.key == "escape" {
            return self.dismiss_context_menus(cx);
        }
        if let Some(forward) = arrow_direction(key) {
            self.move_context_menu_focus(forward, window, cx);
            return true;
        }
        super::keybindings::context_menu_captures_key(key)
    }

    /// The submenu that is open, by hover or by key.
    fn open_submenu(&self) -> Option<ChatMenuSubmenu> {
        self.chat_list.menu.as_ref().and_then(|menu| menu.submenu)
    }

    /// Up, Down, Left, Right, Escape and Enter where a submenu is involved.
    /// With one open every such key is for it, whichever row has the focus
    /// (`PopupMenu::handleKeyPress`). Returns whether the key was taken.
    fn submenu_key_press(
        &mut self,
        key: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if key.modifiers.modified() {
            return false;
        }
        let name = key.key.as_str();
        let (focused, own, open_parent) = {
            let items = &self.frame.context_menu_items;
            let focused = items.focused_key(window);
            let open_parent = self.open_submenu().and_then(|s| items.parent_of(s));
            (
                focused,
                focused.and_then(|key| items.submenu_of(key)),
                open_parent,
            )
        };
        if let Some(parent) = open_parent {
            match name {
                "down" | "up" => {
                    let forward = name == "down";
                    let next = self
                        .frame
                        .context_menu_items
                        .step(forward, Some(parent), window);
                    if let Some(next) = next {
                        window.focus(&next, cx);
                    }
                    return true;
                }
                "left" | "escape" => {
                    self.close_chat_submenu(cx);
                    let handle = self.frame.context_menu_items.handle_for(parent);
                    if let Some(handle) = handle {
                        window.focus(&handle, cx);
                    }
                    return true;
                }
                "right" | "enter" if focused == Some(parent) => {
                    let first = self.frame.context_menu_items.handle_for(parent + 1);
                    if let Some(first) = first {
                        window.focus(&first, cx);
                    }
                    return true;
                }
                _ => {}
            }
        }
        match (name, focused, own) {
            ("right" | "enter", Some(focused), Some(submenu)) => {
                self.open_submenu_and_focus_first(focused, submenu, cx);
                true
            }
            _ => false,
        }
    }

    /// Opens `submenu` and focuses its first row in the frame that draws it:
    /// the rows only exist while it is open.
    fn open_submenu_and_focus_first(
        &mut self,
        parent: u32,
        submenu: ChatMenuSubmenu,
        cx: &mut Context<Self>,
    ) {
        self.open_chat_submenu(submenu, cx);
        self.frame.context_menu_items.focus_next_frame(parent + 1);
        cx.notify();
    }

    /// Up or Down with a menu open: focus the next (or previous) enabled item
    /// of that menu, wrapping, wherever the focus was. With no enabled item
    /// (the message and chat menus, or every item disabled) the menu frame
    /// takes the focus, so the arrows never reach what is under the menu.
    pub(super) fn move_context_menu_focus(&self, forward: bool, window: &mut Window, cx: &mut App) {
        match self.frame.context_menu_items.step(forward, None, window) {
            Some(item) => window.focus(&item, cx),
            None if !self.frame.context_menu_focus.contains_focused(window, cx) => {
                window.focus(&self.frame.context_menu_focus, cx)
            }
            None => {}
        }
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "context_menu_focus_tests.rs"]
mod context_menu_focus_tests;

/// `Some(true)` for a plain Down, `Some(false)` for a plain Up.
pub(super) fn arrow_direction(key: &Keystroke) -> Option<bool> {
    if key.modifiers.modified() {
        return None;
    }
    match key.key.as_str() {
        "down" => Some(true),
        "up" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "context_menu_submenu_tests.rs"]
mod context_menu_submenu_tests;
