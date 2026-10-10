//! Parses TDLib objects for stickers, custom emoji, emoji statuses, reactions, GIFs and the media library.
use crate::telegram::envelope::*;
use serde_json::Value;

/// The stickers domain's TDLib types; `Ok(None)` leaves
/// `type_name` to the other domains.
pub(crate) fn parse_stickers_payload(
    type_name: &str,
    value: &Value,
) -> Result<Option<EnvelopePayload>, ParseError> {
    let payload = match type_name {
        "stickerSets" => Ok(parse_sticker_sets(value)),
        "stickerSet" => Ok(parse_sticker_set(value)),
        "updateStickerSet" => Ok(EnvelopePayload::Stickers(
            StickersPayload::UpdateStickerSet {
                id: int64(value.get("sticker_set").and_then(|set| set.get("id"))).unwrap_or(0),
                is_custom_emoji: value
                    .pointer("/sticker_set/sticker_type/@type")
                    .and_then(Value::as_str)
                    == Some("stickerTypeCustomEmoji"),
            },
        )),
        "trendingStickerSets" => Ok(parse_trending_sticker_sets(value)),
        "stickers" => Ok(parse_stickers(value)),
        // Slice S10: emoji backend payloads (parsers live in envelope_emoji).
        "emojiStatuses" => Ok(crate::telegram::envelope_emoji::parse_emoji_statuses(value)),
        "emojiStatusCustomEmojis" => {
            Ok(crate::telegram::envelope_emoji::parse_emoji_status_custom_emojis(value))
        }
        "animatedEmoji" => Ok(crate::telegram::envelope_emoji::parse_animated_emoji(value)),
        "emojis" => Ok(EnvelopePayload::Stickers(StickersPayload::Emojis {
            emojis: value
                .get("emojis")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
        })),
        "emojiKeywords" => Ok(crate::telegram::envelope_emoji::parse_emoji_keywords(value)),
        "emojiCategories" => Ok(crate::telegram::envelope_emoji::parse_emoji_categories(
            value,
        )),
        "animations" => Ok(parse_animations(value)),
        "updateSavedAnimations" => Ok(EnvelopePayload::Stickers(
            StickersPayload::UpdateSavedAnimations {
                animation_ids: value
                    .get("animation_ids")
                    .and_then(Value::as_array)
                    .map(|ids| {
                        ids.iter()
                            .filter_map(|id| id.as_i64())
                            .map(|id| id.sat_i32())
                            .filter(|id| *id != 0)
                            .collect()
                    })
                    .unwrap_or_default(),
            },
        )),
        // Slice S9: `updateAnimationSearchParameters` (schema 1.8.67,
        // line 11064) — server-pushed; provider is the upstream search
        // provider name, emojis the new suggested search emojis.
        "updateAnimationSearchParameters" => Ok(EnvelopePayload::Stickers(
            StickersPayload::UpdateAnimationSearchParameters {
                provider: value
                    .get("provider")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                emojis: value
                    .get("emojis")
                    .and_then(Value::as_array)
                    .map(|emojis| {
                        emojis
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default(),
            },
        )),
        // Slice S15: `updateInstalledStickerSets` (schema 1.8.67, line
        // 10932). Ids arrive as int64s (string or number); a missing or
        // malformed list parses to empty, which the reducer treats as a
        // no-op.
        "updateInstalledStickerSets" => Ok(EnvelopePayload::Stickers(
            StickersPayload::UpdateInstalledStickerSets {
                sticker_set_ids: value
                    .get("sticker_set_ids")
                    .and_then(Value::as_array)
                    .map(|ids| ids.iter().filter_map(|id| int64(Some(id))).collect())
                    .unwrap_or_default(),
                is_regular: value
                    .get("sticker_type")
                    .and_then(|t| t.get("@type"))
                    .and_then(Value::as_str)
                    == Some("stickerTypeRegular"),
            },
        )),
        "updateRecentStickers" => Ok(EnvelopePayload::Stickers(
            StickersPayload::UpdateRecentStickers {
                is_attached: value
                    .get("is_attached")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            },
        )),
        "updateFavoriteStickers" => Ok(EnvelopePayload::Stickers(
            StickersPayload::UpdateFavoriteStickers,
        )),
        "updateTrendingStickerSets" => Ok(EnvelopePayload::Stickers(
            StickersPayload::UpdateTrendingStickerSets {
                is_regular: value
                    .get("sticker_type")
                    .and_then(|t| t.get("@type"))
                    .and_then(Value::as_str)
                    == Some("stickerTypeRegular"),
            },
        )),
        "updateDefaultReactionType" => Ok(EnvelopePayload::Stickers(
            StickersPayload::UpdateDefaultReactionType {
                reaction_type: crate::telegram::envelope::message_reactions::parse_reaction_type(
                    value.get("reaction_type"),
                )
                .unwrap_or(ReactionType::Unknown),
            },
        )),
        // B7: `updateActiveEmojiReactions` (schema 1.8.67, line 10999).
        "updateActiveEmojiReactions" => Ok(EnvelopePayload::Stickers(
            StickersPayload::UpdateActiveEmojiReactions {
                emojis: value
                    .get("emojis")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect(),
            },
        )),
        _ => return Ok(None),
    };
    payload.map(Some)
}
