//! Where each story's `getStory` stands: one value per story instead of a
//! set of asked stories, a set of gone stories and a map of retry clocks
//! kept in step by hand.
use crate::ids::RequestId;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// The first wait after a failed `loadActiveStories` or `getStory`; each
/// later failure doubles it, up to [`STORIES_RETRY_MAX`].
pub const STORIES_RETRY_FIRST: Duration = Duration::from_secs(5);
pub const STORIES_RETRY_MAX: Duration = Duration::from_secs(5 * 60);

/// The retry clock after one more failure at `now`.
pub(crate) fn next_retry(
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
    /// `getStory` is out as `request`, so a refresh does not send it again;
    /// only its failure acts on the entry. `wait` is the wait of the failure
    /// this ask follows, so the next failure doubles it.
    Asked {
        request: RequestId,
        wait: Option<Duration>,
    },
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

    /// `getStory` was sent for `story` as `request`: it is asked now, and
    /// the wait of an earlier failure rides along so the next failure
    /// doubles it. A story known gone stays gone.
    pub fn asked(&mut self, story: (i64, i32), request: RequestId) {
        let wait = match self.0.get(&story) {
            Some(StoryFetch::Gone) => return,
            Some(StoryFetch::Retry { wait, .. }) => Some(*wait),
            Some(StoryFetch::Asked { wait, .. }) => *wait,
            None => None,
        };
        self.0.insert(story, StoryFetch::Asked { request, wait });
    }

    pub fn is_asked(&self, story: (i64, i32)) -> bool {
        matches!(self.0.get(&story), Some(StoryFetch::Asked { .. }))
    }

    /// TDLib said the story is gone.
    pub fn gone(&mut self, story: (i64, i32)) {
        self.0.insert(story, StoryFetch::Gone);
    }

    pub fn is_gone(&self, story: (i64, i32)) -> bool {
        matches!(self.0.get(&story), Some(StoryFetch::Gone))
    }

    /// `getStory` failed with an error that may pass: `request` is the one
    /// that failed, or `None` when it never left. An older ask's failure,
    /// while a newer one is out, changes nothing.
    pub fn failed(&mut self, story: (i64, i32), request: Option<RequestId>, now: Instant) {
        let previous = match self.0.get(&story) {
            Some(StoryFetch::Asked { request: out, .. })
                if request.is_some_and(|failed| failed != *out) =>
            {
                return;
            }
            Some(StoryFetch::Retry { until, wait }) => Some((*until, *wait)),
            Some(StoryFetch::Asked {
                wait: Some(wait), ..
            }) => Some((now, *wait)),
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
        self.0.retain(|_, fetch| {
            if let StoryFetch::Asked { wait, .. } = fetch {
                *wait = None;
            }
            !matches!(fetch, StoryFetch::Retry { .. })
        });
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
        self.0.retain(|_, fetch| match fetch {
            StoryFetch::Asked { wait, .. } => {
                *wait = None;
                true
            }
            _ => false,
        });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_story_asked_after_a_wait_is_asked_and_its_next_failure_doubles() {
        let mut fetches = StoryFetches::default();
        let story = (7, 3);
        let now = Instant::now();
        fetches.failed(story, None, now);
        assert_eq!(
            fetches.retry(story).map(|(_, wait)| wait),
            Some(STORIES_RETRY_FIRST)
        );
        // The wait is over and the story is asked again: it is asked now.
        fetches.asked(story, RequestId(1));
        assert!(fetches.is_asked(story));
        assert!(fetches.retry(story).is_none());
        // An older request's failure leaves the ask that is out alone.
        fetches.failed(story, Some(RequestId(0)), now);
        assert!(fetches.is_asked(story));
        fetches.failed(story, Some(RequestId(1)), now);
        assert_eq!(
            fetches.retry(story).map(|(_, wait)| wait),
            Some(STORIES_RETRY_FIRST * 2)
        );
        // A story known gone is not asked again by a late ask.
        fetches.gone(story);
        fetches.asked(story, RequestId(2));
        assert!(fetches.is_gone(story));
    }
}
