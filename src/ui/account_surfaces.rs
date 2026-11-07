//! What on screen acts for the account on screen ends when that account
//! changes. A box, a viewer or a mini app keeps the ids of the account it
//! was opened on but sends through `self.live`, so one left open across
//! `switch_account` would act as the account switched to: a delete box
//! would delete another message, a mini app would share the other number
//! or pay from the other card. tdesktop ties these to the session's window
//! controller, which goes away with the session.
//!
//! Every switch runs [`QuillApp::end_account_surfaces`] before the client
//! changes. A new account-bound surface ends here; a dialog says what a
//! switch does to it when it registers (`shell::OnSwitch`), and the ones
//! that close are closed here.

use super::app::QuillApp;
use super::shell::QuillShell;
use gpui_kit::component::WindowExt;
use gpui_kit::{Context, Window};

impl QuillApp {
    /// End every surface bound to the account on screen. Runs while
    /// `self.live` is still that account's client, so what a surface tells
    /// TDLib on close (the mini app's `closeWebApp`, the story's
    /// `closeStory`) goes to the client it was opened on.
    pub(super) fn end_account_surfaces(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_mini_apps_for_switch(cx);
        // Boxes whose flag holds the old account's ids and that have no
        // close of their own.
        self.message_ui.pending_delete = None;
        self.message_ui.pending_stop_poll = None;
        self.message_ui.request_share = None;
        self.message_ui.request_locate = None;
        self.share.pending_forward = None;
        self.share.forward_picker_open = false;
        self.share.forward_bar_dest = None;
        self.share.content_dialog = None;
        self.close_context_menus(cx);
        if self.viewer.state.is_open() {
            self.close_media_viewer(cx);
        }
        self.release_story_viewer();
        self.close_video_pip(cx);
        QuillShell::close_dialogs_for_switch(self, window, cx);
        self.close_main_window_dialogs(window, cx);
        cx.notify();
    }

    /// Close every kit dialog in the main window now, so none can take a
    /// click before the next frame. A box opened with `open_alert_dialog`
    /// holds its ids in its own closures, with no flag to clear. A flagged
    /// dialog still open after [`QuillShell::close_dialogs_for_switch`]
    /// opens again at the next render, built from the new account's state.
    fn close_main_window_dialogs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(main) = self.windows.main else {
            return;
        };
        if main == window.window_handle() {
            window.close_all_dialogs(cx);
        } else {
            let _ = main.update(cx, |_, window, cx| window.close_all_dialogs(cx));
        }
        self.windows.main_dialogs_closed = true;
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "account_surfaces_tests.rs"]
mod tests;
