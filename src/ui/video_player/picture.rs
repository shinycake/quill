//! A picture to draw: a GPU surface (AVPlayer buffer on macOS, NV12 frame
//! on Windows and Linux) or a decoded BGRA image, and turning one for the
//! clip's orientation.
#![deny(clippy::indexing_slicing)]

use super::rotate;
use super::surface;
use gpui_kit::*;
use std::sync::Arc;

/// What a surface picture holds: a GPU pixel buffer from AVPlayer on
/// macOS; NV12 pixels the DirectX (Windows) or wgpu (Linux) renderer
/// uploads and converts.
#[cfg(target_os = "macos")]
pub(crate) type SurfaceHandle = objc2_core_foundation::CFRetained<objc2_core_video::CVPixelBuffer>;
#[cfg(not(target_os = "macos"))]
pub(crate) type SurfaceHandle = Arc<SurfaceFrame>;

/// A picture to draw.
#[derive(Clone)]
pub(crate) enum Picture {
    /// Drawn by GPUI's `surface`: an AVPlayer pixel buffer (macOS) or an
    /// NV12 frame (Windows and Linux, FFmpeg).
    Surface(SurfaceHandle),
    /// A decoded BGRA image (FFmpeg, still frames), drawn by `img`.
    Image(Arc<RenderImage>),
}

impl Picture {
    /// The decoded image, when the picture is one (not a surface).
    pub(crate) fn image(&self) -> Option<Arc<RenderImage>> {
        match self {
            Picture::Image(image) => Some(image.clone()),
            Picture::Surface(_) => None,
        }
    }

    /// The picture as an image to copy out: the image, or a CPU surface
    /// converted (NV12); `None` for a GPU buffer (macOS: no
    /// readback).
    pub(crate) fn snapshot(&self) -> Option<Arc<RenderImage>> {
        match self {
            Picture::Image(image) => Some(image.clone()),
            Picture::Surface(surface) => surface::snapshot(surface),
        }
    }

    /// A surface, which GPUI can't clip (corners are masked instead).
    pub(crate) fn is_surface(&self) -> bool {
        matches!(self, Picture::Surface(_))
    }

    /// The picture's size in pixels (as shown, after any turn).
    pub(crate) fn size(&self) -> (u32, u32) {
        match self {
            Picture::Image(image) => image_size(image),
            Picture::Surface(surface) => surface::surface_size(surface),
        }
    }

    /// The renderer hasn't said yet what it did with this surface (NV12,
    /// painted but not drawn yet), or it refused it: the element
    /// keeps asking for frames until the picture is settled on screen.
    pub(crate) fn unsettled(&self) -> bool {
        match self {
            Picture::Image(_) => false,
            Picture::Surface(surface) => surface::draw_state(surface) != SurfaceDrawState::Drawn,
        }
    }

    /// The renderer could not draw this surface (NV12): it has to
    /// be shown as an image.
    pub(crate) fn refused(&self) -> bool {
        match self {
            Picture::Image(_) => false,
            Picture::Surface(surface) => surface::draw_state(surface) == SurfaceDrawState::Refused,
        }
    }
}

fn image_size(image: &RenderImage) -> (u32, u32) {
    let size = image.size(0);
    let side = |pixels: i32| u32::try_from(pixels).unwrap_or(0);
    (side(size.width.0), side(size.height.0))
}

/// The picture turned for the orientation; `None` when it needs no turn
/// or turning failed (the picture is shown as it is).
pub(super) fn turn(picture: &Picture, turns: u8, mirrored: bool) -> Option<Picture> {
    if turns == 0 && !mirrored {
        return None;
    }
    match picture {
        Picture::Image(image) => {
            rotate::turn_render_image(image, turns, mirrored).map(Picture::Image)
        }
        Picture::Surface(surface) => {
            surface::turn_surface(surface, turns, mirrored).map(Picture::Surface)
        }
    }
}
