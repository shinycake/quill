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
    /// The window showing the dialog, if it is open.
    pub(crate) host: SetBoxHost,
    /// "Edit Stickers" was pressed: cells drag to reorder, the footer is
    /// "Done" (tdesktop `setReorderState`).
    pub(crate) reordering: bool,
    /// The confirmation or rename box over the set box, if any.
    pub(crate) confirm: SetConfirm,
    pub(crate) rename_input: Entity<InputState>,
    /// The "New Sticker" / "New Emoji" box.
    pub(crate) create: Option<CreateSticker>,
    /// The box's "Done" is encoding a video sticker.
    pub(crate) encoding: Encoding,
    pub(crate) emoji_search: Entity<InputState>,
    /// The existing-sticker chooser.
    pub(crate) pick: Option<PickExisting>,
    /// A chosen sticker or video on its way to the editor.
    pub(crate) pending: PickWait,
}

/// The box over the set box (kit dialogs never stack, so one at a time).
#[derive(Debug, Default)]
pub(crate) enum SetConfirm {
    #[default]
    None,
    /// The "Edit Sticker Set Name" box.
    Rename,
    /// The "Delete sticker" box, with the sticker it asks about.
    DeleteSticker(StickerItem),
    /// "Delete for Everyone" confirmation.
    DeleteSet,
    /// "Delete for Myself" confirmation.
    RemoveSet,
}

impl SetConfirm {
    pub(crate) fn is_rename(&self) -> bool {
        matches!(self, Self::Rename)
    }

    pub(crate) fn is_delete_set(&self) -> bool {
        matches!(self, Self::DeleteSet)
    }

    pub(crate) fn is_remove_set(&self) -> bool {
        matches!(self, Self::RemoveSet)
    }

    /// The sticker the "Delete sticker" box asks about.
    pub(crate) fn delete_sticker(&self) -> Option<&StickerItem> {
        match self {
            Self::DeleteSticker(item) => Some(item),
            _ => None,
        }
    }

    /// Close the rename box; another box stays.
    pub(crate) fn close_rename(&mut self) {
        if self.is_rename() {
            *self = Self::None;
        }
    }

    /// Close the "Delete sticker" box, handing back its sticker.
    pub(crate) fn close_delete_sticker(&mut self) -> Option<StickerItem> {
        match std::mem::take(self) {
            Self::DeleteSticker(item) => Some(item),
            other => {
                *self = other;
                None
            }
        }
    }

    pub(crate) fn close_delete_set(&mut self) {
        if self.is_delete_set() {
            *self = Self::None;
        }
    }

    pub(crate) fn close_remove_set(&mut self) {
        if self.is_remove_set() {
            *self = Self::None;
        }
    }
}

/// Where a chosen existing sticker is on its way to the editor: one pick
/// at a time, so a new pick replaces whatever waited before it.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) enum PickWait {
    #[default]
    None,
    /// The file is downloading.
    Downloading(PendingVideo),
    /// A still sticker is being decoded for the photo editor.
    Decoding(PickExisting, FileId),
}

impl PickWait {
    pub(crate) fn downloading(&self) -> Option<&PendingVideo> {
        match self {
            Self::Downloading(pending) => Some(pending),
            _ => None,
        }
    }

    /// Stop waiting for the download, handing back what waited.
    pub(crate) fn take_downloading(&mut self) -> Option<PendingVideo> {
        match std::mem::take(self) {
            Self::Downloading(pending) => Some(pending),
            other => {
                *self = other;
                None
            }
        }
    }

    /// Stop waiting for the decode of this pick; false when it is not the
    /// one waited for (the box closed or another sticker was picked).
    pub(crate) fn finish_decoding(&mut self, pick: PickExisting, file_id: FileId) -> bool {
        if *self == Self::Decoding(pick, file_id) {
            *self = Self::None;
            true
        } else {
            false
        }
    }
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

/// The window that shows the sticker set dialog. The session holds one
/// viewed set and one editing state, so the box is open in one window at a
/// time: opening it elsewhere moves it, and closing it is one transition.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum SetBoxHost {
    #[default]
    Closed,
    /// Open in this window (`None`: the main window).
    In(Option<quill::ids::ChatId>),
}

impl StickerSetBoxUi {
    pub(crate) fn new(window: &mut Window, cx: &mut App) -> Self {
        Self {
            host: SetBoxHost::Closed,
            reordering: false,
            confirm: SetConfirm::None,
            rename_input: cx
                .new(|cx| InputState::new(window, cx).placeholder(tr(keys::Q_MEDIA_SET_NAME))),
            create: None,
            encoding: Encoding::Idle,
            emoji_search: cx
                .new(|cx| InputState::new(window, cx).placeholder(tr(keys::Q_MEDIA_SEARCH_EMOJI))),
            pick: None,
            pending: PickWait::None,
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
        self.confirm = SetConfirm::None;
        self.discard_create();
        self.pick = None;
        self.pending = PickWait::None;
    }
}

/// An encode that runs stops, and the editor's files go: the account on
/// screen is replaced whole, and nothing else would cancel the job or
/// remove them. The set it viewed (and a change to it in flight) lives in
/// the session, which outlives the screen: it closes too, so the account
/// comes back with no stale set.
impl crate::ui::on_screen::Farewell for StickerSetBoxUi {
    fn farewell(
        app: &mut crate::ui::app::QuillApp,
        _: &mut gpui_kit::Window,
        _: &mut gpui_kit::Context<crate::ui::app::QuillApp>,
    ) {
        app.sticker_box.reset();
        if let Some(session) = app.session_mut() {
            session.stickers.sticker_set_view = None;
        }
    }
}

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

#[cfg(test)]
mod confirm_tests {
    use super::{PickExisting, PickWait, SetConfirm, SetKind};
    use quill::ids::FileId;

    fn pick() -> PickExisting {
        PickExisting {
            set_id: 7,
            kind: SetKind::Stickers,
            adapt: false,
        }
    }

    #[test]
    fn closing_one_box_leaves_another_open() {
        let mut confirm = SetConfirm::DeleteSet;
        confirm.close_rename();
        confirm.close_remove_set();
        assert!(
            confirm.is_delete_set(),
            "another box's close is not its own"
        );
        confirm.close_delete_set();
        assert!(matches!(confirm, SetConfirm::None));
    }

    #[test]
    fn opening_a_box_replaces_the_one_under_it() {
        let mut confirm = SetConfirm::Rename;
        assert!(confirm.is_rename());
        confirm = SetConfirm::RemoveSet;
        assert!(!confirm.is_rename() && confirm.is_remove_set());
    }

    #[test]
    fn a_new_pick_replaces_the_one_waiting_and_only_its_own_decode_lands() {
        let mut wait = PickWait::Decoding(pick(), FileId(1));
        assert!(!wait.finish_decoding(pick(), FileId(2)), "another sticker");
        assert!(wait.finish_decoding(pick(), FileId(1)));
        assert_eq!(wait, PickWait::None);
        assert!(!wait.finish_decoding(pick(), FileId(1)), "already landed");
    }
}
