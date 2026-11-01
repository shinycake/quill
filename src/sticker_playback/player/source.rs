//! A clip's source on its worker, a ThorVG animation drawing frames on
//! demand, and the packer that finishes the frames of a clip decoded up
//! front for its store.
#![deny(clippy::indexing_slicing)]

use super::super::thorvg::Lottie;
use super::store::{self, Scratch, Store};
use super::{ClipKey, FrameSink, Timeline, mirror_rows, rate_step};
use std::path::Path;

/// Draws grid frame `g` as file frame `g / step`.
pub(super) struct Source {
    lottie: Box<Lottie>,
    pub(super) key: ClipKey,
    pub(super) timeline: Timeline,
}

impl Source {
    /// Opens a TGS file for `key`, on the frame grid for `key.hz`.
    pub(super) fn lottie(path: &Path, key: ClipKey) -> Result<Self, String> {
        let tgs = super::super::tgs_json(path, key.fitz)?;
        let lottie = Lottie::load(&tgs.json, key.edge as usize)?;
        let total = lottie.total_frames();
        if !(1.0..=7200.0).contains(&total) || f64::from(total) / tgs.fps > 30.0 {
            return Err("Invalid animated sticker duration".into());
        }
        let step = rate_step(tgs.fps, f64::from(key.hz));
        let frames = (total.floor() as u32).max(1).saturating_mul(step);
        Ok(Self {
            lottie: Box::new(lottie),
            key,
            timeline: Timeline {
                frames,
                file_fps: tgs.fps,
                step,
            },
        })
    }

    /// Draws grid frame `index`, finished. `None` when ThorVG fails.
    pub(super) fn draw(&mut self, index: u32) -> Option<Vec<u8>> {
        let step = self.timeline.step.max(1);
        let frame = index as f32 / step as f32;
        let mut pixels = self.lottie.render(frame).ok()?;
        finish(self.key, &mut pixels);
        Some(pixels)
    }
}

/// Takes the frames of `key` as they are decoded, finished (tinted,
/// mirrored) and packed for its store.
pub(super) struct Packer<'a> {
    key: ClipKey,
    scratch: &'a mut Scratch,
    pub(super) store: Store,
    frames: u32,
}

impl<'a> Packer<'a> {
    pub(super) fn new(key: ClipKey, scratch: &'a mut Scratch) -> Self {
        Self {
            key,
            scratch,
            store: Store::default(),
            frames: 0,
        }
    }

    pub(super) fn timeline(&self, fps: f64) -> Result<Timeline, String> {
        if !(fps.is_finite() && fps > 0.0) {
            return Err("Invalid animated sticker duration".into());
        }
        if self.frames == 0 {
            return Err("Animated sticker has no frames".into());
        }
        Ok(Timeline {
            frames: self.frames,
            file_fps: fps,
            step: 1,
        })
    }
}

impl FrameSink for Packer<'_> {
    fn frame(&mut self, pixels: &mut [u8]) -> Result<(), String> {
        if pixels.len() != frame_len(self.key)? {
            return Err("Animated sticker has no frames".into());
        }
        finish(self.key, pixels);
        let packed = store::encode(pixels, self.scratch)
            .ok_or_else(|| "Could not keep animated sticker frames".to_string())?;
        self.store.insert(self.frames, packed);
        self.frames = self
            .frames
            .checked_add(1)
            .ok_or("Animated sticker is too long")?;
        Ok(())
    }

    fn repeat(&mut self, times: usize) -> Result<(), String> {
        let Some(last) = self.frames.checked_sub(1) else {
            return Ok(());
        };
        let frames = u32::try_from(times)
            .ok()
            .and_then(|times| self.frames.checked_add(times))
            .ok_or("Animated sticker is too long")?;
        self.store.repeat(last, frames);
        self.frames = frames;
        Ok(())
    }
}

/// Bytes in one frame of `key`.
pub(super) fn frame_len(key: ClipKey) -> Result<usize, String> {
    (key.edge as usize)
        .checked_mul(key.edge as usize)
        .and_then(|n| n.checked_mul(4))
        .filter(|n| *n > 0)
        .ok_or_else(|| "Invalid animated sticker size".to_string())
}

/// Tint and mirror a drawn frame as `key` asks.
fn finish(key: ClipKey, pixels: &mut [u8]) {
    if let Some(tint) = key.tint {
        tint.apply(pixels);
    }
    if key.mirror {
        mirror_rows(pixels, key.edge as usize);
    }
}
