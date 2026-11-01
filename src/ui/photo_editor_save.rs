//! The photo editor's Done: the edit renders and encodes off the UI
//! thread, and lands where the editor was opened from.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::composer_sends::TrayItem;
use super::photo_editor::{EditorPurpose, PhotoEditor};
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::lang::{keys, tr};
use std::sync::atomic::{AtomicU64, Ordering};

/// Numbers each attachment render, so the latest Done of a photo wins.
static NEXT_RENDER: AtomicU64 = AtomicU64::new(1);

impl QuillApp {
    /// Render the edit at full resolution into a new PNG and send that
    /// instead of the original. The render and the encode run off the UI
    /// thread; the editor closes at once.
    pub(super) fn save_photo_editor(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(editor) = self.viewer.photo_editor.take() else {
            return;
        };
        cx.notify();
        match editor.purpose.clone() {
            EditorPurpose::Attachment(item) => {
                self.render_attachment_edit(item, editor, window, cx)
            }
            EditorPurpose::Profile(target) => self.finish_profile_crop(editor, target, cx),
            EditorPurpose::MessageEdit => {
                let edit_of = self.edited_message();
                let job = editor.into_render_job();
                let render = cx.background_spawn(async move {
                    super::edit_media_tools::write_edited_png(&job.render())
                });
                self.spawn_scoped(cx, async move |app, cx| {
                    let saved = render.await;
                    let kept = saved.clone();
                    let landed = app.update(cx, |this, cx| {
                        if edit_of.is_some() && this.edited_message() == edit_of {
                            this.finish_edit_image(saved, cx);
                        } else if let Some(saved) = saved {
                            let _ = std::fs::remove_file(saved);
                        }
                    });
                    if landed.is_err()
                        && let Some(kept) = kept
                    {
                        let _ = std::fs::remove_file(kept);
                    }
                })
                .detach();
            }
            EditorPurpose::Set { set_id, kind } => {
                // The canvas render and the fit run off the UI thread.
                let job = editor.set_render_job();
                self.finish_set_image(set_id, kind, job, editor.corners, cx);
            }
        }
    }

    /// The tray item renders until its PNG lands; a Send pressed meanwhile
    /// waits for it. Only the latest Done's render replaces the file.
    fn render_attachment_edit(
        &mut self,
        item: TrayItem,
        editor: PhotoEditor,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let render = NEXT_RENDER.fetch_add(1, Ordering::Relaxed);
        let marked = self.composer_sends.update_item(&item, |attachment| {
            attachment.extras.photo_render = Some(render);
        });
        if marked.is_none() {
            return;
        }
        let job = editor.into_render_job();
        let task =
            cx.background_spawn(
                async move { super::edit_media_tools::write_edited_png(&job.render()) },
            );
        self.spawn_scoped_in(window, cx, async move |app, cx| {
            let saved = task.await;
            let (kept, item_kept) = (saved.clone(), item.clone());
            let landed = app.update_in(cx, |this, window, cx| {
                let held = this.land_attachment_edit(&item, render, saved, cx);
                if let Some(text) = held
                    && this.send_slot().as_ref() == Some(&item.slot)
                {
                    this.submit_composer(text, window, cx);
                }
            });
            // The window closed: the tray outlives it, so the edit still
            // lands there or its file goes.
            if landed.is_err()
                && app
                    .main()
                    .update(cx, |this, cx| {
                        this.land_attachment_edit(&item_kept, render, kept.clone(), cx);
                    })
                    .is_err()
                && let Some(kept) = kept
            {
                let _ = std::fs::remove_file(kept);
            }
        })
        .detach();
    }

    /// The message whose media is being edited, if any.
    fn edited_message(&self) -> Option<(quill::ids::ChatId, quill::ids::MessageId)> {
        let edit = self.composer_ui.pending_edit.as_ref()?;
        Some((edit.chat_id, edit.message_id))
    }

    /// Render `render` of `item` finished. Its PNG replaces the photo in
    /// the chat it was opened from while the photo is still in that tray
    /// and no later Done started; otherwise the PNG goes. Hands back the
    /// held Send once nothing in that tray renders.
    fn land_attachment_edit(
        &mut self,
        item: &TrayItem,
        render: u64,
        saved: Option<std::path::PathBuf>,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        cx.notify();
        let landed = self.composer_sends.update(&item.slot, |send| {
            let attachment = send
                .attachments
                .get_mut(item.index)
                .filter(|attachment| attachment.path == item.path)
                .filter(|attachment| attachment.extras.photo_render == Some(render))?;
            attachment.extras.photo_render = None;
            let Some(saved) = &saved else {
                // The held Send would carry the unedited photo.
                send.held_send = None;
                return Some(None);
            };
            let stem = attachment
                .path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("photo")
                .to_string();
            attachment.path = saved.clone();
            attachment.file_name = format!("{stem}.png");
            attachment.kind = quill::composer::AttachmentKind::Photo;
            Some(
                (!send.renders_an_edit())
                    .then(|| send.held_send.take())
                    .flatten(),
            )
        });
        match (landed, saved) {
            (None, Some(saved)) => {
                let _ = std::fs::remove_file(saved);
                None
            }
            (Some(_), None) => {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_MISC_COULDNT_SAVE_THE_EDITED_PHOTO,
                )));
                None
            }
            (landed, _) => landed.flatten(),
        }
    }
}
