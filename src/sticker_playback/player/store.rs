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
//!
//! A clip's frames are packed one after another into blocks of [`BLOCK`]
//! bytes. A packed frame takes up to about 50 KB, and on macOS freed blocks
//! of a few KB and up stay in malloc's small regions, charged to Quill,
//! after their clip closed; a block of 64 KiB or more goes back to the
//! system when it is freed (`large_alloc`).
//! docs/decisions/sticker-clip-memory.md.
#![deny(clippy::indexing_slicing)]

use std::collections::HashMap;
use std::sync::Arc;

/// The size of a store's blocks. A frame larger than that gets a block of
/// its own size.
const BLOCK: usize = 256 * 1024;

/// One clip's stored frames.
#[derive(Default)]
pub(super) struct Store {
    /// Filled in order; only the last one takes new frames.
    blocks: Vec<Arc<Vec<u8>>>,
    frames: HashMap<u32, Span>,
    /// Bytes written to its blocks.
    bytes: usize,
}

/// Where a stored frame sits in its store.
#[derive(Clone, Copy)]
struct Span {
    block: usize,
    start: usize,
    len: usize,
}

/// One stored frame, for a worker to unpack outside the engine's lock.
#[derive(Clone)]
pub(super) struct StoredFrame {
    block: Arc<Vec<u8>>,
    start: usize,
    len: usize,
}

impl StoredFrame {
    fn data(&self) -> &[u8] {
        self.block
            .get(self.start..self.start.saturating_add(self.len))
            .unwrap_or_default()
    }
}

impl Store {
    /// Bytes its frames take.
    pub(super) fn bytes(&self) -> usize {
        self.bytes
    }

    /// Keeps `packed` (from [`encode`]) as frame `index`, in place of one
    /// kept before.
    pub(super) fn insert(&mut self, index: u32, packed: &[u8]) {
        // A worker unpacking a frame holds its block: then the frame goes
        // to a new one.
        let room = self
            .blocks
            .last_mut()
            .and_then(Arc::get_mut)
            .is_some_and(|block| block.capacity() - block.len() >= packed.len());
        if !room {
            let capacity = BLOCK.max(packed.len());
            self.blocks.push(Arc::new(Vec::with_capacity(capacity)));
        }
        let at = self.blocks.len() - 1;
        let Some(block) = self.blocks.last_mut().and_then(Arc::get_mut) else {
            return;
        };
        let start = block.len();
        block.extend_from_slice(packed);
        self.bytes = self.bytes.saturating_add(packed.len());
        self.frames.insert(
            index,
            Span {
                block: at,
                start,
                len: packed.len(),
            },
        );
    }

    /// Keeps frame `index` as every frame from it up to `end` too, for no
    /// more bytes.
    pub(super) fn repeat(&mut self, index: u32, end: u32) {
        let Some(span) = self.frames.get(&index).copied() else {
            return;
        };
        self.frames
            .extend((index.saturating_add(1)..end).map(|at| (at, span)));
    }

    pub(super) fn get(&self, index: u32) -> Option<StoredFrame> {
        let span = self.frames.get(&index)?;
        Some(StoredFrame {
            block: self.blocks.get(span.block)?.clone(),
            start: span.start,
            len: span.len,
        })
    }

    /// The length and capacity of each block.
    #[cfg(test)]
    pub(super) fn blocks(&self) -> Vec<(usize, usize)> {
        self.blocks
            .iter()
            .map(|block| (block.len(), block.capacity()))
            .collect()
    }
}

/// The packing buffer, reused from frame to frame by one worker.
#[derive(Default)]
pub(super) struct Scratch {
    packed: Vec<u8>,
}

/// Packs `frame` into `scratch`; the packed bytes, for [`Store::insert`].
pub(super) fn encode<'a>(frame: &[u8], scratch: &'a mut Scratch) -> Option<&'a [u8]> {
    scratch
        .packed
        .resize(lz4_flex::block::get_maximum_output_size(frame.len()), 0);
    let len = lz4_flex::block::compress_into(frame, &mut scratch.packed).ok()?;
    scratch.packed.get(..len)
}

/// Unpacks `stored` into `out` (the frame's length). False when the data
/// does not unpack to exactly `out`.
pub(super) fn decode(stored: &StoredFrame, out: &mut [u8]) -> bool {
    matches!(
        lz4_flex::block::decompress_into(stored.data(), out),
        Ok(len) if len == out.len()
    )
}
