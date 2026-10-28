//! Added by the Quill project (see `QUILL-CHANGES.md`): what GPUI's
//! `SystemNotification` has no field for, handed to the platform beside it.
//!
//! `SystemNotification` lives in the `gpui` crate, which Quill does not
//! vendor, so its struct cannot grow fields. Instead the application registers
//! [`NotificationExtras`] under the notification's tag right before
//! `App::show_system_notification`; the backend takes them when it builds the
//! platform notification. The text a user types into an inline reply field
//! travels back the same way: the backend stores it under the tag before it
//! sends the `SystemNotificationResponse`, and the application's response
//! handler takes it with [`take_reply_text`].
//!
//! This file is identical in `gpui-pre-macos` and `gpui-pre-windows`
//! (`scripts/test-notification-extras.sh` checks that and runs the tests).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

/// Extra content for one `SystemNotification`, keyed by its tag.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NotificationExtras {
    /// A PNG shown as the notification's image: the sender's picture. A
    /// notification attachment on macOS, the circle-cropped
    /// `appLogoOverride` on Windows. The file is read when the notification
    /// is posted and may be deleted afterwards.
    pub image: Option<PathBuf>,
    /// A picture shown large with the text: the message's photo. On macOS
    /// it takes the attachment's place (the system shows one); on Windows
    /// an inline image under the text. Read when the notification is
    /// posted, in its own format (JPEG or PNG).
    pub photo: Option<PathBuf>,
    /// The action in `SystemNotification::actions` whose button opens a text
    /// field instead of firing at once. Its response carries the same
    /// `action_id`; the typed text waits in [`take_reply_text`].
    pub text_input: Option<TextInputAction>,
}

/// Which action takes typed text, and how the field looks.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TextInputAction {
    /// The `SystemNotificationAction::id` of the button that becomes a text
    /// field.
    pub action_id: String,
    /// Grey text shown in the empty field.
    pub placeholder: String,
    /// Label of the button that submits the field.
    pub send_label: String,
}

struct Registry {
    extras: HashMap<String, NotificationExtras>,
    reply_texts: HashMap<String, String>,
}

fn registry() -> MutexGuard<'static, Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY
        .get_or_init(|| {
            Mutex::new(Registry {
                extras: HashMap::new(),
                reply_texts: HashMap::new(),
            })
        })
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Register the extras for the notification with `tag`, to be consumed by
/// the next `App::show_system_notification` with that tag. Registering again
/// before that replaces them; `None` for both fields is the same as not
/// registering.
pub fn set_notification_extras(tag: &str, extras: NotificationExtras) {
    let mut registry = registry();
    if extras == NotificationExtras::default() {
        registry.extras.remove(tag);
    } else {
        registry.extras.insert(tag.to_string(), extras);
    }
}

/// The backend's side: the extras registered for `tag`, removed from the
/// registry so a later plain notification with the same tag shows none.
pub(crate) fn take_notification_extras(tag: &str) -> NotificationExtras {
    registry().extras.remove(tag).unwrap_or_default()
}

/// The backend's side: remember the text typed into the notification with
/// `tag`, right before its response is sent.
pub(crate) fn store_reply_text(tag: &str, text: String) {
    registry().reply_texts.insert(tag.to_string(), text);
}

/// The text typed into the inline reply field of the notification with
/// `tag`, if its last response came from that field. Taking it clears it, so
/// a later plain button press on a notification with the same tag reads as
/// one.
pub fn take_reply_text(tag: &str) -> Option<String> {
    registry().reply_texts.remove(tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_input(id: &str) -> TextInputAction {
        TextInputAction {
            action_id: id.to_string(),
            placeholder: "Message".to_string(),
            send_label: "Send".to_string(),
        }
    }

    #[test]
    fn extras_are_consumed_by_one_show() {
        let extras = NotificationExtras {
            image: Some(PathBuf::from("/tmp/a.png")),
            photo: Some(PathBuf::from("/tmp/b.jpg")),
            text_input: Some(text_input("reply")),
        };
        set_notification_extras("extras-once", extras.clone());
        assert_eq!(take_notification_extras("extras-once"), extras);
        assert_eq!(
            take_notification_extras("extras-once"),
            NotificationExtras::default()
        );
    }

    #[test]
    fn registering_again_replaces_and_empty_extras_clear() {
        set_notification_extras(
            "extras-replace",
            NotificationExtras {
                image: Some(PathBuf::from("/tmp/a.png")),
                photo: None,
                text_input: None,
            },
        );
        set_notification_extras(
            "extras-replace",
            NotificationExtras {
                image: None,
                photo: None,
                text_input: Some(text_input("reply")),
            },
        );
        assert_eq!(
            take_notification_extras("extras-replace").text_input,
            Some(text_input("reply"))
        );
        set_notification_extras("extras-clear", NotificationExtras::default());
        assert_eq!(
            take_notification_extras("extras-clear"),
            NotificationExtras::default()
        );
    }

    #[test]
    fn reply_text_is_taken_once_per_tag() {
        store_reply_text("reply-tag", "hello".to_string());
        assert_eq!(take_reply_text("other-tag"), None);
        assert_eq!(take_reply_text("reply-tag"), Some("hello".to_string()));
        assert_eq!(take_reply_text("reply-tag"), None);
    }
}
