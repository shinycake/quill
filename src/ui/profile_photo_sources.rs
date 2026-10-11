//! Profile photo sources in the edit-profile dialog (tdesktop
//! `UserpicButton` menu): a camera snapshot, the emoji avatar builder and
//! a video avatar whose main frame is picked on a scrubber. Each opens a
//! panel in place of the dialog's normal body; the request bodies and the
//! ffmpeg command lines are in `quill::profile_photo_sources`.

use super::app::QuillApp;
use super::pressable::PressableDiv;
use gpui_kit::component::button::*;
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::local_path::sandboxed_display_path;
use quill::profile_photo_sources::{
    ClipInfo, PRESET_COUNT, VideoAvatarPlan, camera_snapshot_args, encode_args, gradient_preset,
    preview_frame_args, probe_clip, video_only_input,
};
use std::path::{Path, PathBuf};
use std::process::Stdio;

/// Which source panel the dialog shows.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum SourceMode {
    /// The dialog's normal body.
    #[default]
    Closed,
    Emoji,
    Video,
    Camera,
}

/// A picked clip and where its scrubber is.
pub(crate) struct VideoDraft {
    path: PathBuf,
    plan: VideoAvatarPlan,
    slider: Entity<SliderState>,
    fraction: f32,
    preview: Option<PathBuf>,
}

/// State of the source panels. Lives in the edit-profile dialog.
#[derive(Default)]
pub(crate) struct PhotoSources {
    mode: SourceMode,
    preset: usize,
    emoji_id: i64,
    video: Option<VideoDraft>,
    shot: Option<PathBuf>,
    busy: bool,
    note: Option<String>,
    /// Bumped per temp file so a new picture never reuses a cached path
    /// and a late result for an older one is dropped.
    serial: u32,
}

fn work_dir() -> PathBuf {
    let dir = std::env::temp_dir().join("quill-profile-photo");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Run ffmpeg to completion; the error is a short sentence for the panel.
fn run_ffmpeg(args: &[String], log: &Path) -> Result<(), String> {
    if !quill::media_tools::is_installed("ffmpeg") {
        return Err(quill::media_tools::ffmpeg_missing_message("Video avatars"));
    }
    let log_file = std::fs::File::create(log).map_err(|e| e.to_string())?;
    let status = quill::media_tools::command("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error"])
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log_file)
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(quill::media_tools::capture_failure(log, true))
    }
}

impl QuillApp {
    fn sources_mut(&mut self) -> Option<&mut PhotoSources> {
        self.dialogs
            .edit_profile_dialog
            .as_mut()
            .map(|dialog| &mut dialog.sources)
    }

    fn sources(&self) -> Option<&PhotoSources> {
        self.dialogs
            .edit_profile_dialog
            .as_ref()
            .map(|dialog| &dialog.sources)
    }

    /// Called as the dialog renders: fetch the builder's emoji thumbnails.
    pub(super) fn photo_sources_prepare_render(&mut self) {
        if self.sources().is_some_and(|s| s.mode == SourceMode::Emoji)
            && let Some(live) = self.live.as_mut()
        {
            live.driver.download_profile_photo_emoji_icons();
        }
    }

    pub(super) fn open_photo_source(&mut self, mode: SourceMode, cx: &mut Context<Self>) {
        if mode == SourceMode::Emoji
            && let Some(live) = self.live.as_mut()
        {
            let _ = live.driver.load_profile_photo_emoji_icons();
        }
        if let Some(sources) = self.sources_mut() {
            sources.mode = mode;
            sources.note = None;
            sources.busy = false;
        }
        cx.notify();
    }

    fn close_photo_source(&mut self, cx: &mut Context<Self>) {
        if let Some(sources) = self.sources_mut() {
            sources.mode = SourceMode::Closed;
            sources.video = None;
            sources.shot = None;
            sources.note = None;
            sources.busy = false;
            sources.serial += 1;
        }
        cx.notify();
    }

