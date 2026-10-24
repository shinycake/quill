//! The `org.freedesktop.Notifications` wire shapes for Linux desktop
//! notifications, as Telegram Desktop uses them
//! (`platform/linux/notifications_manager_linux.cpp`): which capabilities
//! unlock which buttons, the `actions` list, the `hints` and what each
//! signal means. Pure data, so the `ui` feature's `linux_dbus` module only
//! carries the bus traffic (zbus) and this part is unit-tested everywhere.

use super::NotificationAction;

/// The well-known bus name, object path and interface of the service.
pub const BUS_NAME: &str = "org.freedesktop.Notifications";
pub const OBJECT_PATH: &str = "/org/freedesktop/Notifications";
pub const INTERFACE: &str = "org.freedesktop.Notifications";

/// The `app_name` argument of `Notify` and the `desktop-entry` hint (the
/// packaged `assets/org.shinycake.quill.desktop`), so the daemon can show the app icon
/// and group the notifications under the app.
pub const APP_NAME: &str = "Quill";
pub const DESKTOP_ENTRY: &str = "org.shinycake.quill";

/// The action key a daemon with the `inline-reply` capability turns into a
/// text field; its text comes back in the `NotificationReplied` signal.
pub const INLINE_REPLY_ACTION: &str = "inline-reply";

/// The daemon's answer to `GetCapabilities`, reduced to what changes a
/// notification.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Capabilities {
    /// Buttons (`actions`); without it the daemon ignores the action list.
    pub actions: bool,
    /// A text field in the notification (`inline-reply`, KDE Plasma).
    pub inline_reply: bool,
}

impl Capabilities {
    pub fn from_list<S: AsRef<str>>(list: &[S]) -> Self {
        let has = |name: &str| list.iter().any(|entry| entry.as_ref() == name);
        Self {
            actions: has("actions"),
            inline_reply: has(INLINE_REPLY_ACTION),
        }
    }
}

/// tdesktop `ByDefault` on Linux: the daemon matches Quill's own
/// notification windows (body, buttons, inline reply, and either a sound
/// of its own or Do Not Disturb inhibitions), so its notifications are the
/// default. Otherwise Quill's windows are (`crate::notify::custom`).
pub fn full_parity<S: AsRef<str>>(list: &[S]) -> bool {
    let has = |name: &str| list.iter().any(|entry| entry.as_ref() == name);
    ["body", "actions", INLINE_REPLY_ACTION]
        .iter()
        .all(|name| has(name))
        && (has("sound") || has("inhibitions"))
}

/// The `actions` argument of `Notify`: keys and labels alternating. Empty
/// when the daemon has no buttons or the app is locked (a reply or read
/// receipt must not bypass the passcode). The body click is `default`;
/// "Reply" is the daemon's text field where it has one, otherwise a button
/// that opens the chat.
pub fn notify_actions(capabilities: Capabilities, locked: bool) -> Vec<String> {
    if !capabilities.actions || locked {
        return Vec::new();
    }
    let mut actions = vec![
        NotificationAction::Open.id().to_string(),
        NotificationAction::Open.label().to_string(),
    ];
    for action in NotificationAction::BUTTONS {
        let key = match action {
            NotificationAction::Reply if capabilities.inline_reply => INLINE_REPLY_ACTION,
            action => action.id(),
        };
        actions.push(key.to_string());
        actions.push(action.label().to_string());
    }
    actions
}

/// The `hints` argument of `Notify`, all strings (the bus layer wraps them
/// in variants). `desktop-entry` names the app for the daemon's icon and
/// grouping, `category` is the spec's incoming instant message. The sender
/// picture is the `app_icon` argument (see [`app_icon`]), not a hint, so a
/// daemon shows it where the app icon goes. The message's photo ("Show
/// photo previews") is the spec's `image-path`, the notification's image.
pub fn notify_hints(photo: Option<&std::path::Path>) -> Vec<(&'static str, String)> {
    let mut hints = vec![
        ("desktop-entry", DESKTOP_ENTRY.to_string()),
        ("category", "im.received".to_string()),
    ];
    if let Some(photo) = photo {
        hints.push(("image-path", photo.display().to_string()));
    }
    hints
}

/// The `app_icon` argument: the cached circular avatar's path, or empty so
/// the daemon falls back to the `desktop-entry` icon.
pub fn app_icon(avatar: Option<&std::path::Path>) -> String {
    avatar.map_or_else(String::new, |path| path.display().to_string())
}

