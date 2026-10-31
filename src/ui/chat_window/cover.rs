//! A chat window in front that hides the main window whole.
//!
//! Animations run while any Quill window is in front, a chat window's
//! included (`QuillApp::render`), so the main window's own (its chat list
//! and conversation layers, through the frame clock) kept drawing while the
//! active chat window covered all of it. macOS tells a window it is
//! occluded only when another app's window or a borderless one covers it,
//! never for a titled window of its own app, so nothing else stopped them.
//! The main window's own animations now hold still, as behind another app,
//! while the active chat window's frame holds the main window's on the same
//! display, and start again once any part of it shows (`frame_clock`).
//! Background: `docs/decisions/covered-main-window.md`.

use super::super::app::QuillApp;
use super::super::window_control::{Backend, backend};
use super::ChatWindows;
use gpui_kit::*;
use quill::ids::ChatId;

/// Where a window stands on screen, as far as covering goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct WindowPlace {
    bounds: Bounds<Pixels>,
    display: DisplayId,
}

impl WindowPlace {
    /// `window`'s place on `display`. `None` when it can't tell what
    /// covers what: off any display; fullscreen (a Space of its own on
    /// macOS, so a window on the desktop beside it may show); on Linux,
    /// where Wayland gives no window positions and an X11 window manager
    /// may focus a window without raising it.
    pub(super) fn of(window: &Window, display: Option<DisplayId>) -> Option<Self> {
        let active_in_front = matches!(
            backend(window),
            Backend::MacOS | Backend::Windows | Backend::Unknown
        );
        if !active_in_front || window.is_fullscreen() {
            return None;
        }
        Some(Self {
            bounds: window.bounds(),
            display: display?,
        })
    }

    /// Whether a window here, in front, hides all of `under`. Bounds are
    /// compared on one display only: macOS measures them from the
    /// window's screen, Windows in that monitor's scale.
    fn covers(&self, under: &Self) -> bool {
        let (top, under_bounds) = (&self.bounds, &under.bounds);
        self.display == under.display
            && top.left() <= under_bounds.left()
            && top.top() <= under_bounds.top()
            && top.right() >= under_bounds.right()
            && top.bottom() >= under_bounds.bottom()
    }
}

impl ChatWindows {
    /// Whether the active chat window hides the main window whole, as of
    /// their last places.
    fn main_covered(&self) -> bool {
        let Some(main) = self.main_place else {
            return false;
        };
        self.open.iter().any(|open| {
            let slot = open.slot.borrow();
            slot.active && slot.place.is_some_and(|place| place.covers(&main))
        })
    }

    /// `chat_id`'s window is active or not and stands at `place`.
    pub(super) fn note_place(&self, chat_id: ChatId, active: bool, place: Option<WindowPlace>) {
        if let Some(slot) = self.slot_for(chat_id) {
            let mut slot = slot.borrow_mut();
            slot.active = active;
            slot.place = place;
        }
    }
}

impl QuillApp {
    /// The main window's place at its render, and whether its own
    /// animations hold still for a chat window over it: only while it is
    /// not active itself (it is in front then).
    pub(in crate::ui) fn note_main_place(&mut self, window: &Window, main_active: bool) {
        self.windows.main_place = WindowPlace::of(window, self.notify.windows.main_display);
        self.frame
            .main_covered
            .set(!main_active && self.windows.main_covered());
    }

    /// `chat_id`'s window was activated or deactivated, moved, resized or
    /// went to another display: a main window it no longer hides draws
    /// again, and its content asks for ticks again.
    pub(super) fn chat_window_placed(
        &mut self,
        chat_id: ChatId,
        active: bool,
        place: Option<WindowPlace>,
        cx: &mut Context<Self>,
    ) {
        self.windows.note_place(chat_id, active, place);
        if self.frame.main_covered.get() && !self.windows.main_covered() {
            cx.notify();
        }
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "cover_tests.rs"]
mod tests;
