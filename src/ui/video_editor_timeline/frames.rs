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
    /// The cut failed: the slot shows its nearest cut neighbour until the
    /// strip is cut again, so a clip with one bad timestamp has no hole.
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

    /// The pictures to paint, `None` where a frame is not there yet. A
    /// failed frame shows the last cut one before it (the first after it,
    /// when none comes before).
    pub(super) fn pictures(&self) -> Vec<Option<Arc<RenderImage>>> {
        let cut = |frame: &StripFrame| match frame {
            StripFrame::Cut(image) => Some((**image).clone()),
            StripFrame::Empty | StripFrame::Failed => None,
        };
        let slots = self.slots();
        let mut earlier = None;
        let mut pictures: Vec<Option<Arc<RenderImage>>> = slots
            .iter()
            .map(|frame| match frame {
                StripFrame::Cut(_) => {
                    earlier = cut(frame);
                    cut(frame)
                }
                StripFrame::Failed => earlier.clone(),
                StripFrame::Empty => None,
            })
            .collect();
        let mut later = None;
        for (picture, frame) in pictures.iter_mut().zip(slots).rev() {
            match frame {
                StripFrame::Cut(_) => later = cut(frame),
                StripFrame::Failed if picture.is_none() => *picture = later.clone(),
                StripFrame::Failed | StripFrame::Empty => {}
            }
        }
        pictures
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

#[cfg(test)]
mod tests {
    use super::*;
    use smallvec::SmallVec;

    fn cut() -> StripFrame {
        StripFrame::Cut(
            Arc::new(RenderImage::new(SmallVec::from_buf([image::Frame::new(
                image::RgbaImage::new(1, 1),
            )])))
            .into(),
        )
    }

    fn done(frames: Vec<StripFrame>) -> Frames {
        Frames::Done {
            width: 10.0,
            frames,
        }
    }

    #[test]
    fn a_failed_frame_shows_the_last_cut_one_before_it() {
        let frames = done(vec![cut(), StripFrame::Failed, StripFrame::Failed, cut()]);
        let pictures = frames.pictures();
        assert!(pictures.iter().all(Option::is_some));
        let shown = |i: usize| pictures.get(i).cloned().flatten();
        assert!(Arc::ptr_eq(&shown(0).unwrap(), &shown(1).unwrap()));
        assert!(Arc::ptr_eq(&shown(0).unwrap(), &shown(2).unwrap()));
        assert!(!Arc::ptr_eq(&shown(0).unwrap(), &shown(3).unwrap()));
    }

    #[test]
    fn a_failed_first_frame_takes_the_next_cut_one_and_empty_slots_stay_blank() {
        let frames = done(vec![StripFrame::Failed, cut(), StripFrame::Empty]);
        let pictures = frames.pictures();
        assert!(pictures.first().is_some_and(Option::is_some));
        assert!(pictures.get(2).is_some_and(Option::is_none));
        assert!(done(vec![StripFrame::Failed]).pictures()[0].is_none());
    }
}
