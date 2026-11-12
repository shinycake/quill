//! The camera panel (tdesktop `CameraBox`): a live, mirrored square
//! preview, then "Take photo" keeps the frame on screen.

use super::{QuillApp, SourceStage, render_rgba, replace_image, work_dir};
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::profile_photo_camera::{CameraStream, LIVE_FPS, LIVE_SIDE};
use std::path::PathBuf;
use std::sync::Arc;

/// Where the camera panel stands: one value for the start, the live
/// preview, the photo being saved and the photo taken.
#[derive(Default)]
enum CameraStage {
    /// No camera running and no picture: before a start, or after a start
    /// that failed or a camera that stopped before any frame.
    #[default]
    Idle,
    /// The camera is being opened on a worker thread; the stage holds the
    /// task, so leaving it cancels the start.
    Starting { _task: Task<()> },
    /// The stream runs and the newest frame is drawn.
    Live {
        stream: CameraStream,
        /// Serial of the newest frame drawn.
        drawn: u64,
        /// The newest frame as RGB24 bytes, for "Take photo".
        rgb: Vec<u8>,
        /// The newest frame drawn; none before the first one.
        image: Option<Arc<RenderImage>>,
    },
    /// "Take photo" keeps the frame on screen while its JPEG is written.
    Saving {
        frozen: Arc<RenderImage>,
        _task: Task<()>,
    },
    /// A photo was taken: the frozen picture and its file.
    Taken {
        path: PathBuf,
        image: Arc<RenderImage>,
    },
    /// The stream is gone with a frame on screen (the camera stopped, or
    /// the save failed): only a new camera run can take another.
    Stopped { image: Arc<RenderImage> },
}

impl CameraStage {
    /// The picture on screen, if any.
    fn picture(&self) -> Option<Arc<RenderImage>> {
        match self {
            Self::Live { image, .. } => image.clone(),
            Self::Saving { frozen, .. } => Some(frozen.clone()),
            Self::Taken { image, .. } | Self::Stopped { image } => Some(image.clone()),
            Self::Idle | Self::Starting { .. } => None,
        }
    }
}

/// The live preview and the photo taken from it.
#[derive(Default)]
pub(crate) struct CameraDraft {
    stage: CameraStage,
}

impl Drop for CameraDraft {
    fn drop(&mut self) {
        let image = match std::mem::take(&mut self.stage) {
            CameraStage::Live { image, .. } => image,
            CameraStage::Saving { frozen, .. } => Some(frozen),
            CameraStage::Taken { image, .. } | CameraStage::Stopped { image } => Some(image),
            CameraStage::Idle | CameraStage::Starting { .. } => None,
        };
        if let Some(image) = image {
            super::super::image_budget::retire_all([image]);
        }
    }
}

impl QuillApp {
    /// Open the camera on a worker thread (some systems list devices
    /// first) and keep the stream once it runs. The start's task lives in
    /// the `Starting` stage: leaving the stage (Back, the lock, another
    /// start) cancels it, so a late result has nothing to land on.
    pub(super) fn start_camera(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.refused_while_locked() {
            return;
        }
        let Some(sources) = self.sources_mut() else {
            return;
        };
        sources.note = None;
        let target = self.photo_target();
        let log = work_dir().join("camera.log");
        let choice = self.capture_choice();
        let worker = cx.background_spawn(async move { CameraStream::start(&log, &choice) });
        let task = self.spawn_screen_in(window, cx, async move |this, cx| {
            let result = worker.await;
            let _ = this.update(cx, |this, cx| {
                // The answer is for the box that asked, and for the start
                // its stage still waits on.
                if this.photo_target() != target {
                    return;
                }
                let Some(draft) = this.sources_mut().and_then(|s| s.camera_mut()) else {
                    return;
                };
                if !matches!(draft.stage, CameraStage::Starting { .. }) {
                    return;
                }
                match result {
                    Ok(stream) => {
                        draft.stage = CameraStage::Live {
                            stream,
                            drawn: 0,
                            rgb: Vec::new(),
                            image: None,
                        };
                        cx.notify();
                    }
                    Err(message) => {
                        draft.stage = CameraStage::Idle;
                        this.photo_source_note(message, cx);
                    }
                }
            });
        });
        if let Some(sources) = self.sources_mut() {
            sources.stage = SourceStage::Camera(CameraDraft {
                stage: CameraStage::Starting { _task: task },
            });
        }
        cx.notify();
    }

