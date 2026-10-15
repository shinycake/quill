//! What the Touch Bar shows, as plain data (no AppKit), after Telegram
//! Desktop's `platform/mac/touchbar/`:
//!
//! - the main bar (`mac_touchbar_main.mm`): the pinned chats panel, then
//!   the text formatting popover while the composer has a selection, else
//!   the sticker and emoji picker while the open chat accepts them;
//! - the audio player (`mac_touchbar_audio.mm`) while a song is playing;
//! - the media viewer's controls (`mac_touchbar_media_view.mm`): rotate for
//!   a photo, rotate, full screen, picture-in-picture, play/pause, the seek
//!   bar and the position for a video;
//! - nothing while the passcode lock is on or a voice message is recorded.
//!
//! The AppKit side (`mac`) only mirrors this model.

use quill::emoji_catalog::{CATEGORIES, RECENT_LIMIT, catalog};
use quill::ids::{ChatId, FileId};
use quill::lang::{keys, tr};
use quill::telegram::envelope::StickerItem;
use std::path::PathBuf;

/// tdesktop `kCircleDiameter`: the height of every item, the side of a
/// pinned chat's picture.
pub(crate) const CIRCLE_DIAMETER: f64 = 30.;
/// tdesktop `kPinnedButtonsSpace`: the gap between two pinned chats.
pub(crate) const PINNED_SPACE: f64 = 30.;
/// tdesktop `kPinnedButtonsLeftSkip`.
pub(crate) const PINNED_LEFT_SKIP: f64 = PINNED_SPACE / 2.;
/// tdesktop `kMaxStickerSets`: the installed sets the sticker scrubber lists.
pub(crate) const MAX_STICKER_SETS: usize = 5;

/// What a picture in the pinned chats panel stands for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PinKind {
    /// The archive folder, first while it holds chats.
    Archive,
    /// Saved Messages: added after the archive when it is not pinned, and
    /// drawn with the bookmark wherever it is.
    SavedMessages,
    /// A pinned chat.
    Chat,
}

/// The unread badge at a pinned chat's bottom-right.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Badge {
    /// The number, or `None` for a chat only marked as unread (a dot).
    pub(crate) count: Option<i32>,
    pub(crate) muted: bool,
}

/// One picture of the pinned chats panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Pin {
    pub(crate) kind: PinKind,
    /// The chat to open (`None` for the archive).
    pub(crate) chat_id: Option<ChatId>,
    /// For the initials of a chat without a photo.
    pub(crate) title: String,
    pub(crate) photo: Option<PathBuf>,
    /// Pinned chats only; tdesktop shows no badge on Saved Messages.
    pub(crate) badge: Option<Badge>,
    /// A person (not a bot) who is online: the green dot, when no badge.
    pub(crate) online: bool,
    /// One of the main list's pinned chats.
    pub(crate) pinned: bool,
}

impl Pin {
    /// Whether the picture can be dragged to a new place: only pinned
    /// chats move, the archive and an unpinned Saved Messages stay first.
    pub(crate) fn reorderable(&self) -> bool {
        self.chat_id.is_some() && self.pinned
    }
}

/// The pinned chats panel, left to right.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PinnedPanel {
    pub(crate) pins: Vec<Pin>,
}

/// What follows the pinned chats on the main bar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Accessory {
    #[default]
    None,
    /// The composer has a selection: the formatting popover.
    Format,
    /// The open chat accepts stickers or messages: the picker popover.
    Picker,
}

/// The song in the player.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Track {
    pub(crate) playing: bool,
    pub(crate) can_previous: bool,
    pub(crate) can_next: bool,
    pub(crate) position: f64,
    pub(crate) duration: f64,
}

/// The media viewer's controls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ViewerBar {
    Photo,
    Video {
        playing: bool,
        fullscreen: bool,
        position: f64,
        duration: f64,
    },
    /// Anything else (a document, a sticker): no items.
    Other,
}

