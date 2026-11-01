//! BGRA pictures: turned upright into buffers a player fills again.

use super::{lock, nv12};
use std::sync::{Arc, Mutex};

/// Idle buffers kept at most. A playing clip hands one back for each one
/// it takes, so few wait at any time; more would only sit idle once it
/// pauses or moves to NV12 (budgets count these, `INLINE_DECODE_BUDGET`).
pub const BGRA_KEPT: usize = 3;

/// A player's BGRA picture buffers. The images they become hand them back
/// when they are dropped (`RenderImage::with_recycler`), so a playing clip
/// fills the same few buffers instead of allocating one per picture. Clones
/// share the buffers.
#[derive(Clone, Default)]
pub struct BgraPool(Arc<Mutex<Kept>>);

#[derive(Default)]
struct Kept {
    buffers: Vec<Vec<u8>>,
    /// The clip shows NV12 pictures: buffers handed back would sit idle.
    released: bool,
}

impl BgraPool {
    /// A `len`-byte buffer to overwrite whole: a kept one, else a new one.
    /// Kept buffers of other sizes go.
    pub fn take(&self, len: usize) -> Vec<u8> {
        let mut kept = lock(&self.0);
        kept.released = false;
        kept.buffers.retain(|buffer| buffer.len() == len);
        // Zeroed memory this large is fresh pages, not a memset.
        kept.buffers.pop().unwrap_or_else(|| vec![0; len])
    }

    /// Keep `buffer` for a later picture.
    pub fn give(&self, buffer: Vec<u8>) {
        let mut kept = lock(&self.0);
        if !kept.released && kept.buffers.len() < BGRA_KEPT {
            kept.buffers.push(buffer);
        }
    }

    /// A picture that needs no buffer of ours was shown: let the kept ones
    /// go, and the ones handed back until a BGRA picture is decoded again.
    pub(super) fn release(&self) {
        let mut kept = lock(&self.0);
        kept.released = true;
        kept.buffers = Vec::new();
    }

    /// Bytes of the buffers kept.
    pub fn held_bytes(&self) -> usize {
        lock(&self.0).buffers.iter().map(Vec::len).sum()
    }
}

/// Turn a BGRA picture (`stride` bytes per row) upright by `rotation`
/// clockwise degrees into a buffer from `pool`, packing rows tightly.
/// Returns the pixels and size; an empty 0x0 picture when `src` is shorter
/// than `stride` rows say.
pub fn rotate_bgra(
    src: &[u8],
    width: u32,
    height: u32,
    stride: usize,
    rotation: u32,
    pool: &BgraPool,
) -> (Vec<u8>, u32, u32) {
    let mut out = pool.take(
        (width as usize)
            .saturating_mul(height as usize)
            .saturating_mul(4),
    );
    // The last row may stop at its pixels, without the padding.
    if !nv12::turn_plane_into::<4>(
        src,
        (width as usize, height as usize),
        stride,
        rotation,
        &mut out,
    ) {
        return (Vec::new(), 0, 0);
    }
    match rotation {
        90 | 270 => (out, height, width),
        _ => (out, width, height),
    }
}
