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
//! Ticks for the whole app, which any window's content may ask for, redraw
//! every other window that follows the app then ([`QuillApp::tick_app`]).
//! Background: `docs/decisions/covered-main-window.md`.
//!
//! This rests on the active window being in front of the app's others; a
//! visible main window must never hold still. Where that is not sure, the
//! main window never counts as covered:
//!
//! - Linux: Wayland gives no window positions, and an X11 window manager may
//!   focus a window without raising it.
//! - Windows, while "Activate a window by hovering over it with the mouse"
//!   is on (`SPI_GETACTIVEWINDOWTRACKING`): the window under the pointer
//!   becomes active, raised or not (`SPI_GETACTIVEWNDTRKZORDER`, which this
//!   does not trust). Read at each check, so a change in Settings counts
//!   from the next activation or move, the events that could put an active
//!   chat window behind the main window.
//! - Fullscreen windows (a Space of their own on macOS).
//!
//! macOS has no such setting. Ways it orders a window without making it key
//! (a Cmd-click or Cmd-drag on a background window) need a visible part of
//! the main window to click; Quill brings the main window forward only with
//! `makeKeyAndOrderFront` (`activate_window`, `window_control::set_visible`)
//! and never lifts it or a chat window above the normal level
//! (`set_always_on_top` is for call and Picture-in-Picture windows).
//! Switching Spaces makes a window on the new one key. A chat window that is
//! a native tab of the main window covers it while its tab is the selected
//! one, which is right: the main window's tab is hidden then.

use super::super::app::QuillApp;
use super::super::frame_clock::{AppTick, assume_active};
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
    /// covers what: off any display, fullscreen, or on Linux (see above).
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
        }) && !hover_activates()
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

/// Windows activates the window under the pointer, maybe without raising
/// it. A failed read counts as on: the main window animates as before.
#[cfg(windows)]
fn hover_activates() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SPI_GETACTIVEWINDOWTRACKING, SystemParametersInfoW,
    };
    let mut on: i32 = 0;
    // SAFETY: SPI_GETACTIVEWINDOWTRACKING writes one BOOL to the pointer.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETACTIVEWINDOWTRACKING,
            0,
            (&mut on as *mut i32).cast(),
            0,
        )
    };
    ok == 0 || on != 0
}

#[cfg(not(windows))]
fn hover_activates() -> bool {
    false
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
    /// went to another display: a main window it now hides holds still at
    /// once, and one it no longer hides draws again, its content asking for
    /// ticks again.
    pub(super) fn chat_window_placed(
        &mut self,
        chat_id: ChatId,
        active: bool,
        place: Option<WindowPlace>,
        cx: &mut Context<Self>,
    ) {
        self.windows.note_place(chat_id, active, place);
        // Asked of the platform: the main window's own activation callback
        // may run after this one.
        let main_active = assume_active()
            || self
                .windows
                .main
                .is_some_and(|main| cx.active_window() == Some(main));
        let covered = !main_active && self.windows.main_covered();
        if self.frame.main_covered.replace(covered) && !covered {
            cx.notify();
        }
    }

    /// A frame-clock tick for the whole app redraws every window that
    /// follows the app, as any change to the app does. Under the active
    /// chat window, all of them but the main one ([`AppTick`]): a call
    /// window, or a dialog in the chat window, asks for these ticks too,
    /// whichever window's state is in place. A window that draws by its own
    /// clock (Picture-in-Picture, a notification popup) and a cached view
    /// that does not follow the app keep their frame, as with the notify.
    pub(in crate::ui) fn tick_app(&self, cx: &mut Context<Self>) {
        if self.frame.main_covered.get() {
            cx.emit(AppTick);
        } else {
            cx.notify();
        }
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "cover_tests.rs"]
mod tests;
