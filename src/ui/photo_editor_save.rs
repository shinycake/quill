//! The photo editor's Done: the edit renders and encodes off the UI
//! thread, and lands where the editor was opened from.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::composer_sends::{SendSlot, TrayItem};
use super::photo_editor::{EditorPurpose, PhotoEditor};
use super::send_box_media_check::check_photo;
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::composer::RenderJob as OwnedRender;
use quill::composer::{EditRender, RenderToken};
use quill::lang::{keys, tr};
use std::path::PathBuf;

/// Render and write the edit, unless its job was dropped already; a PNG
/// written by a render that was cancelled meanwhile goes at once.
fn render_unless_cancelled(
    token: &RenderToken,
    job: super::photo_edit::RenderJob,
) -> Option<PathBuf> {
    if token.is_cancelled() {
        return None;
    }
    let saved = super::edit_media_tools::write_edited_png(&job.render())?;
    if token.is_cancelled() {
        let _ = std::fs::remove_file(saved);
        return None;
    }
    Some(saved)
}

impl QuillApp {
    /// The editor's Masks strip is wanted while some window's editor is in
    /// Stickers mode: whatever closes that editor (Done, Cancel, a window
    /// or account leaving) releases it, not only the mode button.
    pub(super) fn sync_editor_masks(&mut self) {
        let wanted = self
            .viewer
            .photo_editor
            .as_ref()
            .is_some_and(|editor| editor.mode == super::photo_editor::EditorMode::Stickers)
            || self.windows.slot_edits_stickers();
        let Some(live) = self.live.as_mut() else {
            return;
        };
        if !wanted && live.driver.session.stickers.masks.strip_wanted() {
            live.driver.release_editor_masks();
        }
    }

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
            EditorPurpose::MessageEdit { chat, message } => {
                let edit_of = (chat, message);
                if self.edited_message() != Some(edit_of) {
                    return;
                }
                let token = RenderToken::new();
                let landing = token.clone();
                let job = editor.into_render_job();
                let cancel = token.clone();
                let render =
                    cx.background_spawn(async move { render_unless_cancelled(&cancel, job) });
                let task = self.spawn_scoped(cx, async move |app, cx| {
                    let saved = render.await;
                    let kept = saved.clone();
                    let landed = app.update(cx, |this, cx| {
                        if this.edited_message() == Some(edit_of)
                            && this
                                .chat_ui
                                .pending_edit()
                                .is_some_and(|edit| edit.media_edit.render.waits_for(&landing))
                        {
                            if let Some(edit) = this.chat_ui.pending_edit_mut() {
                                edit.media_edit.render = EditRender::Idle;
                            }
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
                });
                // The edit owns the render: ending it (or a later Done)
                // drops the task.
                if let Some(edit) = self.chat_ui.pending_edit_mut() {
                    edit.media_edit.render = EditRender::Rendering {
                        job: OwnedRender::new(token, task),
                    };
                }
            }
            EditorPurpose::Set { set_id, kind, edit } => {
                // The turn, the canvas render and the fit run off the UI
                // thread.
                let corners = edit.corners;
                let job = editor.into_set_render_job();
                self.finish_set_image(set_id, kind, job, corners, cx);
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
        if !self.composer_sends.holds_item(&item) {
            return;
        }
        let token = RenderToken::new();
        let landing = token.clone();
        let tray_item = item.clone();
        let job = editor.into_render_job();
        let cancel = token.clone();
        let task = cx.background_spawn(async move { render_unless_cancelled(&cancel, job) });
        let render_task = self.spawn_scoped_in(window, cx, async move |app, cx| {
            let saved = task.await;
            let kept = saved.clone();
            let landed = app.update_in(cx, |this, window, cx| {
                let landed = this.land_attachment_edit(&item.slot, &landing, saved, cx);
                let here = this.send_slot().as_ref() == Some(&item.slot);
                let view = this
                    .session()
                    .map(|s| s.view_generation)
                    .unwrap_or_default();
                let opens = this
                    .viewer
                    .photo_editor
                    .take_deferred_for(&landing)
                    .is_some_and(|open| open.view == view);
                match landed.held {
                    // The tray goes now, so no editor opens on it.
                    Some((text, once)) if here => {
                        this.viewer.photo_editor.clear_deferred();
                        this.submit_composer(text, once, window, cx);
                    }
                    Some(_) => this.note_held_send_dropped(),
                    None => {
                        if opens
                            && here
                            && let Some(path) = &landed.path
                        {
                            this.open_photo_editor_on(path, cx);
                        }
                    }
                }
            });
            // The window closed or another account is on screen: the tray
            // outlives both (it sits under its account), so the edit still
            // lands there, whatever screen is current, or its file goes.
            // Without this the card would render forever.
            if landed.is_err()
                && app
                    .app()
                    .update(cx, |this, cx| {
                        this.viewer.photo_editor.take_deferred_for(&landing);
                        let landed =
                            this.land_attachment_edit(&item.slot, &landing, kept.clone(), cx);
                        if landed.held.is_some() {
                            this.note_held_send_dropped();
                        }
                    })
                    .is_err()
                && let Some(kept) = kept
            {
                let _ = std::fs::remove_file(kept);
            }
        });
        // The tray item owns the render: removing the photo, or a later
        // Done, drops the task.
        let job = OwnedRender::new(token, render_task);
        self.composer_sends.update_item(&tray_item, |attachment| {
            attachment.extras.begin_render(job);
        });
    }

    /// A held Send can go only from its own chat: when that chat is no
    /// longer open as the render lands, the user is told nothing went.
    fn note_held_send_dropped(&mut self) {
        self.set_note(StatusNote::error(tr(
            keys::Q_MISC_NOT_SENT_THE_CHAT_CLOSED_BEFORE_THE_PHOTO,
        )));
    }

    /// The message whose media is being edited, if any.
    pub(super) fn edited_message(&self) -> Option<(quill::ids::ChatId, quill::ids::MessageId)> {
        let edit = self.chat_ui.pending_edit()?;
        Some((edit.chat_id, edit.message_id))
    }

    /// Render `render` in the tray of `slot` finished. Its PNG replaces
    /// the photo that render was started for while that photo is still in
    /// the tray and no later Done started; otherwise the PNG goes. The
    /// render token finds the photo, so cards removed or moved meanwhile
    /// do not matter.
    fn land_attachment_edit(
        &mut self,
        slot: &SendSlot,
        render: &RenderToken,
        saved: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) -> super::composer_sends::Landed {
        cx.notify();
        let found = self
            .composer_sends
            .update(slot, |send| send.end_render(render, saved.as_deref()));
        match (found, saved) {
            (None, Some(saved)) => {
                let _ = std::fs::remove_file(saved);
                super::composer_sends::Landed::default()
            }
            (found, None) => {
                if found.is_some() {
                    self.set_note(StatusNote::error(tr(
                        keys::Q_MISC_COULDNT_SAVE_THE_EDITED_PHOTO,
                    )));
                }
                found.unwrap_or_default()
            }
            (found, Some(saved)) => {
                // A PNG: only its header is read, and a held Send goes now.
                let check = check_photo(&saved);
                self.composer_sends.reads.media.set_done(saved, check);
                found.unwrap_or_default()
            }
        }
    }
}
