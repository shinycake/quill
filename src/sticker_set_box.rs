//! Sticker set box menu (tdesktop `boxes/sticker_set_box.cpp`): the share
//! link, its wording and which entries the three-dot menu offers. Kept
//! pure so it is unit tested.

/// Public host for `addstickers` / `addemoji` links.
const LINK_BASE: &str = "https://t.me/";

/// What kind of set the box shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetKind {
    Stickers,
    Emoji,
}

impl SetKind {
    pub fn of(is_custom_emoji: bool) -> Self {
        if is_custom_emoji {
            Self::Emoji
        } else {
            Self::Stickers
        }
    }

    /// The link path segment (`addstickers` or `addemoji`).
    fn segment(self) -> &'static str {
        match self {
            Self::Stickers => "addstickers",
            Self::Emoji => "addemoji",
        }
    }
}

/// `https://t.me/addstickers/<name>` (or `addemoji`); `None` without a
/// short name (tdesktop shows no menu then).
pub fn set_link(name: &str, kind: SetKind) -> Option<String> {
    let name = name.trim();
    (!name.is_empty()).then(|| format!("{LINK_BASE}{}/{name}", kind.segment()))
}

/// The menu's share entry (`lng_stickers_share_pack` / `_emoji`).
pub fn share_label(kind: SetKind) -> &'static str {
    match kind {
        SetKind::Stickers => "Share Stickers",
        SetKind::Emoji => "Share Emoji",
    }
}

/// The toast after Copy Link (`lng_stickers_copied` / `_emoji`).
pub fn copied_note(kind: SetKind) -> &'static str {
    match kind {
        SetKind::Stickers => "Link copied to clipboard.",
        SetKind::Emoji => "Emoji pack link copied to clipboard.",
    }
}

/// Archive is offered for installed sticker sets only, never for emoji
/// packs (`installed && type != Emoji`).
pub fn can_archive(installed: bool, kind: SetKind) -> bool {
    installed && kind == SetKind::Stickers
}

/// The note after archiving (`lng_stickers_has_been_archived`).
pub const ARCHIVED_NOTE: &str = "Sticker set has been archived.";

/// Most stickers an owned set may hold (tdesktop `kStickersInOwnedSetMax`;
/// TDLib documents the same 120 on `addStickerToSet`).
pub const STICKERS_IN_OWNED_SET_MAX: usize = 120;
/// Most emoji an owned emoji set may hold (`kEmojiInOwnedSetMax`, 200).
pub const EMOJI_IN_OWNED_SET_MAX: usize = 200;
/// Side of a sticker image (`kStickerSide`).
pub const STICKER_SIDE: u32 = 512;
/// Largest side of a custom emoji image (`kEmojiStickerSideMax`).
pub const EMOJI_SIDE: u32 = 100;
/// Most emoji a new sticker may be tagged with (`kMaxEmojis`).
pub const MAX_EMOJIS: usize = 7;
/// Longest set name the rename box accepts (`kMaxSetNameLength`).
pub const SET_NAME_MAX: usize = 50;
/// The emoji a sticker without one is added under (`StickerEmojiOrDefault`).
pub const DEFAULT_EMOJI: &str = "\u{1F642}";
/// Where "Check the @stickers bot for more options" leads.
pub const STICKERS_BOT_LINK: &str = "https://t.me/stickers";

/// Toasts after an edit (`lng_stickers_create_added`, `lng_emoji_added`,
/// `lng_stickers_create_upload_failed`, `lng_attach_failed`).
pub const ADDED_NOTE: &str = "Sticker added.";
pub const EMOJI_ADDED_NOTE: &str = "Emoji added.";
pub const UPLOAD_FAILED_NOTE: &str = "Sticker upload failed. Please try again.";
pub const ADD_EXISTING_FAILED_NOTE: &str = "Failed";
/// `lng_stickers_create_emoji_required`.
pub const EMOJI_REQUIRED_NOTE: &str = "Please choose an emoji.";
/// `lng_stickers_create_open_failed`.
pub const OPEN_FAILED_NOTE: &str = "Could not load image.";

impl SetKind {
    /// How many stickers an owned set of this kind may hold.
    pub fn owned_limit(self) -> usize {
        match self {
            Self::Stickers => STICKERS_IN_OWNED_SET_MAX,
            Self::Emoji => EMOJI_IN_OWNED_SET_MAX,
        }
    }

