//! Keyboard Shortcuts page entries.

use crate::lang::keys;
use crate::settings_catalog::{Entry, Page};

const P: Page = Page::Shortcuts;

pub(super) const ENTRIES: &[Entry] = &[
    Entry::new("shortcuts.rebind", P, keys::Q_PREFS_KEYBOARD_SHORTCUTS).keywords(&[
        "hotkeys",
        "keybindings",
        "rebind",
        "change keys",
    ]),
    Entry::new(
        "shortcuts.reference",
        P,
        keys::Q_SETTINGS_SHORTCUTS_REFERENCE,
    )
    .keywords(&["cheat sheet", "list of shortcuts", "keys"]),
];
