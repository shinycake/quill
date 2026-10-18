//! Keyboard side of the small context menus (archive row, story tile,
//! playlist track, folder tab): one item builder with the tab stop and focus
//! treatment, and Up/Down that move between the items.
//!
//! Telegram Desktop (`Ui::Menu::handleKeyPress`, lib_ui/ui/widgets/menu/
//! menu.cpp) moves the selection with Up and Down, wraps at both ends and
//! skips disabled actions; Enter, Return and Space run the selected action
//! (menu_action.cpp) and Escape hides the menu (popup_menu.cpp). Tab is not
//! part of it there; here it also works, because every item is a tab stop.
//!
//! Which item is selected is GPUI's focus, not a field on the app: Up and
//! Down walk the same tab stops as Tab, and an item that is disabled has no
//! tab stop, so it is skipped for free.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::assets::IconName;
use gpui_kit::component::*;
use gpui_kit::*;

actions!(
    quill_ui,
    [
        /// Context menu focused: focus the item above, wrapping.
        ContextMenuUp,
        /// Context menu focused: focus the item below, wrapping.
        ContextMenuDown,
    ]
);

/// Key context of a context menu's overlay.
pub(super) const CONTEXT_MENU_CONTEXT: &str = "QuillContextMenu";

/// Bound on tab stops walked before giving up (the whole window's, not the
/// menu's).
const MAX_STEPS: usize = 200;

/// The menu's keys. Not user-rebindable, like tdesktop's widget keys.
pub(super) fn context_menu_bindings() -> Vec<KeyBinding> {
    let menu = Some(CONTEXT_MENU_CONTEXT);
    vec![
        KeyBinding::new("up", ContextMenuUp, menu),
        KeyBinding::new("down", ContextMenuDown, menu),
    ]
}

/// One row of a context menu: icon and label in `color`, hover fill, a tab
/// stop (so Tab, Up and Down reach it and Enter or Space run it) and a ring
/// in the theme's focus colour while focused. A disabled row is dimmed and
/// is not a tab stop. The caller adds `on_click`.
pub(super) fn context_menu_item(
    id: impl Into<ElementId>,
    icon: IconName,
    label: &'static str,
    color: Hsla,
    enabled: bool,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let hover = theme.accent;
    let ring = theme.ring;
    let item = div()
        .id(id)
        .flex()
        .items_center()
        .gap_3()
        .px_3()
        .py_1p5()
        .rounded_md()
        // The border is always there, transparent, so the ring takes no room.
        .border_1()
        .border_color(transparent_black())
        .text_sm()
        .text_color(color)
        .role(Role::MenuItem)
        .aria_label(label)
        .child(Icon::new(icon).size(px(16.)).text_color(color))
        .child(label);
    if enabled {
        item.cursor_pointer()
            .hover(|style| style.bg(hover))
            .tab_index(0)
            .focus_visible(|style| style.bg(hover).border_color(ring))
    } else {
        item.opacity(0.5)
    }
}

impl QuillApp {
    /// Whether any context menu is showing. The one list the focus hand-off
    /// and the Escape stack both read.
    pub(super) fn context_menu_open(&self) -> bool {
        self.message_ui.menu.is_some()
            || self.chat_list.menu.is_some()
            || self.chat_list.archive_menu.is_some()
            || self.chat_list.global.story_menu.is_some()
            || self.playback.playlist_menu.is_some()
            || self.folders.tab_menu.is_some()
    }

    /// Add the Up/Down handlers and key context to a menu overlay.
    pub(super) fn context_menu_keys<E: InteractiveElement>(
        &self,
        overlay: E,
        cx: &mut Context<Self>,
    ) -> E {
        overlay
            .key_context(CONTEXT_MENU_CONTEXT)
            .on_action(cx.listener(|this, _: &ContextMenuDown, window, cx| {
                this.move_context_menu_focus(true, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ContextMenuUp, window, cx| {
                this.move_context_menu_focus(false, window, cx)
            }))
    }

    /// Focus the next (or previous) tab stop inside the open menu. A step
    /// that leaves the menu keeps going until it comes back in, which wraps
    /// from the last item to the first and the other way round.
    fn move_context_menu_focus(&self, forward: bool, window: &mut Window, cx: &mut App) {
        let menu = &self.frame.context_menu_focus;
        for _ in 0..MAX_STEPS {
            if forward {
                window.focus_next(cx);
            } else {
                window.focus_prev(cx);
            }
            if menu.contains_focused(window, cx) {
                return;
            }
        }
    }
}
