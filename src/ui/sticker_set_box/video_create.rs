//! The "New Sticker" / "New Emoji" box's "Done" for a video (tdesktop
//! `CreateMediaBox` → `RunWebmSticker`): the WEBM is encoded off the UI
//! thread with a cancel flag the box's close sets, then uploaded like any
//! other new sticker. Also the chosen video that was still downloading:
//! the editor opens on it once the file is here (`LoadDocumentMedia`).
use super::state::CreateSticker;
use super::*;
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
        if self.sticker_box.encoding.is_some() {
            return false;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.sticker_box.encoding = Some(cancel.clone());
        self.connection.status_note = ENCODING_NOTE.into();
        let kind = create.kind;
        let worker = cx.background_spawn(async move {
            encode_sticker(&job.source, &job.edits, &job.info, kind, &cancel)
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = worker.await;
            let _ = this.update_in(cx, |this, window, cx| {
                // The box moved on (closed, or another sticker): the
                // result is nobody's.
                let current = this.sticker_box.create.as_ref() == Some(&create)
                    && this.sticker_box.encoding.is_some();
                this.sticker_box.encoding = None;
                match result {
                    Ok(encoded) if current => {
                        let uploaded = CreateSticker {
                            path: encoded.path,
                            video: None,
                            ..create
                        };
                        this.upload_created_sticker(uploaded, cx);
                        this.close_kit_dialog_if_done(DialogKind::StickerCreate, window, cx);
                    }
                    Ok(encoded) => {
                        let _ = std::fs::remove_file(encoded.path);
                    }
                    Err(error) => {
                        if let Some(note) = error.note().filter(|_| current) {
                            this.connection.status_note = note;
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
        false
    }

    /// Each frame: the chosen video's download finished, so the editor
    /// opens on it (the poll has no window to do this).
    pub(in crate::ui) fn sticker_box_frame_upkeep(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pending) = self.sticker_box.pending_video.clone() else {
            return;
        };
        let Some(path) = self.local_media_path(pending.file_id) else {
            return;
        };
        self.sticker_box.pending_video = None;
        self.open_set_video_editor(
            pending.pick.set_id,
            pending.pick.kind,
            PathBuf::from(path),
            pending.emoji,
            window,
            cx,
        );
    }
}
