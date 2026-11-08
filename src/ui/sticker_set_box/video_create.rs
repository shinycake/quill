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
        if self.sticker_box.encoding.is_some() {
            return false;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.sticker_box.encoding = Some(cancel.clone());
        self.connection.set_note(StatusNote::silent(ENCODING_NOTE));
        let kind = create.kind;
        let worker = cx.background_spawn(async move {
            encode_sticker(&job.source, &job.edits, &job.info, kind, &cancel)
        });
        self.spawn_screen_in(window, cx, async move |this, cx| {
            // Removed with the continuation when the screen is gone.
            let result = worker
                .await
                .map(|encoded| quill::scratch::OwnedFile::new(encoded.path));
            let _ = this.update_in(cx, |this, window, cx| {
                // The box moved on (closed, or another sticker): the
                // result is nobody's.
                let current = this.sticker_box.create.as_ref() == Some(&create)
                    && this.sticker_box.encoding.is_some();
                this.sticker_box.encoding = None;
                match result {
                    Ok(encoded) if current => {
                        let uploaded = CreateSticker {
                            path: encoded.keep(),
                            video: None,
                            ..create
                        };
                        this.upload_created_sticker(uploaded, cx);
                        this.close_kit_dialog_if_done(DialogKind::StickerCreate, window, cx);
                    }
                    // Nobody's: dropping it removes it.
                    Ok(_) => {}
                    Err(error) => {
                        if let Some(note) = error.note().filter(|_| current) {
                            this.connection.set_note(StatusNote::error(note));
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

    /// Each main-window frame: the chosen video's download finished, so
    /// the editor opens on it (the poll has no window to do this), in the
    /// window the pick was made in. A pick whose chat window closed is
    /// dropped with it.
    pub(in crate::ui) fn sticker_box_frame_upkeep(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pending) = self.sticker_box.pending_video.clone() else {
            return;
        };
        if let Some(chat) = pending.window
            && !self.windows.open_windows().any(|(open, _)| open == chat)
        {
            self.sticker_box.pending_video = None;
            return;
        }
        let Some(path) = self.local_media_path(pending.file_id) else {
            return;
        };
        self.sticker_box.pending_video = None;
        let path = PathBuf::from(path);
        let host = pending.window;
        let super::state::PendingVideo { pick, emoji, .. } = pending;
        let open = move |app: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
            app.open_set_video_editor(pick.set_id, pick.kind, path, emoji, window, cx);
        };
        match host {
            Some(chat) => {
                let _ = self.hand_to_chat_window(chat, cx, open);
            }
            None => open(self, window, cx),
        }
    }
}
