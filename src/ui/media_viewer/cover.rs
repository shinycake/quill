//! The viewer over the whole main window. tdesktop's is a window of its own,
//! so the main window loses activation and its animations pause
//! (`isGifPausedAtLeastFor`). Quill's is an overlay: what its backdrop hides
//! gets no ticks.

use super::VIEWER_SHOW_MS;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq)]
struct Opening {
    open_gen: u64,
    opaque_at: Instant,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(in crate::ui) struct ViewerCover {
    open: Option<Opening>,
}

impl ViewerCover {
    pub(in crate::ui) fn follow(self, open_gen: Option<u64>, now: Instant) -> Self {
        let open = open_gen.map(|open_gen| match self.open {
            Some(open) if open.open_gen == open_gen => open,
            _ => Opening {
                open_gen,
                opaque_at: now + Duration::from_millis(VIEWER_SHOW_MS),
            },
        });
        Self { open }
    }

    pub(in crate::ui) fn is_opaque_at(self, now: Instant) -> bool {
        self.open.is_some_and(|open| now >= open.opaque_at)
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "cover_tests.rs"]
mod tests;
