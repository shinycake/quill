use super::super::{MessageContent, StickerFormat, parse_content};
use crate::ids::FileId;
use crate::sticker_playback::FitzType;
use crate::text::TextEntityKind;
use serde_json::{Value, json};

fn file(id: i32) -> Value {
    json!({
        "@type": "file", "id": id, "size": 10, "expected_size": 10,
        "local": {"@type": "localFile", "path": "", "can_be_downloaded": true,
            "can_be_deleted": false, "is_downloading_active": false,
            "is_downloading_completed": false, "download_offset": 0,
            "downloaded_prefix_size": 0, "downloaded_size": 0},
        "remote": {"@type": "remoteFile", "id": "r", "unique_id": "u",
            "is_uploading_active": false, "is_uploading_completed": true, "uploaded_size": 10}
    })
}

fn sticker(full_type: Value) -> Value {
    json!({
        "@type": "sticker", "id": "900", "set_id": "1", "width": 512, "height": 512,
        "emoji": "👍", "format": {"@type": "stickerFormatTgs"},
        "full_type": full_type, "thumbnail": null, "sticker": file(31)
    })
}

fn message(emoji: &str, animated: Value) -> Value {
    json!({"@type": "messageAnimatedEmoji", "emoji": emoji, "animated_emoji": animated})
}

#[test]
fn a_lone_emoji_keeps_its_text_and_carries_the_sticker_and_skin_tone() {
    let value = message(
        "👍🏽",
        json!({
            "@type": "animatedEmoji",
            "sticker": sticker(json!({"@type": "stickerFullTypeRegular", "premium_animation": null})),
            "sticker_width": 512, "sticker_height": 512, "fitzpatrick_type": 4,
            "sound": file(32), "is_regular_emoji": true
        }),
    );
    let (content, files) = parse_content(Some(&value));
    let MessageContent::Text(text) = content else {
        panic!("a text message");
    };
    assert_eq!(text.text, "👍🏽");
    assert!(text.entities.is_empty());
    let animated = text.animated.expect("the sticker");
    assert_eq!(animated.sticker.file_id, FileId(31));
    assert_eq!(animated.sticker.format, StickerFormat::Tgs);
    assert_eq!(animated.fitz, FitzType::Type4);
    assert_eq!(animated.sound, Some(FileId(32)));
    assert!(animated.is_regular_emoji);
    assert_eq!((animated.width, animated.height), (512, 512));
    // The sticker and the sound are remembered with the message.
    let ids: Vec<i32> = files.iter().map(|file| file.id.0).collect();
    assert_eq!(ids, [31, 32]);
}

#[test]
fn an_emoji_without_a_skin_tone_draws_the_sticker_as_is() {
    let value = message(
        "🎉",
        json!({
            "@type": "animatedEmoji",
            "sticker": sticker(json!({"@type": "stickerFullTypeRegular", "premium_animation": null})),
            "sticker_width": 512, "sticker_height": 512, "fitzpatrick_type": 0,
            "sound": null, "is_regular_emoji": true
        }),
    );
    let (MessageContent::Text(text), files) = parse_content(Some(&value)) else {
        panic!("a text message");
    };
    let animated = text.animated.expect("the sticker");
    assert_eq!(animated.fitz, FitzType::None);
    assert_eq!(animated.sound, None);
    assert_eq!(files.len(), 1);
}

#[test]
fn a_custom_emoji_gets_its_entity_back() {
    let value = message(
        "😀",
        json!({
            "@type": "animatedEmoji",
            "sticker": sticker(json!({
                "@type": "stickerFullTypeCustomEmoji",
                "custom_emoji_id": "5368324170671202286", "needs_repainting": false
            })),
            "sticker_width": 100, "sticker_height": 100, "fitzpatrick_type": 0,
            "sound": null, "is_regular_emoji": false
        }),
    );
    let (MessageContent::Text(text), _) = parse_content(Some(&value)) else {
        panic!("a text message");
    };
    assert_eq!(text.text, "😀");
    assert_eq!(text.entities.len(), 1);
    let entity = &text.entities[0];
    assert_eq!((entity.utf8_start, entity.utf8_end), (0, "😀".len()));
    assert_eq!(
        entity.kind,
        TextEntityKind::CustomEmoji {
            custom_emoji_id: 5_368_324_170_671_202_286
        }
    );
    let animated = text.animated.expect("the custom emoji's sticker");
    assert!(!animated.is_regular_emoji);
    assert_eq!(
        animated.sticker.custom_emoji_id,
        Some(5_368_324_170_671_202_286)
    );
}

#[test]
fn an_unknown_sticker_leaves_plain_text() {
    for animated in [
        Value::Null,
        json!({"@type": "animatedEmoji", "sticker": null, "sticker_width": 100,
            "sticker_height": 100, "fitzpatrick_type": 0, "sound": null,
            "is_regular_emoji": false}),
    ] {
        let (MessageContent::Text(text), files) = parse_content(Some(&message("🥰", animated)))
        else {
            panic!("a text message");
        };
        assert_eq!(text.text, "🥰");
        assert!(text.entities.is_empty());
        assert!(text.animated.is_none());
        assert!(files.is_empty());
    }
}
