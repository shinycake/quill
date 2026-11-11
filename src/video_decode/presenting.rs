//! What the player has put on screen since the last seek (or open): no
//! picture yet, the newest one, or the end. One value, replaced whole by a
//! seek, so the first-picture hold, the end position and "ran out" never
//! disagree.
#![deny(clippy::indexing_slicing)]

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Presenting {
    /// No picture shown since the last seek (or open).
    AwaitingFirst,
    /// The newest picture handed out.
    Showing { last_pts: f64 },
    /// Ran out at the end (until the next seek); `last_pts` is the newest
    /// picture handed out, if any, for the end position.
    Ended { last_pts: Option<f64> },
}

impl Presenting {
    pub(super) fn awaiting_first(&self) -> bool {
        matches!(self, Self::AwaitingFirst)
    }

    pub(super) fn is_showing(&self) -> bool {
        matches!(self, Self::Showing { .. })
    }

    pub(super) fn is_ended(&self) -> bool {
        matches!(self, Self::Ended { .. })
    }

    pub(super) fn last_pts(&self) -> Option<f64> {
        match *self {
            Self::AwaitingFirst => None,
            Self::Showing { last_pts } => Some(last_pts),
            Self::Ended { last_pts } => last_pts,
        }
    }

    /// A picture with this time was handed out.
    pub(super) fn shown(&mut self, pts: f64) {
        *self = match *self {
            Self::Ended { .. } => Self::Ended { last_pts: Some(pts) },
            Self::AwaitingFirst | Self::Showing { .. } => Self::Showing { last_pts: pts },
        };
    }

    /// The clip ran out.
    pub(super) fn end(&mut self) {
        *self = Self::Ended {
            last_pts: self.last_pts(),
        };
    }
}
