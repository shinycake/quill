use super::envelope::{
    EnvelopePayload, StickerItem, int53, json_field_str, parse_file, parse_sticker_value,
};
use serde_json::Value;

/// Slice S10: one `emojiStatus` row (schema 1.8.67, line 2358) from an
/// `emojiStatuses` list. Only the custom-emoji id is kept — the
/// upgraded-gift type carries render detail the UI slice owns, and the
/// `expiration_date` drives the timed-status presets (1h/2h/8h/2d/custom).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmojiStatusItem {
    pub custom_emoji_id: i64,
    pub expiration_date: i32,
}

/// Slice S10: one `emojiKeyword` row (schema 1.8.67, line 6426) from
/// `searchEmojis`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmojiKeyword {
    pub emoji: String,
    pub keyword: String,
}

/// Slice S10: one `emojiCategory` row (schema 1.8.67, line 6496) from
/// `getEmojiCategories`. The `source` (search/premium) is UI affordance
/// detail the picker slice owns; the icon sticker renders the row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmojiCategory {
    pub name: String,
    pub icon: Option<StickerItem>,
    pub is_greeting: bool,
}

/// Slice S10: `emojiStatuses` — `getRecentEmojiStatuses` /
/// `getUpgradedGiftEmojiStatuses` (schema 1.8.67, lines 2361, 13957, 13960).
/// Only the custom-emoji id is kept per row (0 for the upgraded-gift type).
pub(crate) fn parse_emoji_statuses(value: &Value) -> EnvelopePayload {
    let statuses = value
        .get("emoji_statuses")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let type_ = entry.get("type")?;
            let custom_emoji_id = match type_.get("@type")?.as_str()? {
                "emojiStatusTypeCustomEmoji" => int53(type_.get("custom_emoji_id")).ok()?,
                _ => 0,
            };
            Some(EmojiStatusItem {
                custom_emoji_id,
                expiration_date: entry
                    .get("expiration_date")
                    .and_then(Value::as_i64)
                    .unwrap_or(0) as i32,
            })
        })
        .collect();
    EnvelopePayload::EmojiStatuses { statuses }
}

/// Slice S10: `emojiStatusCustomEmojis` — `getThemedEmojiStatuses` /
/// `getDefaultEmojiStatuses` (schema 1.8.67, line 2364): a bare
/// `vector<int64>` of custom emoji ids.
pub(crate) fn parse_emoji_status_custom_emojis(value: &Value) -> EnvelopePayload {
    let custom_emoji_ids = value
        .get("custom_emoji_ids")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|id| int53(Some(id)).ok())
        .collect();
    EnvelopePayload::EmojiStatusCustomEmojis { custom_emoji_ids }
}

/// Slice S10: `animatedEmoji` — `getAnimatedEmoji` (schema 1.8.67, line
/// 632): the animated `sticker` (existing parse) plus its `sound` file.
pub(crate) fn parse_animated_emoji(value: &Value) -> EnvelopePayload {
    let (sticker, mut files) = parse_sticker_value(value.get("sticker"));
    if let Some(sound) = value.get("sound")
        && let Ok(file) = parse_file(Some(sound))
    {
        files.push(file);
    }
    EnvelopePayload::AnimatedEmoji { sticker, files }
}

/// Slice S10: `emojiKeywords` — `searchEmojis` (schema 1.8.67, line 6429).
pub(crate) fn parse_emoji_keywords(value: &Value) -> EnvelopePayload {
    let keywords = value
        .get("emoji_keywords")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            Some(EmojiKeyword {
                emoji: entry.get("emoji")?.as_str()?.to_string(),
                keyword: entry.get("keyword")?.as_str()?.to_string(),
            })
        })
        .collect();
    EnvelopePayload::EmojiKeywords { keywords }
}

