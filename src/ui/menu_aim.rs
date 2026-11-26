//! Menu aim for Quill's hand-built menus with submenus (the chat row's, the
//! search's top-person menu and the collapsed community row's): the pointer
//! can travel diagonally from a row like "Mute notifications" to its
//! submenu across the rows in between without them closing it. The rule is
//! the kit's `SubmenuAim` (Telegram Desktop's `PopupMenu` menu aim), which
//! the kit's own `PopupMenu` follows too.
//!
//! The aim lives with the menu's items ([`ContextMenuItems::aim`]): the
//! window's open menu. It reads the open submenu from the menus themselves
//! (`open_submenu`) every move, so a submenu closed by key, by a click or by
//! the menu closing ends the aim with it. While it waits, the rows the
//! pointer crosses are not lit ([`MenuAimRow::menu_hover`]).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::context_menu_keys::ContextMenuItems;
use gpui_kit::base::ElementExt as _;
use gpui_kit::component::menu::{AimGeometry, AimPointer, SubmenuAim};
use gpui_kit::*;
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

/// The aim of the window's open menu; rows are named by their sort key.
pub(super) type MenuAim = SubmenuAim<u32, Task<()>>;

/// Where the open menu's panel and its submenu were last drawn, in window
/// coordinates. Written as they prepaint (layout output, not state), read
/// by the pointer moves that follow.
#[derive(Clone, Default)]
pub(super) struct MenuPanels(Rc<Cell<PanelBounds>>);

#[derive(Clone, Copy, Default)]
struct PanelBounds {
    menu: Option<Bounds<Pixels>>,
    submenu: Option<Bounds<Pixels>>,
}

impl MenuPanels {
    fn geometry(&self) -> Option<AimGeometry> {
        let bounds = self.0.get();
        Some(AimGeometry {
            menu: bounds.menu?,
            submenu: bounds.submenu?,
        })
    }

    fn record(&self, change: impl FnOnce(&mut PanelBounds)) {
        let mut bounds = self.0.get();
        change(&mut bounds);
        self.0.set(bounds);
    }
}

/// A menu panel that has submenus: its rows' area is measured for the aim.
pub(super) trait MenuAimPanel: ParentElement + Sized {
    fn menu_aim_panel(self, items: &ContextMenuItems) -> Self {
        let panels = items.panels.clone();
        self.on_prepaint(move |bounds, _, _| panels.record(|b| b.menu = Some(bounds)))
    }
}

impl<E: ParentElement> MenuAimPanel for E {}

/// A row of a menu with submenus.
pub(super) trait MenuAimRow: StatefulInteractiveElement + Styled + Sized {
    /// The pointer lights the row with `fill`, except while the menu aims
    /// at its open submenu: the rows the pointer crosses on the way stay
    /// dark, as in Telegram Desktop.
    fn menu_hover(self, fill: Hsla, items: &ContextMenuItems) -> Self {
        if items.aim.holds() {
            self
        } else {
            self.hover(move |style| style.bg(fill))
        }
    }

    /// The row at `key` takes the hover through the aim: hovering it opens
    /// its submenu, or closes the open one, unless the pointer is on its
    /// way to the open submenu. Replaces the row's `on_hover`.
    fn menu_aim_row(self, key: u32, cx: &mut Context<QuillApp>) -> Self {
        self.on_mouse_move(
            cx.listener(move |this, event: &MouseMoveEvent, window, cx| {
                this.menu_row_pointer_moved(key, event, window, cx);
            }),
        )
        .on_hover(cx.listener(move |this, hovered: &bool, _, _| {
            if !*hovered {
                this.frame.menu.items.aim.row_left(key);
            }
        }))
    }
}

impl<E: StatefulInteractiveElement + Styled> MenuAimRow for E {}

impl QuillApp {
    /// The submenu beside its parent row ([`super::menu_surface::submenu_panel`]),
    /// measured for the aim; the pointer on it ends the aim.
    pub(super) fn aimed_submenu_panel(
        &self,
        id: &'static str,
        rows: Vec<AnyElement>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let panels = self.frame.menu.items.panels.clone();
        super::menu_surface::submenu_panel(
            id,
            rows,
            move |bounds| panels.record(|b| b.submenu = Some(bounds)),
            cx.listener(|this, _: &MouseMoveEvent, _, cx| {
                let aim = &mut this.frame.menu.items.aim;
                if aim.holds() {
                    aim.reset();
                    cx.notify();
                }
            }),
            self.look(),
            cx,
        )
    }

    /// The parent row of the open submenu, which the pointer may aim from.
    fn aim_owner(&self) -> Option<u32> {
        let submenu = self.open_submenu()?;
        self.frame.menu.items.parent_of(submenu)
    }

    fn menu_row_pointer_moved(
        &mut self,
        key: u32,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let owner = self.aim_owner();
        // The executor's clock, which tests drive by hand.
        let now = cx.background_executor().now();
        let geometry = self.frame.menu.items.panels.geometry();
        let pointer = AimPointer {
            position: event.position,
            row: Some(key),
            pressed: event.pressed_button.is_some(),
        };
        // Taken out for the call, so its wake-up can be spawned from `self`.
        let mut aim = std::mem::take(&mut self.frame.menu.items.aim);
        let held = aim.holds();
        let takes = aim.pointer_moved(owner, pointer, geometry, now, |wait| {
            self.menu_aim_wake(wait, window, cx)
        });
        if held != aim.holds() {
            cx.notify();
        }
        self.frame.menu.items.aim = aim;
        if takes {
            self.menu_row_takes_hover(key, cx);
        }
    }

    /// The hover reached the row at `key`: its submenu opens, or the open
    /// one closes.
    fn menu_row_takes_hover(&mut self, key: u32, cx: &mut Context<Self>) {
        match self.frame.menu.items.submenu_of(key) {
            Some(submenu) => self.open_chat_submenu(submenu, cx),
            None => self.close_chat_submenu(cx),
        }
    }

    /// The aim's wake-up: the menus with submenus are the main window's,
    /// for the account on screen.
    fn menu_aim_wake(&self, wait: Duration, window: &Window, cx: &Context<Self>) -> Task<()> {
        // window-scope: menus with submenus are drawn by the main window only.
        self.spawn_screen_in(window, cx, async move |this, cx| {
            cx.background_executor().timer(wait).await;
            let _ = this.update_in(cx, |this, window, cx| this.menu_aim_rested(window, cx));
        })
    }

    /// The pointer rested inside the aim: the row under it takes the hover.
    fn menu_aim_rested(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let owner = self.aim_owner();
        // The executor's clock, which tests drive by hand.
        let now = cx.background_executor().now();
        let geometry = self.frame.menu.items.panels.geometry();
        let mut aim = std::mem::take(&mut self.frame.menu.items.aim);
        let row = aim.rested(owner, window.mouse_position(), geometry, now, |wait| {
            self.menu_aim_wake(wait, window, cx)
        });
        self.frame.menu.items.aim = aim;
        if let Some(key) = row {
            self.menu_row_takes_hover(key, cx);
        }
        cx.notify();
    }
}

#[cfg(test)]
#[path = "menu_aim_tests.rs"]
mod tests;
