//! The live camera circle while a video message records (tdesktop
//! `RoundVideoRecorder`): the mirrored preview in a round frame over the
//! chat, with a shadow and a ring filling toward the 60 s limit.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::owned_image::OwnedImage;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme, Sizable};
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::video::ROUND_PREVIEW_SIDE;
use smallvec::SmallVec;
use std::sync::Arc;

/// tdesktop draws the circle at `kSide * 3 / 4` = 300.
const CIRCLE: f32 = 300.;
/// Room around the circle for the progress ring and the shadow.
const EXTENT: f32 = 16.;
const RING: f32 = 3.;

/// The newest preview image of one recording. It belongs to the recording
/// (`Capture`): the images it replaces, and this one when the recording
/// ends, leave the sprite atlas as the [`OwnedImage`] lets go of them.
#[derive(Default)]
pub(super) struct RoundPreview {
    seq: u64,
    image: Option<OwnedImage>,
}

fn starting_camera_circle(cx: &mut Context<QuillApp>) -> AnyElement {
    let theme = cx.theme();
    div()
        .id("round-record-starting")
        .size(px(CIRCLE + 2. * EXTENT))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .size(px(CIRCLE))
                .rounded_full()
                .bg(theme.muted)
                .shadow_md()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .child(Spinner::new().small())
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(tr(keys::Q_AV_STARTING_CAMERA)),
                ),
        )
        .into_any_element()
}

impl RoundPreview {
    /// Take the camera's newest frame when it is newer than the one shown.
    fn take_latest(&mut self, frames: &quill::video::PreviewFrame) {
        if let Ok(latest) = frames.lock()
            && latest.0 != self.seq
            && let Some(buffer) =
                image::RgbaImage::from_raw(ROUND_PREVIEW_SIDE, ROUND_PREVIEW_SIDE, latest.1.clone())
        {
            self.seq = latest.0;
            self.image = Some(OwnedImage::new(Arc::new(RenderImage::new(
                SmallVec::from_buf([image::Frame::new(buffer)]),
            ))));
        }
    }
}

impl QuillApp {
    /// The recording circle, while a video message records, in the window
    /// it belongs to (`chat_window::recording`): another window shows its
    /// own chat, with no camera over it.
    pub(super) fn round_record_overlay(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let here = Some(self.windows.scope);
        if self.recording.origin() != here || self.recording.video().is_none() {
            return (self.recording.opening_video() && self.recording.opening_origin() == here)
                .then(|| starting_camera_circle(cx));
        }
        self.request_animation_tick(30, cx);
        let (progress, image) = {
            let (capture, preview) = self.recording.video_with_preview_mut()?;
            preview.take_latest(&capture.preview());
            (capture.progress(), preview.image.as_deref().cloned())
        };
        let ring = cx.theme().primary;
        let placeholder = cx.theme().muted;
        let side = CIRCLE + 2. * EXTENT;
        Some(
            div()
                .id("round-record")
                .size(px(side))
                .flex_none()
                .child(
                    canvas(
                        |_, _, _| {},
                        move |bounds, _, window, _| {
                            let center = bounds.center();
                            let radius = px(CIRCLE / 2.);
                            let circle =
                                Bounds::centered_at(center, size(radius * 2., radius * 2.));
                            // tdesktop's soft radial shadow.
                            window.paint_quad(
                                fill(circle.dilate(px(4.)), hsla(0., 0., 0., 0.22))
                                    .corner_radii(radius + px(4.)),
                            );
                            match image.clone() {
                                Some(image) => {
                                    let _ = window.paint_image(
                                        circle,
                                        circle,
                                        Corners::all(radius),
                                        image,
                                        0,
                                        false,
                                    );
                                }
                                None => window
                                    .paint_quad(fill(circle, placeholder).corner_radii(radius)),
                            }
                            if progress > 0.0 {
                                let ring_radius = radius + px(EXTENT / 2.);
                                let steps = ((progress * 128.).ceil() as usize).max(2);
                                let points: Vec<_> = (0..=steps)
                                    .map(|step| {
                                        let angle = -std::f32::consts::FRAC_PI_2
                                            + std::f32::consts::TAU * progress * step as f32
                                                / steps as f32;
                                        point(
                                            center.x + ring_radius * angle.cos(),
                                            center.y + ring_radius * angle.sin(),
                                        )
                                    })
                                    .collect();
                                let mut path = PathBuilder::stroke(px(RING));
                                path.add_polygon(&points, false);
                                if let Ok(path) = path.build() {
                                    window.paint_path(path, ring);
                                }
                            }
                        },
                    )
                    .size_full(),
                )
                .into_any_element(),
        )
    }
}
