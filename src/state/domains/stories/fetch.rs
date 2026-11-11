//! Where each story's `getStory` stands: one value per story instead of a
//! set of asked stories, a set of gone stories and a map of retry clocks
//! kept in step by hand.
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// The first wait after a failed `loadActiveStories` or `getStory`; each
/// later failure doubles it, up to [`STORIES_RETRY_MAX`].
pub const STORIES_RETRY_FIRST: Duration = Duration::from_secs(5);
pub const STORIES_RETRY_MAX: Duration = Duration::from_secs(5 * 60);

/// The retry clock after one more failure at `now`.
pub(super) fn next_retry(
    previous: Option<(Instant, Duration)>,
    now: Instant,
) -> (Instant, Duration) {
    let wait = previous.map_or(STORIES_RETRY_FIRST, |(_, wait)| {
        wait.saturating_mul(2).min(STORIES_RETRY_MAX)
    });
    (now.checked_add(wait).unwrap_or(now), wait)
}

/// How the fetch of one story stands, until the story arrives (then there
/// is no entry: the story cache holds it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoryFetch {
    /// `getStory` was sent for a reply strip or a story card, so a refresh
    /// does not send it again.
    Asked,
    /// TDLib answered 400 or 404: the story is gone or not reachable.
    Gone,
    /// `getStory` failed with an error that may pass (a timeout,
    /// FLOOD_WAIT, a 500): no prefetch asks again before `until`; `wait`
    /// doubles per failure. A user opening the story drops the wait.
    Retry { until: Instant, wait: Duration },
}

/// The fetch stage of every story that has one, by `(poster chat, story)`.
#[derive(Debug, Clone, Default)]
pub struct StoryFetches(HashMap<(i64, i32), StoryFetch>);

impl StoryFetches {
    pub fn get(&self, story: (i64, i32)) -> Option<&StoryFetch> {
        self.0.get(&story)
    }

    /// `getStory` was sent for `story`. A wait left by an earlier failure
    /// stays, so the next failure doubles it.
    pub fn asked(&mut self, story: (i64, i32)) {
        self.0.entry(story).or_insert(StoryFetch::Asked);
    }

    pub fn is_asked(&self, story: (i64, i32)) -> bool {
        matches!(self.0.get(&story), Some(StoryFetch::Asked))
    }

    /// TDLib said the story is gone.
    pub fn gone(&mut self, story: (i64, i32)) {
        self.0.insert(story, StoryFetch::Gone);
    }

    pub fn is_gone(&self, story: (i64, i32)) -> bool {
        matches!(self.0.get(&story), Some(StoryFetch::Gone))
    }

    /// `getStory` failed with an error that may pass.
    pub fn failed(&mut self, story: (i64, i32), now: Instant) {
        let previous = match self.0.get(&story) {
            Some(StoryFetch::Retry { until, wait }) => Some((*until, *wait)),
            _ => None,
        };
        let (until, wait) = next_retry(previous, now);
        self.0.insert(story, StoryFetch::Retry { until, wait });
    }

    /// The wait before the next ask, if one is running.
    pub fn retry(&self, story: (i64, i32)) -> Option<(Instant, Duration)> {
        match self.0.get(&story) {
            Some(StoryFetch::Retry { until, wait }) => Some((*until, *wait)),
            _ => None,
        }
    }

    /// The story still waits out a failed fetch at `now`.
    pub fn waiting(&self, story: (i64, i32), now: Instant) -> bool {
        self.retry(story).is_some_and(|(until, _)| now < until)
    }

    /// The wait ends sooner than its clock (the user opened the story).
    pub fn forget_retry(&mut self, story: (i64, i32)) {
        if matches!(self.0.get(&story), Some(StoryFetch::Retry { .. })) {
            self.0.remove(&story);
        }
    }

    /// Every wait ends (back online).
    pub fn forget_retries(&mut self) {
        self.0
            .retain(|_, fetch| !matches!(fetch, StoryFetch::Retry { .. }));
    }

    /// The story arrived: whatever said it was gone or waiting is over.
    pub fn arrived(&mut self, story: (i64, i32)) {
        self.0.remove(&story);
    }

    /// The poster chat became known: what was learnt about its stories
    /// before does not hold.
    pub fn poster_known(&mut self, chat_id: i64) {
        self.0.retain(|(chat, _), _| *chat != chat_id);
    }

    /// Gone stories and waits go (a new session of the list); stories asked
    /// for stay asked.
    pub fn forget_outcomes(&mut self) {
        self.0.retain(|_, fetch| matches!(fetch, StoryFetch::Asked));
    }

    /// The wait of `story` is over now (a test lets time pass).
    #[cfg(test)]
    pub fn end_wait(&mut self, story: (i64, i32)) {
        if let Some(StoryFetch::Retry { until, .. }) = self.0.get_mut(&story) {
            *until = Instant::now();
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
