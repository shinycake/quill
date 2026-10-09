use crate::ids::{ChatId, FileId, MessageId, RequestId};
use crate::telegram::requests::*;
use serde_json::Value;

#[test]
fn send_sticker_uses_input_file_id_and_int64_set() {
    let json = send_sticker(
        RequestId(13),
        ChatId(7),
        StickerSend {
            file_id: FileId(41),
            emoji: "😀",
            width: 512,
            height: 512,
            thumb: Some((FileId(42), 128, 128)),
            reply_to: Some(SendReply::plain(MessageId(101))),
            topic_id: None,
        },
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "sendMessage");
    assert_eq!(v["input_message_content"]["@type"], "inputMessageSticker");
    assert_eq!(v["input_message_content"]["emoji"], "😀");
    let sticker = &v["input_message_content"]["sticker"];
    assert_eq!(sticker["@type"], "inputSticker");
    assert_eq!(sticker["sticker"]["@type"], "inputFileId");
    assert_eq!(sticker["sticker"]["id"], 41);
    assert_eq!(sticker["width"], 512);
    assert_eq!(sticker["height"], 512);
    assert_eq!(sticker["thumbnail"]["@type"], "inputThumbnail");
    assert_eq!(sticker["thumbnail"]["thumbnail"]["id"], 42);
    assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToMessage");
    let installed = get_installed_sticker_sets(RequestId(14));
    let installed: serde_json::Value = serde_json::from_str(&installed).unwrap();
    assert_eq!(installed["@type"], "getInstalledStickerSets");
    assert_eq!(installed["sticker_type"]["@type"], "stickerTypeRegular");
    let set = get_sticker_set(RequestId(15), 77);
    let set: serde_json::Value = serde_json::from_str(&set).unwrap();
    assert_eq!(set["set_id"], "77");
    let bare = send_sticker(
        RequestId(16),
        ChatId(7),
        StickerSend {
            file_id: FileId(41),
            emoji: "",
            width: 512,
            height: 512,
            thumb: None,
            reply_to: None,
            topic_id: None,
        },
    );
    let bare: serde_json::Value = serde_json::from_str(&bare).unwrap();
    assert_eq!(
        bare["input_message_content"]["sticker"]["thumbnail"],
        Value::Null
    );
}

/// Slice S8: the eleven sticker-set backend request shapes against the
/// pinned schema (1.8.67): `getTrendingStickerSets` (:14669),
/// `viewTrendingStickerSets` (:14695), `searchStickerSets` (:14689),
/// `searchStickers` (:14648), `getFavoriteStickers` (:14716),
/// `addFavoriteSticker` (:14721), `removeFavoriteSticker` (:14724),
/// `getRecentStickers` (:14701), `clearRecentStickers` (:14713),
/// `changeStickerSet` (:14692), `reorderInstalledStickerSets` (:14698).
#[test]
fn s8_sticker_backend_request_shapes_match_1_8_67() {
    let v: serde_json::Value =
        serde_json::from_str(&get_trending_sticker_sets(RequestId(21), 0, 100)).unwrap();
    assert_eq!(v["@type"], "getTrendingStickerSets");
    assert_eq!(v["sticker_type"]["@type"], "stickerTypeRegular");
    assert_eq!(v["offset"], 0);
    assert_eq!(v["limit"], 100);

    let v: serde_json::Value =
        serde_json::from_str(&view_trending_sticker_sets(RequestId(22), &[77, 78])).unwrap();
    assert_eq!(v["@type"], "viewTrendingStickerSets");
    assert_eq!(v["sticker_set_ids"], serde_json::json!([77, 78]));

    let v: serde_json::Value =
        serde_json::from_str(&search_sticker_sets(RequestId(23), "cats")).unwrap();
    assert_eq!(v["@type"], "searchStickerSets");
    assert_eq!(v["sticker_type"]["@type"], "stickerTypeRegular");
    assert_eq!(v["query"], "cats");

    let v: serde_json::Value =
        serde_json::from_str(&search_stickers(RequestId(24), "😀", "grin", 0, 50)).unwrap();
    assert_eq!(v["@type"], "searchStickers");
    assert_eq!(v["emojis"], "😀");
    assert_eq!(v["query"], "grin");
    assert_eq!(v["input_language_codes"], serde_json::json!([]));
    assert_eq!(v["limit"], 50);

    let v: serde_json::Value = serde_json::from_str(&get_favorite_stickers(RequestId(25))).unwrap();
    assert_eq!(v["@type"], "getFavoriteStickers");

    let v: serde_json::Value =
        serde_json::from_str(&add_favorite_sticker(RequestId(26), FileId(41))).unwrap();
    assert_eq!(v["@type"], "addFavoriteSticker");
    assert_eq!(v["sticker"]["@type"], "inputFileId");
    assert_eq!(v["sticker"]["id"], 41);

    let v: serde_json::Value =
        serde_json::from_str(&remove_favorite_sticker(RequestId(27), FileId(41))).unwrap();
    assert_eq!(v["@type"], "removeFavoriteSticker");
    assert_eq!(v["sticker"]["id"], 41);

    let v: serde_json::Value =
        serde_json::from_str(&get_recent_stickers(RequestId(28), false)).unwrap();
    assert_eq!(v["@type"], "getRecentStickers");
    assert_eq!(v["is_attached"], false);

    let v: serde_json::Value =
        serde_json::from_str(&clear_recent_stickers(RequestId(29), true)).unwrap();
    assert_eq!(v["@type"], "clearRecentStickers");
    assert_eq!(v["is_attached"], true);

    let v: serde_json::Value =
        serde_json::from_str(&change_sticker_set(RequestId(30), 77, true, false)).unwrap();
    assert_eq!(v["@type"], "changeStickerSet");
    assert_eq!(v["set_id"], "77");
    assert_eq!(v["is_installed"], true);
    assert_eq!(v["is_archived"], false);

    let v: serde_json::Value =
        serde_json::from_str(&reorder_installed_sticker_sets(RequestId(31), &[78, 77])).unwrap();
    assert_eq!(v["@type"], "reorderInstalledStickerSets");
    assert_eq!(v["sticker_type"]["@type"], "stickerTypeRegular");
    assert_eq!(v["sticker_set_ids"], serde_json::json!([78, 77]));
}

