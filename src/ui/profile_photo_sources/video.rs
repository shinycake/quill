//! The video avatar panel (tdesktop `PrepareProfileVideo` and its video
//! editor): pick a clip, trim it to 1 to 9.6 s, crop a square and choose
//! the cover frame. The preview is the frame as it will be cut.

use super::{PhotoSources, QuillApp, run_ffmpeg, work_dir};
use gpui_kit::component::button::*;
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::profile_photo_video::{
    MAX_ZOOM, SquareCrop, Trim, VideoAvatarPlan, encode_args, preview_frame_args, probe_clip,
};
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Knob {
    Range,
    Cover,
    Zoom,
    X,
    Y,
}

/// A picked clip and the edits on it.
pub(crate) struct VideoDraft {
    path: PathBuf,
    plan: VideoAvatarPlan,
    trim: Trim,
    crop: SquareCrop,
    /// Cover frame, 0 to 1 over the trimmed part.
    cover: f32,
    preview: Option<PathBuf>,
    range: Entity<SliderState>,
    cover_slider: Entity<SliderState>,
    zoom: Entity<SliderState>,
    x: Entity<SliderState>,
    y: Entity<SliderState>,
}

impl QuillApp {
    pub(super) fn choose_avatar_video(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose Video".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picker.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update_in(cx, |this, window, cx| {
                    this.load_avatar_video(path, window, cx);
                });
            }
        })
        .detach();
    }

    fn load_avatar_video(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let Some(sources) = self.sources_mut() else {
            return;
        };
        sources.busy = true;
        sources.note = Some("Reading the video…".into());
        sources.serial += 1;
        let serial = sources.serial;
        let probed = path.clone();
        let worker = cx.background_spawn(async move {
            let (duration, width, height) =
                probe_clip(&probed).ok_or("This isn't a video Quill can read.")?;
            VideoAvatarPlan::new(duration, width, height).map_err(|e| e.message().to_string())
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = worker.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.sources().map(|s| s.serial) != Some(serial) {
                    return;
                }
                match result {
                    Ok(plan) => this.start_avatar_editor(path, plan, window, cx),
                    Err(message) => this.photo_source_note(message, cx),
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn knob(
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
                this.avatar_knob(knob, value.start(), value.end(), release, window, cx);
            },
        )
        .detach();
        entity
    }

    fn start_avatar_editor(
        &mut self,
        path: PathBuf,
        plan: VideoAvatarPlan,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let trim = plan.default_trim();
        let range = self.knob(
            Knob::Range,
            SliderState::new()
                .min(0.0)
                .max(plan.duration as f32)
                .step(0.1)
                .default_value((trim.start as f32, trim.end as f32)),
            window,
            cx,
        );
        let unit = |value: f32| {
            SliderState::new()
                .min(0.0)
                .max(1.0)
                .step(0.001)
                .default_value(value)
        };
        let cover_slider = self.knob(Knob::Cover, unit(0.0), window, cx);
        let x = self.knob(Knob::X, unit(0.5), window, cx);
        let y = self.knob(Knob::Y, unit(0.5), window, cx);
        let zoom = self.knob(
            Knob::Zoom,
            SliderState::new()
                .min(1.0)
                .max(MAX_ZOOM)
                .step(0.01)
                .default_value(1.0),
            window,
            cx,
        );
        if let Some(sources) = self.sources_mut() {
            sources.video = Some(VideoDraft {
                path,
                plan,
                trim,
                crop: SquareCrop::default(),
                cover: 0.0,
                preview: None,
                range,
                cover_slider,
                zoom,
                x,
                y,
            });
            sources.busy = false;
            sources.note = None;
        }
        self.refresh_avatar_preview(cx);
    }

    /// A slider moved. While dragging only the labels follow; on release
    /// the range snaps to the limits and the preview is drawn again.
    fn avatar_knob(
        &mut self,
        knob: Knob,
        start: f32,
        end: f32,
        release: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(video) = self.sources_mut().and_then(|s| s.video.as_mut()) else {
            return;
        };
        match knob {
            Knob::Range => {
                let moved_start = (f64::from(start) - video.trim.start).abs() > 1e-3;
                video.trim = video
                    .plan
                    .clamp_trim(f64::from(start), f64::from(end), moved_start);
                if release {
                    let snapped = (video.trim.start as f32, video.trim.end as f32);
                    video
                        .range
                        .update(cx, |state, cx| state.set_value(snapped, window, cx));
                }
            }
            Knob::Cover => video.cover = end.clamp(0.0, 1.0),
            Knob::Zoom => video.crop.zoom = end,
            Knob::X => video.crop.x = end,
            Knob::Y => video.crop.y = end,
        }
        if release {
            self.refresh_avatar_preview(cx);
        } else {
            cx.notify();
        }
    }

    /// Draw the cover frame as the avatar will be cut.
    fn refresh_avatar_preview(&mut self, cx: &mut Context<Self>) {
        let Some(sources) = self.sources_mut() else {
            return;
        };
        sources.serial += 1;
        let serial = sources.serial;
        let Some(video) = sources.video.as_ref() else {
            return;
        };
        let at = video.trim.source_time(video.trim.main_frame(video.cover));
        let rect = video.crop.rect(video.plan.width, video.plan.height);
        let output = work_dir().join(format!("frame-{serial}.jpg"));
        let args = preview_frame_args(
            &video.path.to_string_lossy(),
            at,
            rect,
            &output.to_string_lossy(),
        );
        let log = work_dir().join("frame.log");
        let worker = cx.background_spawn(async move { run_ffmpeg(&args, &log) });
        cx.spawn(async move |this, cx| {
            let result = worker.await;
            let _ = this.update(cx, |this, cx| {
                if this.sources().map(|s| s.serial) != Some(serial) {
                    return;
                }
                match result {
                    Ok(()) => {
                        if let Some(video) = this.sources_mut().and_then(|s| s.video.as_mut())
                            && let Some(old) = video.preview.replace(output)
                        {
                            let _ = std::fs::remove_file(old);
                        }
                        cx.notify();
                    }
                    Err(message) => this.photo_source_note(message, cx),
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Encode the cut, then send it with the cover frame as the main frame
    /// (`inputChatPhotoAnimation`).
    fn use_avatar_video(&mut self, cx: &mut Context<Self>) {
        let Some(sources) = self.sources_mut() else {
            return;
        };
        let Some(video) = sources.video.as_ref() else {
            return;
        };
        if sources.busy {
            return;
        }
        let main_frame = video.trim.main_frame(video.cover);
        let rect = video.crop.rect(video.plan.width, video.plan.height);
        let input = video.path.to_string_lossy().into_owned();
        let trim = video.trim;
        sources.busy = true;
        sources.note = Some("Preparing the video…".into());
        sources.serial += 1;
        let serial = sources.serial;
        let output = work_dir().join(format!("avatar-{serial}.mp4"));
        let args = encode_args(&input, &output.to_string_lossy(), trim, rect);
        let log = work_dir().join("encode.log");
        let worker = cx.background_spawn(async move { run_ffmpeg(&args, &log) });
        cx.spawn(async move |this, cx| {
            let result = worker.await;
            let _ = this.update(cx, |this, cx| {
                if this.sources().map(|s| s.serial) != Some(serial) {
                    return;
                }
                match result {
                    Ok(()) => {
                        let path = output.to_string_lossy().into_owned();
                        this.finish_photo_source(
                            |driver| driver.set_profile_photo_video(&path, main_frame),
                            cx,
                        );
                    }
                    Err(message) => this.photo_source_note(message, cx),
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn video_panel(&self, sources: &PhotoSources, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let mut panel = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().font_semibold().child("Use a video as your photo"));
        let Some(video) = &sources.video else {
            panel = panel
                .child(div().text_sm().text_color(muted).child(
                    "Pick a video of at least 1 second. You can trim it to up to 9.6 \
                         seconds, crop a square and choose the cover frame. The sound is removed.",
                ))
                .child(
                    div().flex().gap_2().child(
                        Button::new("profile-photo-pick-video")
                            .label("Choose video…")
                            .disabled(sources.busy)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.choose_avatar_video(window, cx);
                            })),
                    ),
                );
            return panel
                .child(self.source_buttons("Set photo", false, |_, _| {}, cx))
                .into_any_element();
        };
        let picture = div()
            .size(px(160.))
            .flex_none()
            .rounded_full()
            .bg(theme.muted)
            .children(video.preview.as_ref().map(|path| {
                img(path.clone())
                    .size(px(160.))
                    .rounded_full()
                    .object_fit(ObjectFit::Cover)
            }));
        let row = |caption: &'static str, value: String, slider: &Entity<SliderState>| {
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
        };
        let trim = video.trim;
        panel = panel
            .child(div().flex().justify_center().child(picture))
            .child(row(
                "Trim",
                format!(
                    "{:.1} s to {:.1} s ({:.1} s)",
                    trim.start,
                    trim.end,
                    trim.length()
                ),
                &video.range,
            ))
            .child(row(
                "Cover frame",
                format!("{:.1} s", trim.main_frame(video.cover)),
                &video.cover_slider,
            ))
            .child(row("Zoom", format!("{:.1}x", video.crop.zoom), &video.zoom))
            .child(row(
                "Crop left to right",
                format!("{:.0}%", video.crop.x * 100.0),
                &video.x,
            ))
            .child(row(
                "Crop top to bottom",
                format!("{:.0}%", video.crop.y * 100.0),
                &video.y,
            ))
            .child(
                div().flex().gap_2().child(
                    Button::new("profile-photo-pick-video")
                        .label("Choose another…")
                        .ghost()
                        .disabled(sources.busy)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.choose_avatar_video(window, cx);
                        })),
                ),
            );
        panel
            .child(self.source_buttons(
                "Set photo",
                !sources.busy,
                |this, cx| this.use_avatar_video(cx),
                cx,
            ))
            .into_any_element()
    }
}
