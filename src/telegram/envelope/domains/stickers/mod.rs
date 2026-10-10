//! TDLib updates and answers for stickers, custom emoji, emoji statuses, reactions, GIFs and the media library.
mod parse;

use crate::telegram::envelope::*;
use crate::telegram::envelope_emoji::{EmojiCategory, EmojiKeyword, EmojiStatusItem};
pub(crate) use parse::parse_stickers_payload;

/// Payloads for stickers, custom emoji, emoji statuses, reactions, GIFs and the media library; wrapped as
/// [`EnvelopePayload::Stickers`].
#[derive(Debug, Clone, PartialEq)]
pub enum StickersPayload {
    UpdateStickerSet {
        id: i64,
        is_custom_emoji: bool,
    },
    /// `stickerSets` — `getInstalledStickerSets`.
    StickerSets {
        total_count: i32,
        sets: Vec<StickerSetInfo>,
    },
    /// `stickerSet` — `getStickerSet`. Files on each sticker are in `files`.
    StickerSet {
        id: i64,
        title: String,
        name: String,
        /// `stickerSet.is_installed`: the set is in the user's collection.
        is_installed: bool,
        /// `stickerSet.sticker_type` is `stickerTypeCustomEmoji`.
        is_custom_emoji: bool,
        stickers: Vec<StickerItem>,
        files: Vec<ParsedFile>,
    },
    /// Slice S8: `trendingStickerSets` — `getTrendingStickerSets`.
    /// `is_premium` flags the premium-only row (tdesktop renders it
    /// separately).
    TrendingStickerSets {
        total_count: i32,
        sets: Vec<StickerSetInfo>,
        is_premium: bool,
    },
    /// Slice S8: `stickers` — `searchStickers` / `getFavoriteStickers` /
    /// `getRecentStickers`. Files on each sticker are in `files`.
    Stickers {
        stickers: Vec<StickerItem>,
        files: Vec<ParsedFile>,
    },
    /// Slice S10: `emojiStatuses` — `getRecentEmojiStatuses` / `getUpgradedGiftEmojiStatuses`.
    EmojiStatuses {
        statuses: Vec<EmojiStatusItem>,
    },
    /// Slice S10: `emojiStatusCustomEmojis` — `getThemedEmojiStatuses` / `getDefaultEmojiStatuses`.
    EmojiStatusCustomEmojis {
        custom_emoji_ids: Vec<i64>,
    },
    /// Slice S10: `animatedEmoji` — `getAnimatedEmoji` (sticker + `sound` file).
    AnimatedEmoji {
        sticker: Option<StickerItem>,
        files: Vec<ParsedFile>,
    },
    /// `emojis` — `getKeywordEmojis` (schema 1.8.67, line 6435).
    Emojis {
        emojis: Vec<String>,
    },
    /// Slice S10: `emojiKeywords` — `searchEmojis` answers for the picker.
    EmojiKeywords {
        keywords: Vec<EmojiKeyword>,
    },
    /// Slice S10: `emojiCategories` — `getEmojiCategories` answers for the picker.
    EmojiCategories {
        categories: Vec<EmojiCategory>,
        files: Vec<ParsedFile>,
    },
    /// `animations` — `getSavedAnimations`.
    Animations {
        animations: Vec<AnimationItem>,
        files: Vec<ParsedFile>,
    },
    /// `updateSavedAnimations` — file ids of saved GIFs, newest first.
    UpdateSavedAnimations {
        animation_ids: Vec<i32>,
    },
    /// Slice S9: `updateAnimationSearchParameters` (schema 1.8.67, line
    /// 11064) — the server-pushed animation-search provider parameters:
    /// the upstream provider name (e.g. GIPHY/Tenor) and the suggested
    /// search emojis.
    UpdateAnimationSearchParameters {
        provider: String,
        emojis: Vec<String>,
    },
    /// Slice S15: `updateInstalledStickerSets` (schema 1.8.67, line 10932)
    /// — TDLib's authoritative new order of installed set ids after a
    /// reorder (manual, or usage-driven via `update_order_of_installed_
    /// sticker_sets` on a sticker send). `is_regular` selects the sticker
    /// panel's sets; other types route to the emoji panel's installed sets.
    UpdateInstalledStickerSets {
        sticker_set_ids: Vec<i64>,
        is_regular: bool,
    },
    /// `updateRecentStickers` (schema 1.8.67, line 10938): the recent
    /// stickers changed (possibly on another device). The ids are not
    /// used; the cache is refetched.
    UpdateRecentStickers {
        is_attached: bool,
    },
    /// `updateFavoriteStickers` (line 10941): the favorites changed.
    UpdateFavoriteStickers,
    /// `updateTrendingStickerSets` (line 10935): the trending list of a
    /// sticker type changed. `is_regular` picks the sticker panel; other
    /// types mark the emoji panel's trending stale.
    UpdateTrendingStickerSets {
        is_regular: bool,
    },
    /// `updateDefaultReactionType` (line 11007): the quick reaction.
    UpdateDefaultReactionType {
        reaction_type: ReactionType,
    },
    /// B7: `updateActiveEmojiReactions` (schema 1.8.67, line 10999) — the
    /// emoji that can be used as reactions, in display order.
    UpdateActiveEmojiReactions {
        emojis: Vec<String>,
    },
}
