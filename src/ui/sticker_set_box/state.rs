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
    /// The one editing flow the box is in (reordering, choosing an
    /// existing sticker, waiting for its file, creating a sticker).
    pub(crate) editing: SetEditing,
    /// The confirmation or rename box over the set box, if any.
    pub(crate) confirm: SetConfirm,
    pub(crate) rename_input: Entity<InputState>,
    pub(crate) emoji_search: Entity<InputState>,
}

/// The editing flow of the set box. One at a time: entering a flow ends
/// the one before it (`SetEditing::enter`), so no flow can keep a chooser,
/// a pending download or a create box over the wrong set. Every flow has
/// a way out: its answer, a cancel, or the box closing (`reset`).
#[derive(Default)]
pub(crate) enum SetEditing {
    #[default]
    Idle,
    /// "Edit Stickers" was pressed: cells drag to reorder, the footer is
    /// "Done" (tdesktop `setReorderState`).
    Reordering,
    /// The existing-sticker chooser is open.
    Picking(PickExisting),
    /// A chosen sticker or video on its way to the editor.
    Waiting(PickWait),
    /// The open-file dialog for a new sticker's image is up; the stage holds
    /// the task that waits for the pick.
    Choosing { _task: Task<()> },
    /// The edited image is written off the UI thread; the stage holds the
    /// job, and the "New Sticker" box replaces it when it lands.
    Writing { _task: Task<()> },
    /// The "New Sticker" / "New Emoji" box, and the encode behind its
    /// "Done" for a video.
    Creating {
        create: CreateSticker,
        encoding: Encoding,
    },
}

impl SetEditing {
    /// Enter `next`, ending the flow before it: a create box's editor
    /// files go and its encode stops.
    pub(crate) fn enter(&mut self, next: SetEditing) {
        self.discard_create();
        *self = next;
    }

