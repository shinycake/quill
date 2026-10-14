//! The video editor opened for an owned sticker or emoji set (tdesktop
//! `RunVideoEditorAndCreate` with `VideoStickerEditorData`): the range is
//! at most three seconds, the crop stays a square, there is no quality or
//! GIF choice, and "Done" encodes a VP9 WEBM with alpha at the set's side
//! (`Media::Encode::RunWebmSticker`), then opens the "New Sticker" box
//! with a frame of it. The numbers and the encoder are
//! `quill::sticker_video`; the dialog itself is `video_editor.rs`.

use super::app::QuillApp;
use super::sticker_set_box::state::CreateSticker;
use super::video_editor::EditorTarget;
use gpui_kit::*;
use quill::sticker_set_box::SetKind;
use quill::sticker_video::{ENCODING_NOTE, encode_sticker};
use quill::telegram::envelope::StickerFormat;
use std::path::{Path, PathBuf};

/// Side of the frame the "New Sticker" box shows.
const PREVIEW_SIDE: usize = 256;

/// A frame of the encoded sticker, with its alpha (the WEBP/PNG the box can
/// show); `None` when it couldn't be read.
fn sticker_preview(encoded: &Path) -> Option<PathBuf> {
    let dir = std::env::temp_dir().join(format!(
        "quill-sticker-video/preview-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos())
    ));
    let child = std::sync::Arc::new(std::sync::Mutex::new(None));
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    quill::sticker_playback::decode_webm_sized(encoded, &dir, PREVIEW_SIDE, 1, &child, &cancelled)
        .ok()?
        .frames
        .into_iter()
        .next()
}

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

    /// "Done" for a set: encode the sticker off the UI thread, then hand it
    /// to the "New Sticker" box. Returns whether the dialog may close now
    /// (never: the encoder closes it when it is done).
    pub(super) fn finish_set_video_editor(
        &mut self,
        set_id: i64,
        kind: SetKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() else {
            return true;
        };
        let (Some(info), false) = (editor.info, editor.busy) else {
            return false;
        };
        let edits = editor.edits;
        let source = editor.source.clone();
        let emoji = editor.set_emoji.clone();
        editor.playing = false;
        *editor.player.borrow_mut() = None;
        editor.busy = true;
        editor.note = Some(ENCODING_NOTE.into());
        editor.serial += 1;
        let serial = editor.serial;
        let worker = cx.background_spawn(async move {
            let encoded = encode_sticker(&source, &edits, &info, kind)?;
            let preview = sticker_preview(&encoded.path);
            Ok((encoded, preview))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = worker.await;
            let _ = this.update_in(cx, |this, window, cx| {
                let current = this
                    .composer_ui
                    .send_box
                    .video_editor
                    .as_ref()
                    .is_some_and(|editor| editor.serial == serial);
                match result {
                    Ok((encoded, preview)) if current => {
                        this.sticker_box.create = Some(CreateSticker {
                            set_id,
                            kind,
                            path: encoded.path,
                            format: StickerFormat::Webm,
                            preview,
                            emojis: emoji.into_iter().collect(),
                        });
                        this.close_video_editor(cx);
                        window.close_dialog(cx);
                    }
                    Ok((encoded, preview)) => {
                        let _ = std::fs::remove_file(encoded.path);
                        if let Some(preview) = preview {
                            let _ = std::fs::remove_file(preview);
                        }
                    }
                    Err(error) if current => {
                        if let Some(editor) = this.composer_ui.send_box.video_editor.as_mut() {
                            editor.busy = false;
                            editor.note = Some(error.note());
                        }
                        cx.notify();
                    }
                    Err(_) => {}
                }
            });
        })
        .detach();
        cx.notify();
        false
    }
}
