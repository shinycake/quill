// Added by the Quill project (2026). See third_party/gpui-pre-windows/QUILL-CHANGES.md.
//! Switch a window between the system frame (title bar and caption buttons
//! drawn by Windows) and a frame the application draws itself, while the
//! window is open.
//!
//! GPUI decides this once, at creation, from `TitlebarOptions::
//! appears_transparent`, and its `PlatformWindow::request_decorations` is not
//! implemented on Windows (and is called with `Server` while every window is
//! created, so it cannot carry an opt-in). The application calls
//! [`set_native_frame`] with the window's `HWND` instead, the way Telegram
//! Desktop's "Use system window frame" applies at once.

use windows::Win32::Foundation::HWND;

/// Show (`true`) or hide (`false`) the system title bar of the window with
/// the given handle. Returns `false` when the handle is not a GPUI window.
/// Must be called on the thread that owns the window (GPUI's main thread).
pub fn set_native_frame(hwnd: isize, native: bool) -> bool {
    let hwnd = HWND(hwnd as _);
    let Some(inner) = crate::window::window_from_hwnd(hwnd) else {
        return false;
    };
    if inner.hide_title_bar.get() != native {
        return true;
    }
    inner.hide_title_bar.set(!native);
    crate::events::notify_frame_changed(hwnd);
    true
}
