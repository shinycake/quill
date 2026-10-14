//! "Edit Video" for a video that is about to be sent (tdesktop
//! `Editor::OpenWithPreparedVideoFile`): trim the range, pick the cover
//! frame, choose a quality and send the clip as a GIF. Opened from the
//! tray and from the edit box for a staged replacement video.
//!
//! The picture is the frame at the last moved handle, cut by ffmpeg. "Done"
//! encodes the edited copy off the UI thread (only when the file has to
//! change: a new cover frame alone needs no encoding) and swaps it in as
//! the file to send. The math and command lines are `quill::video_edit`.

use super::app::QuillApp;
use super::native_video::NativeVideo;
use super::shell::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::slider::{SliderEvent, SliderState};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::composer::{AttachmentExtras, AttachmentKind, EditMediaKind};
use quill::video_edit::{
    VideoEdits, VideoSource, format_clock, probe_source, quality_label, quality_levels,
    render_edited, render_frame,
};
use quill::sticker_set_box::SetKind;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// What the edited copy replaces.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum EditorTarget {
    /// The video at this place in the tray.
    Tray(usize),
    /// The replacement video of the message being edited.
    MessageEdit,
    /// A video sticker for an owned set (`video_editor_set.rs`): three
    /// seconds at most, a square crop, encoded as WEBM.
    Set { set_id: i64, kind: SetKind },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Knob {
    Range,
    Cover,
    CropX,
    CropY,
}

pub(super) struct VideoEditorDialog {
    target: EditorTarget,
    /// The file the edits start from.
    pub(super) source: PathBuf,
    /// Probed size, length and sound; `None` while it is read.
    pub(super) info: Option<VideoSource>,
    pub(super) edits: VideoEdits,
    /// The frame shown above the sliders.
    pub(super) preview: Option<PathBuf>,
    /// Bumped by every probe, frame and encode; late answers are dropped.
    pub(super) serial: u64,
    /// The edited copy is being encoded.
    busy: bool,
    pub(super) note: Option<String>,
    range: Option<Entity<SliderState>>,
    cover: Option<Entity<SliderState>>,
    pub(super) crop_x: Option<Entity<SliderState>>,
    pub(super) crop_y: Option<Entity<SliderState>>,
    /// The preview player while the trimmed range plays.
    pub(super) player: RefCell<Option<NativeVideo>>,
    pub(super) playing: bool,
    /// Set target: the emoji the sticker already had, for the "New
    /// Sticker" box.
    pub(super) set_emoji: Option<String>,
}

/// Where the editor writes its pictures and copies is
/// `quill::video_edit`'s folder; a copy made by an earlier edit lives
/// there and can be replaced.
fn is_editor_copy(path: &Path) -> bool {
    path.parent()
        .and_then(Path::file_name)
        .is_some_and(|dir| dir == "quill-video-edits")
}

fn slider_seconds(ms: i64) -> f32 {
    ms as f32 / 1000.0
}