/// The whole Touch Bar.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Bar {
    /// Signed out, locked, or recording a voice message.
    Empty,
    Main {
        pinned: PinnedPanel,
        accessory: Accessory,
    },
    Audio(Track),
    Viewer(ViewerBar),
}

/// The inputs `bar` decides from.
#[derive(Clone, Debug, Default)]
pub(crate) struct BarInputs {
    pub(crate) signed_in: bool,
    pub(crate) locked: bool,
    pub(crate) recording: bool,
    /// The media viewer is open on this.
    pub(crate) viewer: Option<ViewerBar>,
    /// A song (not a voice message) is playing; a paused one keeps the
    /// player, as tdesktop switches only on start and stop.
    pub(crate) song: Option<Track>,
    pub(crate) composer_selection: bool,
    pub(crate) chat_accepts_stickers_or_text: bool,
}

/// tdesktop `RootTouchBar`: lock and recording empty the bar, a song takes
/// it over, otherwise the main bar while signed in. The viewer has its own
/// window (and bar) in tdesktop; here it covers the main window, so its
/// controls replace the main bar while it is open.
pub(crate) fn bar(inputs: &BarInputs, pinned: impl FnOnce() -> PinnedPanel) -> Bar {
    if !inputs.signed_in || inputs.locked {
        return Bar::Empty;
    }
    if let Some(viewer) = inputs.viewer {
        return Bar::Viewer(viewer);
    }
    if inputs.recording {
        return Bar::Empty;
    }
    if let Some(track) = inputs.song {
        return Bar::Audio(track);
    }
    let accessory = if inputs.composer_selection {
        Accessory::Format
    } else if inputs.chat_accepts_stickers_or_text {
        Accessory::Picker
    } else {
        Accessory::None
    };
    Bar::Main {
        pinned: pinned(),
        accessory,
    }
}

/// tdesktop's order: the archive while it has chats, Saved Messages unless
/// it is pinned, then the pinned chats in their order.
pub(crate) fn pinned_panel(archive: bool, saved: Option<Pin>, pinned: Vec<Pin>) -> PinnedPanel {
    let saved_pinned = pinned.iter().any(|pin| pin.kind == PinKind::SavedMessages);
    let mut pins = Vec::with_capacity(pinned.len() + 2);
    if archive {
        pins.push(Pin {
            kind: PinKind::Archive,
            chat_id: None,
            title: tr(keys::Q_MISC_ARCHIVED_CHATS).into(),
            photo: None,
            badge: None,
            online: false,
            pinned: false,
        });
    }
    if !saved_pinned && let Some(saved) = saved {
        pins.push(saved);
    }
    pins.extend(pinned);
    PinnedPanel { pins }
}

/// The left edge of the picture in slot `index` (tdesktop
/// `PeerRectByIndex`).
pub(crate) fn pin_x(index: usize) -> f64 {
    index as f64 * (CIRCLE_DIAMETER + PINNED_SPACE) + PINNED_LEFT_SKIP
}

/// The panel's width for `count` pictures.
pub(crate) fn panel_width(count: usize) -> f64 {
    if count == 0 {
        return 0.;
    }
    pin_x(count) - PINNED_SPACE
}

/// The slot a touch at `x` lands on (tdesktop `indexFromX`), if any.
pub(crate) fn index_at(x: f64, count: usize) -> Option<usize> {
    let shifted = x - PINNED_LEFT_SKIP + PINNED_SPACE / 2.;
    if shifted < 0. {
        return None;
    }
    let index = (shifted / (CIRCLE_DIAMETER + PINNED_SPACE)).floor() as usize;
    (index < count).then_some(index)
}

/// Where a dragged picture drops: the dragged one moved `shift` points
/// from slot `from`; the others step aside once its middle passes theirs
/// (tdesktop's horizontal `updateOrder`). Stays within `first..count`,
/// the reorderable slots.
pub(crate) fn drop_index(from: usize, shift: f64, first: usize, count: usize) -> usize {
    let pitch = CIRCLE_DIAMETER + PINNED_SPACE;
    let steps = (shift / pitch).round();
    let target = from as f64 + steps;
    let last = count.saturating_sub(1);
    (target.max(first as f64).min(last as f64)) as usize
}

