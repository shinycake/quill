//! Advanced page entries.

use crate::lang::keys;
use crate::settings_catalog::{Entry, Page};

const P: Page = Page::Advanced;

pub(super) const ENTRIES: &[Entry] = &[
    Entry::new("advanced.experimental", P, keys::SETTINGS_EXPERIMENTAL)
        .keywords(&["labs", "unstable"]),
    Entry::new(
        "advanced.split_profile_media",
        P,
        keys::Q_SETTINGS_ADVANCED_SPLIT_PROFILE_MEDIA,
    )
    .section(keys::SETTINGS_EXPERIMENTAL)
    .keywords(&["shared media"]),
    Entry::new("advanced.fast_buttons", P, keys::Q_MEDIA_FAST_BUTTONS_MODE)
        .section(keys::SETTINGS_EXPERIMENTAL)
        .keywords(&["inline keyboard", "1-9", "bots"]),
    Entry::new("advanced.welcome_intro", P, keys::Q_FIRST_RUN_REPLAY).keywords(&[
        "first run",
        "onboarding",
        "tour",
    ]),
    Entry::new(
        "advanced.restore_defaults",
        P,
        keys::Q_SETTINGS_ADVANCED_RESTORE_DEFAULTS,
    )
    .section(keys::Q_SETTINGS_ADVANCED_SEC_SETTINGS_FILE)
    .keywords(&["reset settings", "factory"]),
    Entry::new(
        "advanced.export_settings",
        P,
        keys::Q_SETTINGS_ADVANCED_EXPORT_SETTINGS,
    )
    .section(keys::Q_SETTINGS_ADVANCED_SEC_SETTINGS_FILE)
    .keywords(&["backup", "save"]),
    Entry::new(
        "advanced.import_settings",
        P,
        keys::Q_SETTINGS_ADVANCED_IMPORT_SETTINGS,
    )
    .section(keys::Q_SETTINGS_ADVANCED_SEC_SETTINGS_FILE)
    .keywords(&["restore", "load"]),
    Entry::new("advanced.debug_logs", P, keys::Q_OPS_DEBUG_LOGS).keywords(&[
        "log",
        "diagnostics",
        "troubleshoot",
        "tdlib",
    ]),
];