    /// At rest: nothing chosen is on its way and no box is open, so the
    /// set box's controls act; any other stage would be ended by a press.
    pub(crate) fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }

    pub(crate) fn is_reordering(&self) -> bool {
        matches!(self, Self::Reordering)
    }

    /// "Edit Stickers": only from rest, so a chosen file still on its way
    /// is not dropped by a press.
    pub(crate) fn begin_reordering(&mut self) {
        if matches!(self, Self::Idle) {
            *self = Self::Reordering;
        }
    }

    pub(crate) fn finish_reordering(&mut self) {
        if self.is_reordering() {
            *self = Self::Idle;
        }
    }

    pub(crate) fn pick(&self) -> Option<PickExisting> {
        match self {
            Self::Picking(pick) => Some(*pick),
            _ => None,
        }
    }

    /// The image dialog opens, held by `task`.
    pub(crate) fn begin_choosing(&mut self, task: Task<()>) {
        self.enter(Self::Choosing { _task: task });
    }

    /// The image dialog answered; another flow stays.
    pub(crate) fn finish_choosing(&mut self) {
        if matches!(self, Self::Choosing { .. }) {
            *self = Self::Idle;
        }
    }

    /// The edited image is being written, held by `task`.
    pub(crate) fn begin_writing(&mut self, task: Task<()>) {
        self.enter(Self::Writing { _task: task });
    }

    /// The write ended without a sticker to create; another flow stays.
    pub(crate) fn finish_writing(&mut self) {
        if matches!(self, Self::Writing { .. }) {
            *self = Self::Idle;
        }
    }

    /// The chooser closed; another flow stays.
    pub(crate) fn close_pick(&mut self) {
        if matches!(self, Self::Picking(_)) {
            *self = Self::Idle;
        }
    }

    pub(crate) fn create(&self) -> Option<&CreateSticker> {
        match self {
            Self::Creating { create, .. } => Some(create),
            _ => None,
        }
    }

    pub(crate) fn create_mut(&mut self) -> Option<&mut CreateSticker> {
        match self {
            Self::Creating { create, .. } => Some(create),
            _ => None,
        }
    }

    /// The "New Sticker" box opens on `create`.
    pub(crate) fn begin_create(&mut self, create: CreateSticker) {
        self.enter(Self::Creating {
            create,
            encoding: Encoding::Idle,
        });
    }

    /// Drop the "New Sticker" box's sticker: the encode that runs stops
    /// (dropping its stage cancels it) and the editor's files go. Another
    /// flow stays.
    pub(crate) fn discard_create(&mut self) {
        if matches!(self, Self::Creating { .. })
            && let Self::Creating { create, encoding } = std::mem::take(self)
        {
            drop(encoding);
            create.discard();
        }
    }

    /// The sticker was handed to TDLib: the box is over (its encode, if
    /// any, finished already).
    pub(crate) fn finish_upload(&mut self) {
        if matches!(self, Self::Creating { .. }) {
            *self = Self::Idle;
        }
    }

    pub(crate) fn encoding_running(&self) -> bool {
        matches!(
            self,
            Self::Creating {
                encoding: Encoding::Running { .. },
                ..
            }
        )
    }

    /// "Done" starts the video encode of the create box; false when the
    /// box is gone or one already runs.
    pub(crate) fn start_encoding(&mut self, running: Encoding) -> bool {
        match self {
            Self::Creating { encoding, .. } if !encoding.is_running() => {
                *encoding = running;
                true
            }
            _ => false,
        }
    }

    /// The encode finished: whether it was running (a stage that left
    /// cancelled its job, so the answer of an old one never lands).
    pub(crate) fn finish_encoding(&mut self) -> bool {
        match self {
            Self::Creating { encoding, .. } if encoding.is_running() => {
                *encoding = Encoding::Idle;
                true
            }
            _ => false,
        }
    }

    pub(crate) fn downloading(&self) -> Option<&PendingVideo> {
        match self {
            Self::Waiting(wait) => wait.downloading(),
            _ => None,
        }
    }

    /// The download of the waited-for file ended with the file: the pick
    /// moves on to open its editor in the window it was made in. False
    /// when nothing waits on a download.
    pub(crate) fn file_landed(&mut self, path: String) -> bool {
        match std::mem::take(self) {
            Self::Waiting(PickWait::Downloading(pending)) => {
                *self = Self::Waiting(PickWait::Downloaded { pending, path });
                true
            }
            other => {
                *self = other;
                false
            }
        }
    }

    /// The pick whose file is here, waiting for its window's frame.
    pub(crate) fn downloaded(&self) -> Option<&PendingVideo> {
        match self {
            Self::Waiting(PickWait::Downloaded { pending, .. }) => Some(pending),
            _ => None,
        }
    }

    /// Stop waiting for the window's frame, handing back the pick and the
    /// path of its file.
    pub(crate) fn take_downloaded(&mut self) -> Option<(PendingVideo, String)> {
        match std::mem::take(self) {
            Self::Waiting(PickWait::Downloaded { pending, path }) => Some((pending, path)),
            other => {
                *self = other;
                None
            }
        }
    }

    /// The wait for a download or a decode ends (the box closed, the file
    /// failed); another flow stays.
    pub(crate) fn end_wait(&mut self) {
        if matches!(self, Self::Waiting(_)) {
            *self = Self::Idle;
        }
    }

    /// The decode of this pick landed: false when it is not the one
    /// waited for (the box closed or another sticker was picked).
    pub(crate) fn finish_decoding(&mut self, pick: PickExisting, file_id: FileId) -> bool {
        let waited = matches!(
            self,
            Self::Waiting(PickWait::Decoding { pick: p, file_id: f, .. }) if *p == pick && *f == file_id
        );
        if waited {
            *self = Self::Idle;
        }
        waited
    }
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
pub(crate) enum PickWait {
    /// The chosen file is not local yet and has been asked for. Quill holds
    /// no request or task for it: `downloadFile` answers at once and shares
    /// a transfer already under way, and the download tracker
    /// (`media.downloads`) owns the transfer, keyed by `pending.file_id`.
    /// The wait ends with that transfer's own end, or when none is out
    /// (`settle_sticker_pick_download`, in the session update path): the
    /// file landed (`Downloaded`) or nothing will bring it.
    Downloading(PendingVideo),
    /// The file is here. The editor opens in the window the pick was made
    /// in, so it opens in that window's next frame.
    Downloaded { pending: PendingVideo, path: String },
    /// A still sticker is being decoded for the photo editor; the stage
    /// holds the job, so leaving it cancels the decode.
    Decoding {
        pick: PickExisting,
        file_id: FileId,
        _job: Task<()>,
    },
}

