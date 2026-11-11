//! Where a cached image's decode stands.
#![deny(clippy::indexing_slicing)]

use std::time::Instant;

/// Where an entry's decode stands, set where its result is first seen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EntryStage {
    /// The decode has not landed.
    Loading,
    /// Decoded; `bytes` is its atlas size.
    Ready { bytes: usize },
    /// The decode failed, seen at `at` on the executor clock; it costs
    /// nothing and is tried again after `RETRY_FAILED_AFTER`.
    Failed { at: Instant },
}

impl EntryStage {
    /// What the entry holds in the atlas (0 until loaded, and for failures).
    pub(super) fn bytes(self) -> usize {
        match self {
            Self::Ready { bytes } => bytes,
            Self::Loading | Self::Failed { .. } => 0,
        }
    }
}
