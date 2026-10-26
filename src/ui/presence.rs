//! Batch 4: online presence. The window reports its last input to a
//! process-wide clock (an invisible canvas listening to mouse, scroll and
//! key events); the 40 ms poll loop turns "window active and not idle"
//! (`quill::presence`, tdesktop's `updateOnline`) into TDLib's `online`
//! option, sent only when it changes.
//!
//! The OS idle time is not used: while this window is active it receives
//! all of the user's input, so in-window input is the same signal on
//! macOS, Windows and Linux, without a per-platform backend.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::*;
use quill::agent::ui_driver::takeover::RealInput;
use quill::presence::should_be_online;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

fn epoch() -> Instant {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    *EPOCH.get_or_init(Instant::now)
}

/// Milliseconds since `epoch()` of the last input; the app start counts
/// as input.
static LAST_INPUT_MS: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> u64 {
    epoch().elapsed().as_millis() as u64
}

/// Record user input now.
pub(super) fn note_input() {
    LAST_INPUT_MS.store(now_ms(), Ordering::Relaxed);
}

/// Milliseconds since the last input.
pub(super) fn idle_ms() -> u64 {
    now_ms().saturating_sub(LAST_INPUT_MS.load(Ordering::Relaxed))
}

/// Input from the user. Input the agent UI driver injects is not the user
/// (it neither keeps them online nor counts as their activity); the user's
/// own input pauses an agent driving the window (`agent_takeover`).
fn note_real_input(input: RealInput, window: &Window) {
    if quill::agent::ui_driver::guard::injecting() {
        return;
    }
    note_input();
    quill::agent::ui_driver::bridge::note_real_input();
    super::agent_takeover::note_real_input(input, window);
}

/// An invisible element that notes every mouse move, press, scroll and
/// key press in the window. Capture phase, so nothing that handles an
/// event can hide it.
pub(super) fn input_probe() -> impl IntoElement {
    canvas(
        |_, _, _| (),
        |_, _, window, _| {
            window.on_mouse_event(|event: &MouseMoveEvent, phase, window, _| {
                if phase == DispatchPhase::Capture {
                    let (x, y) = (event.position.x.as_f32(), event.position.y.as_f32());
                    note_real_input(RealInput::Move { x, y }, window);
                }
            });
            window.on_mouse_event(|_: &MouseDownEvent, phase, window, _| {
                if phase == DispatchPhase::Capture {
                    note_real_input(RealInput::Press, window);
                }
            });
            window.on_mouse_event(|_: &ScrollWheelEvent, phase, window, _| {
                if phase == DispatchPhase::Capture {
                    note_real_input(RealInput::Scroll, window);
                }
            });
            window.on_key_event(|_: &KeyDownEvent, phase, window, _| {
                if phase == DispatchPhase::Capture {
                    note_real_input(RealInput::Key, window);
                }
            });
        },
    )
    .absolute()
    .size_0()
}

impl QuillApp {
    /// Send `online` when "window active and not idle" changed. Restarts
    /// of the connection (auth not Ready) forget what was sent.
    pub(super) fn sync_presence(&mut self) {
        let active = self.frame.window_active.get();
        let Some(live) = self.live.as_mut() else {
            return;
        };
        if !matches!(
            live.driver.session.auth,
            quill::telegram::envelope::AuthorizationState::Ready
        ) {
            self.connection.presence.reset();
            return;
        }
        // A locked window is not in use: offline until unlocked.
        let desired = should_be_online(active && !self.account.passcode.locked, idle_ms());
        if let Some(value) = self.connection.presence.next(desired)
            && live.driver.set_online(value).is_err()
        {
            self.connection.presence.reset();
        }
    }
}
