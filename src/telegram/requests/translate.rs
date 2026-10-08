//! Translation requests (TDLib 1.8.67 `translateText` / `translateMessageText`).

use crate::ids::{ChatId, MessageId, RequestId};
use serde_json::json;

/// `translateText text:formattedText to_language_code:string tone:string =
/// FormattedText` (`schema/td_api.tl`, "Translates a text to the given
/// language"). The text goes as plain `formattedText` (a selection has no
/// entities to carry); `tone` is empty, which TDLib reads as "neutral".
pub fn translate_text(extra: RequestId, text: &str, to_language_code: &str) -> String {
    json!({
        "@type": "translateText",
        "@extra": extra.as_extra(),
        "text": { "@type": "formattedText", "text": text, "entities": [] },
        "to_language_code": to_language_code,
        "tone": "",
    })
    .to_string()
}

/// `translateMessageText chat_id:int53 message_id:int53 to_language_code:string
/// tone:string = FormattedText` — "Extracts text or caption of the given
/// message and translates it"; formatting is kept for Premium users. Must not
/// be used in secret chats.
pub fn translate_message_text(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    to_language_code: &str,
) -> String {
    json!({
        "@type": "translateMessageText",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "to_language_code": to_language_code,
        "tone": "",
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::{translate_message_text, translate_text};
    use crate::ids::{ChatId, MessageId, RequestId};
    use serde_json::Value;

    #[test]
    fn translate_text_request_shape() {
        let v: Value = serde_json::from_str(&translate_text(RequestId(5), "Привет", "en")).unwrap();
        assert_eq!(v["@type"], "translateText");
        assert_eq!(v["text"]["@type"], "formattedText");
        assert_eq!(v["text"]["text"], "Привет");
        assert_eq!(v["text"]["entities"], serde_json::json!([]));
        assert_eq!(v["to_language_code"], "en");
        assert_eq!(v["tone"], "");
    }

    #[test]
    fn translate_message_text_request_shape() {
        let v: Value = serde_json::from_str(&translate_message_text(
            RequestId(6),
            ChatId(-100123),
            MessageId(4096),
            "zh-CN",
        ))
        .unwrap();
        assert_eq!(v["@type"], "translateMessageText");
        assert_eq!(v["chat_id"], -100123);
        assert_eq!(v["message_id"], 4096);
        assert_eq!(v["to_language_code"], "zh-CN");
    }
}
