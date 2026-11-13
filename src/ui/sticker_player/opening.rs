//! How a clip's open stands, and what a window lets go of.
#![deny(clippy::indexing_slicing)]

use super::{ChatId, Part, Place};
use quill::sticker_playback::player::{ClipView, Timeline};
use std::time::{Duration, Instant};

/// How long a view keeps ticking for an open that has not answered. The
/// engine always answers an open (ready or failed) and wakes the UI, so
/// this only stops the polling ticks; it never fails the clip.
pub(super) const OPEN_GIVE_UP: Duration = Duration::from_secs(30);

/// Whether the open of a clip started at `since` has outlived [`OPEN_GIVE_UP`].
pub(super) fn open_gave_up(since: Instant, now: Instant) -> bool {
    now.saturating_duration_since(since) >= OPEN_GIVE_UP
}

/// Whether a clip opened.
#[derive(Clone, Copy)]
pub(super) enum Opening {
    /// Asked of the engine at `since`, not answered yet.
    Pending {
        since: Instant,
    },
    Ready(Timeline),
    Failed,
}

impl Opening {
    /// This open once the engine says `state` of its clip: a pending one
    /// follows the engine's answer, an answered one stays.
    pub(super) fn settled(self, state: Option<ClipView>) -> Self {
        if !matches!(self, Self::Pending { .. }) {
            return self;
        }
        match state {
            Some(ClipView::Ready(info)) => Self::Ready(info.timeline),
            Some(ClipView::Failed(_)) | None => Self::Failed,
            Some(ClipView::Opening) => self,
        }
    }
}

/// What a window lets go of (`Player::leave`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum Leaving {
    /// Its chat: another chat opens there, or the chat moves to a window
    /// of its own. The rest of the window stays as it is.
    Chat,
    /// All of it: the window closes.
    Window,
}

impl Leaving {
    /// Whether `window` letting go of this loses what views at `place` use.
    pub(super) fn loses(self, window: Option<ChatId>, place: Place) -> bool {
        place.window == window && (self == Leaving::Window || place.part == Part::Chat)
    }
}
