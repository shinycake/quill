//! Answers shown by deep links: `premiumGiftCodeInfo` (`t.me/giftcode/<code>`)
//! and `languagePackInfo` (`t.me/setlanguage/<id>`).
use super::*;
use serde_json::Value;

/// `premiumGiftCodeInfo` (schema `td_api.tl:1401`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GiftCodeInfoData {
    /// Who created the code; `None` when TDLib does not say.
    pub creator: Option<MessageSender>,
    pub creation_date: i64,
    pub is_from_giveaway: bool,
    /// 0 when the length is not a whole number of months.
    pub month_count: i64,
    pub day_count: i64,
    /// The user the code was made for; 0 if none.
    pub user_id: i64,
    /// When the code was activated; 0 if it was not.
    pub use_date: i64,
}

impl GiftCodeInfoData {
    pub(crate) fn parse(value: &Value) -> Self {
        Self {
            creator: parse_message_sender(value.get("creator_id")).ok(),
            creation_date: int53_or_zero(value.get("creation_date")),
            is_from_giveaway: value
                .get("is_from_giveaway")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            month_count: int53_or_zero(value.get("month_count")),
            day_count: int53_or_zero(value.get("day_count")),
            user_id: int53_or_zero(value.get("user_id")),
            use_date: int53_or_zero(value.get("use_date")),
        }
    }

    pub fn is_used(&self) -> bool {
        self.use_date > 0
    }
}

/// `languagePackInfo` (schema `td_api.tl:8332`), the fields the link box shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguagePackInfoData {
    pub id: String,
    pub name: String,
    pub native_name: String,
    pub is_official: bool,
    pub is_rtl: bool,
    pub is_beta: bool,
    pub is_installed: bool,
    pub total_string_count: i64,
    pub translated_string_count: i64,
}

impl LanguagePackInfoData {
    pub(crate) fn parse(value: &Value) -> Self {
        let flag = |key: &str| value.get(key).and_then(Value::as_bool).unwrap_or(false);
        Self {
            id: json_field_str(value, "id"),
            name: json_field_str(value, "name"),
            native_name: json_field_str(value, "native_name"),
            is_official: flag("is_official"),
            is_rtl: flag("is_rtl"),
            is_beta: flag("is_beta"),
            is_installed: flag("is_installed"),
            total_string_count: int53_or_zero(value.get("total_string_count")),
            translated_string_count: int53_or_zero(value.get("translated_string_count")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{GiftCodeInfoData, LanguagePackInfoData};
    use crate::telegram::envelope::MessageSender;
    use serde_json::Value;

    #[test]
    fn gift_code_info_parses_creator_and_use_date() {
        let value: Value = serde_json::from_str(
            r#"{"@type":"premiumGiftCodeInfo","creator_id":{"@type":"messageSenderChat","chat_id":-100},"creation_date":1700000000,"is_from_giveaway":true,"month_count":3,"day_count":90,"user_id":0,"use_date":0}"#,
        )
        .unwrap();
        let info = GiftCodeInfoData::parse(&value);
        assert_eq!(info.creator, Some(MessageSender::Chat { chat_id: -100 }));
        assert_eq!(info.month_count, 3);
        assert!(info.is_from_giveaway && !info.is_used());
    }

    #[test]
    fn gift_code_info_without_creator_is_none() {
        let value: Value = serde_json::from_str(
            r#"{"@type":"premiumGiftCodeInfo","month_count":0,"day_count":10,"use_date":5}"#,
        )
        .unwrap();
        let info = GiftCodeInfoData::parse(&value);
        assert_eq!(info.creator, None);
        assert!(info.is_used());
    }

    #[test]
    fn language_pack_info_parses() {
        let value: Value = serde_json::from_str(
            r#"{"@type":"languagePackInfo","id":"pt","name":"Portuguese","native_name":"Português","is_rtl":false,"is_beta":true,"is_official":false,"is_installed":false,"total_string_count":100,"translated_string_count":90}"#,
        )
        .unwrap();
        let info = LanguagePackInfoData::parse(&value);
        assert_eq!(info.id, "pt");
        assert_eq!(info.native_name, "Português");
        assert!(info.is_beta && !info.is_official);
        assert_eq!(info.translated_string_count, 90);
    }

    #[test]
    fn envelopes_route_to_payments_and_settings() {
        use crate::telegram::envelope::{
            EnvelopePayload, PaymentsPayload, SettingsPayload, parse_envelope,
        };
        let env =
            parse_envelope(r#"{"@type":"premiumGiftCodeInfo","month_count":3,"day_count":90}"#)
                .unwrap();
        assert!(matches!(
            env.payload,
            EnvelopePayload::Payments(PaymentsPayload::GiftCodeInfo(_))
        ));
        let env = parse_envelope(r#"{"@type":"languagePackInfo","id":"pt"}"#).unwrap();
        assert!(matches!(
            env.payload,
            EnvelopePayload::Settings(SettingsPayload::LanguagePackInfo(_))
        ));
    }
}
