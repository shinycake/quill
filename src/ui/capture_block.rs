//! Screenshot and screen-recording prevention for secret chats.
//!
//! Telegram's mobile apps block screenshots in secret chats. Desktop
//! equivalents:
//! - macOS: `NSWindow.sharingType = NSWindowSharingNone` — other processes
//!   can't read the window, so screenshots, recordings and screen sharing
//!   show it blank. `.readOnly` is the default.
//! - Windows 10 2004+: `SetWindowDisplayAffinity(hwnd,
//!   WDA_EXCLUDEFROMCAPTURE)` — the window is left out of captures
//!   (`WDA_NONE` restores it). Older Windows ignores the flag.
//! - Linux: X11 and Wayland give apps no way to hide a window from screen
//!   capture, so nothing is blocked; a secret chat says so plainly
//!   (`capture_notice`) instead of implying a protection it can't give.

use super::app::QuillApp;
use gpui_kit::*;
use quill::telegram::envelope::ChatKind;

/// Capture is blocked while a secret chat is open, or while the media
/// viewer overlay (drawn inside the main window) shows media of one.
pub(super) fn should_block(open_chat_secret: bool, viewer_chat_secret: bool) -> bool {
    open_chat_secret || viewer_chat_secret
}

#[cfg(target_os = "macos")]
fn apply_sharing(window: &Window, blocked: bool) {
    use objc2_app_kit::{NSView, NSWindowSharingType};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    if let Ok(handle) = HasWindowHandle::window_handle(window)
        && let RawWindowHandle::AppKit(handle) = handle.as_raw()
    {
        // GPUI owns this NSView; its raw handle is valid during the render callback.
        let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
        if let Some(ns_window) = view.window() {
            ns_window.setSharingType(if blocked {
                NSWindowSharingType::None
            } else {
                NSWindowSharingType::ReadOnly
            });
        }
    }
}

#[cfg(target_os = "windows")]
fn apply_sharing(window: &Window, blocked: bool) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_NONE,
    };
    if let Ok(handle) = HasWindowHandle::window_handle(window)
        && let RawWindowHandle::Win32(handle) = handle.as_raw()
    {
        let affinity = if blocked {
            WDA_EXCLUDEFROMCAPTURE
        } else {
            WDA_NONE
        };
        // SAFETY: GPUI owns this HWND; it is valid during the render callback.
        unsafe { SetWindowDisplayAffinity(handle.hwnd.get() as _, affinity) };
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn apply_sharing(_window: &Window, _blocked: bool) {}

/// Whether this platform can keep a window out of screen capture.
pub(super) const CAPTURE_BLOCK_SUPPORTED: bool =
    cfg!(any(target_os = "macos", target_os = "windows"));

/// Shown in secret chats where capture can't be blocked (Linux).
pub(super) const CAPTURE_BLOCK_UNSUPPORTED_NOTE: &str = "Screenshots can't be blocked here. \
     Linux desktops don't let apps hide a window from screen capture, so anything in this \
     secret chat can be captured on this computer.";

impl QuillApp {
    /// Called every render; touches AppKit only when the desired state changes.
    pub(super) fn sync_capture_block(&mut self, window: &Window) {
        let viewer_chat_secret = self
            .viewer
            .state
            .current()
            .is_some_and(|item| self.chat_is_secret(item.chat_id.0));
        let blocked = should_block(self.open_chat_is_secret(), viewer_chat_secret);
        if blocked != self.frame.capture_blocked {
            apply_sharing(window, blocked);
            self.frame.capture_blocked = blocked;
        }
    }

    /// Where capture can't be blocked, a secret chat says so once per
    /// session: a quiet, dismissible line above the history.
    pub(super) fn capture_notice(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        use gpui_kit::component::{ActiveTheme, Sizable};
        if CAPTURE_BLOCK_SUPPORTED
            || self.frame.capture_notice_dismissed
            || !self.open_chat_is_secret()
        {
            return None;
        }
        Some(
            div()
                .id("capture-block-notice")
                .flex_none()
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_2()
                .border_b_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().secondary)
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(div().flex_1().child(CAPTURE_BLOCK_UNSUPPORTED_NOTE))
                .child(
                    gpui_kit::component::button::Button::new("capture-block-notice-dismiss")
                        .label("Got it")
                        .xsmall()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.frame.capture_notice_dismissed = true;
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
    }

    fn chat_is_secret(&self, chat_id: i64) -> bool {
        self.session()
            .as_ref()
            .and_then(|s| s.chats.get(&chat_id))
            .is_some_and(|chat| matches!(chat.kind, ChatKind::Secret { .. }))
    }
}

#[cfg(test)]
mod tests {
    use super::should_block;

    #[test]
    fn blocks_for_secret_chat_or_secret_viewer() {
        assert!(!should_block(false, false));
        assert!(should_block(true, false));
        assert!(should_block(false, true));
        assert!(should_block(true, true));
    }
}
