//! The streaming sticker player's engine: animated stickers and custom
//! emoji drawn frame by frame, at the size they show at, by a few worker
//! threads, with recent frames kept compressed.
//!
//! - A clip is one file drawn at one pixel size ([`ClipKey`]: file, edge,
//!   skin tone, tint, mirroring, display rate). The UI asks for the frames
//!   its views will show next ([`Engine::want`]); a worker draws them with
//!   ThorVG (or unpacks them from the clip's [`store`]) and hands them back
//!   ([`Engine::take_rendered`]).
//! - Frames sit on a grid `step` times finer than the file's own rate when
//!   the display is faster than 60 Hz (up to 120 frames a second), and
//!   ThorVG draws the fractional frames in between ([`head::Timeline`]).
//! - Each worker keeps the clips it opened: ThorVG's software renderer uses
//!   the memory pool of the thread that created a canvas
//!   (`tvgSwMemPool.cpp`, a `thread_local`), so a canvas is drawn only on
//!   the thread that made it. Clips decoded up front (WebM) are decoded on
//!   opener threads of their own ([`opener`]), off the workers.
//! - Stored frames are evicted least recently used first under one byte
//!   budget for every clip ([`Engine::new`]); a clip shown in the last half
//!   second keeps its frames.
//!
//! The UI half (play heads per view, frames on screen, the frame clock) is
//! `src/ui/sticker_player`. docs/decisions/sticker-player.md.
#![deny(clippy::indexing_slicing)]

mod engine;
pub mod head;
mod opener;
mod source;
mod store;
mod worker;

pub use engine::{Engine, EngineStats};
pub use head::{AHEAD, Pick, PlayHead, PlayMode, Rest, Timeline};

use super::{FitzType, Tint};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

/// The largest edge a clip is drawn at.
pub const MAX_EDGE: u32 = 1024;
/// The smallest.
pub const MIN_EDGE: u32 = 16;

/// The pixel edge to draw a clip at for a box `side` pixels across: up
/// to the next multiple of 8, so a box a pixel larger or smaller reuses
/// the same clip.
pub fn edge_for(side: f32) -> u32 {
    if !side.is_finite() || side <= 0.0 {
        return MIN_EDGE;
    }
    let side = side.ceil().min(MAX_EDGE as f32) as u32;
    side.div_ceil(8).saturating_mul(8).clamp(MIN_EDGE, MAX_EDGE)
}

/// One file drawn one way at one size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ClipKey {
    /// TDLib's file id.
    pub file: i32,
    /// Frames are `edge` × `edge` px.
    pub edge: u32,
    pub fitz: FitzType,
    /// The live story's gold burst.
    pub tint: Option<Tint>,
    /// Flipped left to right (tdesktop `mirrorHorizontal`).
    pub mirror: bool,
    /// The display rate the frame grid is made for, from `grid_hz`. A
    /// window moving to a faster display opens the clip again on a finer
    /// grid.
    pub hz: u32,
}

/// Straight-alpha BGRA frames, `edge` × `edge` each, at `fps`.
pub struct DecodedFrames {
    pub frames: Vec<Vec<u8>>,
    pub fps: f64,
}

/// Lets the engine stop a decode when its clip closes: the flag, and the
/// `ffmpeg` process the WebM fallback runs.
#[derive(Clone, Default)]
pub struct Cancel {
    pub flag: Arc<AtomicBool>,
    pub child: Arc<Mutex<Option<std::process::Child>>>,
}

/// A decode that produces every frame at once (WebM video stickers, test
/// clips): its frames all go to the store.
pub type DecodeAll = Box<dyn FnOnce(&Cancel) -> Result<DecodedFrames, String> + Send>;

/// Where a clip's frames come from.
pub enum Origin {
    /// A TGS file, drawn frame by frame.
    Lottie(PathBuf),
    /// Frames decoded up front.
    Decoded(DecodeAll),
}

/// An opened clip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipInfo {
    pub timeline: Timeline,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ClipState {
    Opening,
    Ready(ClipInfo),
    Failed(String),
}

/// A drawn frame of `key`, straight-alpha BGRA.
pub struct Rendered {
    pub key: ClipKey,
    pub index: u32,
    pub pixels: Vec<u8>,
}

/// The display rate class a clip's frame grid is made for: 60 at 61 Hz or
/// less, else the display's rate up to 120.
pub fn grid_hz(display_hz: f64) -> u32 {
    if !display_hz.is_finite() || display_hz <= 61.0 {
        return 60;
    }
    display_hz.round().min(120.0) as u32
}

/// The grid step for a `file_fps` animation on a display refreshing at
/// `display_hz`: 1 (the file's own frames) at 60 Hz or less, else as many
/// in-between frames as fit the display's rate, up to 120 a second.
pub fn rate_step(file_fps: f64, display_hz: f64) -> u32 {
    const MAX_FPS: f64 = 120.0;
    if !(file_fps.is_finite() && display_hz.is_finite()) || file_fps <= 0.0 || display_hz <= 61.0 {
        return 1;
    }
    let target = display_hz.min(MAX_FPS);
    let mut step = (target / file_fps).round().max(1.0) as u32;
    while step > 1 && file_fps * f64::from(step) > MAX_FPS + 0.5 {
        step -= 1;
    }
    step
}

/// Mirror `height` rows of `width` 4-byte pixels left to right, in place.
pub fn mirror_rows(pixels: &mut [u8], width: usize) {
    let Some(row_bytes) = width.checked_mul(4).filter(|n| *n > 0) else {
        return;
    };
    for row in pixels.chunks_exact_mut(row_bytes) {
        row.as_chunks_mut::<4>().0.reverse();
    }
}

#[cfg(test)]
mod tests;
