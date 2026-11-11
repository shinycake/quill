//! The camera panel (tdesktop `CameraBox`): a live, mirrored square
//! preview, then "Take photo" keeps the frame on screen.

use super::{PhotoSources, QuillApp, render_rgba, replace_image, work_dir};
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::profile_photo_camera::{CameraStream, LIVE_FPS, LIVE_SIDE};
use std::path::PathBuf;
use std::sync::Arc;

/// The live preview and the photo taken from it.
#[derive(Default)]
pub(crate) struct CameraDraft {
    stream: Option<CameraStream>,
    /// Serial of the newest frame drawn.
    drawn: u64,
    /// The newest frame as RGB24 bytes, for "Take photo".
    rgb: Vec<u8>,
    live: Option<Arc<RenderImage>>,
    /// A photo was taken: the frozen picture and its file.
    shot: Option<(PathBuf, Arc<RenderImage>)>,
    starting: bool,
}

impl Drop for CameraDraft {
    fn drop(&mut self) {
        if let Some(image) = self.live.take() {
            super::super::image_budget::retire_all([image]);
        }
        if let Some((_, image)) = self.shot.take() {
            super::super::image_budget::retire_all([image]);
        }
    }
}

impl QuillApp {
    /// Open the camera on a worker thread (some systems list devices
    /// first) and keep the stream once it runs.
    pub(super) fn start_camera(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.refused_while_locked() {
            return;
        }
        let Some(sources) = self.sources_mut() else {
            return;
        };
        sources.camera = CameraDraft::default();
        sources.camera.starting = true;
        sources.note = None;
        sources.serial += 1;
        let serial = sources.serial;
        let target = self.photo_target();
        let log = work_dir().join("camera.log");
        let choice = self.capture_choice();
        let worker = cx.background_spawn(async move { CameraStream::start(&log, &choice) });
        self.spawn_screen_in(window, cx, async move |this, cx| {
            let result = worker.await;
            let _ = this.update(cx, |this, cx| {
                // The serial counts per `PhotoSources`: the answer is for
                // the sources of the box that asked.
                if this.photo_target() != target || this.sources().map(|s| s.serial) != Some(serial)
                {
                    return;
                }
                match result {
                    Ok(stream) => {
                        if let Some(sources) = this.sources_mut() {
                            sources.camera.stream = Some(stream);
                            sources.camera.starting = false;
                        }
                        cx.notify();
                    }
                    Err(message) => {
                        if let Some(sources) = this.sources_mut() {
                            sources.camera.starting = false;
                        }
                        this.photo_source_note(message, cx);
                    }
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Take the newest frame and keep the clock running while the preview
    /// is live (one redraw per camera frame, no looping animation).
    pub(super) fn camera_prepare_render(&mut self, cx: &mut Context<Self>) {
        let Some(sources) = self.sources_mut() else {
            return;
        };
        let camera = &mut sources.camera;
        let Some(stream) = camera.stream.as_ref() else {
            return;
        };
        if let Some((serial, rgb)) = stream.frame_after(camera.drawn) {
            camera.drawn = serial;
            let mut rgba = Vec::with_capacity(rgb.len() / 3 * 4);
            for pixel in rgb.as_chunks::<3>().0 {
                rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
            }
            camera.rgb = rgb;
            replace_image(&mut camera.live, render_rgba(LIVE_SIDE, LIVE_SIDE, rgba));
        }
        let ended = stream.finished();
        let no_picture = camera.drawn == 0;
        let failure = (ended && no_picture).then(|| stream.failure());
        if let Some(message) = failure {
            camera.stream = None;
            sources.note = Some(message);
        } else if ended {
            camera.stream = None;
            sources.note = Some(tr(keys::Q_PEOPLE_THE_CAMERA_STOPPED).into());
        } else {
            self.request_animation_tick(LIVE_FPS, cx);
        }
    }

    /// Keep the frame on screen as a JPEG and release the camera.
    fn take_camera_photo(&mut self, cx: &mut Context<Self>) {
        let Some(sources) = self.sources_mut() else {
            return;
        };
        let camera = &mut sources.camera;
        if camera.rgb.len() != quill::profile_photo_camera::frame_len(LIVE_SIDE)
            || camera.live.is_none()
            || sources.busy
        {
            return;
        }
        sources.busy = true;
        sources.serial += 1;
        let serial = sources.serial;
        let rgb = camera.rgb.clone();
        // The frame kept is the one on screen: the stream stops now, so no
        // newer frame replaces the picture the JPEG is saved from.
        camera.stream = None;
        let owner = self.photo_target();
        // Removed unless the shot is taken (`camera.shot`), also when the
        // screen is gone before the save answers.
        let output =
            quill::scratch::OwnedFile::new(work_dir().join(format!("camera-{serial}.jpg")));
        let target = output.path().to_path_buf();
        let worker = cx.background_spawn(async move {
            let frame = image::RgbImage::from_raw(LIVE_SIDE, LIVE_SIDE, rgb)
                .ok_or(tr(keys::Q_PEOPLE_THE_CAMERA_PICTURE_WAS_DAMAGED))?;
            frame
                .save_with_format(&target, image::ImageFormat::Jpeg)
                .map_err(|e| e.to_string())
        });
        self.spawn_screen(cx, async move |this, cx| {
            let result = worker.await;
            let _ = this.update(cx, |this, cx| {
                if this.photo_target() != owner || this.sources().map(|s| s.serial) != Some(serial)
                {
                    return;
                }
                match result {
                    Ok(()) => {
                        if let Some(sources) = this.sources_mut() {
                            let camera = &mut sources.camera;
                            if let Some(frozen) = camera.live.take() {
                                camera.shot = Some((output.keep(), frozen));
                            }
                            sources.busy = false;
                        }
                        cx.notify();
                    }
                    Err(message) => {
                        if let Some(sources) = this.sources_mut() {
                            sources.busy = false;
                        }
                        this.photo_source_note(message, cx);
                    }
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn use_camera_photo(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self
            .sources()
            .and_then(|s| s.camera.shot.as_ref().map(|(path, _)| path.clone()))
        else {
            return;
        };
        let Some(target) = self.photo_target() else {
            return;
        };
        // The shot goes through the crop, as in tdesktop's camera box.
        self.close_photo_source(cx);
        self.open_profile_photo_crop_file(target, path, cx);
    }

    pub(super) fn camera_panel(
        &self,
        sources: &PhotoSources,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let camera = &sources.camera;
        let taken = camera.shot.is_some();
        // A frame frozen by a Take photo whose save failed: only a new
        // camera run can take another.
        let stopped = camera.live.is_some()
            && camera.stream.is_none()
            && !camera.starting
            && !sources.busy
            && !taken;
        let picture = camera
            .shot
            .as_ref()
            .map(|(_, image)| image.clone())
            .or_else(|| camera.live.clone());
        let status = if picture.is_some() {
            None
        } else if camera.starting || camera.stream.is_some() {
            Some(tr(keys::Q_PEOPLE_OPENING_THE_CAMERA))
        } else {
            Some(tr(keys::Q_PEOPLE_NO_CAMERA_PICTURE))
        };
        let circle = self
            .round_picture(200., picture, cx)
            .flex()
            .items_center()
            .justify_center()
            .children(status.map(|text| {
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(text)
                    .into_any_element()
            }));
        let can_take = camera.live.is_some() && camera.stream.is_some() && !sources.busy;
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().font_semibold().child(tr(keys::Q_PEOPLE_TAKE_A_PHOTO)))
            .child(div().flex().justify_center().child(circle))
            .child(div().flex().justify_center().child(if taken || stopped {
                Button::new("profile-photo-retake")
                    .label(tr(keys::Q_PEOPLE_RETAKE))
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.start_camera(window, cx);
                    }))
            } else {
                Button::new("profile-photo-snap")
                    .label(tr(keys::Q_PEOPLE_TAKE_PHOTO))
                    .disabled(!can_take)
                    .on_click(cx.listener(|this, _, _, cx| this.take_camera_photo(cx)))
            }))
            .child(self.source_buttons(
                tr(keys::PROFILE_SET_PHOTO_BUTTON),
                taken && !sources.busy,
                |this, cx| this.use_camera_photo(cx),
                cx,
            ))
            .into_any_element()
    }
}
