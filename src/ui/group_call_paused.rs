//! A paused video tile of the voice chat, as tdesktop draws it
//! (`calls_group_viewport_raster.cpp`): the last frame scaled into
//! 90 × 90 and blurred (radius 15), under a black shadow (alpha 80) with
//! the pause icon, and "Video is paused" when the view is wide (in Quill:
//! the full-screen stage).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::calls::engine::VideoFrame;
use quill::lang::{keys, tr};
use quill::wallpaper_raster::{Raster, blur};
use smallvec::SmallVec;
use std::sync::Arc;

/// tdesktop `kPausedVideoSize`.
const PAUSED_SIZE: u32 = 90;
/// tdesktop `kBlurRadius`.
const BLUR_RADIUS: u32 = 15;
/// tdesktop `Viewport::kShadowMaxAlpha` (80 of 255).
const SHADOW: u32 = 0x0000_0050;

/// `frame` fitted into 90 × 90 and blurred, as BGRA for GPUI.
pub(super) fn paused_raster(frame: &VideoFrame) -> Option<Raster> {
    let (width, height) = (u32::from(frame.width), u32::from(frame.height));
    if width == 0 || height == 0 {
        return None;
    }
    let rgba = image::RgbaImage::from_raw(width, height, frame.rgba.clone())?;
    let scale = (PAUSED_SIZE as f32 / width as f32).min(PAUSED_SIZE as f32 / height as f32);
    let fit_w = ((width as f32 * scale).round() as u32).max(1);
    let fit_h = ((height as f32 * scale).round() as u32).max(1);
    let small = image::imageops::resize(&rgba, fit_w, fit_h, image::imageops::FilterType::Triangle);
    let mut bgra = small.into_raw();
    for pixel in bgra.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let mut raster = Raster {
        width: fit_w,
        height: fit_h,
        bgra,
    };
    blur(&mut raster, BLUR_RADIUS);
    Some(raster)
}

fn render_image(raster: Raster) -> Option<Arc<RenderImage>> {
    let buffer = image::RgbaImage::from_raw(raster.width, raster.height, raster.bgra)?;
    Some(Arc::new(RenderImage::new(SmallVec::from_buf([
        image::Frame::new(buffer),
    ]))))
}

impl QuillApp {
    /// The blurred last frame of a paused stream, rebuilt only when the
    /// frame changes.
    pub(super) fn cached_paused_image(
        &mut self,
        call_id: i32,
        user_id: i64,
        screen: bool,
        frame: &VideoFrame,
    ) -> Option<Arc<RenderImage>> {
        let key = (call_id, user_id, screen);
        let cache = &mut self.screen_share.paused_images;
        if let Some((seq, image)) = cache.get(&key)
            && *seq == frame.seq
        {
            return Some(image.clone());
        }
        let image = render_image(paused_raster(frame)?)?;
        cache.retain(|(call, _, _), _| *call == call_id);
        cache.insert(key, (frame.seq, image.clone()));
        Some(image)
    }
}

/// The shadow, pause icon and (when `wide`) "Video is paused" over a
/// paused tile.
pub(super) fn paused_overlay(wide: bool) -> Div {
    div()
        .absolute()
        .inset_0()
        .bg(rgba(SHADOW))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .text_color(white())
        .child(Icon::new(IconName::Pause).with_size(px(if wide { 40. } else { 28. })))
        .when(wide, |this| {
            this.child(
                div()
                    .text_size(px(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(tr(keys::GROUP_CALL_VIDEO_PAUSED)),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::{VideoFrame, paused_raster};

    fn frame(width: u16, height: u16) -> VideoFrame {
        VideoFrame {
            seq: 1,
            width,
            height,
            rgba: vec![200; usize::from(width) * usize::from(height) * 4],
            is_local: true,
            participant_user_id: None,
            is_screen: true,
        }
    }

    #[test]
    fn a_paused_frame_fits_ninety_pixels_and_keeps_its_shape() {
        let raster = paused_raster(&frame(320, 180)).expect("raster");
        assert_eq!((raster.width, raster.height), (90, 51));
        assert_eq!(raster.bgra.len(), 90 * 51 * 4);
        let raster = paused_raster(&frame(100, 400)).expect("raster");
        assert_eq!((raster.width, raster.height), (23, 90));
        assert!(paused_raster(&frame(0, 0)).is_none());
    }
}