    /// The side a new image is rendered at.
    pub fn image_side(self) -> u32 {
        match self {
            Self::Stickers => STICKER_SIDE,
            Self::Emoji => EMOJI_SIDE,
        }
    }

    /// The footer button of an owned set (`lng_stickers_edit_pack_button`,
    /// `lng_custom_emoji_edit_pack_button`).
    pub fn edit_label(self) -> &'static str {
        match self {
            Self::Stickers => "Edit Stickers",
            Self::Emoji => "Edit Emoji",
        }
    }

    /// The entries of the "+" cell's menu, in tdesktop's order.
    pub fn add_menu(self) -> &'static [AddEntry] {
        match self {
            Self::Stickers => &[AddEntry::CreateNew, AddEntry::AddExisting],
            Self::Emoji => &[
                AddEntry::CreateNew,
                AddEntry::AddExisting,
                AddEntry::AdaptSticker,
            ],
        }
    }

    /// Title of the box a new image is tagged in (`lng_stickers_create_image_title`).
    pub fn create_title(self) -> &'static str {
        match self {
            Self::Stickers => "New Sticker",
            Self::Emoji => "New Emoji",
        }
    }

    /// The emoji picker's caption (`lng_stickers_create_emoji_about`).
    pub fn emoji_about(self) -> &'static str {
        match self {
            Self::Stickers => "Choose emojis that match your sticker",
            Self::Emoji => "Choose emojis that match your emoji",
        }
    }

    /// "Delete sticker" / "Delete emoji" (`lng_stickers_context_delete`).
    pub fn delete_label(self) -> &'static str {
        match self {
            Self::Stickers => "Delete sticker",
            Self::Emoji => "Delete emoji",
        }
    }

    /// The delete box's question (`lng_stickers_context_delete_sure`).
    pub fn delete_question(self) -> &'static str {
        match self {
            Self::Stickers => "Are you sure you want to delete the sticker from your sticker set?",
            Self::Emoji => "Are you sure you want to delete this emoji from your set?",
        }
    }

    /// "Add to Sticker Set" / "Add to Emoji Set" (`lng_stickers_add_to_set`).
    pub fn add_to_set_label(self) -> &'static str {
        match self {
            Self::Stickers => "Add to Sticker Set",
            Self::Emoji => "Add to Emoji Set",
        }
    }
}

/// What the "+" cell of an owned set offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddEntry {
    /// `lng_stickers_create_new` / `lng_emoji_create_new`: pick an image.
    CreateNew,
    /// `lng_stickers_add_existing` / `lng_emoji_add_existing`.
    AddExisting,
    /// `lng_emoji_adapt_sticker`: a sticker turned into an emoji.
    AdaptSticker,
}

impl AddEntry {
    pub fn label(self, kind: SetKind) -> &'static str {
        match (self, kind) {
            (Self::CreateNew, SetKind::Stickers) => "Create a New Sticker",
            (Self::CreateNew, SetKind::Emoji) => "Create a New Emoji",
            (Self::AddExisting, SetKind::Stickers) => "Add an Existing Sticker",
            (Self::AddExisting, SetKind::Emoji) => "Add an Existing Emoji",
            (Self::AdaptSticker, _) => "Adapt from a Sticker",
        }
    }
}

/// The "+" cell shows for an owned, non-empty set under its limit
/// (`hasAddCell`).
pub fn has_add_cell(is_owned: bool, count: usize, kind: SetKind) -> bool {
    is_owned && count > 0 && count < kind.owned_limit()
}

/// The emoji a sticker is added under: its own, or the default
/// (`StickerEmojiOrDefault`).
pub fn emoji_or_default(emoji: &str) -> &str {
    if emoji.trim().is_empty() {
        DEFAULT_EMOJI
    } else {
        emoji
    }
}

/// Which owned sets a sticker may join (`AddAddToOwnedSetAction`): an
/// animated (TGS) sticker fits both kinds; one at most 100 px a side is an
/// emoji; the rest are stickers.
pub fn joinable_kinds(is_lottie: bool, width: i32, height: i32) -> (bool, bool) {
    let fits_emoji =
        width > 0 && height > 0 && width <= EMOJI_SIDE as i32 && height <= EMOJI_SIDE as i32;
    (is_lottie || !fits_emoji, is_lottie || fits_emoji)
}

