//! Request purposes for stickers, custom emoji, emoji statuses, reactions, GIFs and the media library.
use crate::state::request_purpose::flat_purposes;

/// In-flight requests for stickers, custom emoji, emoji statuses, reactions, GIFs and the media library; wrapped as
/// [`RequestPurpose::Stickers`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StickersPurpose {
    GetEmojiSet,
    ViewTrendingEmojiSets,
    /// `getStickerSet` for the composer's emoji/sticker panel library
    /// (`Session::media_library`), one per installed set as its section
    /// comes into view.
    LoadLibrarySet {
        set_id: i64,
    },
    /// "View Sticker Set" / "Add Stickers" on a sticker message:
    /// `getStickerSet`, answered into `Session::sticker_set_view`.
    ViewStickerSet {
        set_id: i64,
    },
    /// A tapped custom emoji: `getStickerSet` for the pack's title,
    /// answered into `Session::custom_emoji_preview`.
    CustomEmojiPack {
        emoji_id: i64,
        set_id: i64,
    },
    /// The message menu's emoji pack footer: `getStickerSet` for the
    /// pack's title, answered into `Session::emoji_pack_titles`.
    EmojiPackTitle {
        set_id: i64,
    },
    /// `getInstalledStickerSets` (`stickerTypeRegular`). Response is `stickerSets`.
    GetInstalledStickerSets,
    GetArchivedStickerSets,
    /// `getStickerSet`. Response is `stickerSet`.
    GetStickerSet,
    /// Slice S8: `getTrendingStickerSets` (regular). Response is
    /// `trendingStickerSets`.
    GetTrendingStickerSets,
    /// Slice S8: `viewTrendingStickerSets`. Response is `ok`.
    ViewTrendingStickerSets,
    /// Slice S8: `searchStickerSets` (regular). Response is `stickerSets`.
    SearchStickerSets,
    /// Slice S8: `searchStickers` (regular). Response is `stickers`.
    SearchStickers,
    /// Slice S12: `searchStickers` for the composer trailing-emoji
    /// suggestions. Response is `stickers`, stored in
    /// `StickerPanel::suggestions` — never the search UI's
    /// `found_stickers` slot.
    SuggestStickers,
    /// Slice S8: `getFavoriteStickers`. Response is `stickers`.
    GetFavoriteStickers,
    /// Slice S8: `addFavoriteSticker`. Response is `ok`; the favorites
    /// cache is cleared so it refetches.
    AddFavoriteSticker,
    /// Slice S8: `removeFavoriteSticker`. Response is `ok`; same
    /// invalidation as add.
    RemoveFavoriteSticker,
    /// Slice S8: `getRecentStickers`. Response is `stickers`.
    GetRecentStickers,
    /// Slice S8: `clearRecentStickers`. Response is `ok`; the recent
    /// cache is cleared.
    ClearRecentStickers,
    /// Slice S8: `changeStickerSet` (install / archive / remove).
    /// Response is `ok`; the installed-sets cache is cleared so the
    /// panel refetches the authoritative list.
    ChangeStickerSet,
    ManageStickerSet {
        set_id: i64,
        installed: bool,
        archived: bool,
    },
    /// Slice S8: `reorderInstalledStickerSets`. Response is `ok`; same
    /// installed-sets invalidation as change.
    ReorderInstalledStickerSets,
    /// Slice S10: `setEmojiStatus` (td_api.tl:14850). Response is `ok`.
    SetEmojiStatus,
    /// Slice S10: `getRecentEmojiStatuses` (td_api.tl:13957). Response is `emojiStatuses`.
    GetRecentEmojiStatuses,
    /// Slice S10: `getThemedEmojiStatuses` (td_api.tl:13954). Response is `emojiStatusCustomEmojis`.
    GetThemedEmojiStatuses,
    /// Slice S10: `getDefaultEmojiStatuses` (td_api.tl:13963). Response is `emojiStatusCustomEmojis`.
    GetDefaultEmojiStatuses,
    /// Slice S10: `getUpgradedGiftEmojiStatuses` (td_api.tl:13960). Response is `emojiStatuses`.
    GetUpgradedGiftEmojiStatuses,
    /// Slice S10: `clearRecentEmojiStatuses` (td_api.tl:13966). Response is `ok`.
    ClearRecentEmojiStatuses,
    /// Slice S10: `getAnimatedEmoji` (td_api.tl:14743). Response is `animatedEmoji`.
    GetAnimatedEmoji,
    /// Slice S10: `getCustomEmojiStickers` (td_api.tl:14751). Response is `stickers`.
    GetCustomEmojiStickers,
    /// Slice S10: `searchEmojis` (td_api.tl:14732). Response is `emojiKeywords`.
    SearchEmojis,
    /// B11: `setDefaultReactionType` (td_api.tl:12852). Response is `ok`.
    SetDefaultReactionType,
    /// B11: `removeRecentSticker` (td_api.tl:14710). Response is `ok`.
    RemoveRecentSticker,
    /// B11: `getKeywordEmojis` (td_api.tl:14737). Response is `emojis`.
    GetKeywordEmojis,
    /// B11: `getAttachedStickerSets` (td_api.tl:14672) for a photo.
    GetAttachedStickerSets {
        file_id: i32,
    },
    /// B11: `getGreetingStickers` (td_api.tl:14651). Response is `stickers`.
    GetGreetingStickers,
    /// Slice S10: `getEmojiCategories` (td_api.tl:14738). Response is `emojiCategories`.
    GetEmojiCategories,
    /// Slice S10: `getInstalledStickerSets` with `stickerTypeCustomEmoji` (td_api.tl:14657). Response is `stickerSets`.
    GetInstalledEmojiSets,
    /// Slice S10: `getArchivedStickerSets` with `stickerTypeCustomEmoji` (td_api.tl:14663). Response is `stickerSets`.
    GetArchivedEmojiSets {
        first_page: bool,
    },
    /// Slice S10: `getTrendingStickerSets` with `stickerTypeCustomEmoji` (td_api.tl:14669). Response is `trendingStickerSets`.
    GetTrendingEmojiSets,
    /// Slice S10: `searchStickerSets` with `stickerTypeCustomEmoji` (td_api.tl:14689). Response is `stickerSets`.
    SearchEmojiSets,
    /// Slice S10: `changeStickerSet` on an emoji set (td_api.tl:14692). Response is `ok`.
    ChangeEmojiSet,
    /// Slice S10: `reorderInstalledStickerSets` with `stickerTypeCustomEmoji` (td_api.tl:14698). Response is `ok`.
    ReorderInstalledEmojiSets,
    /// `getSavedAnimations`. Response is `animations`.
    GetSavedAnimations,
    /// Slice S9: `getInlineQueryResults` against the animation search
    /// bot for the GIF panel search. The bot is resolved via
    /// `getOption("animation_search_bot_username")` + `searchPublicChat`
    /// (schema 1.8.67, lines 6483, 11063); the driver slice will carry the
    /// resolved id when it issues searches.
    /// New queries discard prior tracked pages; late replies are ignored.
    ResolveGifSearchBot,
    GetGifSearchResults {
        first_page: bool,
    },
    /// Slice S9: `addSavedAnimation` (schema 1.8.67, line 14769).
    /// Response is `ok`; the saved-GIF cache is cleared so it refetches.
    AddSavedAnimation,
    /// Slice S9: `removeSavedAnimation` (schema 1.8.67, line 14772).
    /// Response is `ok`; same invalidation as add.
    RemoveSavedAnimation,
}

