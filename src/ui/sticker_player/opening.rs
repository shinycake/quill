//! How a clip's open stands, and what a window lets go of.
#![deny(clippy::indexing_slicing)]

use super::{ChatId, Part, Place};
use quill::sticker_playback::player::Timeline;
use std::time::{Duration, Instant};

/// How long the engine may take to answer an open before the clip counts
/// as failed (a worker that never answers must not keep views opening).
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

/// What a window lets go of (`Player::leave`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Leaving {
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
