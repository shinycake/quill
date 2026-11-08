//! One window's passcode field on the lock screen.
//!
//! Every Quill window shows the lock screen while locked, each with a field
//! of its own (tdesktop builds a `PasscodeLockWidget` per window in
//! `Window::Controller::setupPasscodeLock`). A text field keeps the
//! geometry of its last paint, so one field painted in two windows of
//! different sizes told GPUI its layout changed at every paint, which
//! redrew the other window, whose paint changed it back: both windows
//! redrew at the display's rate for as long as the lock lasted
//! (docs/decisions/lock-field-per-window.md). The main window's field sits
//! in `PasscodeUi::lock`, a chat window's in its slot, swapped in with the
//! rest of the window's state.

use super::super::app::QuillApp;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use quill::lang::{keys, tr};

/// What the window does with its field at its next frame on the lock
/// screen (`QuillApp::passcode_frame`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum FieldTask {
    #[default]
    None,
    /// Focus it: a wrong passcode leaves the field ready for another try.
    Focus,
    /// Empty and focus it: the lock just started. Text typed in this
    /// window during an earlier lock that another window unlocked goes.
    Reset,
}

pub(crate) struct LockField {
    pub(crate) input: Entity<InputState>,
    task: FieldTask,
}

impl LockField {
    /// A field for the window `window`; Enter submits it from that
    /// window. `locked`: the app starts locked, so the field takes the
    /// focus at the first frame.
    pub(crate) fn new(window: &mut Window, cx: &mut Context<QuillApp>, locked: bool) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder(tr(keys::PASSCODE_PH).to_string())
                .submit_on_enter(true)
        });
        cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| {
            this.in_window_of(window, cx, |this, window, cx| match event {
                InputEvent::PressEnter { .. } => this.submit_unlock(window, cx),
                InputEvent::Change if this.account.passcode.lock_error.take().is_some() => {
                    cx.notify();
                }
                _ => {}
            });
        })
        .detach();
        Self {
            input,
            task: if locked {
                FieldTask::Reset
            } else {
                FieldTask::None
            },
        }
    }

    /// The lock started: empty the field and focus it at its window's
    /// next frame.
    pub(crate) fn reset(&mut self) {
        self.task = FieldTask::Reset;
    }

    /// Focus the field at its window's next frame (unless a reset waits).
    pub(crate) fn refocus(&mut self) {
        if self.task == FieldTask::None {
            self.task = FieldTask::Focus;
        }
    }

    /// Run what waits, with the field's own window drawing.
    pub(crate) fn frame(&mut self, window: &mut Window, cx: &mut App) {
        let task = std::mem::take(&mut self.task);
        if task == FieldTask::None {
            return;
        }
        self.input.update(cx, |input, cx| {
            if task == FieldTask::Reset && !input.value().is_empty() {
                input.set_value("", window, cx);
            }
            input.focus(window, cx);
        });
    }
}
