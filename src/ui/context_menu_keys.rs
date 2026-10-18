//! Keyboard side of the small context menus (archive row, story tile,
//! playlist track, folder tab): one item builder with its focus handle and
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
//! The arrows are not in the shortcut list and cannot be rebound: the
//! keystroke interceptor (`app_demo.rs`) handles them, because they must
//! work while the focus is still outside the menu.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::assets::IconName;
use gpui_kit::component::*;
use gpui_kit::*;
use std::cell::{Cell, RefCell};

/// Width of the focus ring drawn inside an item's edge.
const RING_WIDTH: f32 = 2.;

/// Focus handles of the enabled items of the open menu, in drawing order.
/// Handles are kept by position so they survive re-renders; `begin` runs at
/// the top of every frame and each enabled item drawn after it claims the
/// next slot.
#[derive(Default)]
pub(super) struct ContextMenuItems {
    handles: RefCell<Vec<FocusHandle>>,
    drawn: Cell<usize>,
}

impl ContextMenuItems {
    /// A new frame starts: no item has been drawn yet.
    pub(super) fn begin(&self) {
        self.drawn.set(0);
    }

    /// The handle for the next enabled item drawn this frame.
    fn claim(&self, cx: &App) -> FocusHandle {
        let slot = self.drawn.get();
        self.drawn.set(slot + 1);
        let mut handles = self.handles.borrow_mut();
        if let Some(handle) = handles.get(slot) {
            return handle.clone();
        }
        let handle = cx.focus_handle().tab_stop(true);
        handles.push(handle.clone());
        handle
    }

    /// How many enabled items were drawn this frame.
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn enabled_count(&self) -> usize {
        self.drawn.get()
    }

    /// The handle after (or before) the focused one, wrapping; with none
    /// focused, the first (or last). `None` when the menu has no enabled item.
    pub(super) fn step(&self, forward: bool, window: &Window) -> Option<FocusHandle> {
        let handles = self.handles.borrow();
        let live = handles.get(..self.drawn.get())?;
        let count = live.len();
        let next = match live.iter().position(|handle| handle.is_focused(window)) {
            Some(at) if forward => (at + 1) % count,
            Some(at) => (at + count - 1) % count,
            None if forward => 0,
            None => count.checked_sub(1)?,
        };
        live.get(next).cloned()
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
    let theme = cx.theme();
    let hover = theme.accent;
    let ring = BoxShadow {
        color: theme.ring,
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(RING_WIDTH),
        inset: true,
    };
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
        item.cursor_pointer()
            .hover(|style| style.bg(hover))
            .track_focus(&items.claim(cx))
            .focus_visible(|style| style.shadow(vec![ring]))
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

    /// Up or Down with a menu open: focus the next (or previous) enabled item
    /// of that menu, wrapping, wherever the focus was. With no enabled item
    /// (the message and chat menus, or every item disabled) the menu frame
    /// takes the focus, so the arrows never reach what is under the menu.
    pub(super) fn move_context_menu_focus(&self, forward: bool, window: &mut Window, cx: &mut App) {
        match self.frame.context_menu_items.step(forward, window) {
            Some(item) => window.focus(&item, cx),
            None if !self.frame.context_menu_focus.contains_focused(window, cx) => {
                window.focus(&self.frame.context_menu_focus, cx)
            }
            None => {}
        }
    }
}

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
