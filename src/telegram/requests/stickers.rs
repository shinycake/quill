use super::{SendReply, message_send_options, message_topic_value, send_reply_value};
use crate::composer::SendOptions;
use crate::ids::{ChatId, FileId, RequestId};
use serde_json::{Value, json};

/// `getInstalledStickerSets` for regular stickers (Unigram `StickerTypeRegular`).
pub fn get_installed_sticker_sets(extra: RequestId) -> String {
    json!({
        "@type": "getInstalledStickerSets",
        "@extra": extra.as_extra(),
        "sticker_type": { "@type": "stickerTypeRegular" },
    })
    .to_string()
}

/// `getStickerSet`. `set_id` is int64 — JSON string, not a float.
pub fn get_sticker_set(extra: RequestId, set_id: i64) -> String {
    json!({
        "@type": "getStickerSet",
        "@extra": extra.as_extra(),
        "set_id": set_id.to_string(),
    })
    .to_string()
}

/// Slice S8: `getTrendingStickerSets` for regular stickers (Unigram trending
/// tab). `offset`/`limit` page the server list.
pub fn get_trending_sticker_sets(extra: RequestId, offset: i32, limit: i32) -> String {
    json!({
        "@type": "getTrendingStickerSets",
        "@extra": extra.as_extra(),
        "sticker_type": { "@type": "stickerTypeRegular" },
        "offset": offset,
        "limit": limit,
    })
    .to_string()
}

/// Slice S8: `viewTrendingStickerSets` — marks trending sets viewed so the
/// "new" badge clears. Response is `ok`.
pub fn view_trending_sticker_sets(extra: RequestId, set_ids: &[i64]) -> String {
    json!({
        "@type": "viewTrendingStickerSets",
        "@extra": extra.as_extra(),
        "sticker_set_ids": set_ids,
    })
    .to_string()
}

/// Slice S8: `searchStickerSets` — installed + discoverable regular sets
/// matching `query`. Response is `stickerSets`.
pub fn search_sticker_sets(extra: RequestId, query: &str) -> String {
    json!({
        "@type": "searchStickerSets",
        "@extra": extra.as_extra(),
        "sticker_type": { "@type": "stickerTypeRegular" },
        "query": query,
    })
    .to_string()
}

/// Slice S8: `searchStickers` — regular stickers matching emoji text and/or
/// query. `input_language_codes` is empty (server default); response is
/// `stickers`.
pub fn search_stickers(
    extra: RequestId,
    emojis: &str,
    query: &str,
    offset: i32,
    limit: i32,
) -> String {
    json!({
        "@type": "searchStickers",
        "@extra": extra.as_extra(),
        "sticker_type": { "@type": "stickerTypeRegular" },
        "emojis": emojis,
        "query": query,
        "input_language_codes": [],
        "offset": offset,
        "limit": limit,
    })
    .to_string()
}

