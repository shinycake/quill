//! About Quill page entries.

use crate::lang::keys;
use crate::settings_catalog::{Entry, Page};

const P: Page = Page::About;

pub(super) const ENTRIES: &[Entry] = &[
    Entry::new("about.version", P, keys::Q_SETTINGS_ABOUT_VERSION).keywords(&["build", "release"]),
    Entry::new("about.about_quill", P, keys::Q_OPS_ABOUT_QUILL).keywords(&["disclaimer"]),
    Entry::new("about.whats_new", P, keys::Q_PREFS_WHATS_NEW)
        .keywords(&["changelog", "release notes"]),
    Entry::new("about.release_notes", P, keys::Q_OPS_RELEASE_NOTES).keywords(&["changelog"]),
    Entry::new("about.check_updates", P, keys::Q_OPS_CHECK_FOR_UPDATES)
        .section(keys::Q_OPS_QUILL_UPDATES)
        .keywords(&["upgrade", "new version"]),
    Entry::new("about.licenses", P, keys::Q_OPS_OPEN_SOURCE_LICENSES).keywords(&[
        "third party",
        "legal",
        "attribution",
    ]),
    Entry::new("about.faq", P, keys::SETTINGS_FAQ)
        .section(keys::Q_SETTINGS_ABOUT_SEC_HELP)
        .keywords(&["help", "support"]),
    Entry::new("about.features", P, keys::SETTINGS_FEATURES)
        .section(keys::Q_SETTINGS_ABOUT_SEC_HELP)
        .keywords(&["tips", "what can telegram do"]),
    Entry::new("about.ask_question", P, keys::SETTINGS_ASK_QUESTION)
        .section(keys::Q_SETTINGS_ABOUT_SEC_HELP)
        .keywords(&["support", "volunteer", "contact"]),
    Entry::new("about.privacy_policy", P, keys::Q_PREFS_PRIVACY_POLICY)
        .section(keys::Q_SETTINGS_ABOUT_SEC_HELP)
        .keywords(&["legal", "terms"]),
];
