//! What the passcode lock reaches, in one place.
//!
//! Locking closes or stops every surface that shows the account or listens
//! to the user (`close_for_lock`), and nothing new opens or starts while it
//! lasts (`refused_while_locked`). tdesktop closes its additional windows
//! when it locks (`closeAdditionalWindows`) and holds start links until it
//! unlocks (`checkStartUrls`). `scripts/lock_surfaces.py` checks both
//! halves: every function that opens an OS window, starts a helper process
//! or starts a recording is listed in `scripts/lock-surfaces.txt` with the
//! closer this function calls, or asks `refused_while_locked` itself.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use gpui_kit::*;

impl QuillApp {
    /// Quill is locked: nothing may open or start for the account now. A
    /// link waits for the unlock; a recording or a mini app is refused.
    pub(in crate::ui) fn refused_while_locked(&self) -> bool {
        self.account.passcode.locked
    }

    /// Close or stop what the lock screen does not cover or must not keep
    /// running. Every surface the lock reaches is called from here.
    pub(super) fn close_for_lock(&mut self, cx: &mut Context<Self>) {
        // The microphone or camera stops, and nothing is sent: a round
        // video reaching its time limit would go out by itself.
        if self.recording_busy() {
            self.cancel_recording(cx);
        }
        // tdesktop closes the media viewer and call panels when it locks:
        // every window's viewer, so its video stops and leaves full screen.
        self.in_every_window(cx, |app, cx| {
            if app.viewer.state.is_open() {
                app.close_media_viewer(cx);
            }
        });
        self.release_story_viewer(cx);
        // The mini app is a helper window the lock screen does not cover;
        // tdesktop closes such windows on lock (`closeAdditionalWindows`).
        self.close_mini_app_window(cx);
        self.close_context_menus(cx);
        // Notification windows on screen show senders and text (tdesktop
        // redraws them hidden on lock).
        self.close_all_notify_windows(cx);
        // tdesktop drops Picture-in-Picture on lock (`passcodeLockChanges`
        // in `switchToPip`); this runs with the main window minimized or
        // hidden too. A chat window's viewer keeps its own link.
        self.close_every_video_pip(cx);
        // The device-linking scanner is a helper window with the camera on.
        self.clear_device_qr();
    }
}
