//! BGRA pictures: turned upright into buffers a player fills again.

use super::{lock, nv12};
use std::sync::{Arc, Mutex};

/// Buffers kept at most: as for NV12, the decoder's queue, the picture
/// being decoded, the one on screen and the ones waiting to leave the atlas.
const KEPT: usize = 8;

/// A player's BGRA picture buffers. The images they become hand them back
/// when they are dropped (`RenderImage::with_recycler`), so a playing clip
/// fills the same few buffers instead of allocating one per picture. Clones
/// share the buffers.
#[derive(Clone, Default)]
pub struct BgraPool(Arc<Mutex<Vec<Vec<u8>>>>);

impl BgraPool {
    /// A `len`-byte buffer to overwrite whole: a kept one, else a new one.
    /// Kept buffers of other sizes go.
    pub fn take(&self, len: usize) -> Vec<u8> {
        let mut kept = lock(&self.0);
        kept.retain(|buffer| buffer.len() == len);
        // Zeroed memory this large is fresh pages, not a memset.
        kept.pop().unwrap_or_else(|| vec![0; len])
    }

    /// Keep `buffer` for a later picture.
    pub fn give(&self, buffer: Vec<u8>) {
        let mut kept = lock(&self.0);
        if kept.len() < KEPT {
            kept.push(buffer);
        }
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
