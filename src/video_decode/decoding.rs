//! What the decode thread is doing, as one value: reading on, a seek
//! waiting for it, the end of the file, a failure, or told to stop. The
//! player and the thread both read it under the state lock.
#![deny(clippy::indexing_slicing)]

#[derive(Default, Debug, Clone, PartialEq)]
pub(super) enum Decoding {
    #[default]
    Active,
    /// A seek to this second waits for the thread (it clears the queue's
    /// end and any failure: reading goes on from there).
    Seeking(f64),
    /// The file is read to its end.
    Ended,
    /// The file could not be opened, read or seeked.
    Failed(String),
    /// The player is gone: the thread leaves.
    Stopped,
}

impl Decoding {
    pub(super) fn is_stopped(&self) -> bool {
        matches!(self, Self::Stopped)
    }

    /// Nothing more will be read until a seek (the end or a failure).
    pub(super) fn at_end(&self) -> bool {
        matches!(self, Self::Ended | Self::Failed(_))
    }

    pub(super) fn seek_pending(&self) -> bool {
        matches!(self, Self::Seeking(_))
    }

    pub(super) fn error(&self) -> Option<&String> {
        match self {
            Self::Failed(error) => Some(error),
            _ => None,
        }
    }

    /// The target of a pending seek, taken: reading goes on.
    pub(super) fn take_seek(&mut self) -> Option<f64> {
        match *self {
            Self::Seeking(target) => {
                *self = Self::Active;
                Some(target)
            }
            _ => None,
        }
    }

    pub(super) fn seek(&mut self, to: f64) {
        if !self.is_stopped() {
            *self = Self::Seeking(to);
        }
    }

    pub(super) fn fail(&mut self, error: String) {
        if !self.is_stopped() {
            *self = Self::Failed(error);
        }
    }

    /// The file ran out; a failure stays one.
    pub(super) fn end(&mut self) {
        if matches!(self, Self::Active) {
            *self = Self::Ended;
        }
    }

    pub(super) fn stop(&mut self) {
        *self = Self::Stopped;
    }

    /// The thread ended without being asked to: a failure, keeping the
    /// reason it already has.
    pub(super) fn thread_died(&mut self) {
        match self {
            Self::Stopped | Self::Failed(_) => {}
            _ => *self = Self::Failed("Video playback stopped unexpectedly".to_string()),
        }
    }
}
