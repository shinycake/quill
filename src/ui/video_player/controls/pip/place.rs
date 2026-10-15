//! Placing the Picture-in-Picture window. GPUI opens a window where asked
//! but has no call to move or resize it later from both ends, and tdesktop
//! moves its PiP itself (`PipPanel::processDrag`, `moveAnimated`), so this
//! works on the native handle like `notify_windows::position`:
//!
//! - macOS: `NSWindow setFrame:display:`, in the coordinates GPUI reports
//!   (points from the top-left of the window's own screen).
//! - Windows: `SetWindowPos` in device pixels.
//! - X11: `ConfigureWindow` on a connection of our own; the frame and the
//!   pointer are read from the server too, because GPUI's bounds come
//!   from `ConfigureNotify` events, which are relative to the window
//!   manager's frame once it reparents the window.
//! - Wayland: a client can't place its window; moves and resizes go to
//!   the compositor (`start_window_move` / `start_window_resize`), as
//!   tdesktop's `startSystemDrag` does.
//!
//! A native move or resize makes the platform call GPUI back (a resize
//! re-lays the window out). GPUI drops that call while the app is busy,
//! so frames are set from a task of their own, after the event or draw
//! that asked for them returned (GPUI's own `resize` does the same).

use crate::ui::window_control;
use gpui_kit::*;
use quill::media_viewer::pip::panel::{PipPoint, PipRect};
#[allow(unused_imports)]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

pub(super) fn to_rect(bounds: Bounds<Pixels>) -> PipRect {
    let round = |value: Pixels| {
        let value = value.as_f32();
        if value.is_finite() {
            value.round() as i32
        } else {
            0
        }
    };
    PipRect::new(
        round(bounds.origin.x),
        round(bounds.origin.y),
        round(bounds.size.width),
        round(bounds.size.height),
    )
}

pub(super) fn to_bounds(rect: PipRect) -> Bounds<Pixels> {
    Bounds::new(
        point(px(rect.x as f32), px(rect.y as f32)),
        size(px(rect.w as f32), px(rect.h as f32)),
    )
}

/// The window system can't be told where a window goes (Wayland).
pub(super) fn is_wayland(window: &Window) -> bool {
    window_control::backend(window) == window_control::Backend::Wayland
}

/// The window's frame, in GPUI's screen coordinates.
pub(super) fn frame(window: &Window) -> PipRect {
    platform::frame(window).unwrap_or_else(|| to_rect(window.bounds()))
}

/// The pointer, in the same coordinates as [`frame`] (Qt's `QCursor::pos`).
pub(super) fn cursor(window: &Window) -> PipPoint {
    platform::cursor(window).unwrap_or_else(|| {
        let bounds = window.bounds();
        let at = window.mouse_position();
        to_rect(Bounds::new(bounds.origin + at, Size::default())).origin()
    })
}

/// The usable area (no taskbar, Dock or menu bar) of the screen the
/// window is on.
pub(super) fn own_screen(window: &Window, cx: &App) -> Option<PipRect> {
    window
        .display(cx)
        .map(|display| to_rect(display.visible_bounds()))
}

/// The usable area of the screen under `at` (`ScreenFromPosition`). On
/// macOS GPUI places windows relative to their own screen, so that one is
/// used.
pub(super) fn screen_at(window: &Window, cx: &App, at: PipPoint) -> Option<PipRect> {
    if cfg!(not(target_os = "macos")) {
        let under = cx.displays().into_iter().find(|display| {
            let bounds = to_rect(display.bounds());
            bounds.contains(at)
        });
        if let Some(display) = under {
            return Some(to_rect(display.visible_bounds()));
        }
    }
    own_screen(window, cx).or_else(|| {
        cx.primary_display()
            .map(|display| to_rect(display.visible_bounds()))
    })
}

/// Move and size the window to `frame`, from a task (see the module
/// docs). Nothing happens on Wayland, or once the window is gone.
pub(super) fn set_frame(window: &Window, frame: PipRect, cx: &mut App) {
    #[cfg(test)]
    tests::record(frame);
    let handle = window.window_handle();
    cx.spawn(async move |cx| {
        let target = cx
            .update_window(handle, |_, window, _| platform::target(window))
            .ok()
            .flatten();
        // The window is alive: it was found just now and nothing ran
        // since.
        if let Some(target) = target {
            platform::apply(target, frame);
        }
    })
    .detach();
}

