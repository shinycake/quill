use crate::ids::RequestId;
use crate::pins::{TDLIB_CMAKE_VERSION, TDLIB_GIT_COMMIT};
use serde_json::json;

/// Phase S2: `getStorageStatistics` (TDLib 1.8.67,
/// `schema/td_api.tl:15781`):
/// `getStorageStatistics chat_limit:int32 = StorageStatistics;`
/// Drives the storage-usage overlay, including the "Secret media and
/// files" category (`fileTypeSecret`, td_api.tl:9728 — "The file was
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
