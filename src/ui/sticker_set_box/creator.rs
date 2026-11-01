//! Editing an owned set from the box: reorder, rename, delete a sticker or
//! the set, add a new image or an existing sticker (tdesktop
//! `api/api_stickers_creator.cpp`). Live sessions go through the connect
//! driver; the demo applies what it can to its own session.
use super::state::{CreateSticker, PickExisting};
use super::*;
use crate::ui::status_note::StatusNote;
use quill::ids::FileId;
use quill::lang::{keys, tr};
use quill::sticker_image::{Corners, prepared_sticker_image};
use quill::sticker_set_box::{
    EMOJI_REQUIRED_NOTE, MAX_EMOJIS, OPEN_FAILED_NOTE, emoji_or_default, joined_emojis,
    valid_set_name,
};
use quill::telegram::envelope::StickerFormat;
use std::path::PathBuf;

/// What the chooser says when a sticker cannot join the set
/// (`lng_stickers_set_is_full`, `lng_emoji_set_is_full`,
/// `lng_stickers_already_in_set`, `lng_emoji_already_in_set`).
fn full_note(kind: SetKind) -> &'static str {
    match kind {
        SetKind::Stickers => tr(keys::STICKERS_SET_IS_FULL),
        SetKind::Emoji => tr(keys::EMOJI_SET_IS_FULL),
    }
}

fn already_note(kind: SetKind) -> &'static str {
    match kind {
        SetKind::Stickers => tr(keys::STICKERS_ALREADY_IN_SET),
        SetKind::Emoji => tr(keys::EMOJI_ALREADY_IN_SET),
    }
}

impl QuillApp {
    /// "Edit Stickers" / "Reorder": cells drag to a new place.
    pub(super) fn begin_edit_stickers(&mut self, cx: &mut Context<Self>) {
        self.sticker_box.reordering = true;
        cx.notify();
    }

    /// "Done" in reorder mode.
    pub(super) fn finish_edit_stickers(&mut self, cx: &mut Context<Self>) {
        self.sticker_box.reordering = false;
        cx.notify();
    }