    fn photo_source_note(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        if let Some(sources) = self.sources_mut() {
            sources.note = Some(text.into());
            sources.busy = false;
        }
        cx.notify();
    }

    /// Send a request and close the panel; a failure stays on the panel.
    fn finish_photo_source(
        &mut self,
        send: impl FnOnce(
            &mut quill::connect::ConnectDriver<quill::connect::LiveSender>,
        ) -> Result<quill::ids::RequestId, quill::connect::ConnectSendError>,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.live.as_mut() else {
            self.photo_source_note("Profile edits need a signed-in account.", cx);
            return;
        };
        live.driver.session.users_state.profile_edit_error = None;
        match send(&mut live.driver) {
            Ok(_) => {
                self.connection.status_note = "Photo update requested.".into();
                self.close_photo_source(cx);
            }
            Err(err) => self.photo_source_note(format!("Couldn't set the photo: {err:?}"), cx),
        }
    }

    // Emoji.

    fn use_emoji_avatar(&mut self, cx: &mut Context<Self>) {
        let Some((id, preset)) = self.sources().map(|s| (s.emoji_id, s.preset)) else {
            return;
        };
        let colors = gradient_preset(preset);
        self.finish_photo_source(|driver| driver.set_profile_photo_emoji(id, &colors), cx);
    }

    // Camera.

