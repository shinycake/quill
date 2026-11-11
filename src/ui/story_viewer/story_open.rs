//! What the story viewer shows, and the press that waits to replace it.
#![deny(clippy::indexing_slicing)]

use super::open::PendingStoryOpen;
use quill::story_viewer::StoryViewer;
use std::ops::{Deref, DerefMut};

/// The story viewer and the open waiting for its `getStory` answer, as one
/// value: a press waits only beside the viewer it will replace, and every
/// way out (the story arrived, a newer press, the user stepping on, the
/// viewer closing) is one transition of this value. It reads as the
/// [`StoryViewer`] it holds.
pub(in crate::ui) enum StoryOpen {
    /// The viewer shows this (closed when it has no stories); no open waits.
    Shown(StoryViewer),
    /// A press waits for its story: `shown` is what the viewer shows
    /// meanwhile. A repost's press (`if_gone`) holds the story still.
    Waiting {
        open: PendingStoryOpen,
        shown: StoryViewer,
    },
}

impl StoryOpen {
    pub(in crate::ui) fn closed() -> Self {
        Self::Shown(StoryViewer::closed())
    }

    /// The press waiting for its story, if any.
    pub(in crate::ui) fn waiting(&self) -> Option<PendingStoryOpen> {
        match self {
            Self::Shown(_) => None,
            Self::Waiting { open, .. } => Some(*open),
        }
    }

    fn take_viewer(&mut self) -> StoryViewer {
        match std::mem::replace(self, Self::closed()) {
            Self::Shown(viewer) | Self::Waiting { shown: viewer, .. } => viewer,
        }
    }

    /// `open` waits for its story; a press that waited before is replaced.
    pub(in crate::ui) fn wait(&mut self, open: PendingStoryOpen) {
        let shown = self.take_viewer();
        *self = Self::Waiting { open, shown };
    }

    /// Nothing waits any more (the story arrived or is gone, or the user
    /// moved on); the viewer stays as it is.
    pub(in crate::ui) fn stop_waiting(&mut self) {
        let shown = self.take_viewer();
        *self = Self::Shown(shown);
    }

    /// Show `viewer` in place of what was shown. A press still waiting is
    /// replaced by it.
    pub(in crate::ui) fn show(&mut self, viewer: StoryViewer) {
        *self = Self::Shown(viewer);
    }

    /// Close the viewer; no press waits after it.
    pub(in crate::ui) fn close(&mut self) {
        *self = Self::closed();
    }
}

impl Deref for StoryOpen {
    type Target = StoryViewer;

    fn deref(&self) -> &StoryViewer {
        match self {
            Self::Shown(viewer) | Self::Waiting { shown: viewer, .. } => viewer,
        }
    }
}

impl DerefMut for StoryOpen {
    fn deref_mut(&mut self) -> &mut StoryViewer {
        match self {
            Self::Shown(viewer) | Self::Waiting { shown: viewer, .. } => viewer,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quill::story_viewer::StoryOpenScope;

    fn open(story_id: i32) -> PendingStoryOpen {
        PendingStoryOpen {
            chat_id: 1,
            story_id,
            scope: StoryOpenScope::Single,
            if_gone: None,
        }
    }

    #[test]
    fn a_newer_press_replaces_the_one_waiting() {
        let mut state = StoryOpen::closed();
        state.wait(open(1));
        state.wait(open(2));
        assert_eq!(state.waiting().map(|o| o.story_id), Some(2));
    }

    #[test]
    fn showing_a_story_ends_the_wait() {
        let mut state = StoryOpen::closed();
        state.wait(open(1));
        state.show(StoryViewer::closed());
        assert_eq!(state.waiting(), None);
    }

    #[test]
    fn closing_and_stopping_end_the_wait() {
        let mut state = StoryOpen::closed();
        state.wait(open(1));
        state.stop_waiting();
        assert_eq!(state.waiting(), None);
        state.wait(open(3));
        state.close();
        assert_eq!(state.waiting(), None);
        assert!(!state.is_open());
    }
}
