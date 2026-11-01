//! Windows and Linux: a live frame as an NV12 surface frame, the way file
//! video is since #859 (`docs/decisions/video-player-live-nv12.md`). The upright
//! I420 planes (turned and mirrored in the plane copy, as on every
//! platform) are packed into NV12 (`i420_to_nv12_packed`: the Y plane
//! copied, U and V interleaved), and the vendored DirectX or wgpu renderer
//! uploads the two planes as textures and converts to RGB on the GPU.
//!
//! - Colour: BT.601, full range, for every size ([`LIVE_COLOR`]): what the
//!   BGRA fallback (`i420_to_bgra`) and the macOS pixel buffers show, so a
//!   frame looks the same on every platform and as its own fallback image.
//! - Buffers come from a small pool ([`POOL`]): a buffer goes back to it
//!   once its frame is shown, and is filled again once no frame holds it
//!   any more (nothing else can then read it). Sizes other than the last
//!   are dropped.
//! - An odd side loses its last column or row (NV12 surfaces need even
//!   sides; `nv12_packed_size`), so odd frames (a screen share of a window,
//!   the demo photos) stay on the GPU path too.
//! - When a window's renderer refuses the frame on screen, the player asks
//!   the backend for the same picture as an image (`replace_refused`): it
//!   is converted on the CPU, and the feed converts to BGRA for
//!   [`NV12_RETRY`] (the file player's pause) before it tries NV12 again.
//!
//! The module builds off macOS and in tests (a `SurfaceFrame` is plain
//! data), so its logic is tested on every machine; macOS draws pixel
//! buffers instead.
#![deny(clippy::indexing_slicing)]
#![cfg_attr(target_os = "macos", allow(dead_code))]

use super::Picture;
use super::backend_ffmpeg::{NV12_RETRY, nv12_allowed};
use super::live_picture::render_image;
use super::surface::gpui_color;
use gpui_kit::{RenderImage, SurfaceDrawState, SurfaceFormat, SurfaceFrame};
use quill::calls::live_video::convert::{
    I420, i420_to_nv12_packed, nv12_packed_len, nv12_packed_size,
};
use quill::video_decode::{YuvColor, YuvMatrix, nv12_to_bgra};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

/// How a live frame's samples map to RGB: BT.601, full range, whatever
/// the size. The engine's I420 has always been read this way here (the
/// former `i420_to_rgba`, today's `i420_to_bgra`, and GPUI's macOS
/// full-range surface shader).
pub(super) const LIVE_COLOR: YuvColor = YuvColor {
    matrix: YuvMatrix::Bt601,
    full_range: true,
};

/// Buffers kept for reuse: the frame on screen, the one the last drawn
/// scene may still hold, one landed but not drawn yet, and one converting.
const POOL: usize = 4;

/// Whether live frames become NV12 surface frames here: off macOS,
/// unless `QUILL_LIVE_BGRA=1` (demo-capture builds) asks for images.
pub(super) fn nv12_live() -> bool {
    cfg!(not(target_os = "macos")) && !super::live_picture::bgra_only()
}

/// A frame packed into a pool buffer, on its way to the UI thread.
pub(super) struct Nv12Picture {
    frame: SurfaceFrame,
    /// The frame's bytes, which go back to the pool once it is shown.
    bytes: Arc<[u8]>,
}

impl Nv12Picture {
    pub(super) fn size(&self) -> (u32, u32) {
        (self.frame.width(), self.frame.height())
    }

    /// Bytes the renderer uploads for it.
    pub(super) fn byte_len(&self) -> usize {
        self.bytes.len()
    }

    /// The frame as a picture to draw, and what the backend keeps of it.
    pub(super) fn into_shown(self) -> Option<(Picture, Option<Arc<RenderImage>>, Nv12Shown)> {
        let frame = Arc::new(self.frame);
        let (picture, image) = picture_of(&frame)?;
        Some((
            picture,
            image,
            Nv12Shown {
                frame,
                bytes: self.bytes,
            },
        ))
    }
}

/// An NV12 frame put on screen: the frame (for the renderer's answer)
/// and its buffer (back to the pool).
pub(super) struct Nv12Shown {
    frame: Arc<SurfaceFrame>,
    bytes: Arc<[u8]>,
}

/// Pack `src` into `bytes`, a buffer from [`LiveNv12::buffer`]. `None`
/// (the frame converts to BGRA instead) when the buffer is shared or not
/// the frame's packed size.
pub(super) fn fill(src: &I420, mut bytes: Arc<[u8]>) -> Option<Nv12Picture> {
    let out = Arc::get_mut(&mut bytes)?;
    if !i420_to_nv12_packed(src, out) {
        return None;
    }
    let (width, height) = nv12_packed_size(src.width, src.height)?;
    let width = u32::try_from(width).ok()?;
    let height = u32::try_from(height).ok()?;
    let frame = SurfaceFrame::new(width, height, SurfaceFormat::Nv12, bytes.clone())
        .ok()?
        .with_yuv(gpui_color(LIVE_COLOR));
    Some(Nv12Picture { frame, bytes })
}

