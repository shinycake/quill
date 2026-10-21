//! Language page entries.

use crate::lang::keys;
use crate::settings_catalog::{Entry, Page};

const P: Page = Page::Language;

pub(super) const ENTRIES: &[Entry] = &[
    Entry::new("language.interface_language", P, keys::SETTINGS_LANGUAGE).keywords(&[
        "locale",
        "translation",
        "english",
        "change language",
    ]),
    Entry::new(
        "language.translate_button",
        P,
        keys::TRANSLATE_SETTINGS_SHOW,
    )
    .section(keys::TRANSLATE_SETTINGS_SUBTITLE)
    .keywords(&["translator"]),
    Entry::new("language.translate_chats", P, keys::TRANSLATE_SETTINGS_CHAT)
        .section(keys::TRANSLATE_SETTINGS_SUBTITLE)
        .keywords(&["premium", "whole conversation"]),
    Entry::new(
        "language.do_not_translate",
        P,
        keys::TRANSLATE_SETTINGS_CHOOSE,
    )
    .section(keys::TRANSLATE_SETTINGS_SUBTITLE)
    .keywords(&["skip languages"]),
];