/// What an `ActionInvoked` key means. `inline-reply` arrives when a daemon
/// with the capability treats the field's button as a plain action (the text
/// itself comes with `NotificationReplied`), so it opens the chat like a
/// reply without text.
pub fn action_for_key(key: &str) -> NotificationAction {
    match key {
        INLINE_REPLY_ACTION => NotificationAction::Reply,
        other => NotificationAction::from_id(Some(other)),
    }
}

/// A signal from the daemon about a shown notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signal {
    /// `ActionInvoked(id, key)`: a button or the body was activated.
    Action { id: u32, action: NotificationAction },
    /// `NotificationReplied(id, text)`: the inline reply field was sent.
    Replied { id: u32, text: String },
    /// `NotificationClosed(id, reason)`: gone from the screen and the shade.
    Closed { id: u32 },
}

/// Interpret a signal by member name and decoded body. `None` for members
/// this backend does not act on (`ActivationToken`).
pub fn interpret_signal(member: &str, id: u32, text: Option<&str>) -> Option<Signal> {
    match member {
        "ActionInvoked" => Some(Signal::Action {
            id,
            action: action_for_key(text.unwrap_or_default()),
        }),
        "NotificationReplied" => Some(Signal::Replied {
            id,
            text: text.unwrap_or_default().to_string(),
        }),
        "NotificationClosed" => Some(Signal::Closed { id }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_come_from_the_daemon_list() {
        let caps = Capabilities::from_list(&["body", "actions", "inline-reply", "sound"]);
        assert!(caps.actions && caps.inline_reply);
        let plain = Capabilities::from_list(&["body"]);
        assert_eq!(plain, Capabilities::default());
    }

    #[test]
    fn actions_follow_capabilities_and_the_lock() {
        let all = Capabilities {
            actions: true,
            inline_reply: true,
        };
        assert_eq!(
            notify_actions(all, false),
            vec![
                "default",
                "Open",
                "inline-reply",
                "Reply",
                "read",
                "Mark as read"
            ]
        );
        let buttons_only = Capabilities {
            actions: true,
            inline_reply: false,
        };
        assert_eq!(
            notify_actions(buttons_only, false),
            vec!["default", "Open", "reply", "Reply", "read", "Mark as read"]
        );
        assert!(notify_actions(Capabilities::default(), false).is_empty());
        assert!(notify_actions(all, true).is_empty());
    }

    #[test]
    fn hints_name_the_desktop_entry_and_category() {
        let hints = notify_hints(None);
        assert!(hints.contains(&("desktop-entry", "org.shinycake.quill".to_string())));
        assert!(hints.contains(&("category", "im.received".to_string())));
        assert!(!hints.iter().any(|(key, _)| *key == "image-path"));
        let hints = notify_hints(Some(std::path::Path::new("/td/photos/1.jpg")));
        assert!(hints.contains(&("image-path", "/td/photos/1.jpg".to_string())));
        assert_eq!(app_icon(None), "");
        assert_eq!(
            app_icon(Some(std::path::Path::new("/cache/a.png"))),
            "/cache/a.png"
        );
    }

    #[test]
    fn signals_map_to_actions_replies_and_closes() {
        assert_eq!(
            interpret_signal("ActionInvoked", 4, Some("default")),
            Some(Signal::Action {
                id: 4,
                action: NotificationAction::Open
            })
        );
        assert_eq!(
            interpret_signal("ActionInvoked", 4, Some("read")),
            Some(Signal::Action {
                id: 4,
                action: NotificationAction::MarkRead
            })
        );
        assert_eq!(
            interpret_signal("ActionInvoked", 4, Some("inline-reply")),
            Some(Signal::Action {
                id: 4,
                action: NotificationAction::Reply
            })
        );
        assert_eq!(
            interpret_signal("NotificationReplied", 4, Some("on my way")),
            Some(Signal::Replied {
                id: 4,
                text: "on my way".to_string()
            })
        );
        assert_eq!(
            interpret_signal("NotificationClosed", 4, None),
            Some(Signal::Closed { id: 4 })
        );
        assert_eq!(interpret_signal("ActivationToken", 4, Some("x")), None);
    }

    #[test]
    fn full_parity_needs_reply_and_sound_or_inhibitions() {
        use super::full_parity;
        assert!(full_parity(&["body", "actions", "inline-reply", "sound"]));
        assert!(full_parity(&[
            "body",
            "actions",
            "inline-reply",
            "inhibitions"
        ]));
        assert!(!full_parity(&["body", "actions", "sound"]), "GNOME Shell");
        assert!(!full_parity(&["body", "actions", "inline-reply"]));
    }
}