    fn take_camera_photo(&mut self, cx: &mut Context<Self>) {
        let Some(sources) = self.sources_mut() else {
            return;
        };
        if sources.busy {
            return;
        }
        sources.busy = true;
        sources.note = None;
        sources.serial += 1;
        let serial = sources.serial;
        let output = work_dir().join(format!("camera-{serial}.jpg"));
        let log = work_dir().join("camera.log");
        let target = output.clone();
        let worker = cx.background_spawn(async move {
            let input = quill::media_tools::capture_input()?;
            let args =
                camera_snapshot_args(&video_only_input(&input.args), &target.to_string_lossy());
            run_ffmpeg(&args, &log)
        });
        cx.spawn(async move |this, cx| {
            let result = worker.await;
            let _ = this.update(cx, |this, cx| {
                if this.sources().map(|s| s.serial) != Some(serial) {
                    return;
                }
                match result {
                    Ok(()) if output.is_file() => {
                        if let Some(sources) = this.sources_mut() {
                            sources.shot = Some(output);
                            sources.busy = false;
                        }
                        cx.notify();
                    }
                    Ok(()) => this.photo_source_note("The camera gave no picture.", cx),
                    Err(message) => this.photo_source_note(message, cx),
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn use_camera_photo(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.sources().and_then(|s| s.shot.clone()) else {
            return;
        };
        let path = path.to_string_lossy().into_owned();
        self.finish_photo_source(|driver| driver.set_profile_photo(&path), cx);
    }

    // Video.

    pub(super) fn choose_avatar_video(&mut self, cx: &mut Context<Self>) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose Video".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picker.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update(cx, |this, cx| this.load_avatar_video(path, cx));
            }
        })
        .detach();
    }

    fn load_avatar_video(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let Some(sources) = self.sources_mut() else {
            return;
        };
        sources.busy = true;
        sources.note = None;
        sources.serial += 1;
        let serial = sources.serial;
        let probed = path.clone();
        let worker = cx.background_spawn(async move {
            let info: ClipInfo = probe_clip(&probed).ok_or("This isn't a video Quill can read.")?;
            VideoAvatarPlan::new(info.duration, info.width, info.height)
                .map_err(|e| e.message().to_string())
        });
        cx.spawn(async move |this, cx| {
            let result = worker.await;
            let _ = this.update(cx, |this, cx| {
                if this.sources().map(|s| s.serial) != Some(serial) {
                    return;
                }
                match result {
                    Ok(plan) => this.start_avatar_scrubber(path, plan, cx),
                    Err(message) => this.photo_source_note(message, cx),
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn start_avatar_scrubber(
        &mut self,
        path: PathBuf,
        plan: VideoAvatarPlan,
        cx: &mut Context<Self>,
    ) {
        let slider = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(1.0)
                .step(0.001)
                .default_value(0.0)
        });
        cx.subscribe(&slider, |this, _entity, event: &SliderEvent, cx| {
            if let SliderEvent::Release(value) = event {
                this.scrub_avatar_video(value.end(), cx);
            }
        })
        .detach();
        if let Some(sources) = self.sources_mut() {
            sources.video = Some(VideoDraft {
                path,
                plan,
                slider,
                fraction: 0.0,
                preview: None,
            });
            sources.busy = false;
        }
        self.scrub_avatar_video(0.0, cx);
    }

    /// The scrubber was released at `fraction`: grab that frame.
    fn scrub_avatar_video(&mut self, fraction: f32, cx: &mut Context<Self>) {
        let Some(sources) = self.sources_mut() else {
            return;
        };
        sources.serial += 1;
        let serial = sources.serial;
        let Some(video) = sources.video.as_mut() else {
            return;
        };
        video.fraction = fraction.clamp(0.0, 1.0);
        let seconds = video.plan.main_frame(video.fraction);
        let input = video.path.to_string_lossy().into_owned();
        let output = work_dir().join(format!("frame-{serial}.jpg"));
        let args = preview_frame_args(&input, seconds, &output.to_string_lossy());
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

    /// Encode the clip, then send it with the scrubbed frame as the main
    /// frame (`inputChatPhotoAnimation`).
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
        let plan = video.plan;
        let main_frame = plan.main_frame(video.fraction);
        let input = video.path.to_string_lossy().into_owned();
        sources.busy = true;
        sources.note = Some("Preparing the video…".into());
        sources.serial += 1;
        let serial = sources.serial;
        let output = work_dir().join(format!("avatar-{serial}.mp4"));
        let args = encode_args(&input, &output.to_string_lossy(), plan);
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

    // Panels.

    /// The panel for the open source, or `None` for the normal body.
    pub(super) fn photo_sources_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let sources = self.sources()?;
        let body = match sources.mode {
            SourceMode::Closed => return None,
            SourceMode::Emoji => self.emoji_panel(sources, cx),
            SourceMode::Video => self.video_panel(sources, cx),
            SourceMode::Camera => self.camera_panel(sources, cx),
        };
        let theme = cx.theme();
        let note = sources.note.clone().map(|text| {
            div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(text)
        });
        Some(
            div()
                .id("profile-photo-source")
                .flex()
                .flex_col()
                .gap_3()
                .child(body)
                .children(note)
                .into_any_element(),
        )
    }

    fn source_buttons(
        &self,
        use_label: &'static str,
        can_use: bool,
        on_use: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .gap_2()
            .child(
                Button::new("profile-photo-source-back")
                    .label("Back")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.close_photo_source(cx))),
            )
            .child(
                Button::new("profile-photo-source-use")
                    .label(use_label)
                    .disabled(!can_use)
                    .on_click(cx.listener(move |this, _, _, cx| on_use(this, cx))),
            )
            .into_any_element()
    }

    fn avatar_circle(&self, side: f32, content: AnyElement) -> Div {
        div()
            .size(px(side))
            .flex_none()
            .rounded_full()
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_center()
            .child(content)
    }

    fn emoji_icon_visual(&self, custom_emoji_id: i64, size: f32) -> AnyElement {
        let session = self.session();
        let roots = self.media_display_roots();
        let sticker = session.and_then(|s| {
            s.users_state
                .profile_photo_emoji_icons
                .iter()
                .find(|sticker| sticker.custom_emoji_id == Some(custom_emoji_id))
        });
        let Some(sticker) = sticker else {
            return div().into_any_element();
        };
        let path = sticker
            .display_file_id()
            .and_then(|id| session.and_then(|s| s.media.files.get(&id.0)))
            .and_then(|file| file.usable_path())
            .and_then(|path| sandboxed_display_path(path, &roots));
        match path {
            Some(path) => img(path)
                .w(px(size))
                .h(px(size))
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
            None => div()
                .text_size(px(size * 0.7))
                .child(sticker.emoji.clone())
                .into_any_element(),
        }
    }

    fn emoji_panel(&self, sources: &PhotoSources, cx: &mut Context<Self>) -> AnyElement {
        let colors = gradient_preset(sources.preset);
        let theme = cx.theme();
        let failed = self
            .session()
            .is_some_and(|s| s.users_state.profile_photo_emoji_icons_failed);
        let icons: Vec<(usize, i64, String)> = self
            .session()
            .map(|s| {
                s.users_state
                    .profile_photo_emoji_icons
                    .iter()
                    .enumerate()
                    .filter_map(|(ix, st)| Some((ix, st.custom_emoji_id?, st.emoji.clone())))
                    .collect()
            })
            .unwrap_or_default();
        let preview = self
            .avatar_circle(
                96.,
                if sources.emoji_id != 0 {
                    self.emoji_icon_visual(sources.emoji_id, 52.)
                } else {
                    div().into_any_element()
                },
            )
            .bg(linear_gradient(
                45.,
                linear_color_stop(rgb(colors[0]), 0.),
                linear_color_stop(rgb(colors[2]), 1.),
            ));
        let mut palette = div().flex().gap_2().justify_center();
        for index in 0..PRESET_COUNT {
            let c = gradient_preset(index);
            let chosen = index == sources.preset;
            palette = palette.child(
                div()
                    .id(("profile-photo-preset", index))
                    .size(px(28.))
                    .rounded_full()
                    .border_2()
                    .border_color(if chosen {
                        theme.primary
                    } else {
                        theme.transparent
                    })
                    .bg(linear_gradient(
                        45.,
                        linear_color_stop(rgb(c[0]), 0.),
                        linear_color_stop(rgb(c[2]), 1.),
                    ))
                    .cursor_pointer()
                    .role(Role::Button)
                    .aria_label(format!("Background {}", index + 1))
                    .tab_index(0)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(s) = this.sources_mut() {
                            s.preset = index;
                        }
                        cx.notify();
                    })),
            );
        }
        let mut grid = div()
            .id("profile-photo-emoji-grid")
            .flex()
            .flex_wrap()
            .gap_1()
            .max_h(px(180.))
            .overflow_y_scroll()
            .role(Role::List)
            .aria_label("Emoji");
        for (ix, id, emoji) in icons {
            let tile = div()
                .id(("profile-photo-emoji", ix))
                .size(px(40.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded_md();
            let tile = if sources.emoji_id == id {
                tile.bg(theme.primary.opacity(0.25))
            } else {
                tile
            };
            grid = grid.child(
                tile.pressable(theme)
                    .role(Role::Button)
                    .aria_label(if emoji.is_empty() {
                        "Emoji".to_string()
                    } else {
                        format!("Emoji {emoji}")
                    })
                    .tab_index(0)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(s) = this.sources_mut() {
                            s.emoji_id = id;
                        }
                        cx.notify();
                    }))
                    .child(self.emoji_icon_visual(id, 28.)),
            );
        }
        let empty = if failed {
            Some("Couldn't load the emoji. Close and reopen this panel to retry.")
        } else if self
            .session()
            .is_none_or(|s| s.users_state.profile_photo_emoji_icons.is_empty())
        {
            Some("Loading emoji…")
        } else {
            None
        };
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().font_semibold().child("Use an emoji as your photo"))
            .child(div().flex().justify_center().child(preview))
            .child(palette)
            .child(grid)
            .children(empty.map(|text| {
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(text)
            }))
            .child(self.source_buttons(
                "Set photo",
                sources.emoji_id != 0,
                |this, cx| this.use_emoji_avatar(cx),
                cx,
            ))
            .into_any_element()
    }

    fn camera_panel(&self, sources: &PhotoSources, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let picture = match &sources.shot {
            Some(path) => img(path.clone())
                .size_full()
                .object_fit(ObjectFit::Cover)
                .into_any_element(),
            None => div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(if sources.busy {
                    "Opening the camera…"
                } else {
                    "No picture yet"
                })
                .into_any_element(),
        };
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().font_semibold().child("Take a photo"))
            .child(
                div()
                    .flex()
                    .justify_center()
                    .child(self.avatar_circle(160., picture).bg(theme.muted)),
            )
            .child(
                div().flex().justify_center().child(
                    Button::new("profile-photo-snap")
                        .label(if sources.shot.is_some() {
                            "Retake"
                        } else {
                            "Take photo"
                        })
                        .disabled(sources.busy)
                        .on_click(cx.listener(|this, _, _, cx| this.take_camera_photo(cx))),
                ),
            )
            .child(self.source_buttons(
                "Set photo",
                sources.shot.is_some() && !sources.busy,
                |this, cx| this.use_camera_photo(cx),
                cx,
            ))
            .into_any_element()
    }

    fn video_panel(&self, sources: &PhotoSources, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let mut panel = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().font_semibold().child("Use a video as your photo"));
        match &sources.video {
            None => {
                panel = panel
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("Pick a video of at least 1 second. Quill uses its first 9.6 seconds, cropped to a square, without sound."),
                    )
                    .child(
                        div().flex().gap_2().child(
                            Button::new("profile-photo-pick-video")
                                .label("Choose video…")
                                .disabled(sources.busy)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.choose_avatar_video(cx);
                                })),
                        ),
                    );
            }
            Some(video) => {
                let picture = match &video.preview {
                    Some(path) => img(path.clone())
                        .size_full()
                        .object_fit(ObjectFit::Cover)
                        .into_any_element(),
                    None => div().into_any_element(),
                };
                let seconds = video.plan.main_frame(video.fraction);
                panel = panel
                    .child(
                        div()
                            .flex()
                            .justify_center()
                            .child(self.avatar_circle(160., picture).bg(theme.muted)),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("Choose the frame shown while the photo isn't playing."),
                    )
                    .child(Slider::new(&video.slider))
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format!("Frame at {seconds:.1} s"))
                            .child(format!("Clip {:.1} s", video.plan.length)),
                    )
                    .child(
                        div().flex().gap_2().child(
                            Button::new("profile-photo-pick-video")
                                .label("Choose another…")
                                .ghost()
                                .disabled(sources.busy)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.choose_avatar_video(cx);
                                })),
                        ),
                    );
            }
        }
        panel
            .child(self.source_buttons(
                "Set photo",
                sources.video.is_some() && !sources.busy,
                |this, cx| this.use_avatar_video(cx),
                cx,
            ))
            .into_any_element()
    }

    /// The three buttons under the Photo section.
    pub(super) fn photo_source_buttons(&self, cx: &mut Context<Self>) -> AnyElement {
        let button =
            |id: &'static str, label: &'static str, mode: SourceMode, cx: &mut Context<Self>| {
                Button::new(id)
                    .label(label)
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| this.open_photo_source(mode, cx)))
            };
        div()
            .flex()
            .flex_wrap()
            .gap_2()
            .child(button(
                "edit-profile-camera",
                "Camera…",
                SourceMode::Camera,
                cx,
            ))
            .child(button(
                "edit-profile-emoji",
                "Use an emoji…",
                SourceMode::Emoji,
                cx,
            ))
            .child(button(
                "edit-profile-video",
                "Video…",
                SourceMode::Video,
                cx,
            ))
            .into_any_element()
    }
}
