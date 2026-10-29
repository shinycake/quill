//! "Edit Video" for a video that is about to be sent (tdesktop
//! `Editor::VideoEditor` as opened by `OpenWithPreparedVideoFile`): a
//! full-window layer that plays the kept range in a loop, with the crop on
//! the picture, the frame strip with its trim handles and cover playhead,
//! the quality dots and the Cancel / Rotate / Flip / GIF / Done bar. Opened
//! from the tray, from the edit box for a staged replacement video, and for
//! a video sticker (`video_editor_set.rs`).
//!
//! "Done" encodes the edited copy off the UI thread (only when the file has
//! to change: a new cover frame alone needs no encoding) and swaps it in as
//! the file to send. The numbers and command lines are `quill::video_edit`
//! and `quill::video_timeline`; the picture and crop are
//! `video_editor_tools.rs`, the strip and the cover bubble
//! `video_editor_timeline.rs`, the quality dots and the bar
//! `video_editor_controls.rs`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::composer_sends::TrayItem;
use super::shell::{DialogKind, QuillShell};
use super::video_editor_timeline::EditorMotion;
use super::video_player::VideoPlayer;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::composer::{AttachmentExtras, AttachmentKind, EditMediaKind};
use quill::lang::{keys, tr};
use quill::sticker_set_box::SetKind;
use quill::video_edit::{VideoEdits, VideoSource, probe_source, render_edited, render_frame};
use quill::video_timeline::Timeline;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

/// What the edited copy replaces.
#[derive(Clone, PartialEq, Eq)]
pub(super) enum EditorTarget {
    /// A tray video, as it was when the editor opened.
    Tray(TrayItem),
    /// The replacement video of the message being edited.
    MessageEdit,
    /// A video sticker for an owned set (`video_editor_set.rs`): three
    /// seconds at most, a square crop, encoded as WEBM.
    Set { set_id: i64, kind: SetKind },
    /// The video avatar of the edit-profile dialog
    /// (`profile_photo_sources/video.rs`): 1 to 9.6 s, a square crop,
    /// encoded at 800 x 800 and set as the profile photo.
    Avatar,
}

impl EditorTarget {
    /// The crop is always a square (`exactSize` in tdesktop).
    pub(super) fn square(&self) -> bool {
        matches!(self, Self::Set { .. } | Self::Avatar)
    }

    /// The crop is shown as a circle (`CropType::Ellipse`, an avatar).
    pub(super) fn round(&self) -> bool {
        *self == Self::Avatar
    }

    /// The longest and the shortest range in ms; 0 is no limit.
    fn range_limits(&self) -> (i64, i64) {
        match self {
            Self::Set { .. } => (
                quill::sticker_video::MAX_DURATION_MS,
                quill::sticker_video::MIN_DURATION_MS,
            ),
            Self::Avatar => (
                quill::profile_photo_video::AVATAR_MAX_MS,
                quill::profile_photo_video::AVATAR_MIN_MS,
            ),
            Self::Tray(_) | Self::MessageEdit => (0, 0),
        }
    }
}

/// The layer's own colors: like tdesktop's editor it is dark in both
/// themes (the `mediaview*` palette), so a picture is judged on black.
pub(super) mod palette {
    use gpui_kit::{Hsla, black, hsla, white};

    /// The layer behind the picture.
    pub fn layer() -> Hsla {
        hsla(0.0, 0.0, 0.07, 1.0)
    }
    /// `mediaviewCaptionFg` / `mediaviewControlFg`: labels and handles.
    pub fn text() -> Hsla {
        white()
    }
    /// Secondary text (`videoEditorHint`), still AA on the layer.
    pub fn muted() -> Hsla {
        white().opacity(0.72)
    }
    /// `mediaviewCaptionBg`: the dim outside the kept range, the grips and
    /// the playhead outline.
    pub fn dim() -> Hsla {
        black().opacity(0.55)
    }
    /// `mediaviewPipPlaybackInactive`: the strip before its frames load,
    /// the quality line.
    pub fn inactive() -> Hsla {
        white().opacity(0.18)
    }
    /// `roundedBg` under the bar's buttons: darker than the layer, as in
    /// tdesktop where the pill reads as a hole in the blurred backdrop.
    pub fn bar() -> Hsla {
        black().opacity(0.45)
    }
    /// `photoEditorEdgeButtonBg`: a hovered button of the bar.
    pub fn hover() -> Hsla {
        white().opacity(0.1)
    }
    /// `mediaviewTextLinkFg`: "Done".
    pub fn link() -> Hsla {
        hsla(201.0 / 360.0, 1.0, 0.78, 1.0)
    }
    /// `videoTimelineDotFg` and an active tool.
    pub fn accent() -> Hsla {
        hsla(210.0 / 360.0, 0.9, 0.62, 1.0)
    }
    /// `photoCropFadeBg`: outside the crop.
    pub fn crop_fade() -> Hsla {
        black().opacity(0.5)
    }
}

