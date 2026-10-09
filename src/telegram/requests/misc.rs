use crate::ids::RequestId;
use crate::pins::{TDLIB_CMAKE_VERSION, TDLIB_GIT_COMMIT};
use serde_json::json;

/// Phase S2: `getStorageStatistics` (TDLib 1.8.68,
/// `schema/td_api.tl:16373`):
/// `getStorageStatistics chat_limit:int32 = StorageStatistics;`
/// Drives the storage-usage overlay, including the "Secret media and
/// files" category (`fileTypeSecret`, td_api.tl:10088 — "The file was
/// sent to a secret chat (the file type is not known to the server)").
/// `chat_limit` 0 is honest here: the overlay aggregates by file type
/// across chats, so per-chat splits are not needed.
pub fn get_storage_statistics(extra: RequestId, chat_limit: i32) -> String {
    json!({
        "@type": "getStorageStatistics",
        "@extra": extra.as_extra(),
        "chat_limit": chat_limit,
    })
    .to_string()
}

/// Batch 4: `setOption name:string value:OptionValue = Ok;` (TDLib 1.8.68,
/// `schema/td_api.tl:16069`) with `optionValueBoolean`.
pub fn set_option_boolean(extra: RequestId, name: &str, value: bool) -> String {
    json!({
        "@type": "setOption",
        "@extra": extra.as_extra(),
        "name": name,
        "value": {"@type": "optionValueBoolean", "value": value},
    })
    .to_string()
}

/// Batch 6: `setOption` with `optionValueInteger` (an `int64`, which the
/// TDLib JSON interface carries as a string). `None` resets the option
/// to TDLib's default (`optionValueEmpty`).
pub fn set_option_integer(extra: RequestId, name: &str, value: Option<i64>) -> String {
    let value = match value {
        Some(v) => json!({"@type": "optionValueInteger", "value": v.to_string()}),
        None => json!({"@type": "optionValueEmpty"}),
    };
    json!({
        "@type": "setOption",
        "@extra": extra.as_extra(),
        "name": name,
        "value": value,
    })
    .to_string()
}

/// Batch 6: the `optimizeStorage` arguments (TDLib 1.8.68,
/// `schema/td_api.tl:16391`). `-1` means "TDLib's default limit" for
/// `size` / `ttl` / `count` / `immunity_delay`; `file_types` are
/// `FileType` constructor names (empty = every type except thumbnails,
/// profile photos, stickers and wallpapers); `chat_ids` empty = every
/// chat.
pub struct OptimizeStorage<'a> {
    pub size: i64,
    pub ttl: i32,
    pub count: i32,
    pub immunity_delay: i32,
    pub file_types: &'a [&'a str],
    pub chat_ids: &'a [i64],
    pub exclude_chat_ids: &'a [i64],
    pub chat_limit: i32,
}

impl OptimizeStorage<'_> {
    /// Clear everything the user can re-download (tdesktop "Clear all"):
    /// no size, age or count limit (0 deletes every candidate) and no
    /// immunity delay, over every type and chat.
    pub fn everything(chat_limit: i32) -> Self {
        OptimizeStorage {
            size: 0,
            ttl: 0,
            count: 0,
            immunity_delay: 0,
            file_types: &[],
            chat_ids: &[],
            exclude_chat_ids: &[],
            chat_limit,
        }
    }
}

/// Batch 6: `optimizeStorage` — deletes cached files and returns the
/// statistics of what was deleted (`return_deleted_file_statistics`).
pub fn optimize_storage(extra: RequestId, params: &OptimizeStorage<'_>) -> String {
    let file_types: Vec<_> = params
        .file_types
        .iter()
        .map(|name| json!({"@type": name}))
        .collect();
    json!({
        "@type": "optimizeStorage",
        "@extra": extra.as_extra(),
        "size": params.size,
        "ttl": params.ttl,
        "count": params.count,
        "immunity_delay": params.immunity_delay,
        "file_types": file_types,
        "chat_ids": params.chat_ids,
        "exclude_chat_ids": params.exclude_chat_ids,
        "return_deleted_file_statistics": true,
        "chat_limit": params.chat_limit,
    })
    .to_string()
}

pub fn runtime_version_request() -> String {
    json!({
        "@type": "getOption",
        "name": "version",
    })
    .to_string()
}

pub fn expected_runtime_label() -> String {
    format!("{TDLIB_CMAKE_VERSION} ({TDLIB_GIT_COMMIT})")
}
