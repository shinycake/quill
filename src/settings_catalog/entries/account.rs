//! Account page entries.

use crate::lang::keys;
use crate::settings_catalog::{Entry, Page};

const P: Page = Page::Account;

pub(super) const ENTRIES: &[Entry] = &[
    Entry::new("account.photo", P, keys::Q_PEOPLE_PHOTO).keywords(&[
        "avatar",
        "picture",
        "profile picture",
    ]),
    Entry::new("account.first_name", P, keys::Q_PEOPLE_FIRST_NAME).keywords(&["display name"]),
    Entry::new("account.last_name", P, keys::Q_PEOPLE_LAST_NAME).keywords(&["surname"]),
    Entry::new("account.bio", P, keys::INFO_BIO_LABEL).keywords(&["about"]),
    Entry::new("account.username", P, keys::INFO_USERNAME_LABEL)
        .keywords(&["handle", "public link"]),
    Entry::new(
        "account.collectible_usernames",
        P,
        keys::Q_SETTINGS_ACCOUNT_COLLECTIBLE_USERNAMES,
    )
    .keywords(&["fragment", "activate", "reorder"]),
    Entry::new("account.phone_number", P, keys::Q_PEOPLE_PHONE_NUMBER).keywords(&["mobile"]),
    Entry::new("account.change_number", P, keys::Q_PEOPLE_CHANGE_NUMBER).keywords(&["new phone"]),
    Entry::new(
        "account.phone_suggestion",
        P,
        keys::Q_SETTINGS_ACCOUNT_PHONE_SUGGESTION,
    )
    .keywords(&["confirm phone"]),
    Entry::new(
        "account.name_color",
        P,
        keys::Q_PEOPLE_NAME_AND_PROFILE_COLOR,
    )
    .keywords(&["accent", "your color", "emoji status"]),
    Entry::new("account.accounts", P, keys::Q_OPS_ACCOUNTS)
        .section(keys::Q_SETTINGS_ACCOUNT_SEC_ACCOUNTS)
        .keywords(&["multiple", "several"]),
    Entry::new("account.add_account", P, keys::Q_OPS_ADD_ACCOUNT)
        .section(keys::Q_SETTINGS_ACCOUNT_SEC_ACCOUNTS)
        .keywords(&["another account", "new account"]),
    Entry::new("account.sync_contacts", P, keys::Q_PREFS_SYNC_CONTACTS)
        .section(keys::CONTACTS_HEADER)
        .keywords(&["address book"]),
    Entry::new("account.import_contacts", P, keys::Q_PREFS_IMPORT_CONTACTS)
        .section(keys::CONTACTS_HEADER)
        .keywords(&["vcard"]),
    Entry::new(
        "account.delete_synced_contacts",
        P,
        keys::Q_PREFS_DELETE_SYNCED_CONTACTS,
    )
    .section(keys::CONTACTS_HEADER)
    .keywords(&["remove contacts"]),
    Entry::new("account.log_out", P, keys::Q_PREFS_LOG_OUT)
        .section(keys::Q_SETTINGS_ACCOUNT_SEC_SESSION)
        .keywords(&["sign out", "logout"]),
    Entry::new(
        "account.delete_account",
        P,
        keys::Q_PREFS_DELETE_MY_ACCOUNT_NOW,
    )
    .section(keys::Q_SETTINGS_ACCOUNT_SEC_SESSION)
    .keywords(&["remove account", "deactivate", "erase"]),
];