    /// A cell was dropped on the cell at `to`: the box reorders at once and
    /// TDLib hears the new position.
    pub(super) fn drop_sticker_in_set(
        &mut self,
        set_id: i64,
        file_id: FileId,
        to: usize,
        cx: &mut Context<Self>,
    ) {
        let Some((_, _, _, stickers)) = self.viewed_set() else {
            return;
        };
        let Some(from) = stickers.iter().position(|s| s.file_id == file_id) else {
            return;
        };
        if let Some(live) = self.live.as_mut() {
            if live.driver.move_sticker_in_set(set_id, from, to).is_err() {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_MEDIA_COULDNT_MOVE_THE_STICKER,
                )));
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            session.move_viewed_sticker(set_id, from, to);
        }
        cx.notify();
    }

    /// The ⋯ menu's "Edit name": the box with the current title.
    pub(super) fn open_rename_set(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let title = self
            .session()
            .and_then(|s| s.stickers.sticker_set_view.as_ref())
            .and_then(|view| match &view.stage {
                StickerSetViewStage::Ready { title, .. } => Some(title.clone()),
                _ => None,
            })
            .unwrap_or_default();
        self.sticker_box
            .rename_input
            .update(cx, |input, cx| input.set_value(&title, window, cx));
        self.sticker_box.rename_open = true;
        cx.notify();
    }

    /// "Done" in the rename box; an empty or too long name is refused.
    pub(super) fn confirm_rename_set(&mut self, set_id: i64, cx: &mut Context<Self>) -> bool {
        let raw = self.sticker_box.rename_input.read(cx).value().to_string();
        let Some(title) = valid_set_name(&raw) else {
            return false;
        };
        if let Some(live) = self.live.as_mut() {
            if live.driver.rename_sticker_set(set_id, &title).is_err() {
                self.connection
                    .set_note(StatusNote::error(tr(keys::Q_MEDIA_COULDNT_RENAME_THE_SET)));
            }
        } else if let Some(session) = self.demo_session.as_mut()
            && let Some(view) = session.stickers.sticker_set_view.as_mut()
            && let StickerSetViewStage::Ready { title: current, .. } = &mut view.stage
        {
            *current = title;
        }
        self.sticker_box.rename_open = false;
        cx.notify();
        true
    }

    /// A cell's "Delete sticker": the confirmation box.
    pub(super) fn ask_delete_sticker(&mut self, item: StickerItem, cx: &mut Context<Self>) {
        self.sticker_box.delete_sticker = Some(item);
        cx.notify();
    }

    /// "Delete" in that box.
    pub(super) fn confirm_delete_sticker(&mut self, set_id: i64, cx: &mut Context<Self>) {
        let Some(item) = self.sticker_box.delete_sticker.take() else {
            return;
        };
        if let Some(live) = self.live.as_mut() {
            if live
                .driver
                .remove_sticker_from_set(set_id, item.file_id)
                .is_err()
            {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_MEDIA_COULDNT_DELETE_THE_STICKER,
                )));
            }
        } else if let Some(session) = self.demo_session.as_mut()
            && let Some(view) = session.stickers.sticker_set_view.as_mut()
            && let StickerSetViewStage::Ready { stickers, .. } = &mut view.stage
        {
            stickers.retain(|s| s.file_id != item.file_id);
        }
        cx.notify();
    }

    /// "Remove" in "Delete for Everyone": the set is gone for good.
    pub(super) fn confirm_delete_set_everyone(&mut self, set_id: i64, cx: &mut Context<Self>) {
        self.sticker_box.delete_set_open = false;
        if let Some(live) = self.live.as_mut() {
            if live.driver.delete_owned_sticker_set(set_id).is_err() {
                self.connection
                    .set_note(StatusNote::error(tr(keys::Q_MEDIA_COULDNT_DELETE_THE_SET)));
            }
        } else {
            self.close_sticker_set_dialog(cx);
        }
        cx.notify();
    }

    /// "Remove" in "Delete for Myself": the set is uninstalled.
    pub(super) fn confirm_remove_set(&mut self, set_id: i64, cx: &mut Context<Self>) {
        self.sticker_box.remove_set_open = false;
        self.toggle_viewed_sticker_set(set_id, false, cx);
    }

    /// "Create a New Sticker" / "Create a New Emoji": pick an image, then
    /// the photo editor on the sticker canvas.
    pub(super) fn begin_create_sticker(
        &mut self,
        set_id: i64,
        kind: SetKind,
        cx: &mut Context<Self>,
    ) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(tr(keys::Q_MEDIA_CHOOSE_IMAGE).into()),
        });
        self.spawn_scoped(cx, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picker.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update(cx, |this, cx| {
                    this.edit_image_for_set(set_id, kind, &path, cx);
                });
            }
        })
        .detach();
    }

    /// Open `path` in the photo editor for the set: the image on a
    /// transparent square canvas (tdesktop `PrepareEditorState`).
    pub(in crate::ui) fn edit_image_for_set(
        &mut self,
        set_id: i64,
        kind: SetKind,
        path: &std::path::Path,
        cx: &mut Context<Self>,
    ) {
        self.decode_image_then(
            path.to_path_buf(),
            move |this, result, cx| this.edit_pixels_for_set(set_id, kind, result.ok(), cx),
            cx,
        );
    }

    /// The editor on `pixels` for the set, or the "Could not load image."
    /// toast when there are none or the editor refuses them.
    pub(super) fn edit_pixels_for_set(
        &mut self,
        set_id: i64,
        kind: SetKind,
        pixels: Option<image::RgbaImage>,
        cx: &mut Context<Self>,
    ) {
        let editor = pixels.and_then(|pixels| {
            super::super::photo_editor::PhotoEditor::for_set(pixels, set_id, kind)
        });
        match editor {
            Some(editor) => self.viewer.photo_editor = Some(editor),
            None => self
                .connection
                .set_note(StatusNote::error(OPEN_FAILED_NOTE)),
        }
        cx.notify();
    }

    /// The editor's "Done" for a set image: fit it to the set's side, cut
    /// the corners and write it off the UI thread, then ask for its emoji.
    pub(in crate::ui) fn finish_set_image(
        &mut self,
        set_id: i64,
        kind: SetKind,
        job: super::super::photo_editor_set::SetRenderJob,
        corners: Corners,
        cx: &mut Context<Self>,
    ) {
        let worker = cx.background_spawn(async move {
            let prepared = prepared_sticker_image(&job.render(), kind.image_side(), corners);
            super::super::edit_media_tools::write_edited_png(&prepared)
        });
        cx.spawn(async move |this, cx| {
            let written = worker.await;
            let _ = this.update(cx, |this, cx| {
                match written {
                    Some(path) => {
                        this.sticker_box.discard_create();
                        this.sticker_box.create = Some(CreateSticker {
                            set_id,
                            kind,
                            preview: Some(path.clone()),
                            path,
                            format: StickerFormat::Webp,
                            emojis: Vec::new(),
                            video: None,
                        });
                    }
                    None => this
                        .connection
                        .set_note(StatusNote::error(tr(keys::Q_MEDIA_COULDNT_SAVE_THE_IMAGE))),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// An emoji in the "New Sticker" box toggles on or off (at most seven).
    pub(super) fn toggle_create_emoji(&mut self, emoji: &str, cx: &mut Context<Self>) {
        if let Some(create) = self.sticker_box.create.as_mut() {
            if let Some(index) = create.emojis.iter().position(|e| e == emoji) {
                create.emojis.remove(index);
            } else if create.emojis.len() < MAX_EMOJIS {
                create.emojis.push(emoji.to_string());
            }
        }
        cx.notify();
    }

    /// "Done" in the "New Sticker" box: upload and add; a video is encoded
    /// first (`video_create.rs`).
    pub(super) fn confirm_create_sticker(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(create) = self.sticker_box.create.clone() else {
            return false;
        };
        if create.emojis.is_empty() {
            self.connection
                .set_note(StatusNote::silent(EMOJI_REQUIRED_NOTE));
            cx.notify();
            return false;
        }
        if let Some(job) = create.video.clone() {
            return self.encode_created_video(create, job, window, cx);
        }
        self.upload_created_sticker(create, cx);
        true
    }

    /// Hand the finished file to TDLib and leave the box.
    pub(super) fn upload_created_sticker(&mut self, create: CreateSticker, cx: &mut Context<Self>) {
        let emojis = joined_emojis(&create.emojis);
        if let Some(live) = self.live.as_mut() {
            let path = create.path.to_string_lossy().into_owned();
            if live
                .driver
                .add_new_sticker_to_set(create.set_id, &path, &emojis, create.format)
                .is_err()
            {
                self.connection.set_note(StatusNote::error(
                    quill::sticker_set_box::UPLOAD_FAILED_NOTE,
                ));
            }
        } else {
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_MEDIA_DEMO_STICKERS_UPLOAD_WITH_LIVE,
            )));
        }
        self.sticker_box.create = None;
        self.sticker_box.encoding = None;
        create.uploaded();
        cx.notify();
    }

    /// "Add an Existing Sticker" / "Adapt from a Sticker": the chooser.
    pub(super) fn open_pick_existing(
        &mut self,
        set_id: i64,
        kind: SetKind,
        adapt: bool,
        cx: &mut Context<Self>,
    ) {
        self.sticker_box.pick = Some(PickExisting {
            set_id,
            kind,
            adapt,
        });
        cx.notify();
    }

    /// The stickers the chooser offers: favorites, recent and the loaded
    /// sets, without the set being edited.
    pub(super) fn pick_candidates(&self, pick: PickExisting) -> Vec<StickerItem> {
        let Some(session) = self.session() else {
            return Vec::new();
        };
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        let library = &session.media.media_library.set_stickers;
        let sources: Vec<&StickerItem> = session
            .stickers
            .stickers
            .favorites
            .iter()
            .chain(&session.stickers.stickers.recent)
            .chain(library.values().flat_map(|items| items.iter()))
            .collect();
        for item in sources {
            if item.set_id == pick.set_id || !seen.insert(item.file_id) {
                continue;
            }
            // Emoji sets take custom emoji; sticker sets take stickers;
            // adapting takes stickers into an emoji set.
            let is_emoji = item.custom_emoji_id.is_some();
            let wanted = match (pick.kind, pick.adapt) {
                (SetKind::Emoji, false) => is_emoji,
                _ => !is_emoji,
            };
            if wanted {
                out.push(item.clone());
            }
        }
        out
    }

    /// A sticker was chosen: add it (or adapt it) to the set
    /// (`AdaptStickerToEmoji`).
    pub(super) fn pick_existing_sticker(
        &mut self,
        item: &StickerItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pick) = self.sticker_box.pick else {
            return;
        };
        if !pick.adapt {
            self.sticker_box.pick = None;
            self.add_existing_to_owned_set(pick.set_id, pick.kind, item, cx);
            return;
        }
        match item.format {
            // Animated stickers become emoji as they are.
            StickerFormat::Tgs => {
                self.sticker_box.pick = None;
                self.add_existing_to_owned_set(pick.set_id, pick.kind, item, cx);
            }
            // A video sticker goes through the video editor; the chooser
            // stays while the file is still downloading.
            StickerFormat::Webm => {
                let emoji = emoji_or_default(&item.emoji).to_string();
                self.pick_video_for_set(pick, item.file_id, Some(emoji), window, cx);
            }
            StickerFormat::Webp | StickerFormat::Unknown => {
                self.sticker_box.pick = None;
                self.sticker_box.pending_video = None;
                let file_id = item.file_id;
                self.sticker_box.pending_still = Some((pick, file_id));
                let path = self.local_media_path(file_id);
                let format = item.format;
                self.run_off_ui_scoped_then(
                    move || {
                        path.and_then(|path| {
                            super::super::editor_art::sticker_pixels(
                                std::path::Path::new(&path),
                                format,
                            )
                        })
                    },
                    move |this, pixels, cx| this.land_still_pick(pick, file_id, pixels, cx),
                    cx,
                );
            }
        }
    }

    /// A picked still sticker is decoded: open the editor on it, unless the
    /// set box closed or another sticker was picked meanwhile.
    pub(super) fn land_still_pick(
        &mut self,
        pick: PickExisting,
        file_id: FileId,
        pixels: Option<image::RgbaImage>,
        cx: &mut Context<Self>,
    ) {
        if self.sticker_box.pending_still != Some((pick, file_id)) {
            return;
        }
        self.sticker_box.pending_still = None;
        self.edit_pixels_for_set(pick.set_id, pick.kind, pixels, cx);
    }

    /// Add a sticker that already exists to an owned set, refusing a full
    /// set or a repeat (tdesktop `ChooseStickerSetToAdd` checks).
    pub(in crate::ui) fn add_existing_to_owned_set(
        &mut self,
        set_id: i64,
        kind: SetKind,
        item: &StickerItem,
        cx: &mut Context<Self>,
    ) {
        // tdesktop checks the target's count for every owned set, and a
        // repeat wherever the set's stickers are known (the open box or the
        // panel's cache).
        let count = self
            .session()
            .and_then(|s| s.owned_sticker_set_count(set_id))
            .unwrap_or(0);
        if count >= kind.owned_limit() {
            self.connection
                .set_note(StatusNote::silent(full_note(kind)));
            cx.notify();
            return;
        }
        let known: Option<Vec<StickerItem>> = match self.viewed_set() {
            Some((viewed_id, _, _, stickers)) if viewed_id == set_id => Some(stickers),
            _ => self
                .session()
                .and_then(|s| s.media.media_library.set_stickers.get(&set_id).cloned()),
        };
        if known.is_some_and(|stickers| {
            stickers
                .iter()
                .any(|s| s.file_id == item.file_id || (item.id != 0 && s.id == item.id))
        }) {
            self.connection
                .set_note(StatusNote::silent(already_note(kind)));
            cx.notify();
            return;
        }
        let emoji = emoji_or_default(&item.emoji).to_string();
        if let Some(live) = self.live.as_mut() {
            if live
                .driver
                .add_existing_sticker_to_set(set_id, item.file_id, item.format, &emoji)
                .is_err()
            {
                self.connection.set_note(StatusNote::error(
                    quill::sticker_set_box::ADD_EXISTING_FAILED_NOTE,
                ));
            }
        } else {
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_MEDIA_DEMO_STICKERS_ARE_ADDED_WITH,
            )));
        }
        self.message_ui.menu = None;
        cx.notify();
    }

    /// Each poll: an edit's toast, a confirmed deletion (the box closes),
    /// and the refresh after an edit.
    pub(in crate::ui) fn sticker_set_box_tick(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(live) = self.live.as_mut() else {
            return false;
        };
        let mut changed = false;
        if let Some(note) = live.driver.session.take_sticker_set_edit_note() {
            self.connection.set_note(StatusNote::info(note));
            changed = true;
        }
        let deleted = live
            .driver
            .session
            .stickers
            .sticker_set_view
            .as_ref()
            .is_some_and(|view| view.edits.deleted);
        if deleted {
            // tdesktop just closes the box once the set is gone.
            self.close_sticker_set_dialog(cx);
            return true;
        }
        if let Ok(Some(_)) = live.driver.sync_sticker_set_view() {
            changed = true;
        }
        changed
    }

    /// The still picture a "New Sticker" box shows.
    pub(super) fn create_preview_path(&self) -> Option<PathBuf> {
        self.sticker_box.create.as_ref()?.preview.clone()
    }
}
