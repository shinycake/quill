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
//! A chat window the active window (the main one or another chat window)
//! hides whole holds still the same way: no redraws for the app's changes
//! or ticks, no clip frames, until it shows again.
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
use super::super::frame_clock::{AppGate, AppTick, assume_active};
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

    /// The chat windows the active window, the main one or a chat window,
    /// hides whole, as of their last places: their own content holds still
    /// too, as the main window's does ([`Self::main_covered`]).
    fn covered_chat_windows(&self, main_active: bool) -> Vec<WindowId> {
        if hover_activates() {
            return Vec::new();
        }
        let front = if main_active {
            self.main_place
        } else {
            self.open.iter().find_map(|open| {
                let slot = open.slot.borrow();
                slot.place.filter(|_| slot.active)
            })
        };
        let Some(front) = front else {
            return Vec::new();
        };
        self.open
            .iter()
            .filter(|open| {
                let slot = open.slot.borrow();
                !slot.active && slot.place.is_some_and(|place| front.covers(&place))
            })
            .map(|open| open.handle.window_id())
            .collect()
    }

    /// Whether `view` is the view of a chat window the app's gate holds.
    pub(in crate::ui) fn holds_view(&self, view: EntityId, cx: &App) -> bool {
        self.open.iter().any(|open| {
            open.view.entity_id() == view && AppGate::holds(cx, open.handle.window_id())
        })
    }

    /// `chat_id`'s window is active or not and stands at `place`. Returns
    /// whether its activation changed.
    pub(super) fn note_place(
        &self,
        chat_id: ChatId,
        active: bool,
        place: Option<WindowPlace>,
    ) -> bool {
        let Some(slot) = self.slot_for(chat_id) else {
            return false;
        };
        let mut slot = slot.borrow_mut();
        slot.place = place;
        std::mem::replace(&mut slot.active, active) != active
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
        let uncovered = self.frame.main_covered.replace(covered) && !covered;
        // The app came to the front through this window: the other windows'
        // content asks for ticks again once redrawn.
        let was_active = self.frame.window_active.get();
        let woke = self.follow_windows(cx) && !was_active;
        if uncovered {
            cx.notify();
        } else if woke {
            self.tick_app(cx);
        }
    }

    /// Animations run while any Quill window is active, a chat window's
    /// included (`frame.window_active`), as of the main window's activation
    /// (`frame.main_active`) and the chat windows', each noted as it
    /// changes, whether or not its window draws: a minimized main window
    /// draws nothing, so its render alone left the gate as its own
    /// deactivation set it, and a chat window it had paused stayed paused.
    /// Run at each activation change of a window (its observer, or a chat
    /// window's render that finds it changed), at the main window's render,
    /// and when a chat window closes. The inline players follow the gate
    /// and the main window's cover. Returns the gate.
    pub(in crate::ui) fn follow_windows(&self, cx: &mut App) -> bool {
        let active = self.frame.main_active.get() || self.windows.any_active();
        let covered = self.frame.main_covered.get();
        let videos = &self.playback.inline_videos;
        if self.frame.window_active.replace(active) != active {
            videos.set_window_active(active);
        }
        videos.set_main_covered(covered, cx);
        let chats = self
            .windows
            .covered_chat_windows(self.frame.main_active.get());
        // For the clips' frame requests (`request_video_frame`) and the
        // chat windows' redraws (`follow_app_unless_held`).
        let held: Vec<WindowId> = self
            .windows
            .main
            .filter(|_| covered)
            .map(|main| main.window_id())
            .into_iter()
            .chain(chats.iter().copied())
            .collect();
        let before = cx.try_global::<AppGate>().cloned();
        let gate = AppGate { active, held };
        if before.as_ref() != Some(&gate) {
            cx.set_global(gate);
            // A chat window that shows again draws what it skipped.
            let was_held = before.map(|gate| gate.held).unwrap_or_default();
            for open in self.windows.open.iter() {
                let id = open.handle.window_id();
                if was_held.contains(&id) && !chats.contains(&id) {
                    let _ = open.view.update(cx, |_, cx| cx.notify());
                }
            }
        }
        active
    }

    /// The main window moved or was resized: a chat window it hides, or no
    /// longer hides, while it is the active one follows at once.
    pub(in crate::ui) fn main_window_placed(&mut self, window: &Window, cx: &mut Context<Self>) {
        let main_active = window.is_window_active() || assume_active();
        self.note_main_place(window, main_active);
        self.follow_windows(cx);
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
