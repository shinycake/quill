//! The story viewer's clock: when a photo story started and how long it
//! has run, and how long a failed fetch is waited out. The app reads the
//! time; a test stops it (`freeze`) and moves it on (`skip`), so a story
//! does not run out while a busy machine is slow between two steps.
#![deny(clippy::indexing_slicing)]

#[cfg(test)]
use std::time::Duration;
use std::time::Instant;

#[derive(Debug, Default)]
pub(in crate::ui) struct StoryClock {
    /// Where a test stopped the clock.
    #[cfg(test)]
    frozen: Option<Instant>,
}

impl StoryClock {
    /// The time the viewer's playback and waits are judged by.
    pub(in crate::ui) fn now(&self) -> Instant {
        #[cfg(test)]
        if let Some(at) = self.frozen {
            return at;
        }
        Instant::now()
    }

    /// Stop the clock where it is: from then on only [`Self::skip`] moves it.
    #[cfg(test)]
    pub(in crate::ui) fn freeze(&mut self) {
        self.frozen = Some(Instant::now());
    }

    /// Move a stopped clock on by `by`.
    #[cfg(test)]
    pub(in crate::ui) fn skip(&mut self, by: Duration) {
        if let Some(at) = self.frozen.as_mut() {
            *at = at.checked_add(by).unwrap_or(*at);
        }
    }
}
