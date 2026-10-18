//! Interface language packs (docs/decisions/language-packs.md): the
//! options that point TDLib at Telegram Desktop's strings, and the pack
//! requests of Settings → Language.
use crate::ids::RequestId;
use serde_json::json;

/// `setOption` with `optionValueString` (`localization_target`,
/// `language_pack_database_path`, `language_pack_id`). Response is `ok`.
pub fn set_option_string(extra: RequestId, name: &str, value: &str) -> String {
    json!({
        "@type": "setOption",
        "@extra": extra.as_extra(),
        "name": name,
        "value": {"@type": "optionValueString", "value": value},
    })
    .to_string()
}

/// `getLocalizationTargetInfo`. Response is `localizationTargetInfo` (the
/// official and installed packs).
pub fn get_localization_target_info(extra: RequestId, only_local: bool) -> String {
    json!({
        "@type": "getLocalizationTargetInfo",
        "@extra": extra.as_extra(),
        "only_local": only_local,
    })
    .to_string()
}

/// `getLanguagePackStrings` with no keys: every string of the pack.
/// Response is `languagePackStrings`.
pub fn get_language_pack_strings(extra: RequestId, language_pack_id: &str) -> String {
    json!({
        "@type": "getLanguagePackStrings",
        "@extra": extra.as_extra(),
        "language_pack_id": language_pack_id,
        "keys": [],
    })
    .to_string()
}

/// `getLanguagePackStrings` for the given keys only.
/// Response is `languagePackStrings`.
pub fn get_language_pack_keys(extra: RequestId, language_pack_id: &str, keys: &[String]) -> String {
    json!({
        "@type": "getLanguagePackStrings",
        "@extra": extra.as_extra(),
        "language_pack_id": language_pack_id,
        "keys": keys,
    })
    .to_string()
}

/// `synchronizeLanguagePack`: fetch the latest strings of a pack (changes
/// then arrive as `updateLanguagePackStrings`). Response is `ok`.
pub fn synchronize_language_pack(extra: RequestId, language_pack_id: &str) -> String {
    json!({
        "@type": "synchronizeLanguagePack",
        "@extra": extra.as_extra(),
        "language_pack_id": language_pack_id,
    })
    .to_string()
}

/// `addCustomServerLanguagePack`: install an unofficial pack opened from a
/// `t.me/setlanguage` link. Response is `ok`.
pub fn add_custom_server_language_pack(extra: RequestId, language_pack_id: &str) -> String {
    json!({
        "@type": "addCustomServerLanguagePack",
        "@extra": extra.as_extra(),
        "language_pack_id": language_pack_id,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn parse(json: &str) -> Value {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn builds_language_requests() {
        let extra = RequestId(7);
        let v = parse(&set_option_string(extra, "localization_target", "tdesktop"));
        assert_eq!(v["@type"], "setOption");
        assert_eq!(v["value"]["@type"], "optionValueString");
        assert_eq!(v["value"]["value"], "tdesktop");
        let v = parse(&get_localization_target_info(extra, false));
        assert_eq!(v["only_local"], false);
        let v = parse(&get_language_pack_strings(extra, "pt-br"));
        assert_eq!(v["@type"], "getLanguagePackStrings");
        assert_eq!(v["keys"], serde_json::json!([]));
        let v = parse(&get_language_pack_keys(extra, "en", &["a".to_string()]));
        assert_eq!(v["keys"], serde_json::json!(["a"]));
        let v = parse(&synchronize_language_pack(extra, "pt-br"));
        assert_eq!(v["language_pack_id"], "pt-br");
        let v = parse(&add_custom_server_language_pack(extra, "x-custom"));
        assert_eq!(v["@type"], "addCustomServerLanguagePack");
    }
}
