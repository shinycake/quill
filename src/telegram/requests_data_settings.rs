//! Slice S4 (Data & Storage screen): `setAutoDownloadSettings` /
//! `removeAllFilesFromDownloads` / `getAutoDownloadSettingsPresets`
//! request builders (schema 1.8.67).

use crate::ids::RequestId;
use serde_json::{Value, json};

/// Slice S4: `setAutoDownloadSettings` (schema 1.8.67, :15820 —
/// "Sets auto-download settings"). `settings` is a serialized
/// `autoDownloadSettings` (see
/// `crate::data_settings::AutoDownloadNetSettings::to_json`);
/// `network_td_type` is a verbatim `NetworkType` constructor name (see
/// `crate::data_settings::NetworkKind::td_type`).
pub fn set_auto_download_settings(
    extra: RequestId,
    settings: Value,
    network_td_type: &str,
) -> String {
    json!({
        "@type": "setAutoDownloadSettings",
        "@extra": extra.as_extra(),
        "settings": settings,
        "type": {"@type": network_td_type},
    })
    .to_string()
}

/// Slice S4: `removeAllFilesFromDownloads` (schema 1.8.67, :14056 —
/// `removeAllFilesFromDownloads only_active:Bool only_completed:Bool
/// delete_from_cache:Bool = Ok;`). "Clear cache": completed downloads
/// are dropped from the filesystem cache; in-flight downloads are left
/// alone (`only_active: false`, `only_completed: true`).
pub fn remove_all_files_from_downloads(extra: RequestId) -> String {
    json!({
        "@type": "removeAllFilesFromDownloads",
        "@extra": extra.as_extra(),
        "only_active": false,
        "only_completed": true,
        "delete_from_cache": true,
    })
    .to_string()
}

/// Slice S4: `getAutoDownloadSettingsPresets` (schema 1.8.67, :15817 —
/// "Returns auto-download settings presets for the current user").
/// The local prefs are seeded from these once (Wi-Fi ← high,
/// mobile ← medium, roaming ← low); TDLib has no getter for the
/// *current* per-network settings.
pub fn get_auto_download_settings_presets(extra: RequestId) -> String {
    json!({
        "@type": "getAutoDownloadSettingsPresets",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telegram::envelope::{EnvelopePayload, parse_envelope};
    use serde_json::Value;

    /// Slice S4: `autoDownloadSettingsPresets` parses into
    /// (low, medium, high); a missing leg is a malformed answer.
    #[test]
    fn auto_download_settings_presets_response_parses() {
        let json = r#"{"@type":"autoDownloadSettingsPresets","low":{"@type":"autoDownloadSettings","is_auto_download_enabled":false,"max_photo_file_size":0,"max_video_file_size":0,"max_other_file_size":0,"video_upload_bitrate":0,"preload_large_videos":false,"preload_next_audio":false,"preload_stories":false,"use_less_data_for_calls":false},"medium":{"@type":"autoDownloadSettings","is_auto_download_enabled":true,"max_photo_file_size":1048576,"max_video_file_size":15728640,"max_other_file_size":0,"video_upload_bitrate":0,"preload_large_videos":false,"preload_next_audio":true,"preload_stories":false,"use_less_data_for_calls":false},"high":{"@type":"autoDownloadSettings","is_auto_download_enabled":true,"max_photo_file_size":52428800,"max_video_file_size":524288000,"max_other_file_size":524288000,"video_upload_bitrate":0,"preload_large_videos":true,"preload_next_audio":true,"preload_stories":true,"use_less_data_for_calls":false}}"#;
        let env = parse_envelope(json).unwrap();
        match env.payload {
            EnvelopePayload::AutoDownloadSettingsPresets { low, medium, high } => {
                assert!(!low.is_auto_download_enabled);
                assert!(medium.is_auto_download_enabled);
                assert_eq!(medium.max_photo_file_size, 1048576);
                assert!(medium.preload_next_audio);
                assert!(high.preload_large_videos);
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(parse_envelope(r#"{"@type":"autoDownloadSettingsPresets"}"#).is_err());
    }

    fn parsed(json: &str) -> Value {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn set_auto_download_settings_shape_matches_1_8_67() {
        // `setAutoDownloadSettings settings:autoDownloadSettings
        // type:NetworkType = Ok;` (schema:15820).
        let settings = json!({"@type": "autoDownloadSettings"});
        let v = parsed(&set_auto_download_settings(
            RequestId(41),
            settings,
            "networkTypeWiFi",
        ));
        assert_eq!(v["@type"], "setAutoDownloadSettings");
        assert_eq!(v["settings"]["@type"], "autoDownloadSettings");
        assert_eq!(v["type"]["@type"], "networkTypeWiFi");
        assert!(v["@extra"].is_number() || v["@extra"].is_string());
    }

    #[test]
    fn remove_all_files_from_downloads_clears_cache_not_active() {
        // `removeAllFilesFromDownloads only_active:Bool
        // only_completed:Bool delete_from_cache:Bool = Ok;`
        // (schema:14056): clear the cache, keep active downloads.
        let v = parsed(&remove_all_files_from_downloads(RequestId(42)));
        assert_eq!(v["@type"], "removeAllFilesFromDownloads");
        assert_eq!(v["only_active"], false);
        assert_eq!(v["only_completed"], true);
        assert_eq!(v["delete_from_cache"], true);
    }

    #[test]
    fn get_auto_download_settings_presets_shape_matches_1_8_67() {
        // `getAutoDownloadSettingsPresets = AutoDownloadSettingsPresets;`
        // (schema:15817).
        let v = parsed(&get_auto_download_settings_presets(RequestId(43)));
        assert_eq!(v["@type"], "getAutoDownloadSettingsPresets");
    }
}