impl PickWait {
    pub(crate) fn downloading(&self) -> Option<&PendingVideo> {
        match self {
            Self::Downloading(pending) => Some(pending),
            Self::Downloaded { .. } | Self::Decoding { .. } => None,
        }
    }
}

/// Stops the encode worker when its stage is dropped, whichever way the
/// stage ended (the box closed, another set, the account replaced).
pub(crate) struct CancelOnDrop(Arc<AtomicBool>);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// The video encode behind "Done": at most one runs. The stage holds the
/// worker's cancel flag and the task that lands its answer, so leaving the
/// stage stops the worker (between quality steps) and drops the answer.
#[derive(Default)]
pub(crate) enum Encoding {
    #[default]
    Idle,
    Running {
        _cancel: CancelOnDrop,
        _job: Task<()>,
    },
}

impl Encoding {
    /// A running encode: `cancel` is the flag the worker polls.
    pub(crate) fn running(cancel: Arc<AtomicBool>, job: Task<()>) -> Self {
        Self::Running {
            _cancel: CancelOnDrop(cancel),
            _job: job,
        }
    }

    pub(crate) fn is_running(&self) -> bool {
        matches!(self, Self::Running { .. })
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
            editing: SetEditing::Idle,
            confirm: SetConfirm::None,
            rename_input: cx
                .new(|cx| InputState::new(window, cx).placeholder(tr(keys::Q_MEDIA_SET_NAME))),
            emoji_search: cx
                .new(|cx| InputState::new(window, cx).placeholder(tr(keys::Q_MEDIA_SEARCH_EMOJI))),
        }
    }

    /// Leave every editing flow (the box closed or changed set).
    pub(crate) fn reset(&mut self) {
        self.confirm = SetConfirm::None;
        self.editing.enter(SetEditing::Idle);
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
    use super::{CreateSticker, Encoding, SetEditing, SetKind};
    use gpui_kit::Task;
    use quill::telegram::envelope::StickerFormat;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn create() -> CreateSticker {
        CreateSticker {
            set_id: 7,
            kind: SetKind::Stickers,
            path: "/nonexistent/sticker.webm".into(),
            format: StickerFormat::Webm,
            preview: None,
            emojis: vec!["😀".into()],
            video: None,
        }
    }

    /// Whatever ends the create box (a close, another sticker, the box
    /// closing) stops the worker: the stage holds its cancel flag.
    #[test]
    fn leaving_the_create_stage_cancels_the_running_encode() {
        let mut editing = SetEditing::default();
        editing.begin_create(create());
        let old = Arc::new(AtomicBool::new(false));
        assert!(editing.start_encoding(Encoding::running(old.clone(), Task::ready(()))));
        assert!(editing.encoding_running());
        assert!(
            !editing.start_encoding(Encoding::running(
                Arc::new(AtomicBool::new(false)),
                Task::ready(())
            )),
            "one encode at a time"
        );
        editing.discard_create();
        assert!(old.load(Ordering::Relaxed), "the close stopped the job");
        assert!(!editing.encoding_running());
        assert!(editing.create().is_none());
    }

    /// A new sticker replaces the old box's encode, which stops; the new
    /// box starts with no encode of its own.
    #[test]
    fn a_new_create_box_stops_the_old_boxs_encode_and_starts_idle() {
        let mut editing = SetEditing::default();
        editing.begin_create(create());
        let old = Arc::new(AtomicBool::new(false));
        editing.start_encoding(Encoding::running(old.clone(), Task::ready(())));
        editing.begin_create(create());
        assert!(old.load(Ordering::Relaxed));
        assert!(!editing.encoding_running());
        assert!(!editing.finish_encoding(), "nothing runs to finish");
    }

    #[test]
    fn finishing_the_encode_ends_only_a_running_one() {
        let mut editing = SetEditing::default();
        editing.begin_create(create());
        let job = Arc::new(AtomicBool::new(false));
        editing.start_encoding(Encoding::running(job.clone(), Task::ready(())));
        assert!(editing.finish_encoding());
        assert!(!editing.finish_encoding());
        assert!(editing.create().is_some(), "the box stays for another Done");
    }
}

