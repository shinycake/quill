//! What on screen acts for the account on screen ends when that account
//! changes. A box, a viewer or a mini app keeps the ids of the account it
//! was opened on but sends through `self.live`, so one left open across
//! `switch_account` would act as the account switched to: a delete box
//! would delete another message, a mini app would share the other number
//! or pay from the other card. tdesktop ties these to the session's window
//! controller, which goes away with the session.
//!
//! Every switch runs [`QuillApp::end_account_surfaces`] before the client
//! changes: the account's UI is one value (`ui/on_screen.rs`), which says
//! its goodbyes and is replaced whole. Work still running when the switch
//! lands is dropped by the screen epoch (`chat_window::scoped`).

use super::app::QuillApp;
use gpui_kit::component::WindowExt;
use gpui_kit::{Context, Window};

impl QuillApp {
    /// End every surface bound to the account on screen. Runs while
    /// `self.live` is still that account's client, so what a surface tells
    /// TDLib on close (the mini app's `closeWebApp`, the story's
    /// `closeStory`) goes to the client it was opened on.
    pub(super) fn end_account_surfaces(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.farewell_screen(window, cx);
        self.close_main_window_dialogs(window, cx);
        self.replace_screen(window, cx);
    }

    /// The main window leaves its chat as it does for any other leave
    /// (`leave_open_chat`): the composer's text is saved as the old
    /// account's draft, through its own client, and the composer, its edit
    /// and reply go with it. Without this the text stayed in the composer
    /// for the next account to send. `HistoryUi`'s farewell.
    pub(super) fn leave_chat_for_switch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let outer = self.windows.scope;
        if self.set_window_scope(None) {
            self.leave_open_chat(window, cx);
        }
        self.set_window_scope(outer);
    }

    /// Close every kit dialog in the main window now, so none can take a
    /// click before the next frame. A box opened with `open_alert_dialog`
    /// holds its ids in its own closures, with no flag to clear; a flagged
    /// dialog's flag goes with the screen (`replace_screen`), and one whose
    /// flag the session holds is the new account's own at the next render.
    fn close_main_window_dialogs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(main) = self.main_window.handle else {
            return;
        };
        if main == window.window_handle() {
            window.close_all_dialogs(cx);
        } else {
            let _ = main.update(cx, |_, window, cx| window.close_all_dialogs(cx));
        }
        self.main_window.dialogs_closed = true;
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "account_surfaces_tests.rs"]
mod tests;
