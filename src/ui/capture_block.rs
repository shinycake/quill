//! Screenshot and screen-recording prevention for secret chats (macOS).
//!
//! Telegram's mobile apps block screenshots in secret chats. The macOS
//! equivalent is `NSWindow.sharingType = NSWindowSharingNone`: AppKit
//! documents that with `.none` the window's content cannot be read by
//! other processes, so screenshots, screen recording and screen sharing
//! show the window as absent/blank. `.readOnly` is the default. Other
//! platforms have no such API (X11/Wayland have no FLAG_SECURE), so this
//! compiles to a no-op there.

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

#[cfg(not(target_os = "macos"))]
fn apply_sharing(_window: &Window, _blocked: bool) {}

impl QuillApp {
    /// Called every render; touches AppKit only when the desired state changes.
    pub(super) fn sync_capture_block(&mut self, window: &Window) {
        let viewer_chat_secret = self
            .media_viewer
            .current()
            .is_some_and(|item| self.chat_is_secret(item.chat_id.0));
        let blocked = should_block(self.open_chat_is_secret(), viewer_chat_secret);
        if blocked != self.capture_blocked {
            apply_sharing(window, blocked);
            self.capture_blocked = blocked;
        }
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
