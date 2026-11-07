//! What on screen acts for the account on screen ends when that account
//! changes. A box, a viewer or a mini app keeps the ids of the account it
//! was opened on but sends through `self.live`, so one left open across
//! `switch_account` would act as the account switched to: a delete box
//! would delete another message, a mini app would share the other number
//! or pay from the other card. tdesktop ties these to the session's window
//! controller, which goes away with the session.
//!
//! Every switch runs [`QuillApp::end_account_surfaces`] before the client
//! changes. A new account-bound surface ends here, or gives its dialog a
//! close of its own (`DialogSpec::closes`), which this runs.

use super::app::QuillApp;
use super::shell::QuillShell;
use gpui_kit::component::WindowExt;
use gpui_kit::{Context, Window};

/// Dialogs with a close of their own that do not belong to the account on
/// screen: another account's ringing call (the switch may be its own
/// Answer) and the app-wide passcode.
const KEPT_ON_SWITCH: &[&str] = &["BackgroundCall", "Passcode"];

impl QuillApp {
    /// End every surface bound to the account on screen. Runs while
    /// `self.live` is still that account's client, so what a surface tells
    /// TDLib on close (the mini app's `closeWebApp`, the story's
    /// `closeStory`) goes to the client it was opened on.
    pub(super) fn end_account_surfaces(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_mini_apps_for_switch(cx);
        self.leave_chat_for_switch(window, cx);
        // Checked ids are the old account's chats; the bulk actions would
        // act on the new account's chats with the same ids.
        self.chat_list.selected.clear();
        // The bar's track and its next / previous are the old account's
        // messages.
        self.close_player(cx);
        self.playback.player.chat = None;
        self.playback.player.meta = None;
        // An upload still running belongs to the old client; its answer
        // never reaches the new one.
        self.notify.tone_upload_target = None;
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
        // The whole close: the reply, the privacy editor and the report
        // belong to the old account's story too.
        self.close_story_viewer(cx);
        self.close_video_pip(cx);
        QuillShell::close_dialogs_for_switch(self, KEPT_ON_SWITCH, window, cx);
        self.close_main_window_dialogs(window, cx);
        cx.notify();
    }

    /// The main window leaves its chat as it does for any other leave
    /// (`leave_open_chat`): the composer's text is saved as the old
    /// account's draft, through its own client, and the composer, its edit
    /// and reply, and a recording still running go with it. Without this
    /// the text stayed in the composer for the next account to send.
    fn leave_chat_for_switch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let outer = self.windows.scope;
        if self.set_window_scope(None) {
            self.leave_open_chat(window, cx);
        }
        self.set_window_scope(outer);
        // A recording in a chat window, or one whose camera is starting.
        if self.recording_busy() {
            self.cancel_recording(cx);
        }
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
