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
    /// The file could not be read or seeked; a seek tries again.
    Failed(String),
    /// The decode thread has ended (the file never opened, or the thread
    /// died): nothing is left to take a seek, so the failure stays.
    Gone(String),
    /// The player is gone: the thread leaves.
    Stopped,
}

impl Decoding {
    pub(super) fn is_stopped(&self) -> bool {
        matches!(self, Self::Stopped)
    }

    /// Nothing more will be read until a seek (the end or a failure).
    pub(super) fn at_end(&self) -> bool {
        matches!(self, Self::Ended | Self::Failed(_) | Self::Gone(_))
    }

    pub(super) fn seek_pending(&self) -> bool {
        matches!(self, Self::Seeking(_))
    }

    pub(super) fn error(&self) -> Option<&String> {
        match self {
            Self::Failed(error) | Self::Gone(error) => Some(error),
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

    /// A seek waits for the thread, unless there is none to take it (the
    /// player is stopped, or the thread is gone): that would leave the clip
    /// reading as opening with nothing to settle it.
    pub(super) fn seek(&mut self, to: f64) {
        if !matches!(self, Self::Stopped | Self::Gone(_)) {
            *self = Self::Seeking(to);
        }
    }

    pub(super) fn fail(&mut self, error: String) {
        if !matches!(self, Self::Stopped | Self::Gone(_)) {
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

    /// The thread ended without being asked to: a failure that stays,
    /// keeping the reason it already has.
    pub(super) fn thread_died(&mut self) {
        *self = match std::mem::take(self) {
            Self::Stopped => Self::Stopped,
            Self::Failed(error) | Self::Gone(error) => Self::Gone(error),
            _ => Self::Gone("Video playback stopped unexpectedly".to_string()),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::Decoding;

    /// A seek after the thread is gone leaves the failure: it would
    /// otherwise read as a clip still opening, with no thread to settle it.
    #[test]
    fn a_seek_does_not_hide_a_dead_thread() {
        let mut state = Decoding::Failed("no such file".into());
        state.thread_died();
        state.seek(3.0);
        assert_eq!(state.error().map(String::as_str), Some("no such file"));
        assert!(!state.seek_pending());
        state.fail("later".into());
        assert_eq!(state.error().map(String::as_str), Some("no such file"));
    }

    /// A failure with the thread alive is retried by a seek.
    #[test]
    fn a_seek_retries_a_failure_the_thread_can_still_take() {
        let mut state = Decoding::Failed("read error".into());
        state.seek(1.0);
        assert!(state.seek_pending());
        assert_eq!(state.take_seek(), Some(1.0));
        assert_eq!(state, Decoding::Active);
    }

    #[test]
    fn a_thread_that_dies_unasked_is_a_failure_that_stays_and_a_stop_stays_a_stop() {
        let mut state = Decoding::Active;
        state.thread_died();
        assert!(state.error().is_some() && state.at_end());
        let mut stopped = Decoding::Stopped;
        stopped.thread_died();
        assert!(stopped.is_stopped());
    }
}
