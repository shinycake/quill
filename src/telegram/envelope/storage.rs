use crate::data_settings::StorageChatStats;

/// Phase S2: one aggregated file-type entry of a `getStorageStatistics`
/// answer. `storageStatisticsByFileType file_type:FileType size:int53
/// count:int32 = StorageStatisticsByFileType;` (schema 1.8.67, line
/// 9780), summed across the `by_chat` entries (schema line 9787), TGX
/// `TGStorageStats.Entry` style. `fileTypeSecret` (line 9728, "The file
/// was sent to a secret chat (the file type is not known to the
/// server)") is kept as its own entry so the UI can show TGX's
/// "Secret media and files" category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageFileTypeStats {
    /// The `fileType` constructor name, e.g. `fileTypeSecret`.
    pub file_type: String,
    pub size: i64,
    pub count: i32,
}

/// Phase S2: aggregated `getStorageStatistics` answer (`storageStatistics
/// size:int53 count:int32 by_chat:vector<storageStatisticsByChat> =
/// StorageStatistics;`, schema 1.8.67, line 9793). Entries are unordered;
/// the UI orders by its fixed category list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StorageStats {
    pub total_size: i64,
    pub by_file_type: Vec<StorageFileTypeStats>,
    /// Slice S4: per-chat entries (`storageStatisticsByChat
    /// chat_id:int53 size:int53 count:int32
    /// by_file_type:vector<storageStatisticsByFileType> =
    /// StorageStatisticsByChat;`, schema 1.8.67, line 9787) — the usage
    /// screen's per-chat breakdown. Empty when `chat_limit` was 0.
    pub by_chat: Vec<StorageChatStats>,
}

/// Phase S2: storage-usage category order, matching TGX
/// `SettingsCacheController`'s `switch` over `TGStorageStats` file
/// types.
pub(crate) const STORAGE_CATEGORY_ORDER: &[&str] = &[
    "fileTypePhoto",
    "fileTypeVideo",
    "fileTypeVoiceNote",
    "fileTypeVideoNote",
    "fileTypeDocument",
    "fileTypeAudio",
    "fileTypeAnimation",
    "fileTypeSecret",
    "fileTypeThumbnail",
    "fileTypeSticker",
    "fileTypeProfilePhoto",
    "fileTypeWallpaper",
];

/// Phase S2: storage-usage category labels (TGX copy; `fileTypeSecret`
/// → `SecretFiles` "Secret media and files",
/// `app/src/main/res/values/strings.xml:1679`). Unknown types fold
/// into "Other" (TGX buckets `fileTypeSecretThumbnail` in its
/// internal database entry — "Other" is the honest minimal
/// equivalent).
pub(crate) fn storage_category_label(file_type: &str) -> &'static str {
    match file_type {
        "fileTypePhoto" => "Photos",
        "fileTypeVideo" => "Videos",
        "fileTypeVoiceNote" => "Voice messages",
        "fileTypeVideoNote" => "Video messages",
        "fileTypeDocument" => "Files",
        "fileTypeAudio" => "Music",
        "fileTypeAnimation" => "GIFs",
        "fileTypeSecret" => "Secret media and files",
        "fileTypeThumbnail" => "Thumbnails",
        "fileTypeSticker" => "Stickers",
        "fileTypeProfilePhoto" => "Profile photos",
        "fileTypeWallpaper" => "Wallpapers",
        _ => "Other",
    }
}

/// Phase S2: category rows for the storage overlay in TGX order,
/// skipping empty categories (TGX skips zero-size entries). Leftovers
/// (unknown types, `fileTypeSecretThumbnail`) aggregate into "Other".
pub fn storage_category_rows(stats: &StorageStats) -> Vec<(&'static str, i64, i32)> {
    let mut rows = Vec::new();
    for file_type in STORAGE_CATEGORY_ORDER {
        if let Some(entry) = stats
            .by_file_type
            .iter()
            .find(|e| e.file_type == *file_type)
            && entry.size > 0
        {
            rows.push((storage_category_label(file_type), entry.size, entry.count));
        }
    }
    let (other_size, other_count) = stats
        .by_file_type
        .iter()
        .filter(|e| !STORAGE_CATEGORY_ORDER.contains(&e.file_type.as_str()))
        .fold((0i64, 0i32), |(size, count), e| {
            (size.saturating_add(e.size), count.saturating_add(e.count))
        });
    if other_size > 0 || other_count > 0 {
        rows.push(("Other", other_size, other_count));
    }
    rows
}
