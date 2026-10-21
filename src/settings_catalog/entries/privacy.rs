//! Privacy page entries.

use crate::lang::keys;
use crate::settings_catalog::{Entry, Page};

const P: Page = Page::Privacy;

pub(super) const ENTRIES: &[Entry] = &[
    Entry::new(
        "privacy.phone_number",
        P,
        keys::Q_SETTINGS_PRIVACY_SEE_PHONE,
    )
    .section(keys::Q_PREFS_WHO_CAN_SEE)
    .keywords(&["number", "mobile"]),
    Entry::new(
        "privacy.find_by_number",
        P,
        keys::Q_SETTINGS_PRIVACY_FIND_BY_NUMBER,
    )
    .section(keys::Q_PREFS_WHO_CAN_SEE)
    .keywords(&["discover", "search by phone"]),
    Entry::new(
        "privacy.last_seen",
        P,
        keys::Q_SETTINGS_PRIVACY_SEE_LAST_SEEN,
    )
    .section(keys::Q_PREFS_WHO_CAN_SEE)
    .keywords(&["online status", "last seen"]),
    Entry::new("privacy.read_time", P, keys::EDIT_LASTSEEN_HIDE_READ_TIME)
        .section(keys::Q_PREFS_WHO_CAN_SEE)
        .keywords(&["read receipts", "seen"]),
    Entry::new(
        "privacy.profile_photo",
        P,
        keys::Q_SETTINGS_PRIVACY_SEE_PHOTO,
    )
    .section(keys::Q_PREFS_WHO_CAN_SEE)
    .keywords(&["avatar"]),
    Entry::new("privacy.bio", P, keys::Q_SETTINGS_PRIVACY_SEE_BIO)
        .section(keys::Q_PREFS_WHO_CAN_SEE)
        .keywords(&["about"]),
    Entry::new(
        "privacy.birthdate",
        P,
        keys::Q_SETTINGS_PRIVACY_SEE_BIRTHDATE,
    )
    .section(keys::Q_PREFS_WHO_CAN_SEE)
    .keywords(&["birthday", "age"]),
    Entry::new("privacy.gifts", P, keys::Q_SETTINGS_PRIVACY_SEE_GIFTS)
        .section(keys::Q_PREFS_WHO_CAN_SEE)
        .keywords(&["presents"]),
    Entry::new("privacy.gift_icon", P, keys::EDIT_PRIVACY_GIFTS_SHOW_ICON)
        .section(keys::Q_PREFS_WHO_CAN_SEE),
    Entry::new("privacy.gift_types", P, keys::EDIT_PRIVACY_GIFTS_TYPES)
        .section(keys::Q_PREFS_WHO_CAN_SEE)
        .keywords(&["unlimited", "limited", "unique"]),
    Entry::new("privacy.saved_music", P, keys::Q_SETTINGS_PRIVACY_SEE_MUSIC)
        .section(keys::Q_PREFS_WHO_CAN_SEE)
        .keywords(&["audio"]),
    Entry::new(
        "privacy.forwarded",
        P,
        keys::Q_SETTINGS_PRIVACY_SEE_FORWARDS,
    )
    .section(keys::Q_PREFS_WHO_CAN_SEE)
    .keywords(&["forward link", "forwarded messages"]),
    Entry::new("privacy.calls", P, keys::EDIT_PRIVACY_CALLS_HEADER)
        .section(keys::Q_PREFS_WHO_CAN_CONTACT_ME)
        .keywords(&["phone calls", "voice calls"]),
    Entry::new("privacy.peer_to_peer", P, keys::Q_SETTINGS_PRIVACY_P2P)
        .section(keys::Q_PREFS_WHO_CAN_CONTACT_ME)
        .keywords(&["p2p", "direct connection", "ip address"]),
    Entry::new(
        "privacy.voice_messages",
        P,
        keys::Q_SETTINGS_PRIVACY_VOICE_MESSAGES,
    )
    .section(keys::Q_PREFS_WHO_CAN_CONTACT_ME)
    .keywords(&["audio", "video messages"]),
    Entry::new("privacy.messages", P, keys::Q_PREFS_WHO_CAN_MESSAGE_ME)
        .section(keys::Q_PREFS_WHO_CAN_CONTACT_ME)
        .keywords(&["dm", "new chats"]),
    Entry::new("privacy.paid_messages", P, keys::MESSAGES_PRIVACY_CHARGE)
        .section(keys::Q_PREFS_WHO_CAN_CONTACT_ME)
        .keywords(&["stars", "price", "fee"]),
    Entry::new("privacy.group_invites", P, keys::Q_SETTINGS_PRIVACY_INVITES)
        .section(keys::Q_PREFS_WHO_CAN_CONTACT_ME)
        .keywords(&["invite", "add to group"]),
    Entry::new("privacy.passcode", P, keys::SETTINGS_PASSCODE_TITLE)
        .section(keys::Q_PREFS_SECURITY_AND_CONTENT)
        .keywords(&["lock", "pin", "password"]),
    Entry::new("privacy.auto_lock", P, keys::Q_PREFS_AUTO_LOCK_IF_AWAY_FOR)
        .section(keys::Q_PREFS_SECURITY_AND_CONTENT)
        .keywords(&["timeout", "inactivity"]),
    Entry::new(
        "privacy.system_unlock",
        P,
        keys::Q_SETTINGS_PRIVACY_SYSTEM_UNLOCK,
    )
    .section(keys::Q_PREFS_SECURITY_AND_CONTENT)
    .keywords(&["touch id", "windows hello", "fingerprint", "biometric"]),
    Entry::new("privacy.passkeys", P, keys::SETTINGS_PASSKEYS_TITLE)
        .section(keys::Q_PREFS_SECURITY_AND_CONTENT)
        .keywords(&["passwordless", "webauthn", "face id"]),
    Entry::new(
        "privacy.two_step",
        P,
        keys::SETTINGS_CLOUD_PASSWORD_START_TITLE,
    )
    .section(keys::Q_PREFS_SECURITY_AND_CONTENT)
    .keywords(&["2fa", "password", "recovery email", "cloud password"]),
    Entry::new(
        "privacy.check_password",
        P,
        keys::Q_SETTINGS_PRIVACY_CHECK_PASSWORD,
    )
    .section(keys::Q_PREFS_SECURITY_AND_CONTENT)
    .keywords(&["remember"]),
    Entry::new("privacy.websites", P, keys::SETTINGS_LOGGED_IN)
        .section(keys::Q_PREFS_SECURITY_AND_CONTENT)
        .keywords(&["login with telegram", "disconnect"]),
    Entry::new(
        "privacy.sensitive_content",
        P,
        keys::Q_PREFS_SHOW_18_CONTENT,
    )
    .section(keys::Q_PREFS_SECURITY_AND_CONTENT)
    .keywords(&["adult", "nsfw", "sensitive"]),
    Entry::new(
        "privacy.file_open",
        P,
        keys::Q_PREFS_FILE_OPEN_CONFIRMATIONS,
    )
    .section(keys::Q_PREFS_SECURITY_AND_CONTENT)
    .keywords(&["extensions", "ip reveal", "whitelist"]),
    Entry::new("privacy.auto_delete", P, keys::Q_PREFS_AUTO_DELETE_MESSAGES)
        .section(keys::Q_SETTINGS_PRIVACY_SEC_DATA)
        .keywords(&["ttl", "self-destruct", "timer"]),
    Entry::new(
        "privacy.account_ttl",
        P,
        keys::Q_PREFS_DELETE_MY_ACCOUNT_IF_AWAY,
    )
    .section(keys::Q_SETTINGS_PRIVACY_SEC_DATA)
    .keywords(&["inactivity", "self-destruct"]),
    Entry::new(
        "privacy.clear_payment",
        P,
        keys::Q_PREFS_CLEAR_SAVED_PAYMENT_SHIPPING_INFO,
    )
    .section(keys::Q_SETTINGS_PRIVACY_SEC_DATA)
    .keywords(&["card", "shipping", "payments"]),
    Entry::new(
        "privacy.frequent_contacts",
        P,
        keys::Q_PREFS_SUGGEST_FREQUENT_CONTACTS,
    )
    .section(keys::Q_SETTINGS_PRIVACY_SEC_DATA)
    .keywords(&["top people", "suggestions"]),
    Entry::new("privacy.blocked", P, keys::Q_PREFS_BLOCKED_USERS)
        .section(keys::Q_SETTINGS_PRIVACY_SEC_DATA)
        .keywords(&["block", "unblock"]),
    Entry::new(
        "privacy.archive_unknown",
        P,
        keys::Q_SETTINGS_PRIVACY_ARCHIVE_UNKNOWN,
    )
    .section(keys::SETTINGS_AUTO_ARCHIVE)
    .keywords(&["spam", "strangers", "non-contacts"]),
    Entry::new(
        "privacy.archive_unmuted",
        P,
        keys::Q_SETTINGS_PRIVACY_ARCHIVE_UNMUTED,
    )
    .section(keys::SETTINGS_AUTO_ARCHIVE)
    .keywords(&["always keep archived"]),
    Entry::new(
        "privacy.archive_folders",
        P,
        keys::Q_SETTINGS_PRIVACY_ARCHIVE_FOLDERS,
    )
    .section(keys::SETTINGS_AUTO_ARCHIVE),
];
