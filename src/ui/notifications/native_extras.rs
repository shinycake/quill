//! What GPUI's `SystemNotification` cannot carry, per platform: the sender
//! picture and the inline reply field going out, the typed text coming back.
//!
//! macOS and Windows: the Quill patch in `third_party/gpui-pre-{macos,windows}`
//! (`notification_extras`) registers both under the notification's tag right
//! before `show_system_notification`, and parks the typed text under the tag
//! before the response arrives. Linux: `linux_dbus` keeps the typed text per
//! chat itself. Everywhere else the functions are no-ops, so the callers stay
//! free of `cfg`.

use quill::ids::ChatId;
use std::path::Path;

#[cfg(target_os = "macos")]
use gpui_macos::notification_extras as backend;
#[cfg(target_os = "windows")]
use gpui_windows::notification_extras as backend;

/// Hand the platform the pictures and the reply field for the notification
/// `tag` is about to show: `image` is the sender's picture, `photo` the
/// message's photo ("Show photo previews"). `inline_reply` is the id of the action that
/// becomes a text field (`NotificationAction::Reply.id()`); `None` leaves
/// the buttons plain (locked app).
#[cfg_attr(
    not(any(target_os = "macos", target_os = "windows")),
    allow(unused_variables)
)]
pub(in crate::ui) fn register(
    tag: &str,
    image: Option<&Path>,
    photo: Option<&Path>,
    inline_reply: Option<&str>,
) {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    backend::set_notification_extras(
        tag,
        backend::NotificationExtras {
            image: image.map(Path::to_path_buf),
            photo: photo.map(Path::to_path_buf),
            text_input: inline_reply.map(|action_id| backend::TextInputAction {
                action_id: action_id.to_string(),
                placeholder: quill::notify::INLINE_REPLY_PLACEHOLDER.to_string(),
                send_label: quill::notify::INLINE_REPLY_SEND.to_string(),
            }),
        },
    );
}

/// The text typed into the reply field of the chat's notification, if its
/// last activation came from that field. Taking it clears it.
#[cfg_attr(
    not(any(target_os = "macos", target_os = "windows", target_os = "linux")),
    allow(unused_variables)
)]
pub(in crate::ui) fn take_reply_text(tag: &str, chat_id: ChatId) -> Option<String> {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let _ = chat_id;
        backend::take_reply_text(tag)
    }
    #[cfg(target_os = "linux")]
    {
        let _ = tag;
        super::linux_dbus::take_reply_text(chat_id)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    None
}
