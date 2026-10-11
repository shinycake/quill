//! Parsers for animated emoji taps: the `sticker` answer to
//! `clickAnimatedEmojiMessage` and `updateAnimatedEmojiMessageClicked`.
use crate::telegram::envelope::{EnvelopePayload, StickersPayload, parse_sticker_value};
use serde_json::Value;

/// The bare `sticker` that answers `clickAnimatedEmojiMessage`.
pub(crate) fn parse_effect_sticker(value: &Value) -> EnvelopePayload {
    let (sticker, files) = parse_sticker_value(Some(value));
    EnvelopePayload::Stickers(StickersPayload::EmojiEffectSticker { sticker, files })
}

/// `updateAnimatedEmojiMessageClicked chat_id message_id sticker`.
pub(crate) fn parse_animated_emoji_clicked(value: &Value) -> EnvelopePayload {
    let (sticker, files) = parse_sticker_value(value.get("sticker"));
    EnvelopePayload::Stickers(StickersPayload::AnimatedEmojiMessageClicked {
        chat_id: value.get("chat_id").and_then(Value::as_i64).unwrap_or(0),
        message_id: value.get("message_id").and_then(Value::as_i64).unwrap_or(0),
        sticker,
        files,
    })
}

#[cfg(test)]
mod tests {
    use crate::ids::FileId;
    use crate::telegram::envelope::{EnvelopePayload, StickersPayload, parse_envelope};

    const STICKER: &str = r#"{"@type":"sticker","id":"9001","set_id":"77","width":512,"height":512,"emoji":"👍","format":{"@type":"stickerFormatTgs"},"full_type":{"@type":"stickerFullTypeRegular","premium_animation":null},"thumbnail":null,"sticker":{"@type":"file","id":42,"size":7,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"is_downloading_completed":false},"remote":{"@type":"remoteFile","id":"r","unique_id":"u"}}}"#;

    #[test]
    fn click_answer_parses_the_effect_sticker() {
        let json = STICKER.replacen("\"sticker\",", "\"sticker\",\"@extra\":\"3\",", 1);
        match parse_envelope(&json).unwrap().payload {
            EnvelopePayload::Stickers(StickersPayload::EmojiEffectSticker { sticker, files }) => {
                let sticker = sticker.expect("sticker");
                assert_eq!(sticker.file_id, FileId(42));
                assert_eq!(sticker.emoji, "👍");
                assert!(files.iter().any(|f| f.id == FileId(42)));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn clicked_update_parses_ids_and_sticker() {
        let json = format!(
            r#"{{"@type":"updateAnimatedEmojiMessageClicked","chat_id":5,"message_id":8388608,"sticker":{STICKER}}}"#
        );
        match parse_envelope(&json).unwrap().payload {
            EnvelopePayload::Stickers(StickersPayload::AnimatedEmojiMessageClicked {
                chat_id,
                message_id,
                sticker,
                ..
            }) => {
                assert_eq!((chat_id, message_id), (5, 8_388_608));
                assert_eq!(sticker.unwrap().file_id, FileId(42));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn clicked_update_without_sticker_is_kept_empty() {
        let json = r#"{"@type":"updateAnimatedEmojiMessageClicked","chat_id":5,"message_id":6}"#;
        match parse_envelope(json).unwrap().payload {
            EnvelopePayload::Stickers(StickersPayload::AnimatedEmojiMessageClicked {
                sticker,
                ..
            }) => assert!(sticker.is_none()),
            other => panic!("unexpected {other:?}"),
        }
    }
}