pub(super) struct VideoEditorDialog {
    pub(super) target: EditorTarget,
    /// The file the edits start from.
    pub(super) source: PathBuf,
    /// Probed size, length and sound; `None` while it is read.
    pub(super) info: Option<VideoSource>,
    pub(super) edits: VideoEdits,
    /// The strip's selection; `None` until the video is read.
    pub(super) timeline: Option<Timeline>,
    /// The still frame (turned and mirrored like the edit) at the cover:
    /// the picture when the video can't play here, and the cover bubble.
    pub(super) preview: Option<PathBuf>,
    pub(super) still: Option<super::owned_image::OwnedImage>,
    /// A still is being cut; one more is wanted at this time after it.
    pub(super) still_busy: bool,
    pub(super) still_wanted: Option<i64>,
    /// Bumped by every probe and encode; late answers are dropped.
    pub(super) serial: u64,
    /// The edited copy is being encoded.
    pub(super) busy: bool,
    pub(super) note: Option<String>,
    /// The player looping the kept range, muted.
    pub(super) player: Option<VideoPlayer>,
    /// Paused by a click on the picture (tdesktop `_userPaused`).
    pub(super) user_paused: bool,
    /// Where the player is, for the playhead.
    pub(super) playback_ms: Option<i64>,
    /// Drags, animations and the measured layout.
    pub(super) motion: EditorMotion,
    /// Set target: the emoji the sticker already had, for the "New
    /// Sticker" box.
    pub(super) set_emoji: Option<String>,
}

/// Where the editor writes its pictures and copies is
/// `quill::video_edit`'s folder; a copy made by an earlier edit lives
/// there and can be replaced.
fn is_editor_copy(path: &Path) -> bool {
    let folder = quill::scratch::temp_work_dir("quill-video-edits");
    // The folder name, not its full path: an attachment path may be
    // canonicalized (`/private/var/...`, `\\?\...`) while the temp dir is not.
    let name = path.parent().and_then(Path::file_name);
    name.is_some() && name == folder.file_name()
}

/// A JPEG still as a GPUI picture, read off the UI thread.
pub(super) fn load_still(path: &Path) -> Option<Arc<RenderImage>> {
    let image = image::open(path).ok()?.to_rgba8();
    Some(super::photo_editor::preview_of(&image))
}

impl QuillApp {
    /// Open the editor on a tray video.
    pub(super) fn open_tray_video_editor(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(item) = self.tray_item(index) else {
            return;
        };
        let Some(attachment) = self.pending_attachments().get(index) else {
            return;
        };
        if attachment.kind != AttachmentKind::Video {
            return;
        }
        let source = attachment
            .extras
            .source
            .clone()
            .unwrap_or_else(|| attachment.path.clone());
        // A soundless video is a GIF from the start (tdesktop sets
        // `modifications.gif = !hasAudio`); the range is filled in once the
        // video is read.
        let initial = attachment.extras.edits.or_else(|| {
            attachment.extras.gif.then(|| VideoEdits {
                gif: true,
                ..VideoEdits::full(0)
            })
        });
        self.open_video_editor(EditorTarget::Tray(item), source, initial, window, cx);
    }