    /// Take the newest frame and keep the clock running while the preview
    /// is live (one redraw per camera frame, no looping animation).
    pub(super) fn camera_prepare_render(&mut self, cx: &mut Context<Self>) {
        let Some(sources) = self.sources_mut() else {
            return;
        };
        let Some(camera) = sources.camera_mut() else {
            return;
        };
        let CameraStage::Live {
            stream,
            drawn,
            rgb,
            image,
        } = &mut camera.stage
        else {
            return;
        };
        if let Some((serial, frame)) = stream.frame_after(*drawn) {
            *drawn = serial;
            let mut rgba = Vec::with_capacity(frame.len() / 3 * 4);
            for pixel in frame.as_chunks::<3>().0 {
                rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
            }
            *rgb = frame;
            replace_image(image, render_rgba(LIVE_SIDE, LIVE_SIDE, rgba));
        }
        let ended = stream.finished();
        let failure = (ended && *drawn == 0).then(|| stream.failure());
        if let Some(message) = failure {
            camera.stage = CameraStage::Idle;
            sources.note = Some(message);
        } else if ended {
            // The camera stopped with a picture on screen.
            camera.stage = match std::mem::take(&mut camera.stage) {
                CameraStage::Live {
                    image: Some(image), ..
                } => CameraStage::Stopped { image },
                _ => CameraStage::Idle,
            };
            sources.note = Some(tr(keys::Q_PEOPLE_THE_CAMERA_STOPPED).into());
        } else {
            self.request_animation_tick(LIVE_FPS, cx);
        }
    }

    /// Keep the newest frame on screen as a JPEG and release the camera.
    /// The save's task lives in the `Saving` stage.
    fn take_camera_photo(&mut self, cx: &mut Context<Self>) {
        static SHOTS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let Some(camera) = self.sources_mut().and_then(|s| s.camera_mut()) else {
            return;
        };
        let CameraStage::Live {
            rgb,
            image: Some(frozen),
            ..
        } = &camera.stage
        else {
            return;
        };
        if rgb.len() != quill::profile_photo_camera::frame_len(LIVE_SIDE) {
            return;
        }
        let (rgb, frozen) = (rgb.clone(), frozen.clone());
        let shot = SHOTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let owner = self.photo_target();
        // Removed unless the shot is taken (`CameraStage::Taken`), also when
        // the screen is gone before the save answers.
        let output = quill::scratch::OwnedFile::new(work_dir().join(format!("camera-{shot}.jpg")));
        let target = output.path().to_path_buf();
        let worker = cx.background_spawn(async move {
            let frame = image::RgbImage::from_raw(LIVE_SIDE, LIVE_SIDE, rgb)
                .ok_or(tr(keys::Q_PEOPLE_THE_CAMERA_PICTURE_WAS_DAMAGED))?;
            frame
                .save_with_format(&target, image::ImageFormat::Jpeg)
                .map_err(|e| e.to_string())
        });
        let task = self.spawn_screen(cx, async move |this, cx| {
            let result = worker.await;
            let _ = this.update(cx, |this, cx| {
                if this.photo_target() != owner {
                    return;
                }
                let Some(camera) = this.sources_mut().and_then(|s| s.camera_mut()) else {
                    return;
                };
                if !matches!(camera.stage, CameraStage::Saving { .. }) {
                    return;
                }
                match result {
                    Ok(()) => {
                        camera.stage = match std::mem::take(&mut camera.stage) {
                            CameraStage::Saving { frozen, .. } => CameraStage::Taken {
                                path: output.keep(),
                                image: frozen,
                            },
                            other => other,
                        };
                        cx.notify();
                    }
                    Err(message) => {
                        camera.stage = match std::mem::take(&mut camera.stage) {
                            CameraStage::Saving { frozen, .. } => {
                                CameraStage::Stopped { image: frozen }
                            }
                            other => other,
                        };
                        this.photo_source_note(message, cx);
                    }
                }
            });
        });
        // The frame kept is the one on screen: the stream stops now (it is
        // dropped with the live stage), so no newer frame replaces the
        // picture the JPEG is saved from.
        if let Some(camera) = self.sources_mut().and_then(|s| s.camera_mut()) {
            camera.stage = CameraStage::Saving {
                frozen,
                _task: task,
            };
        }
        cx.notify();
    }

    fn use_camera_photo(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self
            .sources()
            .and_then(|s| match s.camera().map(|c| &c.stage) {
                Some(CameraStage::Taken { path, .. }) => Some(path.clone()),
                _ => None,
            })
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

    pub(super) fn camera_panel(&self, camera: &CameraDraft, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let taken = matches!(camera.stage, CameraStage::Taken { .. });
        // A frame frozen by a camera that stopped, or a Take photo whose
        // save failed: only a new camera run can take another.
        let stopped = matches!(camera.stage, CameraStage::Stopped { .. });
        let picture = camera.stage.picture();
        let status = if picture.is_some() {
            None
        } else if matches!(
            camera.stage,
            CameraStage::Starting { .. } | CameraStage::Live { .. }
        ) {
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
        let can_take = matches!(camera.stage, CameraStage::Live { image: Some(_), .. });
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
                taken,
                |this, cx| this.use_camera_photo(cx),
                cx,
            ))
            .into_any_element()
    }
}