/// The frame as the renderer draws it: a surface.
#[cfg(not(target_os = "macos"))]
fn picture_of(frame: &Arc<SurfaceFrame>) -> Option<(Picture, Option<Arc<RenderImage>>)> {
    Some((Picture::Surface(frame.clone()), None))
}

/// Tests on macOS: no renderer there draws NV12 frames, so the frame is
/// shown as its CPU image (what a refused frame becomes elsewhere).
#[cfg(target_os = "macos")]
fn picture_of(frame: &Arc<SurfaceFrame>) -> Option<(Picture, Option<Arc<RenderImage>>)> {
    let image = fallback_image(frame)?;
    Some((Picture::Image(image.clone()), Some(image)))
}

/// The frame converted to a BGRA image on the CPU, with the colour it
/// carries on the GPU.
pub(super) fn fallback_image(frame: &SurfaceFrame) -> Option<Arc<RenderImage>> {
    let (width, height) = (frame.width(), frame.height());
    let bgra = nv12_to_bgra(frame.bytes(), width, height, LIVE_COLOR)?;
    render_image(bgra, width, height)
}

/// A new zeroed buffer of `len` bytes (one allocation).
fn new_buffer(len: usize) -> Arc<[u8]> {
    std::iter::repeat_n(0u8, len).collect()
}

/// A live player's NV12 state: the buffer pool, the pause after a
/// refused frame, and the frame on screen.
#[derive(Default)]
pub(super) struct LiveNv12 {
    pool: VecDeque<Arc<[u8]>>,
    /// Images only until then: a window refused the frame on screen.
    paused_until: Option<Instant>,
    /// The NV12 frame on screen, if the picture on screen is one.
    shown: Option<Arc<SurfaceFrame>>,
}

impl LiveNv12 {
    /// Whether a frame shown at `width × height` converts to NV12 at
    /// `now`: at least 2 × 2, and not in the pause after a refusal.
    pub(super) fn takes(&self, width: u32, height: u32, now: Instant) -> bool {
        nv12_allowed(self.paused_until, now) && packed_len(width, height).is_some()
    }

    /// A buffer to pack a frame shown at `width × height` into: one from
    /// the pool no frame holds any more, else a new one. `None` when the
    /// frame converts to BGRA ([`Self::takes`]).
    pub(super) fn buffer(&mut self, width: u32, height: u32, now: Instant) -> Option<Arc<[u8]>> {
        if !self.takes(width, height, now) {
            return None;
        }
        let len = packed_len(width, height)?;
        let free = self.pool.iter().position(|bytes| {
            bytes.len() == len && Arc::strong_count(bytes) == 1 && Arc::weak_count(bytes) == 0
        });
        Some(
            free.and_then(|at| self.pool.remove(at))
                .unwrap_or_else(|| new_buffer(len)),
        )
    }

    /// The picture on screen changed: an NV12 frame (its buffer goes back
    /// to the pool), or `None` for an image.
    pub(super) fn shown(&mut self, shown: Option<Nv12Shown>) {
        let Some(Nv12Shown { frame, bytes }) = shown else {
            self.shown = None;
            return;
        };
        self.pool.retain(|kept| kept.len() == bytes.len());
        self.pool.push_back(bytes);
        while self.pool.len() > POOL {
            self.pool.pop_front();
        }
        self.shown = Some(frame);
    }

    /// The NV12 frame on screen has not been drawn yet, or was refused:
    /// the element looks again on the next display frame.
    pub(super) fn unsettled(&self) -> bool {
        self.shown
            .as_ref()
            .is_some_and(|frame| frame.draw_state() != SurfaceDrawState::Drawn)
    }

    /// A window refused the frame on screen: images for [`NV12_RETRY`]
    /// from `now`, starting with that frame converted on the CPU.
    pub(super) fn refused(&mut self, now: Instant) -> Option<Arc<RenderImage>> {
        self.paused_until = now.checked_add(NV12_RETRY);
        let frame = self.shown.take()?;
        fallback_image(&frame)
    }

    /// The feed ended: no frame converts any more, so the pool goes (the
    /// frame on screen stays).
    pub(super) fn feed_ended(&mut self) {
        self.pool.clear();
    }

    /// The player goes: the pool and the frame on screen go.
    pub(super) fn release(&mut self) {
        self.pool.clear();
        self.shown = None;
    }

    /// The NV12 frame on screen (tests).
    #[cfg(test)]
    pub(super) fn shown_frame(&self) -> Option<Arc<SurfaceFrame>> {
        self.shown.clone()
    }

    /// Buffers in the pool (tests).
    #[cfg(test)]
    pub(super) fn pooled(&self) -> Vec<std::sync::Weak<[u8]>> {
        self.pool.iter().map(Arc::downgrade).collect()
    }
}

/// The bytes a frame shown at `width × height` packs into.
fn packed_len(width: u32, height: u32) -> Option<usize> {
    let (width, height) =
        nv12_packed_size(usize::try_from(width).ok()?, usize::try_from(height).ok()?)?;
    nv12_packed_len(width, height)
}

#[cfg(test)]
#[path = "live_surface_tests.rs"]
mod tests;
