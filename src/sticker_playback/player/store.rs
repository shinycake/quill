//! Compressed frames of a clip: each frame is one LZ4 block of its BGRA
//! pixels, lossless, so a frame from the store is the frame ThorVG drew.
//!
//! Telegram Desktop's `lib_lottie` cache XORs each frame against the one
//! before it first. On 41 real stickers that came out larger, not smaller
//! (at 448 px 7.6% of the raw size against 6.0% for frames packed on their
//! own: the transparent margins pack better untouched, and the XOR turns
//! antialiased edges that move into noise), and it ties each frame to the
//! one before. Frames packed on their own unpack in any order, which views
//! of one clip at different points need. docs/decisions/sticker-player.md.
#![deny(clippy::indexing_slicing)]

use std::sync::Arc;

/// One stored frame.
#[derive(Clone)]
pub(super) struct StoredFrame {
    data: Arc<[u8]>,
}

impl StoredFrame {
    pub(super) fn bytes(&self) -> usize {
        self.data.len()
    }
}

/// The packing buffer, reused from frame to frame by one worker.
#[derive(Default)]
pub(super) struct Scratch {
    packed: Vec<u8>,
}

pub(super) fn encode(frame: &[u8], scratch: &mut Scratch) -> Option<StoredFrame> {
    scratch
        .packed
        .resize(lz4_flex::block::get_maximum_output_size(frame.len()), 0);
    let len = lz4_flex::block::compress_into(frame, &mut scratch.packed).ok()?;
    Some(StoredFrame {
        data: Arc::from(scratch.packed.get(..len)?),
    })
}

/// Unpacks `stored` into `out` (the frame's length). False when the data
/// does not unpack to exactly `out`.
pub(super) fn decode(stored: &StoredFrame, out: &mut [u8]) -> bool {
    matches!(
        lz4_flex::block::decompress_into(&stored.data, out),
        Ok(len) if len == out.len()
    )
}
