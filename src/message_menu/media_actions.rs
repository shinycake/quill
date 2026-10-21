//! The media actions of the message menu: the variants, their fixed slots
//! and labels.

use super::order;

/// Declares [`MediaAction`] and [`MediaAction::ALL`] from one list. Each
/// entry is a variant plus the slots it owns, as `pattern => value` pairs: a
/// variant with no slot cannot be written, and one that is not written does
/// not exist. The list order is tdesktop's `AddPhotoActions` /
/// `AddDocumentActions` order, and a variant with fields (`AddToSet`) gets
/// one slot per listed value.
macro_rules! media_actions {
    ($(
        $(#[$meta:meta])*
        $name:ident $({ $($field:ident : $ty:ty),+ })? => [$($pat:pat => $value:expr),+ $(,)?]
    ),+ $(,)?) => {
        /// One media action. Telegram Desktop's labels are in
        /// [`MediaAction::label`].
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum MediaAction {
            $($(#[$meta])* $name $({ $($field: $ty),+ })?,)+
        }

        impl MediaAction {
            /// Every slot's action, in slot order. Its length is the block width.
            pub const ALL: [Self; 0 $($(+ { let _ = stringify!($pat); 1 })+)+] =
                [$($($value),+),+];

            /// The listed value that stands for this action's slot.
            const fn canonical(self) -> Self {
                match self {
                    $($($pat => $value,)+)+
                }
            }
        }
    };
}

media_actions! {
    CancelDownload => [Self::CancelDownload => Self::CancelDownload],
    OpenGif => [Self::OpenGif => Self::OpenGif],
    SaveGif => [Self::SaveGif => Self::SaveGif],
    ViewStickerSet { installed: bool } => [
        Self::ViewStickerSet { .. } => Self::ViewStickerSet { installed: true },
    ],
    /// "Add to Sticker Set" / "Add to Emoji Set" for a sticker that
    /// belongs to no set (tdesktop `AddAddToOwnedSetAction`): a page
    /// listing the sets the user created. One slot per kind.
    AddToSet { emoji: bool } => [
        Self::AddToSet { emoji: false } => Self::AddToSet { emoji: false },
        Self::AddToSet { emoji: true } => Self::AddToSet { emoji: true },
    ],
    ToggleFavorite { remove: bool } => [
        Self::ToggleFavorite { .. } => Self::ToggleFavorite { remove: false },
    ],
    ShowInFolder => [Self::ShowInFolder => Self::ShowInFolder],
    /// "Save for Notifications" (`addSavedNotificationSound`).
    SaveForNotifications => [Self::SaveForNotifications => Self::SaveForNotifications],
    /// A song: "Save to..." with Profile, Saved Messages and Downloads.
    SaveTo => [Self::SaveTo => Self::SaveTo],
    SaveAs => [Self::SaveAs => Self::SaveAs],
    CopyImage => [Self::CopyImage => Self::CopyImage],
    CopyFilename => [Self::CopyFilename => Self::CopyFilename],
}

impl MediaAction {
    /// Fixed place in the media block, in tdesktop's order. Every action has
    /// its own slot, so rows never tie and never reach the Copy Text section.
    pub fn slot(self) -> u8 {
        let canonical = self.canonical();
        let index = Self::ALL.iter().position(|listed| *listed == canonical);
        // `canonical` returns a listed value, so the position exists.
        index.and_then(|i| u8::try_from(i).ok()).unwrap_or(0)
    }

    /// Sort key of this action's menu row.
    pub fn sort_key(self) -> u8 {
        // `MEDIA + MEDIA_SLOTS <= COPY_TEXT` is asserted above, and every slot
        // is below `MEDIA_SLOTS` (tested), so this stays inside the block.
        order::MEDIA.saturating_add(self.slot())
    }
}

impl MediaAction {
    pub fn label(self, finder: bool) -> &'static str {
        match self {
            Self::CancelDownload => "Cancel Download",
            Self::OpenGif => "Open GIF",
            Self::SaveGif => "Add to GIFs",
            Self::ViewStickerSet { installed: true } => "View Sticker Set",
            Self::ViewStickerSet { installed: false } => "Add Stickers",
            Self::AddToSet { emoji } => {
                crate::sticker_set_box::SetKind::of(emoji).add_to_set_label()
            }
            Self::ToggleFavorite { remove: false } => "Add to Favorites",
            Self::ToggleFavorite { remove: true } => "Remove from Favorites",
            Self::ShowInFolder if finder => "Show in Finder",
            Self::ShowInFolder => "Show in Folder",
            Self::SaveForNotifications => "Save for Notifications",
            Self::SaveTo => "Save to...",
            Self::SaveAs => "Save As...",
            Self::CopyImage => "Copy Image",
            Self::CopyFilename => "Copy Filename",
        }
    }

    /// Stable element id for the row.
    pub fn id(self) -> &'static str {
        match self {
            Self::CancelDownload => "menu-cancel-download",
            Self::OpenGif => "menu-open-gif",
            Self::SaveGif => "menu-save-gif",
            Self::ViewStickerSet { .. } => "menu-sticker-set",
            Self::AddToSet { emoji: false } => "menu-add-to-sticker-set",
            Self::AddToSet { emoji: true } => "menu-add-to-emoji-set",
            Self::ToggleFavorite { .. } => "menu-favorite-sticker",
            Self::ShowInFolder => "menu-show-in-folder",
            Self::SaveForNotifications => "menu-save-notification-tone",
            Self::SaveTo => "menu-save-to",
            Self::SaveAs => "menu-save-as",
            Self::CopyImage => "menu-copy-image",
            Self::CopyFilename => "menu-copy-filename",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MediaAction, order};
    use crate::message_menu::MEDIA_SLOTS;

    /// Every media action has its own sort key inside the media block, so
    /// the rows never tie with each other or reach Copy Text.
    #[test]
    fn media_actions_have_distinct_keys_inside_their_block() {
        let all = MediaAction::ALL;
        assert!(
            all.iter()
                .enumerate()
                .all(|(index, action)| usize::from(action.slot()) == index),
            "ALL lists every variant once, in slot order"
        );
        assert_eq!(all.len(), MEDIA_SLOTS, "one slot per action");
        let keys: Vec<u8> = all.iter().map(|action| action.sort_key()).collect();
        assert!(
            keys.iter().zip(keys.iter().skip(1)).all(|(a, b)| a < b),
            "{keys:?}"
        );
        assert!(keys.iter().all(|key| *key >= order::MEDIA));
        assert!(keys.iter().all(|key| *key < order::COPY_TEXT));
    }

    /// Every slot is a distinct action.
    #[test]
    fn all_has_no_duplicates() {
        let all = MediaAction::ALL;
        assert!(
            all.iter()
                .enumerate()
                .all(|(i, a)| all.iter().position(|b| b == a) == Some(i))
        );
    }

    /// `slot()` falls back to 0 only if `canonical()` returned a value
    /// missing from `ALL`; each entry's canonical value must sit at its own
    /// index.
    #[test]
    fn canonical_is_listed_at_its_own_index() {
        for (index, action) in MediaAction::ALL.iter().enumerate() {
            assert_eq!(
                MediaAction::ALL
                    .iter()
                    .position(|a| *a == action.canonical()),
                Some(index),
                "{action:?}"
            );
        }
    }
}