/// A rendered image scaled to fit `side` keeping its ratio, at least 1 px
/// (`FittedStickerSize`).
pub fn fitted_size(width: u32, height: u32, side: u32) -> (u32, u32) {
    if width == 0 || height == 0 {
        return (side.max(1), side.max(1));
    }
    let scale = f64::from(side) / f64::from(width.max(height));
    let fit = |value: u32| ((f64::from(value) * scale).round() as u32).clamp(1, side.max(1));
    (fit(width), fit(height))
}

/// A set name the rename box accepts: trimmed, non-empty, at most
/// [`SET_NAME_MAX`] characters.
pub fn valid_set_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    (!trimmed.is_empty() && trimmed.chars().count() <= SET_NAME_MAX).then(|| trimmed.to_string())
}

/// Chosen emoji joined for `newSticker.emojis`, capped at [`MAX_EMOJIS`].
pub fn joined_emojis(chosen: &[String]) -> String {
    chosen.iter().take(MAX_EMOJIS).map(String::as_str).collect()
}

/// The label under a tapped custom emoji
/// (`lng_context_animated_emoji_preview`).
pub fn custom_emoji_preview_label(title: &str) -> String {
    format!("This emoji is from the {title} pack.")
}

#[cfg(test)]
mod tests {
    use super::{
        AddEntry, SetKind, can_archive, custom_emoji_preview_label, emoji_or_default, fitted_size,
        has_add_cell, joinable_kinds, joined_emojis, set_link, share_label, valid_set_name,
    };

    #[test]
    fn add_cell_follows_tdesktop_limits() {
        assert!(has_add_cell(true, 1, SetKind::Stickers));
        assert!(!has_add_cell(true, 0, SetKind::Stickers));
        assert!(!has_add_cell(false, 5, SetKind::Stickers));
        assert!(!has_add_cell(true, 120, SetKind::Stickers));
        assert!(has_add_cell(true, 120, SetKind::Emoji));
        assert!(!has_add_cell(true, 200, SetKind::Emoji));
        assert_eq!(SetKind::Emoji.add_menu().len(), 3);
        assert_eq!(
            AddEntry::AdaptSticker.label(SetKind::Emoji),
            "Adapt from a Sticker"
        );
    }

    #[test]
    fn joinable_kinds_split_by_size_and_lottie() {
        assert_eq!(joinable_kinds(false, 512, 512), (true, false));
        assert_eq!(joinable_kinds(false, 100, 100), (false, true));
        assert_eq!(joinable_kinds(true, 512, 512), (true, true));
        assert_eq!(joinable_kinds(false, 0, 0), (true, false));
    }

    #[test]
    fn images_fit_the_side_and_names_are_validated() {
        assert_eq!(fitted_size(1000, 500, 512), (512, 256));
        assert_eq!(fitted_size(10, 2000, 512), (3, 512));
        assert_eq!(fitted_size(0, 0, 100), (100, 100));
        assert_eq!(valid_set_name("  Cats "), Some("Cats".into()));
        assert_eq!(valid_set_name("   "), None);
        assert_eq!(valid_set_name(&"x".repeat(51)), None);
        assert_eq!(emoji_or_default(" "), "\u{1F642}");
        assert_eq!(emoji_or_default("😀"), "😀");
        let many: Vec<String> = (0..9).map(|_| "😀".to_string()).collect();
        assert_eq!(joined_emojis(&many).chars().count(), 7);
    }

    #[test]
    fn links_follow_the_set_kind() {
        assert_eq!(
            set_link("Cats", SetKind::Stickers).as_deref(),
            Some("https://t.me/addstickers/Cats")
        );
        assert_eq!(
            set_link(" Fire ", SetKind::Emoji).as_deref(),
            Some("https://t.me/addemoji/Fire")
        );
        assert_eq!(set_link("  ", SetKind::Stickers), None);
    }

    #[test]
    fn archive_only_for_installed_sticker_sets() {
        assert!(can_archive(true, SetKind::Stickers));
        assert!(!can_archive(false, SetKind::Stickers));
        assert!(!can_archive(true, SetKind::Emoji));
    }

    #[test]
    fn wording_matches_telegram_desktop() {
        assert_eq!(share_label(SetKind::Emoji), "Share Emoji");
        assert_eq!(
            custom_emoji_preview_label("Fluffy"),
            "This emoji is from the Fluffy pack."
        );
    }
}
