//! Data and Storage page entries.

use crate::lang::keys;
use crate::settings_catalog::{Entry, Page};

const P: Page = Page::DataStorage;

pub(super) const ENTRIES: &[Entry] = &[
    Entry::new("data.export", P, keys::SETTINGS_EXPORT_DATA).keywords(&[
        "backup",
        "download my data",
        "json",
        "html",
    ]),
    Entry::new("data.data_saver", P, keys::Q_PREFS_DATA_SAVER)
        .section(keys::Q_OPS_AUTOMATIC_DOWNLOADS)
        .keywords(&["low data", "pause downloads"]),
    Entry::new(
        "data.auto_download_mobile",
        P,
        keys::Q_SETTINGS_DATA_AUTO_DOWNLOAD_MOBILE,
    )
    .section(keys::Q_OPS_AUTOMATIC_DOWNLOADS)
    .keywords(&["cellular"]),
    Entry::new(
        "data.auto_download_wifi",
        P,
        keys::Q_SETTINGS_DATA_AUTO_DOWNLOAD_WIFI,
    )
    .section(keys::Q_OPS_AUTOMATIC_DOWNLOADS)
    .keywords(&["wireless"]),
    Entry::new(
        "data.auto_download_roaming",
        P,
        keys::Q_SETTINGS_DATA_AUTO_DOWNLOAD_ROAMING,
    )
    .section(keys::Q_OPS_AUTOMATIC_DOWNLOADS)
    .keywords(&["abroad"]),
    Entry::new("data.max_file_size", P, keys::Q_OPS_MAXIMUM_FILE_SIZE)
        .section(keys::Q_OPS_AUTOMATIC_DOWNLOADS)
        .keywords(&["photos", "videos", "limit"]),
    Entry::new(
        "data.auto_download_chat_types",
        P,
        keys::Q_PREFS_AUTO_DOWNLOAD,
    )
    .section(keys::Q_OPS_AUTOMATIC_DOWNLOADS)
    .keywords(&["private", "groups", "channels", "gifs", "music"]),
    Entry::new(
        "data.less_data_calls",
        P,
        keys::Q_PREFS_USE_LESS_DATA_FOR_CALLS,
    )
    .section(keys::Q_OPS_AUTOMATIC_DOWNLOADS)
    .keywords(&["bandwidth", "traffic"]),
    Entry::new("data.download_folder", P, keys::Q_OPS_DOWNLOAD_FOLDER)
        .section(keys::ALL_DOWNLOADS)
        .keywords(&["save location", "directory", "path"]),
    Entry::new(
        "data.ask_where_to_save",
        P,
        keys::Q_OPS_ASK_WHERE_TO_SAVE_EACH,
    )
    .section(keys::ALL_DOWNLOADS)
    .keywords(&["save as", "prompt"]),
    Entry::new("data.storage_usage", P, keys::Q_OPS_STORAGE_USAGE)
        .keywords(&["cache", "clear", "disk", "space"]),
    Entry::new("data.clear_cache", P, keys::Q_SETTINGS_DATA_CLEAR_CACHE)
        .section(keys::Q_OPS_STORAGE_USAGE)
        .keywords(&["free space", "delete cache"]),
    Entry::new("data.size_limit", P, keys::LOCAL_STORAGE_SIZE_LIMIT_TITLE)
        .section(keys::Q_OPS_STORAGE_USAGE)
        .keywords(&["maximum cache", "gb"]),
    Entry::new("data.clear_older", P, keys::LOCAL_STORAGE_TIME_LIMIT_TITLE)
        .section(keys::Q_OPS_STORAGE_USAGE)
        .keywords(&["auto clear", "days", "keep media"]),
    Entry::new("data.network_usage", P, keys::Q_OPS_NETWORK_USAGE).keywords(&[
        "traffic",
        "bandwidth",
        "sent",
        "received",
    ]),
];
