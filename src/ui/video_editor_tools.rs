//! Crop, rotate, flip and preview playback for the video editor (tdesktop
//! `VideoEditor`: its crop, `_rotate`, `_flip` and the looping playback of
//! the trimmed range). The dialog and the trim and cover controls are in
//! `video_editor.rs`.
//!
//! The picture is the frame turned and flipped like the edit; the crop is a
//! frame drawn over it, set with two range sliders. Playback shows the
//! source through `NativeVideo`, turned for the Rotate choice; a flip shows
//! in the still picture only.

use super::app::QuillApp;
use super::native_video::{NativeVideo, Purpose, supported};
use super::video_editor::{Knob, VideoEditorDialog};
use gpui_kit::component::button::*;
use gpui_kit::component::slider::{Slider, SliderState};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::video_edit::{Crop, quality_levels, turned_size};

/// The stage the picture is fitted into.
const STAGE_W: f32 = 480.0;
const STAGE_H: f32 = 240.0;

/// The crop sliders' positions (0 to 1) for a crop.
fn crop_ranges(crop: Option<Crop>) -> ((f32, f32), (f32, f32)) {
    let crop = crop.unwrap_or(Crop::FULL);
    let unit = |start: u16, len: u16| (f32::from(start) / 1000.0, f32::from(start + len) / 1000.0);
    (unit(crop.x, crop.w), unit(crop.y, crop.h))
}

impl QuillApp {
    /// A crop slider moved: the crop follows while dragging and snaps to its
    /// limits on release.
    pub(super) fn video_editor_crop_knob(
        &mut self,
        knob: Knob,
        start: f32,
        end: f32,
        release: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() else {
            return;
        };
        let to_thousandths = |value: f32| (value.clamp(0.0, 1.0) * 1000.0).round() as i32;
        let (from, till) = (to_thousandths(start), to_thousandths(end));
        let mut crop = editor.edits.crop.unwrap_or(Crop::FULL);
        let (len, from16) = ((till - from).max(0) as u16, from as u16);
        match knob {
            Knob::CropX => {
                crop.x = from16;
                crop.w = len;
            }
            Knob::CropY => {
                crop.y = from16;
                crop.h = len;
            }
            _ => return,
        }
        let crop = crop.clamped();
        editor.edits.crop = (crop != Crop::FULL).then_some(crop);
        if release {
            let ((x0, x1), (y0, y1)) = crop_ranges(editor.edits.crop);
            let (sx, sy) = (editor.crop_x.clone(), editor.crop_y.clone());
            if let (Some(sx), Some(sy)) = (sx, sy) {
                sx.update(cx, |state, cx| state.set_value((x0, x1), window, cx));
                sy.update(cx, |state, cx| state.set_value((y0, y1), window, cx));
            }
            Self::fit_editor_quality(editor);
        }
        cx.notify();
    }

    /// A quality the cropped clip no longer offers goes back to Original.
    fn fit_editor_quality(editor: &mut VideoEditorDialog) {
        if let Some(info) = editor.info {
            let (width, height) = editor.edits.edited_size(&info);
            if !quality_levels(width, height).contains(&editor.edits.quality) {
                editor.edits.quality = 0;
            }
        }
    }

