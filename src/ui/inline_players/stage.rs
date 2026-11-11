//! The stages of one inline player slot (`inline_players.rs`): where it is
//! against the screen, and what it does with sound.
#![deny(clippy::indexing_slicing)]

use std::time::Instant;

/// Where a slot's player is against the screen.
///
/// - `InView`: painted in view, playing (or held by the transport).
/// - `OffScreen`: not painted in view since `since`: paused, released after
///   `RELEASE_AFTER`.
/// - `Asleep`: off screen for `RELEASE_AFTER`; the player has no backend,
///   only its last picture (`VideoPlayer::hibernate_on_last_picture`),
///   until a sweep sees it painted in view.
/// - `Stuck`: a sweep saw it in view, but its file no longer opens
///   (deleted, moved): the last picture stays as a still and no sweep tries
///   again. The slot's row leaving the history ends it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Presence {
    InView,
    OffScreen { since: Instant },
    Asleep,
    Stuck,
}

impl Presence {
    /// Whether the player has a decoder.
    pub(super) fn is_awake(self) -> bool {
        matches!(self, Self::InView | Self::OffScreen { .. })
    }

    pub(super) fn off_screen_since(self) -> Option<Instant> {
        match self {
            Self::OffScreen { since } => Some(since),
            _ => None,
        }
    }
}

/// What a slot does with sound. A click plays a muted loop once with sound,
/// pauses a sounding one, resumes a paused one; a drag along the seek ring
/// holds it still and resumes on release if it was playing. It returns to
/// `Muted` when the clip ends with sound, or when another clip takes the
/// sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Audio {
    Muted,
    Sounding { paused: bool },
    Seeking { was_playing: bool },
}

impl Audio {
    pub(super) fn has_sound(self) -> bool {
        self != Self::Muted
    }

    pub(super) fn is_seeking(self) -> bool {
        matches!(self, Self::Seeking { .. })
    }

    /// Paused by a click, or held by a seek drag.
    pub(super) fn is_held(self) -> bool {
        matches!(self, Self::Sounding { paused: true } | Self::Seeking { .. })
    }

    /// Heard now: sounding and not paused (a seek drag keeps what it
    /// found).
    pub(super) fn is_heard(self) -> bool {
        matches!(
            self,
            Self::Sounding { paused: false } | Self::Seeking { was_playing: true }
        )
    }
}
