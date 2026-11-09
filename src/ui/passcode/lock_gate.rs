//! What the passcode lock reaches, in one place.
//!
//! The lock is a render state (docs/decisions/lock-is-a-render-state.md):
//! while `locked`, every window draws the lock screen or nothing
//! (`lock_layer`), so nothing drawn can show the account or take a click.
//! What is not drawn is here. `set_locked` is the one writer of the flag.
//! `close_for_lock` stops what keeps running without being drawn: devices,
//! helper windows and processes, captures of the next key. And every path
//! that starts an effect or takes input outside a window's elements asks
//! one gate, `refused_while_locked`: keys the app intercepts, OS menus,
//! notification clicks, account switches, links, recordings, the camera,
//! mini apps and the agent socket (`quill::agent::access`). tdesktop closes
//! its additional windows when it locks (`closeAdditionalWindows`) and
//! holds start links until it unlocks (`checkStartUrls`).
//! `scripts/lock_surfaces.py` checks that every such starter in the crate
//! asks the gate or is listed in `scripts/lock-surfaces.txt` with what the
//! lock does to it.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use gpui_kit::*;

impl QuillApp {
    /// Quill is locked: nothing may open or start for the account now. A
    /// link waits for the unlock; a recording or a mini app is refused.
    pub(in crate::ui) fn refused_while_locked(&self) -> bool {
        self.account.passcode.is_locked()
    }

    /// Lock or unlock: the one writer of the flag. Every window draws again
    /// (`lock_layer`), the agent socket pauses or resumes, and the links
    /// held for the unlock are taken again.
    pub(in crate::ui) fn set_locked(&mut self, locked: bool, cx: &mut Context<Self>) {
        self.account.passcode.locked = locked;
        quill::wake::PASSCODE.poke();
        quill::wake::CHROME.poke();
        if !locked {
            quill::wake::LINKS.poke();
        }
        super::lock_layer::publish(locked, cx);
        // The demo apps share the process with the tests: only a real
        // lock pauses the agent socket.
        if !self.account.passcode.demo {
            quill::agent::access::set_paused_by_lock(locked);
        }
        cx.notify();
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
            // Every window's context menus go too.
            app.dismiss_context_menus(cx);
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
        // A profile or group photo's camera panel: the camera and its light
        // go off, and a start still on its way is dropped when it lands.
        self.close_cameras_for_lock(cx);
        // A key capture would take the first key of the passcode as a
        // binding: Settings > Shortcuts and the push-to-talk key.
        self.preferences.keybinding_capture = None;
        self.group_call.ptt_capture = false;
    }
}

/// `intercept_keystrokes` for the app's own key handling (shortcuts, the
/// viewer's keys, a binding capture, the composer's keys): the handler runs
/// only while Quill is unlocked, so every key typed on the lock screen
/// reaches the passcode field and nothing else. The one way the app's
/// interceptors are registered (`scripts/lock_surfaces.py`).
pub(in crate::ui) fn intercept_keys_unlocked(
    cx: &mut Context<QuillApp>,
    handler: impl Fn(&KeystrokeEvent, &mut Window, &mut App) + 'static,
) -> Subscription {
    let app = cx.weak_entity();
    cx.intercept_keystrokes(move |event, window, cx| {
        let locked = app
            .read_with(cx, |app, _| app.refused_while_locked())
            .unwrap_or(true);
        if !locked {
            handler(event, window, cx);
        }
    })
}
