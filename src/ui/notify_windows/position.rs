//! Move a notification window: GPUI places a window when it opens but has
//! no call to move it later, and the stack slides as notifications come
//! and go (tdesktop `Widget::moveByShift`). Like `ui::window_control`,
//! this works on the native handle from `raw-window-handle`.
//!
//! `origin` is in the coordinates GPUI opened the window with: points
//! relative to the window's display on macOS, logical pixels of the
//! virtual screen on Windows and X11.
//! - macOS: `NSWindow setFrameTopLeftPoint:` (AppKit counts up from the
//!   bottom of the screen).
//! - Windows: `SetWindowPos` in device pixels, without activating.
//! - X11: `ConfigureWindow` on a connection of our own, kept for the
//!   process (a slide moves the window every frame).
//! - Wayland and others: no way to place a window; Quill never shows
//!   these windows there (`notify::custom::native_support`).

use gpui_kit::*;
#[allow(unused_imports)]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// Move the window's top-left corner to `origin`. Returns whether the
/// backend could.
pub(super) fn set_origin(window: &Window, origin: Point<Pixels>) -> bool {
    platform::set_origin(window, origin)
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use objc2_app_kit::NSView;
    use objc2_foundation::NSPoint;

    pub(super) fn set_origin(window: &Window, origin: Point<Pixels>) -> bool {
        let Ok(handle) = HasWindowHandle::window_handle(window) else {
            return false;
        };
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return false;
        };
        // SAFETY: GPUI owns this NSView for the lifetime of the window and
        // the handle is read on the main thread inside a window update.
        let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
        let Some(native) = view.window() else {
            return false;
        };
        let Some(screen) = native.screen() else {
            return false;
        };
        let frame = screen.frame();
        let point = NSPoint::new(
            frame.origin.x + f64::from(origin.x.as_f32()),
            frame.origin.y + frame.size.height - f64::from(origin.y.as_f32()),
        );
        native.setFrameTopLeftPoint(point);
        true
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOSIZE, SetWindowPos,
    };

    pub(super) fn set_origin(window: &Window, origin: Point<Pixels>) -> bool {
        let hwnd = match HasWindowHandle::window_handle(window).map(|h| h.as_raw()) {
            Ok(RawWindowHandle::Win32(handle)) => handle.hwnd.get() as HWND,
            _ => return false,
        };
        let scale = window.scale_factor();
        let x = (origin.x.as_f32() * scale).round() as i32;
        let y = (origin.y.as_f32() * scale).round() as i32;
        // SAFETY: the handle belongs to a live GPUI window for the duration
        // of this call; only the position changes (it stays topmost).
        unsafe { SetWindowPos(hwnd, HWND_TOPMOST, x, y, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE) != 0 }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{ConfigureWindowAux, ConnectionExt as _};
    use x11rb::rust_connection::RustConnection;

    fn connection() -> Option<&'static Mutex<RustConnection>> {
        static CONNECTION: OnceLock<Option<Mutex<RustConnection>>> = OnceLock::new();
        CONNECTION
            .get_or_init(|| x11rb::connect(None).ok().map(|(conn, _)| Mutex::new(conn)))
            .as_ref()
    }

    pub(super) fn set_origin(window: &Window, origin: Point<Pixels>) -> bool {
        let x_window = match HasWindowHandle::window_handle(window).map(|h| h.as_raw()) {
            Ok(RawWindowHandle::Xcb(handle)) => handle.window.get(),
            Ok(RawWindowHandle::Xlib(handle)) => match u32::try_from(handle.window) {
                Ok(id) => id,
                Err(_) => return false,
            },
            _ => return false,
        };
        let Some(conn) = connection() else {
            return false;
        };
        let conn = quill::locks::lock_or_recover(conn);
        let scale = window.scale_factor();
        let aux = ConfigureWindowAux::new()
            .x((origin.x.as_f32() * scale).round() as i32)
            .y((origin.y.as_f32() * scale).round() as i32);
        let sent = conn.configure_window(x_window, &aux).is_ok();
        sent && conn.flush().is_ok()
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
mod platform {
    use super::*;

    pub(super) fn set_origin(_window: &Window, _origin: Point<Pixels>) -> bool {
        false
    }
}