/// Slice S10: `emojiCategories` — `getEmojiCategories` (schema 1.8.67, line
/// 6499). Icons parse with the existing sticker value parse.
pub(crate) fn parse_emoji_categories(value: &Value) -> EnvelopePayload {
    let mut categories = Vec::new();
    let mut files = Vec::new();
    if let Some(entries) = value.get("categories").and_then(Value::as_array) {
        for entry in entries {
            let (icon, icon_files) = parse_sticker_value(entry.get("icon"));
            files.extend(icon_files);
            categories.push(EmojiCategory {
                name: json_field_str(entry, "name"),
                icon,
                is_greeting: entry
                    .get("is_greeting")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            });
        }
    }
    EnvelopePayload::EmojiCategories { categories, files }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::FileId;
    use crate::telegram::envelope::parse_envelope;

    fn local_file_json(id: i32, path: &str, completed: bool, can_download: bool) -> String {
        format!(
            r#"{{"@type":"file","id":{id},"size":12,"expected_size":12,"local":{{"@type":"localFile","path":{path},"can_be_downloaded":{can_download},"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":{completed},"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"CANARY_REMOTE_ID","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":12}}}}"#,
            path = serde_json::to_string(path).unwrap(),
            can_download = can_download,
            completed = completed,
        )
    }

    /// Slice S10: emoji-backend payloads against the pinned schema
    /// (1.8.67): `emojiStatuses` (line 2361), `emojiStatusCustomEmojis`
    /// (line 2364), `animatedEmoji` (line 632), `emojiKeywords` (line
    /// 6429), `emojiCategories` (line 6499).
    #[test]
    fn s10_emoji_payloads_parse_1_8_67() {
        // `emojiStatuses`: custom-emoji rows keep the id + expiration; the
        // upgraded-gift type keeps 0 (render detail lives in the UI slice).
        let recent = parse_envelope(
            r#"{"@type":"emojiStatuses","emoji_statuses":[{"@type":"emojiStatus","type":{"@type":"emojiStatusTypeCustomEmoji","custom_emoji_id":"12345"},"expiration_date":3600},{"@type":"emojiStatus","type":{"@type":"emojiStatusTypeUpgradedGift","upgraded_gift_id":"9","gift_title":"t","gift_name":"n","model_custom_emoji_id":"1","symbol_custom_emoji_id":"2","backdrop_colors":{"@type":"upgradedGiftBackdropColors","colors":[]}},"expiration_date":0}]}"#,
        )
        .unwrap();
        match recent.payload {
            EnvelopePayload::EmojiStatuses { statuses } => {
                assert_eq!(statuses.len(), 2);
                assert_eq!(statuses[0].custom_emoji_id, 12345);
                assert_eq!(statuses[0].expiration_date, 3600);
                assert_eq!(statuses[1].custom_emoji_id, 0);
            }
            other => panic!("{other:?}"),
        }

        // `emojiStatusCustomEmojis`: bare custom-emoji-id list.
        let themed =
            parse_envelope(r#"{"@type":"emojiStatusCustomEmojis","custom_emoji_ids":["11","22"]}"#)
                .unwrap();
        match themed.payload {
            EnvelopePayload::EmojiStatusCustomEmojis { custom_emoji_ids } => {
                assert_eq!(custom_emoji_ids, vec![11, 22]);
            }
            other => panic!("{other:?}"),
        }

        // `animatedEmoji`: the sticker via the existing sticker parse, the
        // `sound` file collected alongside.
        let file = local_file_json(41, "/tmp/s.webp", true, true);
        let animated = parse_envelope(&format!(
            r#"{{"@type":"animatedEmoji","sticker":{{"@type":"sticker","id":"9001","set_id":"77","width":512,"height":512,"emoji":"🔥","format":{{"@type":"stickerFormatTgs"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":null,"sticker":{file}}},"sticker_width":512,"sticker_height":512,"fitzpatrick_type":0,"sound":{{"@type":"file","id":42,"size":7,"local":{{"path":"/tmp/s.ogg","is_downloading_completed":true}},"remote":{{}}}}}}"#
        ))
        .unwrap();
        match animated.payload {
            EnvelopePayload::AnimatedEmoji { sticker, files } => {
                let sticker = sticker.expect("sticker");
                assert_eq!(sticker.file_id, FileId(41));
                assert_eq!(sticker.emoji, "🔥");
                assert_eq!(files.len(), 2);
            }
            other => panic!("{other:?}"),
        }

        // `emojiKeywords`: emoji/keyword pairs.
        let keywords = parse_envelope(
            r#"{"@type":"emojiKeywords","emoji_keywords":[{"@type":"emojiKeyword","emoji":"🔥","keyword":"fire"},{"@type":"emojiKeyword","emoji":"❤️","keyword":"love"}]}"#,
        )
        .unwrap();
        match keywords.payload {
            EnvelopePayload::EmojiKeywords { keywords } => {
                assert_eq!(keywords.len(), 2);
                assert_eq!(keywords[0].emoji, "🔥");
                assert_eq!(keywords[0].keyword, "fire");
            }
            other => panic!("{other:?}"),
        }

        // `emojiCategories`: name + icon sticker + greeting flag.
        let categories = parse_envelope(&format!(
            r#"{{"@type":"emojiCategories","categories":[{{"@type":"emojiCategory","name":"Smileys","icon":{{"@type":"sticker","id":"9002","set_id":"77","width":100,"height":100,"emoji":"😀","format":{{"@type":"stickerFormatWebp"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":null,"sticker":{file}}},"source":{{"@type":"emojiCategorySourcePremium"}},"is_greeting":false}}]}}"#
        ))
        .unwrap();
        match categories.payload {
            EnvelopePayload::EmojiCategories { categories, files } => {
                assert_eq!(categories.len(), 1);
                assert_eq!(categories[0].name, "Smileys");
                assert!(!categories[0].is_greeting);
                assert_eq!(
                    categories[0].icon.as_ref().expect("icon").file_id,
                    FileId(41)
                );
                assert_eq!(files.len(), 1);
            }
            other => panic!("{other:?}"),
        }
    }
}