/// Slice S8: `getFavoriteStickers`. Response is `stickers`.
pub fn get_favorite_stickers(extra: RequestId) -> String {
    json!({
        "@type": "getFavoriteStickers",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// Slice S8: `addFavoriteSticker` — `sticker:InputFile` (TDLib 1.8.67, line
/// 14721); passed as `inputFileId id:int32` (schema line 317). Response is
/// `ok`.
pub fn add_favorite_sticker(extra: RequestId, file_id: FileId) -> String {
    json!({
        "@type": "addFavoriteSticker",
        "@extra": extra.as_extra(),
        "sticker": { "@type": "inputFileId", "id": file_id.0 },
    })
    .to_string()
}

/// Slice S8: `removeFavoriteSticker` — same `inputFileId` shape. Response is
/// `ok`.
pub fn remove_favorite_sticker(extra: RequestId, file_id: FileId) -> String {
    json!({
        "@type": "removeFavoriteSticker",
        "@extra": extra.as_extra(),
        "sticker": { "@type": "inputFileId", "id": file_id.0 },
    })
    .to_string()
}

/// Slice S8: `getRecentStickers`. Response is `stickers`.
pub fn get_recent_stickers(extra: RequestId, is_attached: bool) -> String {
    json!({
        "@type": "getRecentStickers",
        "@extra": extra.as_extra(),
        "is_attached": is_attached,
    })
    .to_string()
}

/// Slice S8: `clearRecentStickers`. Response is `ok`.
pub fn clear_recent_stickers(extra: RequestId, is_attached: bool) -> String {
    json!({
        "@type": "clearRecentStickers",
        "@extra": extra.as_extra(),
        "is_attached": is_attached,
    })
    .to_string()
}

/// Slice S8: `changeStickerSet` — install (`is_installed`), archive
/// (`is_archived`), or remove (both false). Response is `ok`; the installed
/// cache is invalidated on success so the panel refetches.
pub fn change_sticker_set(
    extra: RequestId,
    set_id: i64,
    is_installed: bool,
    is_archived: bool,
) -> String {
    json!({
        "@type": "changeStickerSet",
        "@extra": extra.as_extra(),
        "set_id": set_id.to_string(),
        "is_installed": is_installed,
        "is_archived": is_archived,
    })
    .to_string()
}

/// Slice S8: `reorderInstalledStickerSets` — `sticker_set_ids` is the full
/// new order of regular installed sets. Response is `ok`.
pub fn reorder_installed_sticker_sets(extra: RequestId, set_ids: &[i64]) -> String {
    json!({
        "@type": "reorderInstalledStickerSets",
        "@extra": extra.as_extra(),
        "sticker_type": { "@type": "stickerTypeRegular" },
        "sticker_set_ids": set_ids,
    })
    .to_string()
}

/// Fields for `inputMessageSticker` (TDLib 1.8.67). Thumbnail matches Unigram `Thumbnail.ToInput`.
pub struct StickerSend<'a> {
    pub file_id: FileId,
    pub emoji: &'a str,
    pub width: i32,
    pub height: i32,
    pub thumb: Option<(FileId, i32, i32)>,
    pub reply_to: Option<SendReply>,
    /// Parity slice 4: forum topic the send is addressed to (`None` = no topic).
    pub topic_id: Option<i32>,
}

/// `sendMessage` + `inputMessageSticker` / `inputSticker` / `inputFileId` (1.8.67).
/// Thumbnail is `inputThumbnail` + `inputFileId` when the sticker has one (Unigram `Thumbnail.ToInput`).
pub fn send_sticker(extra: RequestId, chat_id: ChatId, sticker: StickerSend<'_>) -> String {
    let thumbnail = match sticker.thumb {
        Some((id, thumb_width, thumb_height)) if id.0 != 0 => json!({
            "@type": "inputThumbnail",
            "thumbnail": { "@type": "inputFileId", "id": id.0 },
            "width": thumb_width,
            "height": thumb_height,
        }),
        _ => Value::Null,
    };
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(sticker.topic_id),
        "reply_to": send_reply_value(sticker.reply_to.as_ref()),
        // S15: the sticker was explicitly picked from the panel — ask TDLib
        // to move its set to the front of the installed order (schema
        // 1.8.67:5934). TDLib persists the order and broadcasts
        // `updateInstalledStickerSets`.
        "options": message_send_options(&SendOptions {
            update_order_of_installed_sticker_sets: true,
            ..SendOptions::default()
        }),
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessageSticker",
            "sticker": {
                "@type": "inputSticker",
                "sticker": { "@type": "inputFileId", "id": sticker.file_id.0 },
                "thumbnail": thumbnail,
                "width": sticker.width,
                "height": sticker.height,
            },
            "emoji": sticker.emoji,
        }
    })
    .to_string()
}

/// Fields for `inputMessageAnimation` (TDLib 1.8.67). Saved GIFs use `inputFileId`.
/// Thumbnail stays null: `inputThumbnail` says file_id upload is not supported, and a
/// saved animation is already known to the server (Unigram sends the file id only).
pub struct AnimationSend {
    pub file_id: FileId,
    pub duration: i32,
    pub width: i32,
    pub height: i32,
    pub reply_to: Option<SendReply>,
    /// Parity slice 4: forum topic the send is addressed to (`None` = no topic).
    pub topic_id: Option<i32>,
}

/// `getSavedAnimations` — saved GIFs, no query and no third-party key.
pub fn get_saved_animations(extra: RequestId) -> String {
    json!({
        "@type": "getSavedAnimations",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// Slice S9: `addSavedAnimation animation:InputFile = Ok;` (TDLib 1.8.67,
/// line 14769); `animation` passed as `inputFileId id:int32` (schema line
/// 317), like S8's `addFavoriteSticker`. Response is `ok`; the saved-GIF
/// cache is invalidated on success so the panel refetches.
pub fn add_saved_animation(extra: RequestId, file_id: FileId) -> String {
    json!({
        "@type": "addSavedAnimation",
        "@extra": extra.as_extra(),
        "animation": { "@type": "inputFileId", "id": file_id.0 },
    })
    .to_string()
}

/// Slice S9: `removeSavedAnimation animation:InputFile = Ok;` (TDLib
/// 1.8.67, line 14772) — same `inputFileId` shape as add. Response is
/// `ok`; same cache invalidation as add.
pub fn remove_saved_animation(extra: RequestId, file_id: FileId) -> String {
    json!({
        "@type": "removeSavedAnimation",
        "@extra": extra.as_extra(),
        "animation": { "@type": "inputFileId", "id": file_id.0 },
    })
    .to_string()
}

/// `sendMessage` + `inputMessageAnimation` / `inputAnimation` / `inputFileId` (1.8.67).
pub fn send_animation(extra: RequestId, chat_id: ChatId, animation: AnimationSend) -> String {
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(animation.topic_id),
        "reply_to": send_reply_value(animation.reply_to.as_ref()),
        "options": Value::Null,
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessageAnimation",
            "animation": {
                "@type": "inputAnimation",
                "animation": { "@type": "inputFileId", "id": animation.file_id.0 },
                "thumbnail": Value::Null,
                "added_sticker_file_ids": [],
                "duration": animation.duration,
                "width": animation.width,
                "height": animation.height,
            },
            "caption": Value::Null,
            "show_caption_above_media": false,
            "has_spoiler": false,
        }
    })
    .to_string()
}
