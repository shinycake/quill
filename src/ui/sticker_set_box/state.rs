//! UI state of the sticker set box and its editing flows.
use gpui_kit::component::input::InputState;
use gpui_kit::*;
use quill::sticker_set_box::SetKind;
use quill::telegram::envelope::{StickerFormat, StickerItem};
use std::path::PathBuf;

/// A new sticker on its way into an owned set: the editor's output and the
/// emoji chosen for it (tdesktop `CreateMediaBox`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CreateSticker {
    pub set_id: i64,
    pub kind: SetKind,
    /// The file to upload: the PNG the photo editor wrote, fitted to the
    /// set's side, or the WEBM the video editor encoded.
    pub path: PathBuf,
    /// `stickerFormatWebp` for the PNG, `stickerFormatWebm` for the video.
    pub format: StickerFormat,
    /// The still the box shows: the PNG itself, or a frame of the video
    /// (`None` when the frame could not be read).
    pub preview: Option<PathBuf>,
    pub emojis: Vec<String>,
}

/// The "Add an Existing Sticker" / "Adapt from a Sticker" chooser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PickExisting {
    pub set_id: i64,
    pub kind: SetKind,
    /// "Adapt from a Sticker": a sticker becomes an emoji (animated ones
    /// go in as they are, images through the editor at emoji size).
    pub adapt: bool,
}

/// The set box's editing state (`ui/sticker_set_box/`).
pub(crate) struct StickerSetBoxUi {
    /// "Edit Stickers" was pressed: cells drag to reorder, the footer is
    /// "Done" (tdesktop `setReorderState`).
    pub(crate) reordering: bool,
    /// The "Edit Sticker Set Name" box is open.
    pub(crate) rename_open: bool,
    pub(crate) rename_input: Entity<InputState>,
    /// The "Delete sticker" box, with the sticker it asks about.
    pub(crate) delete_sticker: Option<StickerItem>,
    /// "Delete for Everyone" confirmation.
    pub(crate) delete_set_open: bool,
    /// "Delete for Myself" confirmation.
    pub(crate) remove_set_open: bool,
    /// The "New Sticker" / "New Emoji" box.
    pub(crate) create: Option<CreateSticker>,
    pub(crate) emoji_search: Entity<InputState>,
    /// The existing-sticker chooser.
    pub(crate) pick: Option<PickExisting>,
}

impl StickerSetBoxUi {
    pub(crate) fn new(window: &mut Window, cx: &mut App) -> Self {
        Self {
            reordering: false,
            rename_open: false,
            rename_input: cx.new(|cx| InputState::new(window, cx).placeholder("Set name")),
            delete_sticker: None,
            delete_set_open: false,
            remove_set_open: false,
            create: None,
            emoji_search: cx.new(|cx| InputState::new(window, cx).placeholder("Search emoji")),
            pick: None,
        }
    }

    /// Leave every editing flow (the box closed or changed set).
    pub(crate) fn reset(&mut self) {
        self.reordering = false;
        self.rename_open = false;
        self.delete_sticker = None;
        self.delete_set_open = false;
        self.remove_set_open = false;
        self.create = None;
        self.pick = None;
    }
}
