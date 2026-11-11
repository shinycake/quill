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
//!
//! What still runs while locked follows one rule (the decision doc's "What
//! runs while locked"): ingest, delivery and teardown go on; work that would
//! show or start something waits for the unlock (`lock_waits`, the queued
//! `wait_for_unlock` intents) or is refused, never dropped without a word.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use gpui_kit::*;

impl QuillApp {
    /// Quill is locked: nothing may open or start for the account now. A
    /// link waits for the unlock; a recording or a mini app is refused.
    pub(in crate::ui) fn refused_while_locked(&self) -> bool {
        self.account.passcode.is_locked()
    }

    /// The one rule for `session.app_active` (docs/decisions/
    /// lock-is-a-render-state.md section 4): a locked Quill is not looked
    /// at, whichever window has the focus. The on-screen account and the
    /// background accounts both take it from here.
    pub(in crate::ui) fn looked_at(&self, window_focused: bool) -> bool {
        window_focused && !self.refused_while_locked()
    }

    /// Locked: the pumps that turn a stored request or answer into
    /// something shown or started do not run, and what they would drain
    /// stays where it is for the first pass after the unlock. The main
    /// frame's upkeep, the poll's drains and the parked plays ask it
    /// (`scripts/lock_surfaces.py` checks those bodies).
    pub(in crate::ui) fn lock_waits(&self) -> bool {
        self.refused_while_locked()
    }

    /// Lock or unlock: the one writer of the flag. Every window draws again
    /// (`lock_layer`), the agent socket pauses or resumes, and the links
    /// held for the unlock are taken again.
    pub(in crate::ui) fn set_locked(&mut self, locked: bool, cx: &mut Context<Self>) {
        self.account.passcode.set_lock_flag(locked);
        quill::wake::PASSCODE.poke();
        quill::wake::CHROME.poke();
        if !locked {
            quill::wake::LINKS.poke();
        }
        if locked {
            // The lock layer draws no toast, so none can report its hover end.
            self.connection
                .toasts
                .pointer_gone(std::time::Instant::now());
        }
        super::lock_layer::publish(locked, cx);
        // The demo apps share the process with the tests: only a real
        // lock pauses the agent socket.
        if !self.account.passcode.demo {
            quill::agent::access::set_paused_by_lock(locked);
        }
        cx.notify();
    }

    /// Locked, outside the demos: a connection would fail on the cleared
    /// master key, so it is marked to start at the unlock instead.
    pub(in crate::ui) fn connect_waits_for_unlock(&mut self) -> bool {
        let waits = self.refused_while_locked() && !self.account.passcode.demo;
        if waits {
            self.account.passcode.connect_at_unlock = self.current_account();
        }
        waits
    }

    /// A lock asked while a passcode job ran lands now that it answered.
    /// After a Turn off there is no passcode, so nothing locks.
    pub(super) fn lock_held_for_job(&mut self, job: super::PasscodeJob, cx: &mut Context<Self>) {
        if job == super::PasscodeJob::RunningThenLock {
            self.lock_by_passcode(cx);
        }
    }

    /// The unlock: what waited for it runs. The connection deferred at a
    /// locked start or under the lock opens, the background accounts that
    /// could not open on the cleared key start, the pumps run at the next
    /// pass (`lock_waits`), and the queued intents run now.
    pub(super) fn resume_after_unlock(&mut self, cx: &mut Context<Self>) {
        // A switch under the lock (a ringing call's Answer) may have put a
        // background client on screen since: that client stays, and the
        // call answered on it with it.
        let held = self.account.passcode.connect_at_unlock.take();
        if held.is_some() && held == self.current_account() && self.live.is_none() {
            self.start_connection(cx);
        }
        self.ensure_background_accounts(cx);
        // The OS media controls come back with what is playing.
        self.publish_now_playing();
        quill::wake::UPDATES.poke();
        self.run_unlock_intents(cx);
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
            // A held press on a history row never sees its release behind
            // the lock screen: its drag ends here, and its poll loop with it.
            app.message_ui.selection_drag = None;
            // And its inline clips: a locked chat window runs no frame
            // upkeep, so a round video playing there kept its sound.
            app.players.inline_videos.leave_window(app.windows.scope);
            // Its message widgets too: a locked window draws no history
            // pass to say they left, so their runtimes would keep running
            // and their public data being polled behind the lock screen.
            app.widgets_left_window(app.windows.scope);
        });
        // A clip parked to play goes with the clips; a voice note or song
        // parked for its download plays on, as tdesktop's player does.
        self.drop_play_intents_for_lock();
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
        // A replayed intro clip is drawn only by the unlocked window: its
        // curtain, volume fade and pause stop running under the lock, so
        // the clip would play its sound on behind it. It ends here.
        if self.first_run.intro_player().is_some() || self.first_run.awaits_intro() {
            self.first_run.finish();
        }
        // The device-linking scanner is a helper window with the camera on.
        self.clear_device_qr();
        // A passkey ceremony box closes and its key is told to stop; its
        // PIN sheet would take focus from the passcode field.
        self.close_passkey_ceremony_for_lock();
        // A profile or group photo's camera panel: the camera and its light
        // go off, and a start still on its way is dropped when it lands.
        self.close_cameras_for_lock(cx);
        // A key capture would take the first key of the passcode as a
        // binding: Settings > Shortcuts and the push-to-talk key.
        self.preferences.keybinding_capture = None;
        self.group_call.ptt_capture = false;
        // Now Playing names the sender or the song: the OS controls go off.
        self.publish_now_playing();
        // A locked window is not looked at: every chat window's chat
        // notifies again (`shown_in_active_window`).
        // A chat window's locked frame does not mark it active again
        // (`render_locked`).
        if let Some(session) = self.session_mut() {
            session.active_window_chats.clear();
        }
        // The call windows stay answerable, but their boxes and secrets go
        // (tdesktop `hidePanelLayers`).
        self.close_call_layers_for_lock(cx);
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
