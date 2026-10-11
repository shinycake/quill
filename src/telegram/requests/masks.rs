//! Mask sticker sets: the installed and archived lists and their order.
//!
//! Masks are `stickerTypeMask` sets. TDLib answers with the same
//! `stickerSets` shape as for regular stickers; `changeStickerSet` is not
//! typed, so the existing `change_sticker_set` serves masks too.
use crate::ids::RequestId;
use serde_json::json;

/// `getInstalledStickerSets` for mask sets.
pub fn get_installed_mask_sets(extra: RequestId) -> String {
    json!({
        "@type": "getInstalledStickerSets",
        "@extra": extra.as_extra(),
        "sticker_type": { "@type": "stickerTypeMask" },
    })
    .to_string()
}

/// `getArchivedStickerSets` for mask sets, paged after the previous page's
/// last set id (0 for the first page).
pub fn get_archived_mask_sets(extra: RequestId, offset_set_id: i64) -> String {
    json!({
        "@type": "getArchivedStickerSets",
        "@extra": extra.as_extra(),
        "sticker_type": { "@type": "stickerTypeMask" },
        "offset_sticker_set_id": offset_set_id.to_string(),
        "limit": 100,
    })
    .to_string()
}

/// `reorderInstalledStickerSets` for mask sets; `set_ids` is the full new order.
pub fn reorder_installed_mask_sets(extra: RequestId, set_ids: &[i64]) -> String {
    json!({
        "@type": "reorderInstalledStickerSets",
        "@extra": extra.as_extra(),
        "sticker_type": { "@type": "stickerTypeMask" },
        "sticker_set_ids": set_ids,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn parse(json: String) -> Value {
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn installed_masks_ask_for_the_mask_type() {
        let v = parse(get_installed_mask_sets(RequestId(5)));
        assert_eq!(v["@type"], "getInstalledStickerSets");
        assert_eq!(v["sticker_type"]["@type"], "stickerTypeMask");
    }

    #[test]
    fn archived_masks_page_by_int64_string() {
        let v = parse(get_archived_mask_sets(RequestId(6), 4_000_000_000_001));
        assert_eq!(v["@type"], "getArchivedStickerSets");
        assert_eq!(v["sticker_type"]["@type"], "stickerTypeMask");
        assert_eq!(v["offset_sticker_set_id"], "4000000000001");
        assert_eq!(v["limit"], 100);
    }

    #[test]
    fn reorder_masks_sends_the_mask_type_and_full_order() {
        let v = parse(reorder_installed_mask_sets(RequestId(7), &[3, 1, 2]));
        assert_eq!(v["@type"], "reorderInstalledStickerSets");
        assert_eq!(v["sticker_type"]["@type"], "stickerTypeMask");
        assert_eq!(v["sticker_set_ids"], json!([3, 1, 2]));
    }
}
