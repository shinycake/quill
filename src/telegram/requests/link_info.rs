//! Requests behind deep-link boxes: gift codes and language packs.
use crate::ids::RequestId;
use serde_json::json;

/// `checkPremiumGiftCode`. Response is `premiumGiftCodeInfo`.
pub fn check_premium_gift_code(extra: RequestId, code: &str) -> String {
    json!({
        "@type": "checkPremiumGiftCode",
        "@extra": extra.as_extra(),
        "code": code,
    })
    .to_string()
}

/// `applyPremiumGiftCode`. Response is `ok`. Only sent after an explicit click.
pub fn apply_premium_gift_code(extra: RequestId, code: &str) -> String {
    json!({
        "@type": "applyPremiumGiftCode",
        "@extra": extra.as_extra(),
        "code": code,
    })
    .to_string()
}

/// `getLanguagePackInfo`. Response is `languagePackInfo`.
pub fn get_language_pack_info(extra: RequestId, language_pack_id: &str) -> String {
    json!({
        "@type": "getLanguagePackInfo",
        "@extra": extra.as_extra(),
        "language_pack_id": language_pack_id,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::{apply_premium_gift_code, check_premium_gift_code, get_language_pack_info};
    use crate::ids::RequestId;
    use serde_json::Value;

    #[test]
    fn requests_carry_their_arguments() {
        let v: Value = serde_json::from_str(&check_premium_gift_code(RequestId(1), "abc")).unwrap();
        assert_eq!(v["@type"], "checkPremiumGiftCode");
        assert_eq!(v["code"], "abc");
        let v: Value = serde_json::from_str(&apply_premium_gift_code(RequestId(2), "abc")).unwrap();
        assert_eq!(v["@type"], "applyPremiumGiftCode");
        let v: Value = serde_json::from_str(&get_language_pack_info(RequestId(3), "pt")).unwrap();
        assert_eq!(v["language_pack_id"], "pt");
    }
}
