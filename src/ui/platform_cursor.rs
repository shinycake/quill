//! The one cursor call Quill makes that GPUI's `App` does not expose:
//! hide the cursor until the mouse moves (the platform's
//! `hide_cursor_until_mouse_moves`, implemented on macOS, Windows, X11
//! and Wayland). `ZoomPlatform` (`interface_zoom.rs`) wraps the OS
//! platform anyway, so it hands a weak handle here when it is made.
#![deny(clippy::indexing_slicing)]

use gpui_kit::Platform;
use std::cell::Cell;
use std::rc::{Rc, Weak};

thread_local! {
    /// The OS platform, set once at startup.
    static OS_PLATFORM: Cell<Option<Weak<dyn Platform>>> = const { Cell::new(None) };
}

/// Keep a weak handle to the OS platform (called by `ZoomPlatform::new`).
pub(crate) fn remember(platform: &Rc<dyn Platform>) {
    OS_PLATFORM.with(|slot| slot.set(Some(Rc::downgrade(platform))));
}

/// Hide the mouse cursor until it next moves. Does nothing without the
/// OS platform (tests).
pub(crate) fn hide_until_mouse_moves() {
    let platform = OS_PLATFORM.with(|slot| {
        let held = slot.take();
        let platform = held.as_ref().and_then(Weak::upgrade);
        slot.set(held);
        platform
    });
    if let Some(platform) = platform {
        platform.hide_cursor_until_mouse_moves();
    }
}