#[test]
fn send_animation_uses_input_animation_and_saved_list_has_no_query() {
    let json = send_animation(
        RequestId(21),
        ChatId(7),
        AnimationSend {
            file_id: FileId(33),
            duration: 2,
            width: 240,
            height: 140,
            reply_to: Some(SendReply::plain(MessageId(101))),
            topic_id: None,
        },
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "sendMessage");
    assert_eq!(v["@extra"], "21");
    assert_eq!(v["input_message_content"]["@type"], "inputMessageAnimation");
    let animation = &v["input_message_content"]["animation"];
    assert_eq!(animation["@type"], "inputAnimation");
    assert_eq!(animation["animation"]["@type"], "inputFileId");
    assert_eq!(animation["animation"]["id"], 33);
    assert_eq!(animation["thumbnail"], Value::Null);
    assert_eq!(animation["added_sticker_file_ids"], serde_json::json!([]));
    assert_eq!(animation["duration"], 2);
    assert_eq!(animation["width"], 240);
    assert_eq!(animation["height"], 140);
    assert_eq!(v["input_message_content"]["caption"], Value::Null);
    assert_eq!(v["input_message_content"]["has_spoiler"], false);
    assert_eq!(v["reply_to"]["message_id"], 101);
    assert!(!json.contains("tenor"));
    let saved = get_saved_animations(RequestId(22));
    let saved: serde_json::Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(saved["@type"], "getSavedAnimations");
    assert_eq!(saved["@extra"], "22");
    assert!(saved.get("query").is_none());
}

/// Slice S9: `addSavedAnimation` / `removeSavedAnimation` request
/// shapes against the pinned schema (1.8.67): `addSavedAnimation
/// animation:InputFile = Ok;` (:14769), `removeSavedAnimation
/// animation:InputFile = Ok;` (:14772) — `animation` as
/// `inputFileId id:int32` (:317), like S8's `addFavoriteSticker`.
#[test]
fn s9_saved_animation_request_shapes_match_1_8_67() {
    let v: serde_json::Value =
        serde_json::from_str(&add_saved_animation(RequestId(31), FileId(41))).unwrap();
    assert_eq!(v["@type"], "addSavedAnimation");
    assert_eq!(v["@extra"], "31");
    assert_eq!(v["animation"]["@type"], "inputFileId");
    assert_eq!(v["animation"]["id"], 41);

    let v: serde_json::Value =
        serde_json::from_str(&remove_saved_animation(RequestId(32), FileId(42))).unwrap();
    assert_eq!(v["@type"], "removeSavedAnimation");
    assert_eq!(v["@extra"], "32");
    assert_eq!(v["animation"]["@type"], "inputFileId");
    assert_eq!(v["animation"]["id"], 42);
}

#[test]
fn b11_requests_match_the_schema() {
    use serde_json::Value;
    let parse = |json: String| serde_json::from_str::<Value>(&json).unwrap();
    let remove = parse(remove_recent_sticker(RequestId(1), FileId(9), false));
    assert_eq!(remove["@type"], "removeRecentSticker");
    assert_eq!(remove["sticker"]["@type"], "inputFileId");
    assert_eq!(remove["is_attached"], false);
    let keywords = parse(get_keyword_emojis(
        RequestId(2),
        "fire",
        &["en".to_string(), "ru".to_string()],
    ));
    assert_eq!(keywords["@type"], "getKeywordEmojis");
    assert_eq!(
        keywords["input_language_codes"],
        serde_json::json!(["en", "ru"])
    );
    let attached = parse(get_attached_sticker_sets(RequestId(3), FileId(4)));
    assert_eq!(attached["file_id"], 4);
    assert_eq!(
        parse(get_greeting_stickers(RequestId(4)))["@type"],
        "getGreetingStickers"
    );
    let default = parse(crate::telegram::requests::set_default_reaction_type(
        RequestId(5),
        crate::telegram::requests::reaction_type_emoji("👍"),
    ));
    assert_eq!(default["reaction_type"]["@type"], "reactionTypeEmoji");
}
