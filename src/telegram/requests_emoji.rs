use crate::ids::RequestId;
use serde_json::{Value, json};

/// Slice S10: `setEmojiStatus` (TDLib 1.8.67, line 14850). `custom_emoji_id`
/// is the id of the chosen status emoji; `expiration_date` is a Unix
/// timestamp (0 = no expiry) driving the timed-status presets
/// (1h/2h/8h/2d/custom). `None` passes null — the schema's
/// "pass null to switch to the default badge" — which is how the status is
/// cleared. Response is `ok`.
pub fn set_emoji_status(
    extra: RequestId,
    custom_emoji_id: Option<i64>,
    expiration_date: i32,
) -> String {
    let emoji_status = custom_emoji_id.map(|id| {
        json!({
            "@type": "emojiStatus",
            "type": { "@type": "emojiStatusTypeCustomEmoji", "custom_emoji_id": id.to_string() },
            "expiration_date": expiration_date,
        })
    });
    json!({
        "@type": "setEmojiStatus",
        "@extra": extra.as_extra(),
        "emoji_status": emoji_status,
    })
    .to_string()
}

/// Slice S10: `getRecentEmojiStatuses` (TDLib 1.8.67, line 13957).
/// Response is `emojiStatuses`.
pub fn get_recent_emoji_statuses(extra: RequestId) -> String {
    json!({
        "@type": "getRecentEmojiStatuses",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// Slice S10: `getThemedEmojiStatuses` (TDLib 1.8.67, line 13954) — the
/// "trending" row of the status picker ("must be shown right after the
/// default Premium Badge"). Response is `emojiStatusCustomEmojis`.
pub fn get_themed_emoji_statuses(extra: RequestId) -> String {
    json!({
        "@type": "getThemedEmojiStatuses",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// Slice S10: `getDefaultEmojiStatuses` (TDLib 1.8.67, line 13963).
/// Response is `emojiStatusCustomEmojis`.
pub fn get_default_emoji_statuses(extra: RequestId) -> String {
    json!({
        "@type": "getDefaultEmojiStatuses",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// Slice S10: `getUpgradedGiftEmojiStatuses` (TDLib 1.8.67, line 13960).
/// Response is `emojiStatuses`.
pub fn get_upgraded_gift_emoji_statuses(extra: RequestId) -> String {
    json!({
        "@type": "getUpgradedGiftEmojiStatuses",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// Slice S10: `clearRecentEmojiStatuses` (TDLib 1.8.67, line 13966).
/// Response is `ok`; the recent-statuses cache is cleared so it refetches.
pub fn clear_recent_emoji_statuses(extra: RequestId) -> String {
    json!({
        "@type": "clearRecentEmojiStatuses",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// Slice S10: `getAnimatedEmoji` (TDLib 1.8.67, line 14743) — the animated
/// variant of a typed emoji for the composer's "suggest animated emoji".
/// Returns a 404 error for emoji with no animated variant. Response is
/// `animatedEmoji`.
pub fn get_animated_emoji(extra: RequestId, emoji: &str) -> String {
    json!({
        "@type": "getAnimatedEmoji",
        "@extra": extra.as_extra(),
        "emoji": emoji,
    })
    .to_string()
}

/// Slice S10: `getCustomEmojiStickers` (TDLib 1.8.67, line 14751) —
/// resolves custom emoji ids (statuses, message entities) to their
/// stickers. At most 200 ids per call. Response is `stickers`.
pub fn get_custom_emoji_stickers(extra: RequestId, custom_emoji_ids: &[i64]) -> String {
    json!({
        "@type": "getCustomEmojiStickers",
        "@extra": extra.as_extra(),
        "custom_emoji_ids": custom_emoji_ids.iter().map(i64::to_string).collect::<Vec<_>>(),
    })
    .to_string()
}

/// Slice S10: `searchEmojis` (TDLib 1.8.67, line 14732) — keyword search for
/// the emoji picker. `input_language_codes` is empty (server default), like
/// S8's `searchStickers`. Response is `emojiKeywords`.
pub fn search_emojis(extra: RequestId, text: &str) -> String {
    json!({
        "@type": "searchEmojis",
        "@extra": extra.as_extra(),
        "text": text,
        "input_language_codes": [],
    })
    .to_string()
}

/// Slice S10: `getEmojiCategories` (TDLib 1.8.67, line 14738) — picker's
/// category rows. `type` is null → default categories. Response is
/// `emojiCategories`.
pub fn get_emoji_categories(extra: RequestId) -> String {
    json!({
        "@type": "getEmojiCategories",
        "@extra": extra.as_extra(),
        "type": Value::Null,
    })
    .to_string()
}

/// Slice S10: `getInstalledStickerSets` for custom-emoji sets
/// (`stickerTypeEmoji`) — the "Emoji Sets" settings screen's installed list.
/// Response is `stickerSets`. (`changeStickerSet` is type-agnostic, so the
/// existing `change_sticker_set` builder is reused for emoji-set
/// install/archive/remove.)
pub fn get_installed_emoji_sets(extra: RequestId) -> String {
    json!({
        "@type": "getInstalledStickerSets",
        "@extra": extra.as_extra(),
        "sticker_type": { "@type": "stickerTypeEmoji" },
    })
    .to_string()
}

/// Slice S10: `getArchivedStickerSets` for custom-emoji sets
/// (`stickerTypeEmoji`). `offset_set_id` pages (0 = first page). Response is
/// `stickerSets`.
pub fn get_archived_emoji_sets(extra: RequestId, offset_set_id: i64, limit: i32) -> String {
    json!({
        "@type": "getArchivedStickerSets",
        "@extra": extra.as_extra(),
        "sticker_type": { "@type": "stickerTypeEmoji" },
        "offset_sticker_set_id": offset_set_id.to_string(),
        "limit": limit,
    })
    .to_string()
}

/// Slice S10: `getTrendingStickerSets` for custom-emoji sets
/// (`stickerTypeEmoji`) — the discover/trending section. `offset`/`limit`
/// page the server list. Response is `trendingStickerSets`.
/// (`viewTrendingStickerSets` is type-agnostic, so the existing
/// `view_trending_sticker_sets` builder is reused.)
pub fn get_trending_emoji_sets(extra: RequestId, offset: i32, limit: i32) -> String {
    json!({
        "@type": "getTrendingStickerSets",
        "@extra": extra.as_extra(),
        "sticker_type": { "@type": "stickerTypeEmoji" },
        "offset": offset,
        "limit": limit,
    })
    .to_string()
}

/// Slice S10: `searchStickerSets` for custom-emoji sets
/// (`stickerTypeEmoji`). Response is `stickerSets`.
pub fn search_emoji_sets(extra: RequestId, query: &str) -> String {
    json!({
        "@type": "searchStickerSets",
        "@extra": extra.as_extra(),
        "sticker_type": { "@type": "stickerTypeEmoji" },
        "query": query,
    })
    .to_string()
}

/// Slice S10: `reorderInstalledStickerSets` for custom-emoji sets
/// (`stickerTypeEmoji`) — manual pack order; `sticker_set_ids` is the full
/// new order. (The "dynamic order" toggle is computed client-side from
/// recency — no TDLib setting exists.) Response is `ok`.
pub fn reorder_installed_emoji_sets(extra: RequestId, set_ids: &[i64]) -> String {
    json!({
        "@type": "reorderInstalledStickerSets",
        "@extra": extra.as_extra(),
        "sticker_type": { "@type": "stickerTypeEmoji" },
        "sticker_set_ids": set_ids,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Slice S10: the fifteen emoji-backend request shapes against the
    /// pinned schema (1.8.67): `setEmojiStatus` (:14850),
    /// `getRecentEmojiStatuses` (:13957), `getThemedEmojiStatuses`
    /// (:13954), `getDefaultEmojiStatuses` (:13963),
    /// `getUpgradedGiftEmojiStatuses` (:13960),
    /// `clearRecentEmojiStatuses` (:13966), `getAnimatedEmoji` (:14743),
    /// `getCustomEmojiStickers` (:14751), `searchEmojis` (:14732),
    /// `getEmojiCategories` (:14738), and the `stickerTypeEmoji` pack
    /// calls — `getInstalledStickerSets` (:14657),
    /// `getArchivedStickerSets` (:14663), `getTrendingStickerSets`
    /// (:14669), `searchStickerSets` (:14689),
    /// `reorderInstalledStickerSets` (:14698).
    #[test]
    fn s10_emoji_backend_request_shapes_match_1_8_67() {
        // set: custom emoji id + timed expiration; clear passes null.
        let v: serde_json::Value =
            serde_json::from_str(&set_emoji_status(RequestId(40), Some(12345), 3600)).unwrap();
        assert_eq!(v["@type"], "setEmojiStatus");
        assert_eq!(
            v["emoji_status"]["type"]["@type"],
            "emojiStatusTypeCustomEmoji"
        );
        assert_eq!(v["emoji_status"]["type"]["custom_emoji_id"], "12345");
        assert_eq!(v["emoji_status"]["expiration_date"], 3600);

        let v: serde_json::Value =
            serde_json::from_str(&set_emoji_status(RequestId(41), None, 0)).unwrap();
        assert_eq!(v["@type"], "setEmojiStatus");
        assert_eq!(v["emoji_status"], Value::Null);

        for (name, body) in [
            (
                "getRecentEmojiStatuses",
                get_recent_emoji_statuses(RequestId(42)),
            ),
            (
                "getThemedEmojiStatuses",
                get_themed_emoji_statuses(RequestId(43)),
            ),
            (
                "getDefaultEmojiStatuses",
                get_default_emoji_statuses(RequestId(44)),
            ),
            (
                "getUpgradedGiftEmojiStatuses",
                get_upgraded_gift_emoji_statuses(RequestId(45)),
            ),
            (
                "clearRecentEmojiStatuses",
                clear_recent_emoji_statuses(RequestId(46)),
            ),
        ] {
            let v: serde_json::Value = serde_json::from_str(&body).unwrap();
            assert_eq!(v["@type"], name);
        }

        let v: serde_json::Value =
            serde_json::from_str(&get_animated_emoji(RequestId(47), "🔥")).unwrap();
        assert_eq!(v["@type"], "getAnimatedEmoji");
        assert_eq!(v["emoji"], "🔥");

        let v: serde_json::Value =
            serde_json::from_str(&get_custom_emoji_stickers(RequestId(48), &[11, 22])).unwrap();
        assert_eq!(v["@type"], "getCustomEmojiStickers");
        assert_eq!(v["custom_emoji_ids"], serde_json::json!(["11", "22"]));

        let v: serde_json::Value =
            serde_json::from_str(&search_emojis(RequestId(49), "fire")).unwrap();
        assert_eq!(v["@type"], "searchEmojis");
        assert_eq!(v["text"], "fire");
        assert_eq!(v["input_language_codes"], serde_json::json!([]));

        let v: serde_json::Value =
            serde_json::from_str(&get_emoji_categories(RequestId(50))).unwrap();
        assert_eq!(v["@type"], "getEmojiCategories");
        assert_eq!(v["type"], Value::Null);

        // Emoji packs: same constructors as S8's stickers, but
        // `stickerTypeEmoji`.
        let v: serde_json::Value =
            serde_json::from_str(&get_installed_emoji_sets(RequestId(51))).unwrap();
        assert_eq!(v["@type"], "getInstalledStickerSets");
        assert_eq!(v["sticker_type"]["@type"], "stickerTypeEmoji");

        let v: serde_json::Value =
            serde_json::from_str(&get_archived_emoji_sets(RequestId(52), 0, 100)).unwrap();
        assert_eq!(v["@type"], "getArchivedStickerSets");
        assert_eq!(v["sticker_type"]["@type"], "stickerTypeEmoji");
        assert_eq!(v["offset_sticker_set_id"], "0");
        assert_eq!(v["limit"], 100);

        let v: serde_json::Value =
            serde_json::from_str(&get_trending_emoji_sets(RequestId(53), 0, 100)).unwrap();
        assert_eq!(v["@type"], "getTrendingStickerSets");
        assert_eq!(v["sticker_type"]["@type"], "stickerTypeEmoji");

        let v: serde_json::Value =
            serde_json::from_str(&search_emoji_sets(RequestId(54), "blob")).unwrap();
        assert_eq!(v["@type"], "searchStickerSets");
        assert_eq!(v["sticker_type"]["@type"], "stickerTypeEmoji");
        assert_eq!(v["query"], "blob");

        let v: serde_json::Value =
            serde_json::from_str(&reorder_installed_emoji_sets(RequestId(55), &[78, 77])).unwrap();
        assert_eq!(v["@type"], "reorderInstalledStickerSets");
        assert_eq!(v["sticker_type"]["@type"], "stickerTypeEmoji");
        assert_eq!(v["sticker_set_ids"], serde_json::json!([78, 77]));
    }
}