flat_purposes!(Stickers(StickersPurpose) {
    GetEmojiSet,
    ViewTrendingEmojiSets,
    GetInstalledStickerSets,
    GetArchivedStickerSets,
    GetStickerSet,
    GetTrendingStickerSets,
    ViewTrendingStickerSets,
    SearchStickerSets,
    SearchStickers,
    SuggestStickers,
    GetFavoriteStickers,
    AddFavoriteSticker,
    RemoveFavoriteSticker,
    GetRecentStickers,
    ClearRecentStickers,
    ChangeStickerSet,
    ReorderInstalledStickerSets,
    SetEmojiStatus,
    GetRecentEmojiStatuses,
    GetThemedEmojiStatuses,
    GetDefaultEmojiStatuses,
    GetUpgradedGiftEmojiStatuses,
    ClearRecentEmojiStatuses,
    GetAnimatedEmoji,
    GetCustomEmojiStickers,
    SearchEmojis,
    SetDefaultReactionType,
    RemoveRecentSticker,
    GetKeywordEmojis,
    GetGreetingStickers,
    GetEmojiCategories,
    GetInstalledEmojiSets,
    GetTrendingEmojiSets,
    SearchEmojiSets,
    ChangeEmojiSet,
    ReorderInstalledEmojiSets,
    GetSavedAnimations,
    ResolveGifSearchBot,
    AddSavedAnimation,
    RemoveSavedAnimation,
});
