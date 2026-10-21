//! Folders page entries.

use crate::lang::keys;
use crate::settings_catalog::{Entry, Page};

const P: Page = Page::Folders;

pub(super) const ENTRIES: &[Entry] = &[
    Entry::new("folders.manage", P, keys::Q_CHATLIST_MANAGE_CHAT_FOLDERS).keywords(&[
        "edit folders",
        "reorder",
        "filters",
    ]),
    Entry::new("folders.new_folder", P, keys::FILTERS_NEW).keywords(&["create folder"]),
    Entry::new("folders.recommended", P, keys::FILTERS_RECOMMENDED)
        .keywords(&["suggested folders"]),
    Entry::new("folders.folder_tags", P, keys::FILTERS_ENABLE_TAGS).keywords(&["labels", "badges"]),
    Entry::new("folders.tabs_view", P, keys::FILTERS_VIEW_SUBTITLE)
        .section(keys::Q_SETTINGS_FOLDERS_SEC_TABS)
        .keywords(&["position", "layout"]),
    Entry::new("folders.tabs_appearance", P, keys::FILTERS_TABS_SUBTITLE)
        .section(keys::Q_SETTINGS_FOLDERS_SEC_TABS)
        .keywords(&["icons", "text", "labels"]),
    Entry::new(
        "folders.archive_main_menu",
        P,
        keys::CONTEXT_ARCHIVE_TO_MENU,
    )
    .section(keys::Q_CHAT_ARCHIVE)
    .keywords(&["archived chats", "hide archive"]),
    Entry::new(
        "folders.archive_collapsed",
        P,
        keys::Q_SETTINGS_FOLDERS_ARCHIVE_COLLAPSED,
    )
    .section(keys::Q_CHAT_ARCHIVE)
    .keywords(&["archived chats", "hide archive"]),
];
