//! UI state of the sticker set box and its editing flows.
use gpui_kit::component::input::InputState;
use gpui_kit::*;
use quill::ids::FileId;
use quill::lang::{keys, tr};
use quill::sticker_set_box::SetKind;
use quill::sticker_video::VideoStickerJob;
use quill::telegram::envelope::{StickerFormat, StickerItem};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A new sticker on its way into an owned set: the editor's output and the
/// emoji chosen for it (tdesktop `CreateMediaBox`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CreateSticker {
    pub set_id: i64,
    pub kind: SetKind,
    /// The file to upload: the PNG the photo editor wrote, fitted to the
    /// set's side, or the video the editor was on until "Done" encodes it.
    pub path: PathBuf,
    /// `stickerFormatWebp` for the PNG, `stickerFormatWebm` for the video.
    pub format: StickerFormat,
    /// The still the box shows: the PNG itself, or the video editor's
    /// cover frame (`None` when the frame could not be read).
    pub preview: Option<PathBuf>,
    pub emojis: Vec<String>,
    /// A video sticker still to encode; "Done" runs it (`RunWebmSticker`).
    pub video: Option<VideoStickerJob>,
}

/// The file is one Quill's editors wrote under its own temp folders
/// (`quill-edits`, `quill-video-edits`, `quill-sticker-video`; in the
/// private work root, or the shared temp dir of older versions), never a
/// downloaded or picked file.
fn is_editor_temp(path: &Path) -> bool {
    let Some(parent) = path.parent() else {
        return false;
    };
    let grandparent = parent.parent();
    (grandparent == Some(std::env::temp_dir().as_path())
        || grandparent == Some(quill::scratch::work_root().as_path()))
        && parent
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("quill-"))
}

impl CreateSticker {
    /// The box was dismissed: the editor's files are no longer needed.
    pub fn discard(self) {
        for path in self.preview.into_iter().chain(Some(self.path)) {
            if is_editor_temp(&path) {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    /// The sticker was handed to TDLib, which reads the file itself; only
    /// the box's still goes.
    pub fn uploaded(self) {
        if let Some(preview) = self.preview
            && is_editor_temp(&preview)
        {
            let _ = std::fs::remove_file(preview);
        }
    }
}

/// A video chosen for the set that is still downloading; the editor opens
/// on it once the file is here (tdesktop `LoadDocumentMedia`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingVideo {
    pub pick: PickExisting,
    pub file_id: FileId,
    pub emoji: Option<String>,
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
    /// The box's "Done" is encoding a video sticker; set to stop it.
    pub(crate) encoding: Option<Arc<AtomicBool>>,
    pub(crate) emoji_search: Entity<InputState>,
    /// The existing-sticker chooser.
    pub(crate) pick: Option<PickExisting>,
    /// A chosen video waiting for its download.
    pub(crate) pending_video: Option<PendingVideo>,
    /// A chosen still sticker being decoded for the editor.
    pub(crate) pending_still: Option<(PickExisting, FileId)>,
}

impl StickerSetBoxUi {
    pub(crate) fn new(window: &mut Window, cx: &mut App) -> Self {
        Self {
            reordering: false,
            rename_open: false,
            rename_input: cx
                .new(|cx| InputState::new(window, cx).placeholder(tr(keys::Q_MEDIA_SET_NAME))),
            delete_sticker: None,
            delete_set_open: false,
            remove_set_open: false,
            create: None,
            encoding: None,
            emoji_search: cx
                .new(|cx| InputState::new(window, cx).placeholder(tr(keys::Q_MEDIA_SEARCH_EMOJI))),
            pick: None,
            pending_video: None,
            pending_still: None,
        }
    }

    /// Drop the "New Sticker" box's sticker: stop an encode that runs and
    /// remove the editor's files.
    pub(crate) fn discard_create(&mut self) {
        if let Some(cancel) = self.encoding.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        if let Some(create) = self.create.take() {
            create.discard();
        }
    }

    /// Leave every editing flow (the box closed or changed set).
    pub(crate) fn reset(&mut self) {
        self.reordering = false;
        self.rename_open = false;
        self.delete_sticker = None;
        self.delete_set_open = false;
        self.remove_set_open = false;
        self.discard_create();
        self.pick = None;
        self.pending_video = None;
        self.pending_still = None;
    }
}
