//! The live backend's side of NV12 frames (`live_surface.rs`):
//! a refused frame replaced by its image, the element kept asking until
//! the renderer answers, and the test hooks for both. A child of
//! `backend_live`, so it reads the core's state directly.
#![deny(clippy::indexing_slicing)]

use super::super::Picture;
use super::LiveCore;
use std::time::Instant;

impl LiveCore {
    /// The window refused the NV12 frame on screen: the same
    /// picture as an image, and images for the file player's pause.
    pub(super) fn replace_refused(&self, now: Instant) -> Option<Picture> {
        let now = self.time(now);
        let mut track = self.track.try_borrow_mut().ok()?;
        let image = track.nv12.refused(now)?;
        track.held_surfaces.clear();
        track.keep_image(image.clone());
        Some(Picture::Image(image))
    }

    /// The NV12 frame on screen waits for the renderer's answer.
    pub(super) fn nv12_unsettled(&self) -> bool {
        self.track
            .try_borrow()
            .is_ok_and(|track| track.nv12.unsettled())
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
impl super::super::VideoPlayer {
    /// Tests: convert frames to NV12 surface frames (the Windows and Linux path) or
    /// not, from the next frame on.
    pub(crate) fn set_live_nv12(&self, nv12: bool) {
        if let Some(core) = self.0.live.as_ref() {
            core.nv12.set(nv12);
        }
    }

    /// Tests: the NV12 frame on screen, if the picture is one.
    pub(crate) fn live_nv12_frame(&self) -> Option<std::sync::Arc<gpui_kit::SurfaceFrame>> {
        let core = self.0.live.as_ref()?;
        core.track.try_borrow().ok()?.nv12.shown_frame()
    }

    /// Tests: NV12 buffers kept for reuse.
    pub(crate) fn live_nv12_pooled(&self) -> usize {
        let Some(core) = self.0.live.as_ref() else {
            return 0;
        };
        core.track
            .try_borrow()
            .map_or(0, |track| track.nv12.pooled().len())
    }

    /// Tests: what the player's pull does when the renderer refused the
    /// surface on screen, for macOS, whose renderer draws no NV12 frames,
    /// so the picture on screen can't report a refusal.
    pub(crate) fn live_replace_refused(&self, now: Instant) {
        let raw = self
            .with_backend(|backend| backend.replace_refused(now))
            .flatten();
        if let Some(raw) = raw {
            self.with_inner(|inner| inner.show(raw, None));
        }
    }
}
