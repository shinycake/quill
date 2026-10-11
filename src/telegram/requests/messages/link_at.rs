//! `getMessageLink` with a media timestamp: the "Share at 0:12" link of a
//! video post (`media_timestamp` in schema/td_api.tl).

use super::*;

/// `getMessageLink` that starts the video at `media_timestamp` seconds
/// (the link ends in `?t=<seconds>`).
pub fn get_message_link_at(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    media_timestamp: i32,
) -> String {
    json!({
        "@type": "getMessageLink",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "media_timestamp": media_timestamp.max(0),
        "checklist_task_id": 0,
        "poll_option_id": "",
        "for_album": false,
        "in_message_thread": false,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carries_the_media_timestamp() {
        let json: serde_json::Value = serde_json::from_str(&get_message_link_at(
            RequestId(3),
            ChatId(-100),
            MessageId(55),
            72,
        ))
        .unwrap();
        assert_eq!(json["@type"], "getMessageLink");
        assert_eq!(json["chat_id"], -100);
        assert_eq!(json["message_id"], 55);
        assert_eq!(json["media_timestamp"], 72);
        assert_eq!(json["for_album"], false);
    }

    #[test]
    fn a_negative_position_becomes_zero() {
        let json: serde_json::Value = serde_json::from_str(&get_message_link_at(
            RequestId(3),
            ChatId(1),
            MessageId(2),
            -4,
        ))
        .unwrap();
        assert_eq!(json["media_timestamp"], 0);
    }
}