#[cfg(test)]
mod confirm_tests {
    use super::{PickExisting, PickWait, SetConfirm, SetEditing, SetKind};
    use quill::ids::FileId;

    fn pick() -> PickExisting {
        PickExisting {
            set_id: 7,
            kind: SetKind::Stickers,
            adapt: false,
        }
    }

    /// The download's end moves the wait on: the file landing hands the
    /// pick to its window's frame, and nothing else lands a file twice.
    #[test]
    fn a_pick_download_ends_with_the_file() {
        use super::{PendingVideo, PickedFile};
        let pending = PendingVideo {
            pick: pick(),
            file_id: FileId(3),
            file: PickedFile::Video,
            emoji: None,
            window: None,
        };
        let mut editing = SetEditing::Waiting(PickWait::Downloading(pending.clone()));
        assert!(editing.downloading().is_some());
        assert!(editing.downloaded().is_none());
        assert!(editing.file_landed("/tmp/a.webm".into()));
        assert!(editing.downloading().is_none());
        assert_eq!(editing.downloaded(), Some(&pending));
        assert!(!editing.file_landed("/tmp/b.webm".into()), "once");
        assert_eq!(
            editing.take_downloaded(),
            Some((pending, "/tmp/a.webm".to_string()))
        );
        assert!(editing.is_idle());
        assert!(!SetEditing::Idle.file_landed("/tmp/c.webm".into()));
    }

    /// The image dialog and the image write are stages that hold their task:
    /// the answer ends them, and entering another flow drops the task.
    #[test]
    fn the_image_dialog_and_write_stages_end_with_their_answer_or_another_flow() {
        use gpui_kit::Task;
        let mut editing = SetEditing::Idle;
        editing.begin_choosing(Task::ready(()));
        assert!(!editing.is_idle());
        editing.finish_writing();
        assert!(!editing.is_idle(), "another stage's end leaves it");
        editing.finish_choosing();
        assert!(editing.is_idle());
        editing.begin_writing(Task::ready(()));
        assert!(!editing.is_idle());
        editing.enter(SetEditing::Reordering);
        assert!(editing.is_reordering(), "another flow replaces the write");
        editing.finish_writing();
        assert!(editing.is_reordering());
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
    fn entering_a_flow_ends_the_one_before_it() {
        let mut editing = SetEditing::default();
        editing.enter(SetEditing::Picking(pick()));
        assert_eq!(editing.pick(), Some(pick()));
        editing.close_pick();
        assert!(editing.pick().is_none());
        editing.begin_reordering();
        assert!(editing.is_reordering());
        editing.enter(SetEditing::Picking(pick()));
        assert!(!editing.is_reordering(), "the chooser replaced reordering");
        editing.finish_reordering();
        assert_eq!(
            editing.pick(),
            Some(pick()),
            "another flow's end is not its own"
        );
    }

    #[test]
    fn a_press_of_edit_stickers_keeps_a_waiting_pick() {
        let mut editing = SetEditing::Waiting(PickWait::Decoding {
            pick: pick(),
            file_id: FileId(1),
            _job: gpui_kit::Task::ready(()),
        });
        editing.begin_reordering();
        assert!(!editing.is_reordering());
        assert!(
            !editing.finish_decoding(pick(), FileId(2)),
            "another sticker"
        );
        assert!(editing.finish_decoding(pick(), FileId(1)));
        assert!(
            !editing.finish_decoding(pick(), FileId(1)),
            "already landed"
        );
    }
}