impl QuillApp {
    /// Open the editor on a tray video.
    pub(super) fn open_tray_video_editor(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(attachment) = self.composer_ui.pending_attachments.get(index) else {
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
        self.open_video_editor(EditorTarget::Tray(index), source, initial, window, cx);
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
        let serial = self
            .composer_ui
            .send_box
            .video_editor
            .as_ref()
            .map_or(0, |editor| editor.serial)
            + 1;
        self.composer_ui.send_box.video_editor = Some(VideoEditorDialog {
            target,
            source: source.clone(),
            info: None,
            edits: VideoEdits::full(0),
            preview: None,
            serial,
            busy: false,
            note: Some("Reading the video…".into()),
            range: None,
            cover: None,
            crop_x: None,
            crop_y: None,
            player: RefCell::new(None),
            playing: false,
            set_emoji: None,
        });
        let worker = cx.background_spawn(async move { probe_source(&source) });
        cx.spawn_in(window, async move |this, cx| {
            let info = worker.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.video_editor_probed(serial, info, initial, window, cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn video_editor_probed(
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
        let Some(info) = info else {
            if let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() {
                editor.note = Some("This isn't a video Quill can read.".into());
            }
            cx.notify();
            return;
        };
        let for_set = self.video_editor_for_set();
        if for_set && !quill::sticker_video::accepts_source(&info) {
            if let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() {
                editor.note = Some(quill::sticker_video::BAD_VIDEO_NOTE.into());
            }
            cx.notify();
            return;
        }
        // An edit that only asked for a GIF has no range yet.
        let edits = if for_set {
            quill::sticker_video::initial_edits(&info)
        } else {
            initial
                .map(|edits| {
                    let till_ms = if edits.till_ms <= 0 {
                        info.duration_ms
                    } else {
                        edits.till_ms
                    };
                    VideoEdits { till_ms, ..edits }.clamped(info.duration_ms, true)
                })
                .unwrap_or_else(|| VideoEdits::full(info.duration_ms))
        };
        let crop_unit =
            |start: u16, len: u16| (f32::from(start) / 1000.0, f32::from(start + len) / 1000.0);
        let crop = edits.crop.unwrap_or(quill::video_edit::Crop::FULL);
        let crop_x = self.video_editor_knob_slider(
            Knob::CropX,
            SliderState::new()
                .min(0.0)
                .max(1.0)
                .step(0.01)
                .default_value(crop_unit(crop.x, crop.w)),
            window,
            cx,
        );
        let crop_y = self.video_editor_knob_slider(
            Knob::CropY,
            SliderState::new()
                .min(0.0)
                .max(1.0)
                .step(0.01)
                .default_value(crop_unit(crop.y, crop.h)),
            window,
            cx,
        );
        let range = self.video_editor_knob_slider(
            Knob::Range,
            SliderState::new()
                .min(0.0)
                .max(slider_seconds(info.duration_ms))
                .step(0.1)
                .default_value((slider_seconds(edits.from_ms), slider_seconds(edits.till_ms))),
            window,
            cx,
        );
        let cover = self.video_editor_knob_slider(
            Knob::Cover,
            SliderState::new()
                .min(0.0)
                .max(1.0)
                .step(0.001)
                .default_value(cover_fraction(&edits)),
            window,
            cx,
        );
        let at = edits.cover_ms;
        if let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() {
            editor.info = Some(info);
            editor.edits = edits;
            editor.range = Some(range);
            editor.cover = Some(cover);
            editor.crop_x = Some(crop_x);
            editor.crop_y = Some(crop_y);
            editor.note = None;
        }
        self.refresh_video_preview(at, cx);
    }

    fn video_editor_knob_slider(
        &mut self,
        knob: Knob,
        slider: SliderState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<SliderState> {
        let entity = cx.new(|_| slider);
        cx.subscribe_in(
            &entity,
            window,
            move |this, _, event: &SliderEvent, window, cx| {
                let (value, release) = match event {
                    SliderEvent::Change(value) => (*value, false),
                    SliderEvent::Release(value) => (*value, true),
                };
                this.video_editor_knob(knob, value.start(), value.end(), release, window, cx);
            },
        )
        .detach();
        entity
    }

    /// A slider moved. While dragging only the labels follow; on release
    /// the range snaps to its limits and the picture is cut again.
    fn video_editor_knob(
        &mut self,
        knob: Knob,
        start: f32,
        end: f32,
        release: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(knob, Knob::CropX | Knob::CropY) {
            self.video_editor_crop_knob(knob, start, end, release, window, cx);
            return;
        }
        let for_set = self.video_editor_for_set();
        let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() else {
            return;
        };
        let Some(duration) = editor.info.map(|info| info.duration_ms) else {
            return;
        };
        let show_at = match knob {
            Knob::Range => {
                let from = (f64::from(start) * 1000.0).round() as i64;
                let till = (f64::from(end) * 1000.0).round() as i64;
                let moved_start = (from - editor.edits.from_ms).abs() > 1;
                let edits = VideoEdits {
                    from_ms: from,
                    till_ms: till,
                    ..editor.edits
                };
                editor.edits = if for_set {
                    quill::sticker_video::clamp_range(edits, duration, moved_start)
                } else {
                    edits.clamped(duration, moved_start)
                };
                let show_at = if moved_start {
                    editor.edits.from_ms
                } else {
                    (editor.edits.till_ms - 100).max(editor.edits.from_ms)
                };
                if release {
                    let edits = editor.edits;
                    if let Some(range) = editor.range.clone() {
                        range.update(cx, |state, cx| {
                            state.set_value(
                                (slider_seconds(edits.from_ms), slider_seconds(edits.till_ms)),
                                window,
                                cx,
                            )
                        });
                    }
                    if let Some(cover) = editor.cover.clone() {
                        cover.update(cx, |state, cx| {
                            state.set_value(cover_fraction(&edits), window, cx)
                        });
                    }
                }
                if release && let Some(video) = editor.player.borrow_mut().as_mut() {
                    video.seek(editor.edits.from_ms as f64 / 1000.0);
                }
                show_at
            }
            Knob::CropX | Knob::CropY => return,
            Knob::Cover => {
                let fraction = f64::from(end.clamp(0.0, 1.0));
                let length = editor.edits.length_ms() as f64;
                editor.edits.cover_ms =
                    (editor.edits.from_ms as f64 + fraction * length).round() as i64;
                editor.edits.cover_ms
            }
        };
        if release {
            self.refresh_video_preview(show_at, cx);
        } else {
            cx.notify();
        }
    }

    /// Cut the frame at `at_ms` of the source for the picture.
    pub(super) fn refresh_video_preview(&mut self, at_ms: i64, cx: &mut Context<Self>) {
        let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() else {
            return;
        };
        editor.serial += 1;
        let serial = editor.serial;
        let source = editor.source.clone();
        let (turns, flipped) = (editor.edits.turns, editor.edits.flipped);
        let worker =
            cx.background_spawn(async move { render_frame(&source, at_ms, turns, flipped) });
        cx.spawn(async move |this, cx| {
            let result = worker.await;
            let _ = this.update(cx, |this, cx| {
                let Some(editor) = this
                    .composer_ui
                    .send_box
                    .video_editor
                    .as_mut()
                    .filter(|editor| editor.serial == serial)
                else {
                    if let Ok(frame) = result {
                        let _ = std::fs::remove_file(frame);
                    }
                    return;
                };
                match result {
                    Ok(frame) => {
                        if let Some(old) = editor.preview.replace(frame) {
                            let _ = std::fs::remove_file(old);
                        }
                    }
                    Err(note) => editor.note = Some(note),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn set_video_editor_quality(&mut self, level: u32, cx: &mut Context<Self>) {
        if let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() {
            editor.edits.quality = level;
            cx.notify();
        }
    }

    fn toggle_video_editor_gif(&mut self, cx: &mut Context<Self>) {
        if let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() {
            editor.edits.gif = !editor.edits.gif;
            cx.notify();
        }
    }

    fn close_video_editor(&mut self, cx: &mut Context<Self>) {
        if let Some(editor) = self.composer_ui.send_box.video_editor.take()
            && let Some(preview) = editor.preview
        {
            let _ = std::fs::remove_file(preview);
        }
        cx.notify();
    }

    /// The open editor is making a sticker for a set.
    pub(super) fn video_editor_for_set(&self) -> bool {
        self.composer_ui
            .send_box
            .video_editor
            .as_ref()
            .is_some_and(|editor| matches!(editor.target, EditorTarget::Set { .. }))
    }

    /// "Done": encode the edited copy when the file has to change, then
    /// swap it in.
    fn finish_video_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() else {
            return true;
        };
        if let EditorTarget::Set { set_id, kind } = editor.target {
            return self.finish_set_video_editor(set_id, kind, window, cx);
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
        editor.playing = false;
        *editor.player.borrow_mut() = None;
        editor.busy = true;
        editor.note = Some("Preparing the video…".into());
        editor.serial += 1;
        let serial = editor.serial;
        let encode_source = source.clone();
        let worker =
            cx.background_spawn(async move { render_edited(&encode_source, &effective, &info) });
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
            .map(|editor| editor.target);
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
            Some(EditorTarget::Tray(index)) => {
                let several = self.composer_ui.pending_attachments.len() > 1;
                let Some(attachment) = self.composer_ui.pending_attachments.get_mut(index) else {
                    return;
                };
                if is_editor_copy(&attachment.path) && attachment.path != output {
                    let _ = std::fs::remove_file(&attachment.path);
                }
                extras.cover = attachment.extras.cover.take();
                extras.gif = edits.gif;
                if edited {
                    attachment.file_name = mp4_name(&attachment.file_name);
                }
                attachment.path = output;
                attachment.extras = extras;
                if edits.gif && info.has_audio && several {
                    "The GIF leaves the album and goes out on its own"
                } else {
                    "Video edits saved"
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
                "Video edits saved"
            }
            Some(EditorTarget::Set { .. }) | None => return,
        };
        self.connection.status_note = note.into();
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
        app.update(cx, |this, cx| {
            let Some(editor) = this.composer_ui.send_box.video_editor.as_ref() else {
                return dialog.on_close(on_close);
            };
            let body = this.video_editor_body(editor, cx);
            let body = Rc::new(RefCell::new(Some(body)));
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title("Edit Video"))
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    },
                ))
                .on_close(on_close)
        })
    }

    fn video_editor_body(&self, editor: &VideoEditorDialog, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let this_stage = self.video_editor_stage(editor, cx);
        let for_set = matches!(editor.target, EditorTarget::Set { .. });
        let gif_allowed = editor.target != EditorTarget::MessageEdit && !for_set;
        let picture = this_stage;
        let mut body = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(picture)
            .child(self.video_editor_toolbar(editor, cx));
        if let (Some(info), Some(range), Some(cover), Some(crop_x), Some(crop_y)) = (
            editor.info,
            &editor.range,
            &editor.cover,
            &editor.crop_x,
            &editor.crop_y,
        ) {
            let edits = editor.edits;
            body = body
                .child(self.video_editor_slider_row(
                    "Trim",
                    format!(
                        "{} to {} ({})",
                        format_clock(edits.from_ms),
                        format_clock(edits.till_ms),
                        format_clock(edits.length_ms())
                    ),
                    range,
                    cx,
                ))
                .child(self.video_editor_slider_row(
                    "Cover frame",
                    format_clock(edits.cover_ms),
                    cover,
                    cx,
                ))
                .child(self.video_editor_slider_row(
                    "Crop left to right",
                    crop_label(edits.crop.map(|crop| (crop.x, crop.w))),
                    crop_x,
                    cx,
                ))
                .child(self.video_editor_slider_row(
                    "Crop top to bottom",
                    crop_label(edits.crop.map(|crop| (crop.y, crop.h))),
                    crop_y,
                    cx,
                ));
            let (edited_w, edited_h) = edits.edited_size(&info);
            let levels = quality_levels(edited_w, edited_h);
            // A sticker is always encoded at the set's side.
            if levels.len() > 1 && !for_set {
                let mut choices = div().flex().flex_wrap().gap_1();
                for level in levels {
                    choices = choices.child(
                        Button::new(("video-editor-quality", u64::from(level)))
                            .label(quality_label(level))
                            .small()
                            .when(edits.quality == level, |button| button.primary())
                            .when(edits.quality != level, |button| button.ghost())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.set_video_editor_quality(level, cx);
                            })),
                    );
                }
                body = body.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(div().text_xs().text_color(muted).child("Quality"))
                        .child(choices),
                );
            }
            if gif_allowed {
                body = body.child(
                    gpui_kit::component::checkbox::Checkbox::new("video-editor-gif")
                        .label("Send as GIF (no sound)")
                        .checked(edits.gif)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.toggle_video_editor_gif(cx);
                        })),
                );
            }
        }
        if let Some(note) = &editor.note {
            body = body.child(div().text_sm().text_color(muted).child(note.clone()));
        }
        let ready = editor.info.is_some() && !editor.busy;
        body.child(
            div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("video-editor-cancel")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_video_editor(cx);
                            this.close_kit_dialog_if_done(DialogKind::VideoEditor, window, cx);
                        })),
                )
                .child(
                    Button::new("video-editor-done")
                        .label("Done")
                        .primary()
                        .disabled(!ready)
                        .on_click(cx.listener(|this, _, window, cx| {
                            if this.finish_video_editor(window, cx) {
                                this.close_kit_dialog_if_done(DialogKind::VideoEditor, window, cx);
                            }
                        })),
                ),
        )
        .into_any_element()
    }
}

