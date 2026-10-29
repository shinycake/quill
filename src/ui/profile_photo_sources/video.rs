//! The video avatar (tdesktop `PrepareProfileVideo`): "Video…" picks a
//! clip and opens the full video editor on it with a square crop and 1 to
//! 9.6 s; its "Done" encodes the 800 x 800 clip and sets it (or, for a
//! group, keeps it for the Edit box's Save) with the cover as the main
//! frame (`inputChatPhotoAnimation`). The editor is
//! `ui::video_editor`, the limits and the ffmpeg line
//! `quill::profile_photo_video`.

use super::{ChosenPhoto, PhotoPreview, QuillApp, run_ffmpeg, work_dir};
use crate::ui::shell::DialogKind;
use crate::ui::video_editor::EditorTarget;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::profile_photo_sources::ChatPhotoInput;
use quill::profile_photo_video::{encode_args, main_frame_secs};

impl QuillApp {
    pub(super) fn choose_avatar_video(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(tr(keys::Q_AV_CHOOSE_VIDEO).into()),
        });
        self.spawn_scoped_in(window, cx, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picker.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update_in(cx, |this, window, cx| {
                    if this.photo_target().is_some() {
                        this.open_video_editor(EditorTarget::Avatar, path, None, window, cx);
                    }
                });
            }
        })
        .detach();
    }

    /// "Done" on an avatar: encode the clip, then ask for it as the
    /// profile photo. The editor stays, busy, until the encode ends; a
    /// failure is its note. Returns whether the dialog closes now (never:
    /// it closes itself when the request is sent).
    pub(in crate::ui) fn finish_avatar_video_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(target) = self.photo_target() else {
            return true;
        };
        let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() else {
            return true;
        };
        let (Some(info), false) = (editor.info, editor.busy) else {
            return false;
        };
        let edits = editor.edits;
        let input = editor.source.to_string_lossy().into_owned();
        editor.busy = true;
        editor.note = Some(tr(keys::Q_AV_PREPARING_THE_VIDEO).into());
        editor.serial += 1;
        let serial = editor.serial;
        let output = work_dir().join(format!("avatar-{serial}.mp4"));
        let args = encode_args(&input, &output.to_string_lossy(), &edits, &info);
        let log = work_dir().join("encode.log");
        let worker = cx.background_spawn(async move { run_ffmpeg(&args, &log) });
        self.spawn_scoped_in(window, cx, async move |this, cx| {
            let result = worker.await;
            let _ = this.update_in(cx, |this, window, cx| {
                let current = this
                    .composer_ui
                    .send_box
                    .video_editor
                    .as_ref()
                    .is_some_and(|editor| editor.serial == serial);
                if !current {
                    let _ = std::fs::remove_file(&output);
                    return;
                }
                let still = this
                    .composer_ui
                    .send_box
                    .video_editor
                    .as_ref()
                    .and_then(|editor| editor.still.as_deref().cloned());
                let sent = result.and_then(|()| {
                    let photo = ChosenPhoto {
                        input: ChatPhotoInput::Animation {
                            path: output.to_string_lossy().into_owned(),
                            main_frame_timestamp: main_frame_secs(&edits),
                        },
                        preview: still.map(PhotoPreview::Image),
                    };
                    this.take_chosen_photo(target, photo)
                });
                match sent {
                    Ok(()) => {
                        this.close_video_editor(cx);
                        this.close_kit_dialog_if_done(DialogKind::VideoEditor, window, cx);
                    }
                    Err(note) => {
                        let _ = std::fs::remove_file(&output);
                        if let Some(editor) = this.composer_ui.send_box.video_editor.as_mut() {
                            editor.busy = false;
                            editor.note = Some(note);
                            // A still cut dropped by the serial bump frees
                            // the cutter again.
                            editor.still_busy = false;
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
}
