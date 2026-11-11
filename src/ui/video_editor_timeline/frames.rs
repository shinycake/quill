//! The strip's frames: cut one at a time by a task the stage owns.
#![deny(clippy::indexing_slicing)]

use super::super::owned_image::OwnedImage;
use gpui_kit::{RenderImage, Task};
use std::sync::Arc;

/// One frame of the strip.
pub(in crate::ui) enum StripFrame {
    /// Not cut yet.
    Empty,
    Cut(OwnedImage),
    /// The cut failed: the slot stays blank until the strip is cut again.
    Failed,
}

/// The strip's frames: none cut, being cut one at a time (the task cuts
/// them, so dropping the stage stops it), or all cut. `width` is the strip
/// width they were cut for.
#[derive(Default)]
pub(in crate::ui) enum Frames {
    #[default]
    Idle,
    Cutting {
        width: f32,
        frames: Vec<StripFrame>,
        task: Task<()>,
    },
    Done {
        width: f32,
        frames: Vec<StripFrame>,
    },
}

impl Frames {
    /// The width the frames were cut for; 0 with none.
    pub(super) fn width(&self) -> f32 {
        match self {
            Self::Idle => 0.0,
            Self::Cutting { width, .. } | Self::Done { width, .. } => *width,
        }
    }

    pub(super) fn slots(&self) -> &[StripFrame] {
        match self {
            Self::Idle => &[],
            Self::Cutting { frames, .. } | Self::Done { frames, .. } => frames,
        }
    }

    /// The pictures to paint, `None` where a frame is not there.
    pub(super) fn pictures(&self) -> Vec<Option<Arc<RenderImage>>> {
        self.slots()
            .iter()
            .map(|frame| match frame {
                StripFrame::Cut(image) => Some((**image).clone()),
                StripFrame::Empty | StripFrame::Failed => None,
            })
            .collect()
    }

    /// Frame `index` was cut (or failed); the last one ends the stage. The
    /// task is the one running this, so it is let go, not cancelled from
    /// inside itself.
    pub(super) fn land(&mut self, index: usize, frame: StripFrame, last: bool) {
        let Self::Cutting { frames, .. } = self else {
            return;
        };
        if let Some(slot) = frames.get_mut(index) {
            *slot = frame;
        }
        if !last {
            return;
        }
        if let Self::Cutting {
            width,
            frames,
            task,
        } = std::mem::take(self)
        {
            // This task is the one running this call: detach lets it end, dropping would cancel it.
            task.detach();
            *self = Self::Done { width, frames };
        }
    }
}
