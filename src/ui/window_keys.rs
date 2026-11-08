//! The keys every Quill window's state reads before the key bindings do:
//! context menus, the media viewer's playback keys and the composer's
//! pop-up menus (Escape, the arrows, Tab).
//!
//! GPUI runs a keystroke interceptor for every window of the app. The
//! state in place when it runs is the window's own (the key scope
//! interceptor in `chat_window::MainWindow::new` runs first): a chat
//! window's slot, or the main window's for the main window and for the
//! app's other windows (Settings, notification popups, call windows).
//! Those other windows draw none of this state, so their keys are left to
//! them; a chat window sees only the menus it draws itself
//! (`QuillApp::context_menu_open`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::component::WindowExt;
use gpui_kit::*;

/// The window a key landed in, as far as the app's state goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum KeyWindow {
    /// The main window: its own state is in place.
    Main,
    /// A chat window: its slot is in place.
    Chat,
    /// Another window (Settings, a notification popup, a call window):
    /// the main window's state is in place, and none of it is drawn there.
    Other,
}

impl QuillApp {
    /// Which of the app's windows `window` is.
    pub(in crate::ui) fn key_window(&self, window: &Window) -> KeyWindow {
        let id = window.window_handle().window_id();
        if self.window_scope_of(window).is_some() {
            KeyWindow::Chat
        } else if self
            .main_window
            .handle
            .is_some_and(|main| main.window_id() == id)
        {
            KeyWindow::Main
        } else {
            KeyWindow::Other
        }
    }
}

/// Registers the interceptor. Registered after the key scope interceptor
/// and the agent's Escape, before the shortcut capture's.
pub(in crate::ui) fn intercept_window_keys(cx: &mut Context<QuillApp>) {
    let menu_app = cx.weak_entity();
    cx.intercept_keystrokes(move |event, window, cx| {
        // Shortcut capture owns the key. This interceptor is registered
        // first so it observes `keybinding_capture` before the capture
        // handler clears it.
        let (capturing, here) = menu_app
            .update(cx, |this, _| {
                (this.keybinding_capture_active(), this.key_window(window))
            })
            .unwrap_or((false, KeyWindow::Other));
        // A window that draws none of the app's menus, viewer or composer
        // keeps its keys (a popup's reply field, a Settings field).
        if here == KeyWindow::Other {
            return;
        }
        if !capturing {
            let menu_handled = menu_app
                .update(cx, |this, cx| {
                    this.context_menu_key(&event.keystroke, window, cx)
                })
                .unwrap_or(false);
            if menu_handled {
                cx.stop_propagation();
                return;
            }
        }
        // Viewer playback keys (Space/K/J/L/Enter, tdesktop
        // `handleKeyPress`). A dialog over the viewer (delete
        // confirmation) keeps its own Enter and Space.
        if !capturing && !window.has_active_dialog(cx) {
            let viewer_handled = menu_app
                .update(cx, |this, cx| {
                    this.handle_viewer_key(&event.keystroke, window, cx)
                })
                .unwrap_or(false);
            if viewer_handled {
                cx.stop_propagation();
                return;
            }
        }
        if capturing || event.keystroke.modifiers.modified() {
            return;
        }
        let handled = match event.keystroke.key.as_str() {
            "escape" => menu_app
                .update(cx, |this, cx| {
                    this.close_context_menus(cx)
                        || this.close_media_viewer_on_escape(cx)
                        || this.close_media_panel(cx)
                        || this.close_inline_results(cx)
                        || this.close_mention_menu(cx)
                        || this.close_suggest_menu(true, cx)
                        || this.close_command_menu(cx)
                        || this.dismiss_sticker_suggestions(window, cx)
                })
                .unwrap_or(false),
            "up" => menu_app
                .update(cx, |this, cx| {
                    this.step_inline_results(-1, cx)
                        || this.step_mention_menu(-1, cx)
                        || this.step_suggest_menu(-1, false, cx)
                        || this.step_command_menu(-1, cx)
                })
                .unwrap_or(false),
            "down" => menu_app
                .update(cx, |this, cx| {
                    this.step_inline_results(1, cx)
                        || this.step_mention_menu(1, cx)
                        || this.step_suggest_menu(1, false, cx)
                        || this.step_command_menu(1, cx)
                })
                .unwrap_or(false),
            // Tab completes the highlighted `@` / `#` / `:` suggestion.
            "tab" => menu_app
                .update(cx, |this, cx| {
                    this.pick_mention_selection(window, cx)
                        || this.pick_suggest_selection(true, window, cx)
                        || this.insert_command_menu_selection(window, cx)
                })
                .unwrap_or(false),
            // The emoji strip is horizontal (tdesktop steps with
            // Left/Right too); the key keeps moving the caret otherwise.
            "left" => menu_app
                .update(cx, |this, cx| this.step_suggest_menu(-1, true, cx))
                .unwrap_or(false),
            "right" => menu_app
                .update(cx, |this, cx| this.step_suggest_menu(1, true, cx))
                .unwrap_or(false),
            _ => false,
        };
        if handled {
            cx.stop_propagation();
        }
    })
    .detach();
}
