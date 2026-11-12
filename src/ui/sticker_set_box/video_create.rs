//! The "New Sticker" / "New Emoji" box's "Done" for a video (tdesktop
//! `CreateMediaBox` → `RunWebmSticker`): the WEBM is encoded off the UI
//! thread with a cancel flag the box's close sets, then uploaded like any
//! other new sticker. Also the chosen video that was still downloading:
//! the editor opens on it once the file is here (`LoadDocumentMedia`).
use super::state::CreateSticker;
use super::*;
use crate::ui::status_note::StatusNote;
use quill::sticker_video::{ENCODING_NOTE, VideoStickerJob, encode_sticker};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

impl QuillApp {
    /// Encode `job` for `create`, then upload. "Done" is disabled while it
    /// runs; closing the box stops it between quality steps. Returns
    /// whether the box closes now (never: the upload closes it).
    pub(super) fn encode_created_video(
        &mut self,
        create: CreateSticker,
        job: VideoStickerJob,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.sticker_box.editing.encoding_running()
            || self.sticker_box.editing.create().is_none()
        {
            return false;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.set_note(StatusNote::silent(ENCODING_NOTE));
        let kind = create.kind;
        let flag = cancel.clone();
        let worker = cx.background_spawn(async move {
            // Owned from the worker on: a step already running when the
            // box closes still finishes, and dropping the result removes it.
            encode_sticker(&job.source, &job.edits, &job.info, kind, &cancel)
                .map(|encoded| quill::scratch::OwnedFile::new(encoded.path))
        });
        let job = self.spawn_screen_in(window, cx, async move |this, cx| {
            let result = worker.await;
            let _ = this.update_in(cx, |this, window, cx| {
                // The box moved on (closed, or another sticker) cancels
                // this task with its stage, so what lands here is the
                // running encode's.
                let current = this.sticker_box.editing.finish_encoding();
                match result {
                    Ok(encoded) if current => {
                        let uploaded = CreateSticker {
                            path: encoded.keep(),
                            video: None,
                            ..create
                        };
                        if this.upload_created_sticker(uploaded.clone(), cx) {
                            this.close_kit_dialog_if_done(DialogKind::StickerCreate, window, cx);
                        } else {
                            // The box stays for another Done, which
                            // encodes again: this file is of no use.
                            uploaded.drop_encoded_file();
                        }
                    }
                    // Nobody's: dropping it removes it.
                    Ok(_) => {}
                    Err(error) => {
                        if let Some(note) = error.note().filter(|_| current) {
                            this.set_note(StatusNote::error(note));
                        }
                    }
                }
                cx.notify();
            });
        });
        self.sticker_box
            .editing
            .start_encoding(super::state::Encoding::running(flag, job));
        cx.notify();
        false
    }

    /// The session update path (every poll pass, whatever is drawn): the
    /// chosen file's download ended. With the file, the pick moves on to
    /// its window's frame ([`Self::sticker_box_window_upkeep`]); without
    /// it, the wait ends with a note instead of holding the pick for good.
    /// Returns whether the wait moved.
    pub(in crate::ui) fn settle_sticker_pick_download(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(pending) = self.sticker_box.editing.downloading() else {
            return false;
        };
        let (file_id, host) = (pending.file_id, pending.window);
        // The box closed, or the window it was chosen in: nobody wants it.
        if !self.sticker_set_open() || host.is_some_and(|chat| !self.windows.has(chat)) {
            self.sticker_box.editing.end_wait();
            return true;
        }
        if let Some(path) = self.local_media_path(file_id) {
            self.sticker_box.editing.file_landed(path);
            cx.notify();
            return true;
        }
        let failed = self.session().is_some_and(|session| {
            session
                .media
                .downloads
                .ended_without_file(file_id.0, &session.media.files)
        });
        if !failed {
            return false;
        }
        self.sticker_box.editing.end_wait();
        self.set_note(StatusNote::error(tr(
            keys::Q_OPS_COULD_NOT_DOWNLOAD_PLEASE_TRY,
        )));
        cx.notify();
        true
    }

    /// Each window's frame (`per_window_upkeep`): the chosen video's
    /// download finished (`settle_sticker_pick_download`), so the editor
    /// opens on it in the window the pick was made in, in that window's own
    /// frame (the poll has no window to do this, and a minimized main window
    /// draws none). A pick whose chat window closed, or whose box closed
    /// there, is dropped with it.
    pub(in crate::ui) fn sticker_box_window_upkeep(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pending) = self.sticker_box.editing.downloaded() else {
            return;
        };
        let host = pending.window;
        if host.is_some_and(|chat| !self.windows.has(chat)) {
            self.sticker_box.editing.end_wait();
            return;
        }
        if host != self.windows.scope {
            return;
        }
        if !self.sticker_set_open() {
            self.sticker_box.editing.end_wait();
            return;
        }
        let Some((pending, path)) = self.sticker_box.editing.take_downloaded() else {
            return;
        };
        let super::state::PendingVideo {
            pick, emoji, file, ..
        } = pending;
        if let super::state::PickedFile::Still(format) = file {
            self.decode_still_pick(pick, pending.file_id, format, cx);
            return;
        }
        self.open_set_video_editor(
            pick.set_id,
            pick.kind,
            PathBuf::from(path),
            emoji,
            window,
            cx,
        );
    }
}
