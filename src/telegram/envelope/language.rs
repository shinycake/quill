//! Language pack answers and updates (docs/decisions/language-packs.md).
use super::*;
use crate::lang::PackValue;
use serde_json::Value;

/// Language pack answers and updates; wrapped as
/// `SettingsPayload::Language`.
#[derive(Debug, Clone, PartialEq)]
pub enum LanguagePayload {
    /// `localizationTargetInfo`: the official and installed packs.
    TargetInfo(Vec<LanguagePackInfoData>),
    /// `languagePackStrings`: the `getLanguagePackStrings` answer.
    Strings(Vec<(String, PackValue)>),
    /// `updateLanguagePackStrings`: changed strings of a pack; empty when
    /// every string changed (the pack is fetched again).
    UpdateStrings {
        localization_target: String,
        language_pack_id: String,
        strings: Vec<(String, PackValue)>,
    },
}

impl LanguagePayload {
    pub(crate) fn parse(type_name: &str, value: &Value) -> Option<Self> {
        match type_name {
            "localizationTargetInfo" => Some(Self::TargetInfo(
                value
                    .get("language_packs")
                    .and_then(Value::as_array)
                    .map(|packs| packs.iter().map(LanguagePackInfoData::parse).collect())
                    .unwrap_or_default(),
            )),
            "languagePackStrings" => Some(Self::Strings(parse_strings(value.get("strings")))),
            "updateLanguagePackStrings" => Some(Self::UpdateStrings {
                localization_target: json_field_str(value, "localization_target"),
                language_pack_id: json_field_str(value, "language_pack_id"),
                strings: parse_strings(value.get("strings")),
            }),
            _ => None,
        }
    }
}

/// A `languagePackString` list; entries without a key are skipped.
fn parse_strings(list: Option<&Value>) -> Vec<(String, PackValue)> {
    let Some(list) = list.and_then(Value::as_array) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|item| {
            let key = json_field_str(item, "key");
            if key.is_empty() {
                return None;
            }
            Some((key, parse_value(item.get("value"))))
        })
        .collect()
}

fn parse_value(value: Option<&Value>) -> PackValue {
    let Some(value) = value else {
        return PackValue::Deleted;
    };
    let text = |name: &str| json_field_str(value, name);
    match value.get("@type").and_then(Value::as_str) {
        Some("languagePackStringValueOrdinary") => PackValue::Ordinary(text("value")),
        Some("languagePackStringValuePluralized") => PackValue::Pluralized([
            text("zero_value"),
            text("one_value"),
            text("two_value"),
            text("few_value"),
            text("many_value"),
            text("other_value"),
        ]),
        _ => PackValue::Deleted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_strings_values_and_updates() {
        let value = json!({"@type": "languagePackStrings", "strings": [
            {"key": "lng_a", "value": {"@type": "languagePackStringValueOrdinary", "value": "A"}},
            {"key": "lng_b", "value": {"@type": "languagePackStringValuePluralized",
                "zero_value": "", "one_value": "1 b", "two_value": "", "few_value": "",
                "many_value": "", "other_value": "{count} b"}},
            {"key": "lng_c", "value": {"@type": "languagePackStringValueDeleted"}},
            {"key": "", "value": {"@type": "languagePackStringValueOrdinary", "value": "x"}},
        ]});
        let Some(LanguagePayload::Strings(strings)) =
            LanguagePayload::parse("languagePackStrings", &value)
        else {
            panic!("strings")
        };
        assert_eq!(strings.len(), 3);
        assert_eq!(
            strings[0],
            ("lng_a".into(), PackValue::Ordinary("A".into()))
        );
        assert!(
            matches!(&strings[1].1, PackValue::Pluralized(f) if f[1] == "1 b" && f[5] == "{count} b")
        );
        assert_eq!(strings[2].1, PackValue::Deleted);

        let update = json!({"@type": "updateLanguagePackStrings",
            "localization_target": "tdesktop", "language_pack_id": "de", "strings": []});
        assert_eq!(
            LanguagePayload::parse("updateLanguagePackStrings", &update),
            Some(LanguagePayload::UpdateStrings {
                localization_target: "tdesktop".into(),
                language_pack_id: "de".into(),
                strings: vec![],
            })
        );
        let info = json!({"@type": "localizationTargetInfo", "language_packs": [
            {"@type": "languagePackInfo", "id": "de", "base_language_pack_id": "",
             "name": "German", "native_name": "Deutsch", "plural_code": "de",
             "is_official": true, "is_rtl": false}]});
        let Some(LanguagePayload::TargetInfo(packs)) =
            LanguagePayload::parse("localizationTargetInfo", &info)
        else {
            panic!("target info")
        };
        assert_eq!(packs[0].plural_code, "de");
        assert!(packs[0].is_official);
    }
}
