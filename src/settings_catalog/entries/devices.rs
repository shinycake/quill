//! Devices page entries.

use crate::lang::keys;
use crate::settings_catalog::{Entry, Page};

const P: Page = Page::Devices;

pub(super) const ENTRIES: &[Entry] = &[
    Entry::new("devices.sessions", P, keys::SETTINGS_SHOW_SESSIONS).keywords(&[
        "logged in",
        "sign-ins",
        "devices",
    ]),
    Entry::new("devices.current_session", P, keys::Q_PREFS_CURRENT_SESSION)
        .keywords(&["this device"]),
    Entry::new(
        "devices.accept_secret_chats",
        P,
        keys::Q_SETTINGS_DEVICES_ACCEPT_SECRET_CHATS,
    )
    .keywords(&["encrypted"]),
    Entry::new(
        "devices.accept_calls",
        P,
        keys::Q_SETTINGS_DEVICES_ACCEPT_CALLS,
    )
    .keywords(&["incoming calls"]),
    Entry::new("devices.terminate_session", P, keys::SESSIONS_TERMINATE)
        .keywords(&["log out device", "end session"]),
    Entry::new("devices.terminate_others", P, keys::SESSIONS_TERMINATE_ALL)
        .keywords(&["log out everywhere", "sign out all"]),
    Entry::new(
        "devices.terminate_old",
        P,
        keys::Q_PREFS_TERMINATE_OLD_SESSIONS,
    )
    .keywords(&["inactive", "auto terminate"]),
    Entry::new(
        "devices.rename_device",
        P,
        keys::Q_PREFS_RENAME_CURRENT_DEVICE,
    )
    .keywords(&["device name", "device model"]),
    Entry::new("devices.link_device", P, keys::Q_PREFS_LINK_DEVICE).keywords(&[
        "qr code",
        "scan",
        "desktop login",
    ]),
    Entry::new(
        "devices.incomplete_logins",
        P,
        keys::Q_PREFS_INCOMPLETE_LOGIN_ATTEMPTS,
    )
    .keywords(&["failed logins"]),
];