/// Keep Picture-in-Picture above other windows, off the taskbar (a
/// `Qt::Tool` window) and on every Space (macOS), as tdesktop's
/// `PipPanel::init` does.
pub(super) fn configure(window: &Window) {
    // X11 `_NET_WM_STATE_ABOVE`, Windows `HWND_TOPMOST`, macOS the
    // floating level; Wayland has no stacking order for clients.
    window_control::set_always_on_top(window, true);
    // X11 `_NET_WM_STATE_SKIP_TASKBAR`; Windows makes it a tool window in
    // `platform::configure` instead.
    if cfg!(not(target_os = "windows")) && window_control::taskbar_toggle_supported(window) {
        window_control::set_skip_taskbar(window, true);
    }
    platform::configure(window);
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use objc2_app_kit::{NSView, NSWindowButton, NSWindowCollectionBehavior};
    use objc2_foundation::{NSPoint, NSRect, NSSize};

    #[derive(Clone, Copy)]
    pub(super) struct Target(RawWindowHandle);

    pub(super) fn frame(_: &Window) -> Option<PipRect> {
        None
    }

    pub(super) fn cursor(_: &Window) -> Option<PipPoint> {
        None
    }

    pub(super) fn target(window: &Window) -> Option<Target> {
        let handle = HasWindowHandle::window_handle(window).ok()?.as_raw();
        matches!(handle, RawWindowHandle::AppKit(_)).then_some(Target(handle))
    }

    fn with_native(handle: RawWindowHandle, f: impl FnOnce(&objc2_app_kit::NSWindow)) {
        let RawWindowHandle::AppKit(handle) = handle else {
            return;
        };
        // SAFETY: GPUI owns this NSView for the lifetime of the window;
        // callers read the handle from the live window on the main thread
        // with nothing run in between.
        let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
        if let Some(native) = view.window() {
            f(&native);
        }
    }

    pub(super) fn apply(target: Target, frame: PipRect) {
        with_native(target.0, |native| {
            let Some(screen) = native.screen() else {
                return;
            };
            let screen = screen.frame();
            // AppKit counts up from the bottom of the screen.
            let top = screen.origin.y + screen.size.height - f64::from(frame.y);
            let rect = NSRect::new(
                NSPoint::new(
                    screen.origin.x + f64::from(frame.x),
                    top - f64::from(frame.h),
                ),
                NSSize::new(f64::from(frame.w), f64::from(frame.h)),
            );
            native.setFrame_display(rect, true);
        });
    }

    /// Stay visible when Quill is in the background and over full-screen
    /// apps, on every Space; no traffic lights (GPUI's window without a
    /// titlebar is still a titled `NSWindow`, for its rounded corners and
    /// shadow).
    pub(super) fn configure(window: &Window) {
        let Some(Target(handle)) = target(window) else {
            return;
        };
        with_native(handle, |native| {
            for button in [
                NSWindowButton::CloseButton,
                NSWindowButton::MiniaturizeButton,
                NSWindowButton::ZoomButton,
            ] {
                if let Some(button) = native.standardWindowButton(button) {
                    button.setHidden(true);
                }
            }
            native.setHidesOnDeactivate(false);
            native.setCollectionBehavior(
                NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::FullScreenAuxiliary,
            );
        });
    }
}