/// The pinned chat order after dragging slot `from` to slot `to`, as
/// `setPinnedChats` wants it (pinned chats only, in panel order).
pub(crate) fn reordered_ids(pins: &[Pin], from: usize, to: usize) -> Option<Vec<i64>> {
    if from == to {
        return None;
    }
    let mut order: Vec<&Pin> = pins.iter().collect();
    if from >= order.len() || to >= order.len() {
        return None;
    }
    let moved = order.remove(from);
    order.insert(to, moved);
    Some(
        order
            .into_iter()
            .filter(|pin| pin.reorderable())
            .filter_map(|pin| pin.chat_id.map(|id| id.0))
            .collect(),
    )
}

/// A sticker the picker can send.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PickerSticker {
    pub(crate) file_id: FileId,
    pub(crate) emoji: String,
    pub(crate) width: i32,
    pub(crate) height: i32,
    pub(crate) thumb: Option<(FileId, i32, i32)>,
    /// The file the scrubber draws (the static sticker or its thumbnail).
    pub(crate) display: Option<FileId>,
    pub(crate) premium: bool,
}

impl PickerSticker {
    pub(crate) fn from_item(item: &StickerItem) -> Self {
        Self {
            file_id: item.file_id,
            emoji: item.emoji.clone(),
            width: item.width,
            height: item.height,
            thumb: item
                .thumb_file_id
                .filter(|id| id.0 != 0)
                .map(|id| (id, item.thumb_width, item.thumb_height)),
            display: item.display_file_id(),
            premium: item.requires_premium,
        }
    }
}

/// One cell of the sticker or emoji scrubber (tdesktop
/// `PickerScrubberItem`): a section title, a sticker or an emoji.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PickerItem {
    Title(String),
    Sticker(PickerSticker),
    Emoji(String),
}

/// One installed sticker set as the scrubber needs it.
pub(crate) struct StickerSection<'a> {
    pub(crate) title: &'a str,
    pub(crate) stickers: &'a [StickerItem],
}

/// tdesktop `updateStickers`: favorites, then recent stickers that are not
/// favorites, then the first installed sets; "No stickers found" when all
/// are empty. Premium stickers are left out where Premium can't be had.
pub(crate) fn sticker_items(
    favorites: &[StickerItem],
    recent: &[StickerItem],
    sets: &[StickerSection<'_>],
    show_premium: bool,
) -> Vec<PickerItem> {
    let listed = |item: &&StickerItem| show_premium || !item.requires_premium;
    let mut items = Vec::new();
    let favorites: Vec<&StickerItem> = favorites.iter().filter(listed).collect();
    if !favorites.is_empty() {
        items.push(PickerItem::Title(tr(keys::Q_MISC_FAVORITE_STICKERS).into()));
        items.extend(
            favorites
                .iter()
                .map(|item| PickerItem::Sticker(PickerSticker::from_item(item))),
        );
    }
    let recent: Vec<&StickerItem> = recent
        .iter()
        .filter(listed)
        .filter(|item| !favorites.iter().any(|fav| fav.id == item.id))
        .collect();
    if !recent.is_empty() {
        items.push(PickerItem::Title(tr(keys::Q_MISC_RECENTLY_USED).into()));
        items.extend(
            recent
                .iter()
                .map(|item| PickerItem::Sticker(PickerSticker::from_item(item))),
        );
    }
    for set in sets.iter().take(MAX_STICKER_SETS) {
        let stickers: Vec<&StickerItem> = set.stickers.iter().filter(listed).collect();
        if stickers.is_empty() {
            continue;
        }
        items.push(PickerItem::Title(set.title.to_string()));
        items.extend(
            stickers
                .iter()
                .map(|item| PickerItem::Sticker(PickerSticker::from_item(item))),
        );
    }
    if items.is_empty() {
        items.push(PickerItem::Title(tr(keys::Q_MISC_NO_STICKERS_FOUND).into()));
    }
    items
}

/// tdesktop `AppendEmojiPacks`: the recently used emoji, then each
/// category under its title (Quill's catalog and categories, without the
/// skin tone variants, as in the emoji panel).
pub(crate) fn emoji_items(recent: &[String]) -> Vec<PickerItem> {
    let mut items = vec![PickerItem::Title(tr(keys::Q_MISC_RECENTLY_USED).into())];
    items.extend(
        recent
            .iter()
            .take(RECENT_LIMIT)
            .map(|emoji| PickerItem::Emoji(emoji.clone())),
    );
    for category in CATEGORIES {
        items.push(PickerItem::Title((*category).to_string()));
        items.extend(
            catalog()
                .filter(|entry| entry.category == *category && !has_skin_tone(entry.emoji))
                .map(|entry| PickerItem::Emoji(entry.emoji.to_string())),
        );
    }
    items
}

fn has_skin_tone(emoji: &str) -> bool {
    emoji
        .chars()
        .any(|c| ('\u{1F3FB}'..='\u{1F3FF}').contains(&c))
}

/// The formatting popover's segments, in tdesktop's order and words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FormatCommand {
    Bold,
    Italic,
    Underline,
    Strikethrough,
    Quote,
    Monospace,
    Clear,
    Link,
}

