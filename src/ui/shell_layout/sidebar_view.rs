//! The hidden chat list column on screen: the column sliding in and out of
//! the layout, the reveal strip at the window's left edge, the peeking
//! column over the conversation, and the Hide / Show Sidebar buttons.
//! The state and its motion are in `sidebar_reveal`.

use super::ShellLayout;
use crate::ui::actions::ToggleSidebar;
use crate::ui::app::{PaneMode, QuillApp};
use crate::ui::dismiss_layer::DismissLayer;
use crate::ui::sidebar_reveal::{LEAVE_GRACE, Peek, REVEAL_EDGE_WIDTH};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr};
use std::time::Duration;

/// Motion follows the OS reduce-motion preference and the "Interface
/// animations" power-saving switch.
pub(super) fn animate() -> bool {
    !quill::power_saving::reduce_motion_now()
}

impl QuillApp {
    /// Whether the chat list column can be hidden in this layout: the
    /// signed-in main window, with the list beside the chat.
    fn sidebar_hideable(&self, layout: &ShellLayout) -> bool {
        self.windows.scope.is_none()
            && self.pane_mode() == PaneMode::Ready
            && layout.sidebar_toggle()
            && layout.list.is_some()
    }

    /// Hide or show the chat list column (the button, the shortcut and the
    /// View menu). One column has nothing to hide.
    pub(in crate::ui) fn toggle_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let layout = self.shell_layout();
        if !self.sidebar_hideable(&layout) {
            return;
        }
        // In the drawer band the button opens and closes the drawer; the
        // hidden-column preference is not written there.
        if layout.drawer() {
            self.frame
                .sidebar
                .toggle_drawer(animate(), self.frame.sidebar.now());
            if self.frame.sidebar.pointer() != Peek::Held {
                self.release_sidebar_focus(window, cx);
            }
            cx.notify();
            return;
        }
        let hidden = !self.frame.sidebar.hidden();
        self.frame
            .sidebar
            .set_hidden(hidden, animate(), self.frame.sidebar.now());
        if hidden {
            self.release_sidebar_focus(window, cx);
        }
        self.schedule_window_state_save(window, cx);
        // The View menu's Hide / Show Sidebar follows.
        quill::wake::CHROME.poke();
        cx.notify();
    }

    /// Escape: the peeking column slides out.
    pub(in crate::ui) fn dismiss_sidebar_peek(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .frame
            .sidebar
            .dismiss(animate(), self.frame.sidebar.now())
        {
            self.release_sidebar_focus(window, cx);
            cx.notify();
        }
    }

    /// A held drawer closes (a chat opened, a click on the chat beside it).
    pub(in crate::ui) fn close_sidebar_drawer(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .frame
            .sidebar
            .close_drawer(animate(), self.frame.sidebar.now())
        {
            self.release_sidebar_focus(window, cx);
            cx.notify();
        }
    }

    fn hold_sidebar_drawer(&mut self, cx: &mut Context<Self>) {
        if self
            .frame
            .sidebar
            .hold_drawer(animate(), self.frame.sidebar.now())
        {
            cx.notify();
        }
    }

    /// The keyboard leaves a column that goes off screen for the composer.
    fn release_sidebar_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let search = self.search_ui.input.read(cx).focus_handle(cx);
        if self.focus_sidebar.contains_focused(window, cx) || search.is_focused(window) {
            self.composer
                .update(cx, |input, cx| input.focus(window, cx));
        }
    }

    /// Something opened from the peeking column still needs it: a context
    /// menu, or the search field has the keyboard.
    fn sidebar_peek_held(&self, window: &Window, cx: &App) -> bool {
        self.context_menu_open()
            || self
                .search_ui
                .input
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
    }

    /// Once per frame, before the layout: a peek left long enough ago ends,
    /// and the clock runs while anything slides.
    pub(super) fn settle_sidebar(
        &mut self,
        layout: &ShellLayout,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let now = self.frame.sidebar.now();
        // Out of the drawer band a held drawer is gone at once.
        if !layout.drawer() {
            self.frame.sidebar.leave_drawer_band(now);
        }
        if self.sidebar_hideable(layout) {
            let held = self.sidebar_peek_held(window, cx);
            self.frame.sidebar.settle(held, animate(), now);
        } else {
            // Narrowed to one column: nothing peeks.
            self.frame.sidebar.dismiss(false, now);
        }
        if self.frame.sidebar.moving(now) {
            self.request_animation_tick(60, cx);
        }
    }

    fn sidebar_pointer_entered(&mut self, cx: &mut Context<Self>) {
        if self
            .frame
            .sidebar
            .pointer_entered(animate(), self.frame.sidebar.now())
        {
            cx.notify();
        }
    }

    /// The pointer moved over the window outside the peeking column (which
    /// blocks the mouse from what is under it): start the grace period.
    pub(super) fn sidebar_pointer_moved_away(&mut self, cx: &mut Context<Self>) {
        // The wake is held by the `Leaving` stage: dropped with it.
        let now = self.frame.sidebar.now();
        self.frame.sidebar.pointer_left(now, || {
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(LEAVE_GRACE + Duration::from_millis(10))
                    .await;
                let _ = this.update(cx, |_, cx| cx.notify());
            })
        });
    }

    /// The chat list column in the layout: whole, narrowed while it slides
    /// (the list keeps its width and slides out to the left), or gone.
    pub(super) fn docked_sidebar(&self, layout: &ShellLayout, cx: &App) -> Option<AnyElement> {
        let rail = self.folder_rail_width();
        let full = layout.gutter + layout.list? + rail;
        let visible = layout.list_column(rail);
        if visible <= 0. {
            return None;
        }
        if visible >= full {
            return Some(self.sidebar_slot(cx));
        }
        Some(
            div()
                .id("sidebar-docking")
                .relative()
                .flex_none()
                .h_full()
                .w(px(visible))
                .overflow_hidden()
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(px(visible - full))
                        .w(px(full))
                        .child(self.sidebar_slot(cx)),
                )
                .into_any_element(),
        )
    }

    /// Over the conversation while the column is hidden: the reveal strip
    /// at the left edge, or the column peeking in.
    pub(super) fn sidebar_peek_layer(
        &self,
        layout: &ShellLayout,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.sidebar_hideable(layout) || !layout.list_column_hidden() {
            return None;
        }
        let full = layout.gutter + layout.list? + self.folder_rail_width();
        // A drawer with no chat open is shown: there is nothing else to see.
        let empty_drawer =
            layout.drawer() && self.session().is_none_or(|s| s.open_chat_id().is_none());
        let (pointer, shown) = if empty_drawer {
            (Peek::Held, 1.)
        } else {
            (
                self.frame.sidebar.pointer(),
                self.frame.sidebar.peek(self.frame.sidebar.now()),
            )
        };
        if pointer == Peek::Off && shown <= 0. {
            return Some(
                div()
                    .id("sidebar-reveal-edge")
                    .debug_selector(|| "sidebar-reveal-edge".into())
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .w(px(REVEAL_EDGE_WIDTH))
                    .block_mouse_except_scroll()
                    .on_mouse_move(cx.listener(|this, _: &MouseMoveEvent, _, cx| {
                        this.sidebar_pointer_entered(cx);
                    }))
                    // In the drawer band a click at the edge holds it open.
                    .when(layout.drawer(), |this| {
                        this.on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.hold_sidebar_drawer(cx);
                        }))
                    })
                    .into_any_element(),
            );
        }
        // Kit's sheet surface sliding from the left edge, or the list card
        // with its own gutter and shadow (`chrome::frames`).
        let chrome = self.chrome(cx);
        let surface = crate::ui::chrome::frames::peek_surface(
            div()
                .id("sidebar-peek")
                .debug_selector(|| "sidebar-peek".into())
                .absolute()
                .top_0()
                .bottom_0()
                .left(px((shown - 1.) * full))
                .w(px(full)),
            &chrome,
            cx.theme(),
        )
        .child(self.sidebar_slot(cx));
        if pointer == Peek::Off {
            // Sliding out: nothing under the pointer brings it back.
            return Some(surface.into_any_element());
        }
        if empty_drawer {
            return Some(surface.occlude().into_any_element());
        }
        if pointer == Peek::Held {
            // A click on the chat beside the held drawer closes it.
            return Some(
                DismissLayer::new(
                    "sidebar-drawer-layer",
                    cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.close_sidebar_drawer(window, cx);
                    }),
                )
                // Floating's drawer dims the chat behind it as it slides
                // in, so its edge never cuts through the bubbles and pills.
                .layer(
                    |layer| match crate::ui::chrome::frames::drawer_scrim(&chrome, shown) {
                        Some(scrim) => layer.bg(scrim),
                        None => layer,
                    },
                )
                .content(surface)
                .into_any_element(),
            );
        }
        // The whole column's width takes the pointer from the first frame,
        // so a quick move into it does not count as leaving.
        Some(
            div()
                .id("sidebar-peek-area")
                .absolute()
                .top_0()
                .bottom_0()
                .left_0()
                .w(px(full))
                .occlude()
                .on_mouse_move(cx.listener(|this, _: &MouseMoveEvent, _, cx| {
                    this.sidebar_pointer_entered(cx);
                }))
                .child(surface)
                .into_any_element(),
        )
    }

    /// Hide Sidebar / Show Sidebar, leading the chat header in both states
    /// so it stays under the pointer (the chat list's header has no room
    /// for it without dropping its tab labels).
    pub(in crate::ui) fn sidebar_toggle_button(&self, cx: &mut Context<Self>) -> Option<Button> {
        if !self.sidebar_hideable(&self.shell_layout()) {
            return None;
        }
        let closed = if self.shell_layout().drawer() {
            self.frame.sidebar.pointer() != Peek::Held
        } else {
            self.frame.sidebar.hidden()
        };
        let (icon, label) = if closed {
            (IconName::PanelLeftOpen, tr(keys::Q_CHAT_SHOW_SIDEBAR))
        } else {
            (IconName::PanelLeftClose, tr(keys::Q_CHAT_HIDE_SIDEBAR))
        };
        Some(
            Button::new("sidebar-toggle")
                .icon(icon)
                .ghost()
                .tooltip_with_action(label, &ToggleSidebar, None)
                .accessibility_label(label)
                .on_click(cx.listener(|this, _, window, cx| this.toggle_sidebar(window, cx))),
        )
    }
}
