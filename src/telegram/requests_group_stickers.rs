use crate::ids::RequestId;
use serde_json::json;

/// Slice S11: `setSupergroupStickerSet` (TDLib 1.8.67,
/// `schema/td_api.tl:15154`):
/// `setSupergroupStickerSet supergroup_id:int53 sticker_set_id:int64 = Ok;`
/// "Changes the sticker set of a supergroup; requires can_change_info
/// administrator right" — "New value of the supergroup sticker set
/// identifier. Use 0 to remove the supergroup sticker set". `sticker_set_id`
/// is int64, serialized as a JSON string like `changeStickerSet`'s
/// `set_id`. The server confirms via `updateSupergroupFullInfo`.
pub fn set_supergroup_sticker_set(
    extra: RequestId,
    supergroup_id: i64,
    sticker_set_id: i64,
) -> String {
    json!({
        "@type": "setSupergroupStickerSet",
        "@extra": extra.as_extra(),
        "supergroup_id": supergroup_id,
        "sticker_set_id": sticker_set_id.to_string(),
    })
    .to_string()
}

/// Slice S11: `setSupergroupCustomEmojiStickerSet` (TDLib 1.8.67,
/// `schema/td_api.tl:15159`):
/// `setSupergroupCustomEmojiStickerSet supergroup_id:int53 custom_emoji_sticker_set_id:int64 = Ok;`
/// "Changes the custom emoji sticker set of a supergroup; requires
/// can_change_info administrator right" — "New value of the custom emoji
/// sticker set identifier for the supergroup. Use 0 to remove the custom
/// emoji sticker set in the supergroup". Same int64-as-string encoding.
/// The server confirms via `updateSupergroupFullInfo`.
pub fn set_supergroup_custom_emoji_sticker_set(
    extra: RequestId,
    supergroup_id: i64,
    custom_emoji_sticker_set_id: i64,
) -> String {
    json!({
        "@type": "setSupergroupCustomEmojiStickerSet",
        "@extra": extra.as_extra(),
        "supergroup_id": supergroup_id,
        "custom_emoji_sticker_set_id": custom_emoji_sticker_set_id.to_string(),
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::{set_supergroup_custom_emoji_sticker_set, set_supergroup_sticker_set};
    use crate::ids::RequestId;

    #[test]
    fn group_sticker_set_request_shapes() {
        // `setSupergroupStickerSet supergroup_id:int53 sticker_set_id:int64
        // = Ok` (schema 1.8.67, line 15154). int64 ids ride as JSON
        // strings, like `changeStickerSet`'s `set_id`.
        let v: serde_json::Value = serde_json::from_str(&set_supergroup_sticker_set(
            RequestId(88),
            10,
            1234567890123,
        ))
        .unwrap();
        assert_eq!(v["@type"], "setSupergroupStickerSet");
        assert_eq!(v["supergroup_id"], 10);
        assert_eq!(v["sticker_set_id"], "1234567890123");
        // 0 removes the group sticker set per the schema.
        let v: serde_json::Value =
            serde_json::from_str(&set_supergroup_sticker_set(RequestId(89), 10, 0)).unwrap();
        assert_eq!(v["sticker_set_id"], "0");

        // `setSupergroupCustomEmojiStickerSet supergroup_id:int53
        // custom_emoji_sticker_set_id:int64 = Ok` (schema 1.8.67,
        // line 15159).
        let v: serde_json::Value = serde_json::from_str(&set_supergroup_custom_emoji_sticker_set(
            RequestId(90),
            10,
            9876543210987,
        ))
        .unwrap();
        assert_eq!(v["@type"], "setSupergroupCustomEmojiStickerSet");
        assert_eq!(v["supergroup_id"], 10);
        assert_eq!(v["custom_emoji_sticker_set_id"], "9876543210987");
        let v: serde_json::Value = serde_json::from_str(&set_supergroup_custom_emoji_sticker_set(
            RequestId(91),
            10,
            0,
        ))
        .unwrap();
        assert_eq!(v["custom_emoji_sticker_set_id"], "0");
    }
}