/// "20% to 80%" for a crop side, "Whole frame" without one.
fn crop_label(side: Option<(u16, u16)>) -> String {
    match side {
        Some((start, len)) if len < 1000 => {
            format!("{}% to {}%", start / 10, (start + len) / 10)
        }
        _ => "Whole frame".into(),
    }
}

/// The cover frame as a share of the kept range (0 to 1).
fn cover_fraction(edits: &VideoEdits) -> f32 {
    let length = edits.length_ms();
    if length <= 0 {
        0.0
    } else {
        ((edits.cover_ms - edits.from_ms) as f64 / length as f64).clamp(0.0, 1.0) as f32
    }
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
    use super::{cover_fraction, is_editor_copy, mp4_name};
    use quill::video_edit::VideoEdits;
    use std::path::Path;

    #[test]
    fn the_cover_slider_follows_the_kept_range() {
        let edits = VideoEdits {
            from_ms: 2_000,
            till_ms: 6_000,
            cover_ms: 3_000,
            quality: 0,
            gif: false,
            ..VideoEdits::full(0)
        };
        assert!((cover_fraction(&edits) - 0.25).abs() < 1e-6);
        assert_eq!(cover_fraction(&VideoEdits::full(0)), 0.0);
    }

    #[test]
    fn an_encoded_copy_is_an_mp4() {
        assert_eq!(mp4_name("holiday.mov"), "holiday.mp4");
        assert_eq!(mp4_name(""), "video.mp4");
    }

    #[test]
    fn only_files_in_the_editor_folder_are_replaced() {
        let dir = std::env::temp_dir().join("quill-video-edits");
        assert!(is_editor_copy(&dir.join("edit-1.mp4")));
        assert!(!is_editor_copy(Path::new("/home/me/clip.mp4")));
        assert!(!is_editor_copy(Path::new("clip.mp4")));
    }
}