    /// Open the editor on the replacement video of the edit box.
    pub(super) fn open_edit_video_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = self.composer_ui.pending_edit.as_ref() else {
            return;
        };
        let Some(replacement) = edit
            .media_edit
            .replacement
            .as_ref()
            .filter(|rep| rep.kind == EditMediaKind::Video)
        else {
            return;
        };
        let source = edit
            .media_edit
            .video_extras
            .source
            .clone()
            .unwrap_or_else(|| replacement.path.clone());
        let initial = edit.media_edit.video_extras.edits;
        self.open_video_editor(EditorTarget::MessageEdit, source, initial, window, cx);
    }

    pub(super) fn open_video_editor(
        &mut self,
        target: EditorTarget,
        source: PathBuf,
        initial: Option<VideoEdits>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let serial = next_editor_serial();
        self.composer_ui.send_box.video_editor = Some(VideoEditorDialog {
            target,
            source: source.clone(),
            info: None,
            edits: VideoEdits::full(0),
            timeline: None,
            preview: None,
            still: None,
            still_busy: false,
            still_wanted: None,
            serial,
            busy: false,
            note: Some(tr(keys::Q_AV_READING_THE_VIDEO).into()),
            player: None,
            user_paused: false,
            playback_ms: None,
            motion: EditorMotion::default(),
            set_emoji: None,
        });
        let worker = cx.background_spawn(async move { probe_source(&source) });
        self.spawn_scoped_in(window, cx, async move |this, cx| {
            let info = worker.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.video_editor_probed(serial, info, initial, window, cx);
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn video_editor_probed(
        &mut self,
        serial: u64,
        info: Option<VideoSource>,
        initial: Option<VideoEdits>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .composer_ui
            .send_box
            .video_editor
            .as_ref()
            .is_none_or(|editor| editor.serial != serial)
        {
            return;
        }
        let Some(target) = self
            .composer_ui
            .send_box
            .video_editor
            .as_ref()
            .map(|editor| editor.target.clone())
        else {
            return;
        };
        let for_set = matches!(target, EditorTarget::Set { .. });
        // A set's source the editor can't take: the editor goes and the
        // set box toasts, as tdesktop's `lng_bad_video` does.
        if for_set && !info.is_some_and(|info| quill::sticker_video::accepts_source(&info)) {
            self.close_video_editor(cx);
            self.close_kit_dialog_if_done(DialogKind::VideoEditor, window, cx);
            self.connection
                .set_note(StatusNote::error(quill::sticker_video::BAD_VIDEO_NOTE));
            cx.notify();
            return;
        }
        // An avatar's source too short or too thin says why and goes.
        if target == EditorTarget::Avatar
            && let Some(Err(error)) =
                info.map(|info| quill::profile_photo_video::check_source(&info))
        {
            self.close_video_editor(cx);
            self.close_kit_dialog_if_done(DialogKind::VideoEditor, window, cx);
            self.connection.set_note(StatusNote::error(error.message()));
            cx.notify();
            return;
        }
        let Some(info) = info else {
            if let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() {
                editor.note = Some(tr(keys::Q_AV_THIS_ISNT_A_VIDEO_QUILL).into());
            }
            cx.notify();
            return;
        };
        // An edit that only asked for a GIF has no range yet.
        let edits = if for_set {
            quill::sticker_video::initial_edits(&info)
        } else if target == EditorTarget::Avatar {
            quill::profile_photo_video::initial_edits(&info)
        } else {
            initial
                .map(|edits| {
                    let till_ms = if edits.till_ms <= 0 {
                        info.duration_ms
                    } else {
                        edits.till_ms
                    };
                    VideoEdits { till_ms, ..edits }
                })
                .unwrap_or_else(|| VideoEdits::full(info.duration_ms))
        };
        // A sticker lasts three seconds at most (`VideoStickerEditorData`),
        // an avatar 9.6 (`kProfileVideoMaxDuration`).
        let (max, min) = target.range_limits();
        let timeline = Timeline::new(info.duration_ms, max, min, &edits);
        let edits = VideoEdits {
            from_ms: timeline.from,
            till_ms: timeline.till,
            cover_ms: timeline.cover,
            ..edits
        };
        let at = edits.cover_ms;
        if let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() {
            editor.info = Some(info);
            editor.edits = edits;
            editor.timeline = Some(timeline);
            editor.note = None;
        }
        self.start_video_editor_player(cx);
        self.refresh_video_preview(at, cx);
    }

    /// Cut the frame at `at_ms` of the source for the still and the cover
    /// bubble. One cut runs at a time; the latest wish follows it.
    pub(super) fn refresh_video_preview(&mut self, at_ms: i64, cx: &mut Context<Self>) {
        let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() else {
            return;
        };
        if editor.still_busy {
            editor.still_wanted = Some(at_ms);
            return;
        }
        editor.still_busy = true;
        editor.still_wanted = None;
        let serial = editor.serial;
        let source = editor.source.clone();
        let (turns, flipped) = (editor.edits.turns, editor.edits.flipped);
        let worker = cx.background_spawn(async move {
            let frame = render_frame(&source, at_ms, turns, flipped)?;
            let still = load_still(&frame);
            Ok::<_, String>((frame, still))
        });
        self.spawn_scoped(cx, async move |this, cx| {
            let result = worker.await;
            let _ = this.update(cx, |this, cx| {
                let Some(editor) = this
                    .composer_ui
                    .send_box
                    .video_editor
                    .as_mut()
                    .filter(|editor| editor.serial == serial)
                else {
                    if let Ok((frame, _)) = result {
                        let _ = std::fs::remove_file(frame);
                    }
                    return;
                };
                editor.still_busy = false;
                match result {
                    Ok((frame, still)) => {
                        if let Some(old) = editor.preview.replace(frame) {
                            let _ = std::fs::remove_file(old);
                        }
                        if let Some(still) = still {
                            editor.still = Some(still.into());
                        }
                    }
                    Err(note) => editor.note = Some(note),
                }
                if let Some(next) = editor.still_wanted.take() {
                    this.refresh_video_preview(next, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn close_video_editor(&mut self, cx: &mut Context<Self>) {
        if let Some(editor) = self.composer_ui.send_box.video_editor.take() {
            if let Some(preview) = editor.preview {
                let _ = std::fs::remove_file(preview);
            }
            editor.motion.release_frames();
        }
        cx.notify();
    }

    /// The open editor is making a sticker for a set.
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn video_editor_for_set(&self) -> bool {
        self.composer_ui
            .send_box
            .video_editor
            .as_ref()
            .is_some_and(|editor| matches!(editor.target, EditorTarget::Set { .. }))
    }

    /// "Done": encode the edited copy when the file has to change, then
    /// swap it in.
    pub(super) fn finish_video_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() else {
            return true;
        };
        match editor.target {
            EditorTarget::Set { set_id, kind } => {
                return self.finish_set_video_editor(set_id, kind, cx);
            }
            EditorTarget::Avatar => return self.finish_avatar_video_editor(window, cx),
            EditorTarget::Tray(_) | EditorTarget::MessageEdit => {}
        }
        let (Some(info), false) = (editor.info, editor.busy) else {
            return false;
        };
        let edits = editor.edits;
        let source = editor.source.clone();
        // Taking the sound out of a video that has none changes nothing.
        let effective = VideoEdits {
            gif: edits.gif && info.has_audio,
            ..edits
        };
        if !effective.needs_transcode(info.duration_ms) {
            self.apply_video_edit(source.clone(), source, edits, info, cx);
            self.close_video_editor(cx);
            return true;
        }
        editor.player = None;
        editor.busy = true;
        editor.note = Some(tr(keys::Q_AV_PREPARING_THE_VIDEO).into());
        editor.serial += 1;
        let serial = editor.serial;
        let encode_source = source.clone();
        let worker =
            cx.background_spawn(async move { render_edited(&encode_source, &effective, &info) });
        self.spawn_scoped_in(window, cx, async move |this, cx| {
            let result = worker.await;
            let _ = this.update_in(cx, |this, window, cx| {
                let current = this
                    .composer_ui
                    .send_box
                    .video_editor
                    .as_ref()
                    .is_some_and(|editor| editor.serial == serial);
                match result {
                    Ok(output) if current => {
                        this.apply_video_edit(output, source, edits, info, cx);
                        this.close_video_editor(cx);
                        window.close_dialog(cx);
                    }
                    Ok(output) => {
                        let _ = std::fs::remove_file(output);
                    }
                    Err(note) if current => {
                        if let Some(editor) = this.composer_ui.send_box.video_editor.as_mut() {
                            editor.busy = false;
                            editor.note = Some(note);
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

    /// Make `output` the file to send, remembering what it was made from.
    fn apply_video_edit(
        &mut self,
        output: PathBuf,
        source: PathBuf,
        edits: VideoEdits,
        info: VideoSource,
        cx: &mut Context<Self>,
    ) {
        let target = self
            .composer_ui
            .send_box
            .video_editor
            .as_ref()
            .map(|editor| editor.target.clone());
        let edited = output != source;
        let untouched = edits == VideoEdits::full(info.duration_ms);
        let mut extras = AttachmentExtras {
            source: edited.then(|| source.clone()),
            edits: (!untouched).then_some(edits),
            start_timestamp: VideoEdits {
                gif: edits.gif && info.has_audio,
                ..edits
            }
            .start_timestamp_secs(info.duration_ms),
            sound_removed: edits.gif && info.has_audio,
            ..AttachmentExtras::default()
        };
        let note = match target {
            Some(EditorTarget::Tray(item)) => {
                // The copy replaces the video in the chat the editor was
                // opened from, as long as that video is still in its tray.
                let several = self.composer_sends.get(&item.slot).attachments.len() > 1;
                let replaced = self.composer_sends.update_item(&item, |attachment| {
                    if is_editor_copy(&attachment.path) && attachment.path != output {
                        let _ = std::fs::remove_file(&attachment.path);
                    }
                    extras.cover = attachment.extras.cover.take();
                    extras.gif = edits.gif;
                    if edited {
                        attachment.file_name = mp4_name(&attachment.file_name);
                    }
                    attachment.path = output.clone();
                    attachment.extras = extras;
                });
                if replaced.is_none() {
                    if edited && is_editor_copy(&output) {
                        let _ = std::fs::remove_file(&output);
                    }
                    return;
                }
                if edits.gif && info.has_audio && several {
                    tr(keys::Q_AV_THE_GIF_LEAVES_THE_ALBUM)
                } else {
                    tr(keys::Q_AV_VIDEO_EDITS_SAVED)
                }
            }
            Some(EditorTarget::MessageEdit) => {
                let Some(edit) = self.composer_ui.pending_edit.as_mut() else {
                    return;
                };
                let Some(replacement) = edit.media_edit.replacement.as_mut() else {
                    return;
                };
                if is_editor_copy(&replacement.path) && replacement.path != output {
                    let _ = std::fs::remove_file(&replacement.path);
                }
                if edited {
                    replacement.file_name = mp4_name(&replacement.file_name);
                }
                replacement.path = output;
                extras.gif = false;
                extras.sound_removed = false;
                edit.media_edit.video_extras = extras;
                tr(keys::Q_AV_VIDEO_EDITS_SAVED)
            }
            Some(EditorTarget::Set { .. } | EditorTarget::Avatar) | None => return,
        };
        self.connection.set_note(StatusNote::silent(note));
        cx.notify();
    }

    pub(super) fn build_video_editor_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::VideoEditor, |this, _, cx| {
                this.close_video_editor(cx);
            });
        // Enter is "Done" (tdesktop `keyPressEvent`); Esc is the kit's close.
        let done_app = app.clone();
        let on_ok = move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
            done_app.update(cx, |this, cx| this.finish_video_editor(window, cx))
        };
        let body = app.update(cx, |this, cx| this.video_editor_layer(cx));
        let body = Rc::new(RefCell::new(Some(body)));
        // Nearly the whole window, edge to edge inside the kit's margin, on
        // the layer's own dark background.
        dialog
            .overlay(true)
            .close_button(false)
            .w(px(100_000.))
            .margin_top(px(0.))
            .p_0()
            .bg(palette::layer())
            .border_color(palette::layer())
            .content(move |content, window, cx| {
                let margin = cx.theme().spacing_tokens().lg;
                let height = (window.viewport_size().height - margin * 2.).max(px(320.));
                let body = body
                    .borrow_mut()
                    .take()
                    .unwrap_or_else(|| div().into_any_element());
                content.child(div().h(height).w_full().child(body))
            })
            .on_ok(on_ok)
            .on_close(on_close)
    }
}

/// A serial base no earlier editor used, so a probe, still or strip frame
/// still running for a closed editor never matches a newly opened one.
/// Each editor counts up from its base for its own requests.
fn next_editor_serial() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static OPENED: AtomicU64 = AtomicU64::new(0);
    (OPENED.fetch_add(1, Ordering::Relaxed) + 1) << 32
}

/// tdesktop `Mp4FileName`: an encoded copy is an mp4 file.
fn mp4_name(name: &str) -> String {
    let stem = Path::new(name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("video");
    format!("{stem}.mp4")
}

crate::ui::shell::register_dialogs! {
    VideoEditor => DialogSpec::new(
        5442,
        |app| app.composer_ui.send_box.video_editor.is_some(),
        QuillApp::build_video_editor_dialog,
    ),
}

#[cfg(test)]
mod tests {
    use super::{is_editor_copy, mp4_name};
    use std::path::Path;

    #[test]
    fn an_encoded_copy_is_an_mp4() {
        assert_eq!(mp4_name("holiday.mov"), "holiday.mp4");
        assert_eq!(mp4_name(""), "video.mp4");
    }

    #[test]
    fn only_files_in_the_editor_folder_are_replaced() {
        let dir = quill::scratch::temp_work_dir("quill-video-edits");
        assert!(is_editor_copy(&dir.join("edit-1.mp4")));
        assert!(!is_editor_copy(Path::new("/home/me/clip.mp4")));
        assert!(!is_editor_copy(Path::new("clip.mp4")));
    }

    #[test]
    fn a_canonicalized_editor_copy_is_still_recognised() {
        let dir = quill::scratch::temp_work_dir("quill-video-edits");
        let Some(name) = dir.file_name() else {
            panic!("the editor folder has a name");
        };
        let moved = Path::new("/private/elsewhere")
            .join(name)
            .join("edit-1.mp4");
        assert!(is_editor_copy(&moved));
        assert!(!is_editor_copy(
            &Path::new("/private/elsewhere").join("edit-1.mp4")
        ));
    }
}
