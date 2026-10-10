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

/// The label under a tapped custom emoji
/// (`lng_context_animated_emoji_preview`).
pub fn custom_emoji_preview_label(title: &str) -> String {
    format!("This emoji is from the {title} pack.")
}

#[cfg(test)]
mod tests {
    use super::{SetKind, can_archive, custom_emoji_preview_label, set_link, share_label};

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