impl FormatCommand {
    pub(crate) const ALL: [FormatCommand; 8] = [
        Self::Bold,
        Self::Italic,
        Self::Underline,
        Self::Strikethrough,
        Self::Quote,
        Self::Monospace,
        Self::Clear,
        Self::Link,
    ];

    /// tdesktop `lng_menu_formatting_*` and `lng_info_link_label`.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Bold => tr(keys::Q_MISC_BOLD),
            Self::Italic => tr(keys::Q_MISC_ITALIC),
            Self::Underline => tr(keys::Q_MISC_UNDERLINE),
            Self::Strikethrough => tr(keys::Q_MISC_STRIKETHROUGH),
            Self::Quote => tr(keys::Q_MISC_QUOTE),
            Self::Monospace => tr(keys::Q_MISC_MONOSPACE),
            Self::Clear => tr(keys::Q_MISC_CLEAR_FORMATTING),
            Self::Link => tr(keys::Q_MISC_LINK),
        }
    }
}

/// What a touch asks the app to do.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Command {
    OpenChat(ChatId),
    OpenSavedMessages,
    OpenArchive,
    /// The pinned chats in their new order (`setPinnedChats`).
    ReorderPinned(Vec<i64>),
    Format(FormatCommand),
    /// A picker popover opened: load what it lists.
    PickerShown,
    SendSticker(PickerSticker),
    InsertEmoji(String),
    PlayerPlayPause,
    PlayerPrevious,
    PlayerNext,
    PlayerClose,
    /// The seek bar moved to `fraction`; `done` on release.
    PlayerSeek {
        fraction: f64,
        done: bool,
    },
    ViewerPlayPause,
    ViewerRotate,
    ViewerFullscreen,
    ViewerPictureInPicture,
    ViewerSeek {
        fraction: f64,
    },
}

/// "M:SS" or "H:MM:SS" (tdesktop `Ui::FormatDurationText`).
pub(crate) fn duration_text(secs: f64) -> String {
    let total = if secs.is_finite() && secs > 0. {
        secs as u64
    } else {
        0
    };
    let (hours, minutes, seconds) = (total / 3600, total / 60 % 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// tdesktop `ComputeUnreadBadgeText` with two digits allowed: past three
/// digits the count shows as ".." and its last two.
pub(crate) fn badge_text(count: i32) -> String {
    let text = count.to_string();
    if text.len() > 3 {
        format!("..{}", text.get(text.len() - 2..).unwrap_or_default())
    } else {
        text
    }
}

/// The player's and viewer's position label, "0:42 / 3:10".
pub(crate) fn position_text(position: f64, duration: f64) -> String {
    format!("{} / {}", duration_text(position), duration_text(duration))
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
