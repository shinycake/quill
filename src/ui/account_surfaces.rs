//! What on screen acts for the account on screen ends when that account
//! changes. A box, a viewer or a mini app keeps the ids of the account it
//! was opened on but sends through `self.live`, so one left open across
//! `switch_account` would act as the account switched to: a delete box
//! would delete another message, a mini app would share the other number
//! or pay from the other card. tdesktop ties these to the session's window
//! controller, which goes away with the session.
//!
//! Every switch runs [`QuillApp::end_account_surfaces`] before the client
//! changes. A dialog says what a switch does to it when it registers
//! (`shell::OnSwitch`), and so does an Escape layer
//! (`esc_stack::LayerSwitch`); the ones that close are closed here. Work
//! still running when the switch lands checks its account itself
//! (`account_job::StartedOn`).

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
        // Overlays and dialogs of the state still on `QuillApp` (the
        // dialogs phase of the design) close the way they always did; what
        // is on the screen says its goodbyes and is dropped with it.
        self.close_switch_surfaces(window, cx);
        self.farewell_screen(window, cx);
        // Again: what the leave ended can uncover another.
        self.close_switch_surfaces(window, cx);
        self.close_main_window_dialogs(window, cx);
        // The old account's "new login prevented" notice is not the new
        // account's.
        self.auth_ui.login_prevented = None;
        self.replace_screen(window, cx);
    }

    /// Close every Escape layer and every dialog that closes on a switch
    /// (`LayerSwitch::Closes`, `OnSwitch::Closes`). The layers go first:
    /// a dialog can hide behind one (the sticker box behind the photo
    /// editor).
    fn close_switch_surfaces(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_layers_for_switch(window, cx);
        QuillShell::close_dialogs_for_switch(self, window, cx);
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

/// Whether a surface is open, and its own close.
pub(super) type Surface = (
    fn(&QuillApp) -> bool,
    fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>),
);

/// Close every surface in `surfaces` that is open, then look again until
/// none is. A gate can be false while another surface shows (the giveaway
/// behind its checkout, the gift market behind Premium) and true once that
/// one has closed, and a close that steps back one page leaves the next
/// page open. Bounded, so a close that never clears its flag cannot hang
/// the switch.
pub(super) fn close_open_surfaces(
    app: &mut QuillApp,
    window: &mut Window,
    cx: &mut Context<QuillApp>,
    surfaces: &[Surface],
) {
    const PASSES: usize = 8;
    for _ in 0..PASSES {
        let mut closed = false;
        for (is_open, close) in surfaces {
            if is_open(app) {
                close(app, window, cx);
                closed = true;
            }
        }
        if !closed {
            return;
        }
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "account_surfaces_tests.rs"]
mod tests;
