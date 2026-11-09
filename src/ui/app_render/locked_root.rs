//! The main window while Quill is locked: the lock screen and nothing else
//! (docs/decisions/lock-is-a-render-state.md). No chat list, chat, menu,
//! popover or deferred overlay is built, so none can paint over the lock
//! screen or take a click; the window's root layer drops the kit's layers
//! the same way (`passcode::lock_layer`).

use super::super::actions::{CloseWindow, MinimizeWindow, QuitApp, ToggleFullscreen, ZoomWindow};
use super::super::app::QuillApp;
use super::super::chat_window::WindowScope;
use gpui_kit::*;
use quill::agent::ui_driver::guard::{self, HardSurface};

impl QuillApp {
    /// The root the main window draws while locked. The window's own
    /// commands stay (Quit, Close, Minimize, Zoom, Full Screen), as
    /// tdesktop's do on its lock screen; nothing that shows or changes the
    /// account is bound.
    pub(super) fn locked_root(
        &mut self,
        outer_scope: Option<quill::ids::ChatId>,
        cx: &mut Context<Self>,
    ) -> WindowScope {
        let root = div()
            .id("main-window-locked")
            .size_full()
            .relative()
            // Input in the main window puts its own state in place, so the
            // passcode typed here is this window's field.
            .child(super::super::chat_window::scope_probe(
                cx.weak_entity(),
                None,
            ))
            .on_action(cx.listener(|this, _: &QuitApp, window, cx| {
                if guard::refuse_hard_if_injecting(HardSurface::Window) {
                    return;
                }
                this.exit_after_exports(super::super::export_quit::quit_app, window, cx);
            }))
            .on_action(cx.listener(|this, _: &CloseWindow, window, cx| {
                if this.close_main_window(window, cx) {
                    window.remove_window();
                }
            }))
            .on_action(cx.listener(|this, _: &MinimizeWindow, window, cx| {
                if quill::tray::minimize_hides_main_window(
                    this.minimize_to_tray(),
                    quill::tray::tray_available(),
                    cfg!(target_os = "macos"),
                ) {
                    crate::ui::link_inbox::hide_main_window(window, cx);
                } else {
                    window.minimize_window();
                }
            }))
            .on_action(|_: &ZoomWindow, window, _| window.zoom_window())
            .on_action(|_: &ToggleFullscreen, window, _| window.toggle_fullscreen())
            .child(self.lock_overlay(cx))
            .children(self.background_ring_lock_card(cx));
        self.set_window_scope(outer_scope);
        WindowScope::new(cx.weak_entity(), None, root.into_any_element())
    }
}
