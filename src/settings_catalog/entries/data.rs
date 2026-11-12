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
    Entry::new("data.auto_download", P, keys::MEDIA_AUTO_SETTINGS).keywords(&[
        "auto download",
        "automatic downloads",
        "pause downloads",
        "data saver",
    ]),
    Entry::new("data.auto_download_private", P, keys::MEDIA_AUTO_IN_PRIVATE)
        .section(keys::MEDIA_AUTO_SETTINGS)
        .keywords(&["photos", "videos", "files", "gifs", "size limit"]),
    Entry::new("data.auto_download_groups", P, keys::MEDIA_AUTO_IN_GROUPS)
        .section(keys::MEDIA_AUTO_SETTINGS)
        .keywords(&["photos", "videos", "files", "gifs", "size limit"]),
    Entry::new(
        "data.auto_download_channels",
        P,
        keys::MEDIA_AUTO_IN_CHANNELS,
    )
    .section(keys::MEDIA_AUTO_SETTINGS)
    .keywords(&["photos", "videos", "files", "gifs", "size limit"]),
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