#[cfg(target_os = "windows")]
mod platform {
    //! The frame Quill works with is the client area (the picture). A
    //! top-level window keeps a thin frame around it even with GPUI's
    //! titlebar hidden (`WM_NCCALCSIZE` keeps the left, right and bottom
    //! borders), so `SetWindowPos` gets the client rectangle grown by the
    //! window's own border widths, read from `GetWindowInfo` (GPUI's
    //! `calculate_window_rect` adds its stored offsets the same way).
    use super::*;
    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowInfo, GetWindowLongPtrW, HWND_TOPMOST, IsWindowVisible, SW_HIDE,
        SW_SHOWNA, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowLongPtrW,
        SetWindowPos, ShowWindow, WINDOWINFO, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
    };

    #[derive(Clone, Copy)]
    pub(super) struct Target {
        hwnd: HWND,
        scale: f32,
    }

    fn hwnd(window: &Window) -> Option<HWND> {
        match HasWindowHandle::window_handle(window).ok()?.as_raw() {
            RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as HWND),
            _ => None,
        }
    }

    const NO_RECT: RECT = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };

    /// The window and client rectangles in screen device pixels.
    fn info(hwnd: HWND) -> Option<(RECT, RECT)> {
        let mut info = WINDOWINFO {
            cbSize: std::mem::size_of::<WINDOWINFO>() as u32,
            rcWindow: NO_RECT,
            rcClient: NO_RECT,
            dwStyle: 0,
            dwExStyle: 0,
            dwWindowStatus: 0,
            cxWindowBorders: 0,
            cyWindowBorders: 0,
            atomWindowType: 0,
            wCreatorVersion: 0,
        };
        // SAFETY: `hwnd` is a live GPUI window (read from it just before)
        // and `info` is a correctly sized, writable WINDOWINFO.
        let ok = unsafe { GetWindowInfo(hwnd, &mut info) } != 0;
        ok.then_some((info.rcWindow, info.rcClient))
    }

    fn logical(value: i32, scale: f32) -> i32 {
        if scale > 0.0 {
            (value as f32 / scale).round() as i32
        } else {
            value
        }
    }

    /// The client area in screen coordinates (logical pixels).
    pub(super) fn frame(window: &Window) -> Option<PipRect> {
        let (_, client) = info(hwnd(window)?)?;
        let scale = window.scale_factor();
        Some(PipRect::new(
            logical(client.left, scale),
            logical(client.top, scale),
            logical(client.right.saturating_sub(client.left), scale),
            logical(client.bottom.saturating_sub(client.top), scale),
        ))
    }

    pub(super) fn cursor(_: &Window) -> Option<PipPoint> {
        None
    }

    pub(super) fn target(window: &Window) -> Option<Target> {
        Some(Target {
            hwnd: hwnd(window)?,
            scale: window.scale_factor(),
        })
    }

    pub(super) fn apply(target: Target, frame: PipRect) {
        let device = |value: i32| (value as f32 * target.scale).round() as i32;
        let (outer, client) = info(target.hwnd).unwrap_or((NO_RECT, NO_RECT));
        // The border on each side (zero when the window has none).
        let left = client.left.saturating_sub(outer.left).max(0);
        let top = client.top.saturating_sub(outer.top).max(0);
        let right = outer.right.saturating_sub(client.right).max(0);
        let bottom = outer.bottom.saturating_sub(client.bottom).max(0);
        let (x, y) = (device(frame.x), device(frame.y));
        let (w, h) = (device(frame.w), device(frame.h));
        // SAFETY: the handle was read from the live GPUI window just
        // before this call, with nothing run in between; only its
        // position and size change (it stays topmost, not activated).
        unsafe {
            SetWindowPos(
                target.hwnd,
                HWND_TOPMOST,
                x.saturating_sub(left),
                y.saturating_sub(top),
                w.saturating_add(left).saturating_add(right),
                h.saturating_add(top).saturating_add(bottom),
                SWP_NOACTIVATE,
            );
        }
    }

    /// A tool window (`Qt::Tool`): no taskbar button and no Alt+Tab entry,
    /// for good (`ITaskbarList::DeleteTab` can come back on activation).
    /// The taskbar reads the style when a window is shown, so a visible
    /// window is hidden and shown again around the change.
    pub(super) fn configure(window: &Window) {
        let Some(hwnd) = hwnd(window) else {
            return;
        };
        // SAFETY: `hwnd` is the live GPUI window this callback runs for;
        // these are plain user32 calls on it.
        unsafe {
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let tool = (style & !(WS_EX_APPWINDOW as isize)) | WS_EX_TOOLWINDOW as isize;
            if tool == style {
                return;
            }
            let visible = IsWindowVisible(hwnd) != 0;
            if visible {
                ShowWindow(hwnd, SW_HIDE);
            }
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, tool);
            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
            if visible {
                ShowWindow(hwnd, SW_SHOWNA);
            }
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{ConfigureWindowAux, ConnectionExt as _};
    use x11rb::rust_connection::RustConnection;

    #[derive(Clone, Copy)]
    pub(super) struct Target {
        x_window: u32,
        scale: f32,
    }

    /// One connection for the process (a drag configures the window on
    /// every pointer move), with the root window of its screen.
    fn connection() -> Option<&'static Mutex<(RustConnection, u32)>> {
        static CONNECTION: OnceLock<Option<Mutex<(RustConnection, u32)>>> = OnceLock::new();
        CONNECTION
            .get_or_init(|| {
                let (conn, screen) = x11rb::connect(None).ok()?;
                let root = conn.setup().roots.get(screen)?.root;
                Some(Mutex::new((conn, root)))
            })
            .as_ref()
    }

    fn x_window(window: &Window) -> Option<u32> {
        match HasWindowHandle::window_handle(window).ok()?.as_raw() {
            RawWindowHandle::Xcb(handle) => Some(handle.window.get()),
            RawWindowHandle::Xlib(handle) => u32::try_from(handle.window).ok(),
            _ => None,
        }
    }

    fn logical(value: i32, scale: f32) -> i32 {
        if scale > 0.0 {
            (value as f32 / scale).round() as i32
        } else {
            value
        }
    }

    pub(super) fn frame(window: &Window) -> Option<PipRect> {
        let x_window = x_window(window)?;
        let scale = window.scale_factor();
        let conn = quill::locks::lock_or_recover(connection()?);
        let (conn, root) = (&conn.0, conn.1);
        let origin = conn
            .translate_coordinates(x_window, root, 0, 0)
            .ok()?
            .reply()
            .ok()?;
        let geometry = conn.get_geometry(x_window).ok()?.reply().ok()?;
        Some(PipRect::new(
            logical(i32::from(origin.dst_x), scale),
            logical(i32::from(origin.dst_y), scale),
            logical(i32::from(geometry.width), scale),
            logical(i32::from(geometry.height), scale),
        ))
    }

    pub(super) fn cursor(window: &Window) -> Option<PipPoint> {
        x_window(window)?;
        let scale = window.scale_factor();
        let conn = quill::locks::lock_or_recover(connection()?);
        let (conn, root) = (&conn.0, conn.1);
        let pointer = conn.query_pointer(root).ok()?.reply().ok()?;
        Some(PipPoint::new(
            logical(i32::from(pointer.root_x), scale),
            logical(i32::from(pointer.root_y), scale),
        ))
    }

    pub(super) fn target(window: &Window) -> Option<Target> {
        Some(Target {
            x_window: x_window(window)?,
            scale: window.scale_factor(),
        })
    }

    pub(super) fn apply(target: Target, frame: PipRect) {
        let Some(conn) = connection() else {
            return;
        };
        let conn = quill::locks::lock_or_recover(conn);
        let device = |value: i32| (value as f32 * target.scale).round() as i32;
        let side = |value: i32| u32::try_from(device(value)).unwrap_or(1).max(1);
        let aux = ConfigureWindowAux::new()
            .x(device(frame.x))
            .y(device(frame.y))
            .width(side(frame.w))
            .height(side(frame.h));
        if conn.0.configure_window(target.x_window, &aux).is_ok() {
            let _ = conn.0.flush();
        }
    }

    pub(super) fn configure(_: &Window) {}
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
mod platform {
    use super::*;

    #[derive(Clone, Copy)]
    pub(super) struct Target;

    pub(super) fn frame(_: &Window) -> Option<PipRect> {
        None
    }

    pub(super) fn cursor(_: &Window) -> Option<PipPoint> {
        None
    }

    pub(super) fn target(_: &Window) -> Option<Target> {
        None
    }

    pub(super) fn apply(_: Target, _: PipRect) {}

    pub(super) fn configure(_: &Window) {}
}

#[cfg(test)]
pub(crate) mod tests {
    use quill::media_viewer::pip::panel::PipRect;
    use std::cell::RefCell;

    thread_local! {
        /// Every frame Picture-in-Picture asked for on this thread (the
        /// test platform has no native window to move).
        static FRAMES: RefCell<Vec<PipRect>> = const { RefCell::new(Vec::new()) };
    }

    pub(super) fn record(frame: PipRect) {
        FRAMES.with(|frames| frames.borrow_mut().push(frame));
    }

    /// The frames asked for since the last call.
    #[cfg_attr(not(feature = "demo-capture"), allow(dead_code))]
    pub(crate) fn take_frames() -> Vec<PipRect> {
        FRAMES.with(|frames| std::mem::take(&mut *frames.borrow_mut()))
    }
}
