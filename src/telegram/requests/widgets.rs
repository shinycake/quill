//! Message widget requests (TDLib 1.8.70, `schema/td_api.tl`).
use crate::ids::{ChatId, MessageId, RequestId};
use serde_json::json;

/// `getWidgetQueryAnswer chat_id message_id query_type data`: a widget's
/// request to its bot. The type is always `widgetQueryTypeData` without the
/// wallet card: Quill never takes part in TON wallet queries
/// (docs/decisions/message-widgets.md). `data` is `bytes`, base64 in JSON.
/// TDLib answers `widgetQueryAnswer`, or error 502 when the bot is late.
pub fn get_widget_query_answer(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    data: &[u8],
) -> String {
    use base64::Engine;
    json!({
        "@type": "getWidgetQueryAnswer",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "query_type": {"@type": "widgetQueryTypeData", "with_wallet": false},
        "data": base64::engine::general_purpose::STANDARD.encode(data),
    })
    .to_string()
}

/// `getWidgetPublicData chat_id message_id`: only while the widget is on
/// screen and asked to poll. TDLib answers `widgetPublicData`.
pub fn get_widget_public_data(extra: RequestId, chat_id: ChatId, message_id: MessageId) -> String {
    json!({
        "@type": "getWidgetPublicData",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
    })
    .to_string()
}
