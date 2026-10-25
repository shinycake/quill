//! The takeover state's UI side (`agent_takeover`): the wake-up for a
//! phase change nothing animates towards, and Esc while an agent drives.
//! The state itself is `quill::agent::ui_driver::takeover`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::*;
use quill::agent::ui_driver::{guard, takeover};
use std::cell::Cell;

pub(crate) struct TakeoverUi {
    /// When the pending wake-up fires (takeover clock, 0: none), so one
    /// timer covers each change.
    pub(super) wake_at: Cell<u64>,
}

impl TakeoverUi {
    /// Esc while an agent drives pauses it and goes nowhere else: the user
    /// is taking control, not closing a menu. Registered before the app's
    /// other keystroke interceptors (they run in order; this one stops
    /// the key reaching the key bindings).
    pub(super) fn new(_window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        cx.intercept_keystrokes(|event, _window, cx| {
            let keystroke = &event.keystroke;
            if guard::injecting()
                || keystroke.key != "escape"
                || keystroke.modifiers.modified()
                || !super::agent_state::sees_agents()
                // Esc on the agent's own prompt answers it (Deny), as on
                // any dialog.
                || super::agent_takeover::agent_prompt_open()
            {
                return;
            }
            if takeover::note_escape() {
                cx.stop_propagation();
            }
        })
        .detach();
        Self {
            wake_at: Cell::new(0),
        }
    }
}
