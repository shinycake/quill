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

    /// The encoded video of a failed upload: the box keeps the editor's
    /// video, so only this file goes.
    pub fn drop_encoded_file(&self) {
        if is_editor_temp(&self.path) {
            let _ = std::fs::remove_file(&self.path);
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
    /// What opens when the file is here.
    pub file: PickedFile,
    pub emoji: Option<String>,
    /// The chat window the pick was made in (`None`: the main window):
    /// the editor opens there, in that window's composer state.
    pub window: Option<quill::ids::ChatId>,
}

/// What a chosen file becomes once it is downloaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PickedFile {
    /// A video: the video editor opens on it.
    Video,
    /// A still sticker of this format: it is decoded for the photo editor.
    Still(StickerFormat),
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
    /// The box's "Done" is encoding a video sticker.
    pub(crate) encoding: Encoding,
    pub(crate) emoji_search: Entity<InputState>,
    /// The existing-sticker chooser.
    pub(crate) pick: Option<PickExisting>,
    /// A chosen video waiting for its download.
    pub(crate) pending_video: Option<PendingVideo>,
    /// A chosen still sticker being decoded for the editor.
    pub(crate) pending_still: Option<(PickExisting, FileId)>,
}

/// The video encode behind "Done": at most one runs, and only the job that
/// holds the encode's own cancel flag may end it, so an old job finishing
/// after a cancel and a new "Done" leaves the new one alone.
#[derive(Default)]
pub(crate) enum Encoding {
    #[default]
    Idle,
    Running {
        cancel: Arc<AtomicBool>,
    },
}

impl Encoding {
    pub(crate) fn is_running(&self) -> bool {
        matches!(self, Self::Running { .. })
    }

    /// A job starts with its cancel flag.
    pub(crate) fn start(&mut self, cancel: Arc<AtomicBool>) {
        *self = Self::Running { cancel };
    }

    /// The job holding `cancel` finished: whether it is still the running
    /// one (then the encode is over). A job that was cancelled or replaced
    /// changes nothing.
    pub(crate) fn finish(&mut self, cancel: &Arc<AtomicBool>) -> bool {
        match self {
            Self::Running { cancel: running } if Arc::ptr_eq(running, cancel) => {
                *self = Self::Idle;
                true
            }
            _ => false,
        }
    }

    /// Stop the running job, if any, between quality steps.
    pub(crate) fn cancel(&mut self) {
        if let Self::Running { cancel } = std::mem::take(self) {
            cancel.store(true, Ordering::Relaxed);
        }
    }
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
            encoding: Encoding::Idle,
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
        self.encoding.cancel();
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

/// Nothing to say: its state is the account's only app-side.
impl crate::ui::on_screen::Farewell for StickerSetBoxUi {}

#[cfg(test)]
mod encoding_tests {
    use super::Encoding;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// An old encode that finishes after a cancel and a new "Done" must not
    /// clear the new encode, or "Done" would run a second one and the
    /// first would lose its cancel.
    #[test]
    fn a_finished_old_encode_leaves_the_newer_one_running() {
        let mut encoding = Encoding::default();
        let old = Arc::new(AtomicBool::new(false));
        encoding.start(old.clone());
        encoding.cancel();
        assert!(old.load(Ordering::Relaxed), "the close stopped the old job");
        assert!(!encoding.is_running());
        let newer = Arc::new(AtomicBool::new(false));
        encoding.start(newer.clone());
        assert!(!encoding.finish(&old), "the old job is not the running one");
        assert!(encoding.is_running());
        assert!(encoding.finish(&newer));
        assert!(!encoding.is_running());
    }
}
