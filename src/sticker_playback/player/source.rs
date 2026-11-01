//! A clip's source on its worker, a ThorVG animation drawing frames on
//! demand, and the frames of a clip decoded up front, finished for its
//! store.
#![deny(clippy::indexing_slicing)]

use super::super::thorvg::Lottie;
use super::{ClipKey, DecodedFrames, Timeline, mirror_rows, rate_step};
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

/// The timeline of frames decoded up front for `key`, and the frames
/// finished (tinted, mirrored) for its store.
pub(super) fn decoded(
    key: ClipKey,
    decoded: DecodedFrames,
) -> Result<(Timeline, Vec<Vec<u8>>), String> {
    let len = frame_len(key)?;
    let fps = decoded.fps;
    if !(fps.is_finite() && fps > 0.0) {
        return Err("Invalid animated sticker duration".into());
    }
    let mut frames = decoded.frames;
    if frames.is_empty() || frames.iter().any(|frame| frame.len() != len) {
        return Err("Animated sticker has no frames".into());
    }
    for frame in &mut frames {
        finish(key, frame);
    }
    let count = u32::try_from(frames.len()).map_err(|_| "Animated sticker is too long")?;
    let timeline = Timeline {
        frames: count,
        file_fps: fps,
        step: 1,
    };
    Ok((timeline, frames))
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
