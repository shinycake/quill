//! Where a cached image's decode stands.
#![deny(clippy::indexing_slicing)]

use super::Load;
use std::time::Instant;

/// Where an entry's decode stands, set where its result is first seen. The
/// load (and the decode task it holds) is in every stage, so an entry that
/// is dropped while loading cancels its decode.
pub(super) enum EntryStage {
    /// The decode has not landed.
    Loading { load: Load },
    /// Decoded; `bytes` is its atlas size.
    Ready { bytes: usize, load: Load },
    /// The decode failed, seen at `at` on the executor clock; it costs
    /// nothing and is tried again after `RETRY_FAILED_AFTER`.
    Failed { at: Instant, load: Load },
}

impl EntryStage {
    /// What the entry holds in the atlas (0 until loaded, and for failures).
    pub(super) fn bytes(&self) -> usize {
        match self {
            Self::Ready { bytes, .. } => *bytes,
            Self::Loading { .. } | Self::Failed { .. } => 0,
        }
    }

    pub(super) fn load(&self) -> &Load {
        match self {
            Self::Loading { load } | Self::Ready { load, .. } | Self::Failed { load, .. } => load,
        }
    }
}