    /// Turn the picture a quarter turn clockwise. The crop starts over, as
    /// it belongs to the turned frame.
    pub(super) fn rotate_video_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() else {
            return;
        };
        editor.edits.turns = (editor.edits.turns + 1) % 4;
        editor.edits.crop = None;
        for slider in [editor.crop_x.clone(), editor.crop_y.clone()]
            .into_iter()
            .flatten()
        {
            slider.update(cx, |state, cx| state.set_value((0.0, 1.0), window, cx));
        }
        Self::fit_editor_quality(editor);
        if let Some(video) = editor.player.borrow_mut().as_mut() {
            video.set_rotation(editor.edits.turns);
        }
        let at = editor.edits.cover_ms;
        self.refresh_video_preview(at, cx);
    }

    /// Mirror the picture left to right.
    pub(super) fn flip_video_editor(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() else {
            return;
        };
        editor.edits.flipped = !editor.edits.flipped;
        let at = editor.edits.cover_ms;
        self.refresh_video_preview(at, cx);
    }

    /// Play the trimmed range in a loop, or stop.
    pub(super) fn toggle_video_editor_playback(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.composer_ui.send_box.video_editor.as_mut() else {
            return;
        };
        if editor.playing {
            editor.playing = false;
            *editor.player.borrow_mut() = None;
            cx.notify();
            return;
        }
        match NativeVideo::open(&editor.source, Purpose::Viewer) {
            Ok(mut video) => {
                video.set_rotation(editor.edits.turns);
                video.seek(editor.edits.from_ms as f64 / 1000.0);
                video.play();
                *editor.player.borrow_mut() = Some(video);
                editor.playing = true;
            }
            Err(_) => editor.note = Some("Quill can't play this video here.".into()),
        }
        cx.notify();
    }

    /// Playback is available on this platform.
    pub(super) fn video_editor_can_play() -> bool {
        supported()
    }

    /// The stage: the picture (a playing frame or the still), with the crop
    /// frame over it.
    pub(super) fn video_editor_stage(
        &self,
        editor: &VideoEditorDialog,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (stage_bg, accent) = (cx.theme().muted, cx.theme().primary);
        let (width, height) = editor.info.map_or((16, 9), |info| {
            turned_size(info.width, info.height, editor.edits.turns)
        });
        let scale = (STAGE_W / width as f32).min(STAGE_H / height as f32);
        let (shown_w, shown_h) = (px(width as f32 * scale), px(height as f32 * scale));
        let mut picture: Option<AnyElement> = None;
        if editor.playing {
            // The guard lives only for the player calls: it is dropped
            // before the frame clock is asked for a tick, and a re-entrant
            // borrow skips this frame (the still shows) instead of aborting.
            let frame = editor.player.try_borrow_mut().ok().and_then(|mut slot| {
                let video = slot.as_mut().filter(|video| video.error().is_none())?;
                let from = editor.edits.from_ms as f64 / 1000.0;
                let till = editor.edits.till_ms as f64 / 1000.0;
                if video.position_secs() >= till {
                    video.seek(from);
                }
                if !video.is_playing() {
                    video.play();
                }
                video.frame()
            });
            picture = frame
                .map(|frame| frame.element(shown_w, shown_h, ObjectFit::Fill, Corners::default()));
            self.request_animation_tick(30, cx);
        }
        let picture = picture.or_else(|| {
            editor.preview.as_ref().map(|path| {
                img(path.clone())
                    .w(shown_w)
                    .h(shown_h)
                    .object_fit(ObjectFit::Fill)
                    .into_any_element()
            })
        });
        let mut frame = div().relative().w(shown_w).h(shown_h).children(picture);
        if let Some(crop) = editor.edits.crop {
            frame = frame.child(
                div()
                    .absolute()
                    .left(relative(f32::from(crop.x) / 1000.0))
                    .top(relative(f32::from(crop.y) / 1000.0))
                    .w(relative(f32::from(crop.w) / 1000.0))
                    .h(relative(f32::from(crop.h) / 1000.0))
                    .border_2()
                    .border_color(accent),
            );
        }
        div()
            .h(px(STAGE_H))
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .bg(stage_bg)
            .overflow_hidden()
            .child(frame)
            .into_any_element()
    }

    /// Play, Rotate and Flip.
    pub(super) fn video_editor_toolbar(
        &self,
        editor: &VideoEditorDialog,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut bar = div().flex().items_center().gap_1();
        if Self::video_editor_can_play() {
            bar = bar.child(
                Button::new("video-editor-play")
                    .icon(if editor.playing {
                        gpui_kit::assets::IconName::Pause
                    } else {
                        gpui_kit::assets::IconName::Play
                    })
                    .label(if editor.playing { "Pause" } else { "Play" })
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_video_editor_playback(cx);
                    })),
            );
        }
        bar.child(
            Button::new("video-editor-rotate")
                .icon(gpui_kit::assets::IconName::RotateCw)
                .label("Rotate")
                .small()
                .ghost()
                .on_click(cx.listener(|this, _, window, cx| {
                    this.rotate_video_editor(window, cx);
                })),
        )
        .child(
            Button::new("video-editor-flip")
                .label("Flip")
                .small()
                .ghost()
                .selected(editor.edits.flipped)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.flip_video_editor(cx);
                })),
        )
        .into_any_element()
    }

    /// A captioned slider row.
    pub(super) fn video_editor_slider_row(
        &self,
        caption: &'static str,
        value: String,
        slider: &Entity<SliderState>,
        cx: &mut Context<Self>,
    ) -> Div {
        let muted = cx.theme().muted_foreground;
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .text_xs()
                    .text_color(muted)
                    .child(caption)
                    .child(value),
            )
            .child(Slider::new(slider))
    }
}

#[cfg(test)]
mod tests {
    use super::crop_ranges;
    use quill::video_edit::Crop;

    #[test]
    fn crop_sliders_start_on_the_whole_frame() {
        assert_eq!(crop_ranges(None), ((0.0, 1.0), (0.0, 1.0)));
        let crop = Crop {
            x: 250,
            y: 100,
            w: 500,
            h: 800,
        };
        assert_eq!(crop_ranges(Some(crop)), ((0.25, 0.75), (0.1, 0.9)));
    }
}
