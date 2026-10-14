//! The video editor opened for an owned sticker or emoji set (tdesktop
//! `RunVideoEditorAndCreate` with `VideoStickerEditorData`): the range is
//! at most three seconds, the crop stays a square, there is no quality or
//! GIF choice, and "Done" hands the edits and the cover frame to the "New
//! Sticker" box, which encodes the VP9 WEBM on its own "Done"
//! (`ShowVideoEditorThenCreate` → `CreateMediaBox`). The numbers and the
//! encoder are `quill::sticker_video`; the dialog itself is
//! `video_editor.rs`, the box `sticker_set_box/video_create.rs`.

use super::app::QuillApp;
use super::sticker_set_box::state::CreateSticker;
use super::video_editor::EditorTarget;
use gpui_kit::*;
use quill::sticker_set_box::SetKind;
use quill::sticker_video::VideoStickerJob;
use quill::telegram::envelope::StickerFormat;
use std::path::PathBuf;

impl QuillApp {
    /// Open the editor on `source` for the set; `emoji` is the sticker's
    /// own when a sticker is adapted.
    pub(in crate::ui) fn open_set_video_editor(
        &mut self,
        set_id: i64,
        kind: SetKind,
        source: PathBuf,
        emoji: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_video_editor(EditorTarget::Set { set_id, kind }, source, None, window, cx);
        if let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() {
            editor.set_emoji = emoji;
        }
    }

    /// "Done" for a set: the "New Sticker" box opens with the editor's
    /// cover frame and the sticker's emoji; the encode waits for the
    /// box's "Done". Returns whether the dialog closes now (it does).
    pub(super) fn finish_set_video_editor(
        &mut self,
        set_id: i64,
        kind: SetKind,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() else {
            return true;
        };
        let (Some(info), false) = (editor.info, editor.busy) else {
            return false;
        };
        let job = VideoStickerJob {
            source: editor.source.clone(),
            edits: editor.edits,
            info,
        };
        // The frame stays as the box's still (closing the editor would
        // remove it).
        let preview = editor.preview.take();
        let emoji = editor.set_emoji.clone();
        self.sticker_box.discard_create();
        self.sticker_box.create = Some(CreateSticker {
            set_id,
            kind,
            path: job.source.clone(),
            format: StickerFormat::Webm,
            preview,
            emojis: emoji.into_iter().collect(),
            video: Some(job),
        });
        self.close_video_editor(cx);
        true
    }
}
