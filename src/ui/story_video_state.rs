//! The current video story's playback state: one value per story, so the
//! wait clock, the player and the fallback all belong to the story they
//! were made for and a different story always starts over.
#![deny(clippy::indexing_slicing)]

use super::video_player::VideoPlayer;
use std::time::{Duration, Instant};

/// `(chat id, story id)`.
pub(super) type StoryKey = (i64, i32);

/// How long a video story waits for its clip to start downloading before
/// the viewer falls back to the thumbnail and the duration clock.
pub(super) const VIDEO_WAIT: Duration = Duration::from_secs(6);

#[derive(Default)]
pub(super) enum StoryVideo {
    /// No video story is driving playback.
    #[default]
    Idle,
    /// The clip is not local yet; waiting since `since`.
    Waiting { key: StoryKey, since: Instant },
    /// The native player drives the story.
    Playing {
        key: StoryKey,
        player: VideoPlayer,
        /// The viewer paused it (hold, Space, a panel), so the viewer
        /// resumes it.
        paused_by_us: bool,
        /// When `play()` was last asked for.
        play_at: Instant,
    },
    /// The clip could not play: the thumbnail and the duration clock
    /// drive this story.
    Fallback { key: StoryKey },
}

impl StoryVideo {
    pub(super) fn key(&self) -> Option<StoryKey> {
        match self {
            Self::Idle => None,
            Self::Waiting { key, .. } | Self::Playing { key, .. } | Self::Fallback { key } => {
                Some(*key)
            }
        }
    }

    pub(super) fn player(&self) -> Option<&VideoPlayer> {
        match self {
            Self::Playing { player, .. } => Some(player),
            _ => None,
        }
    }

    /// The clip of `key` could not play.
    pub(super) fn failed_for(&self, key: Option<StoryKey>) -> bool {
        matches!(self, Self::Fallback { key: failed } if Some(*failed) == key)
    }

    /// Start waiting for `key` unless this state already belongs to it.
    pub(super) fn follow(&mut self, key: StoryKey, now: Instant) {
        if self.key() != Some(key) {
            *self = Self::Waiting { key, since: now };
        }
    }

    /// Let a failed story try its clip again (the story was shown anew).
    pub(super) fn retry(&mut self) {
        if matches!(self, Self::Fallback { .. }) {
            *self = Self::Idle;
        }
    }

    /// Fall back once the clip has waited out [`VIDEO_WAIT`] with nothing
    /// downloading. Returns whether it fell back.
    pub(super) fn give_up_if_stalled(&mut self, downloading: bool, now: Instant) -> bool {
        let Self::Waiting { key, since } = *self else {
            return false;
        };
        if downloading || now.saturating_duration_since(since) <= VIDEO_WAIT {
            return false;
        }
        *self = Self::Fallback { key };
        true
    }

    /// Whether the viewer paused the player itself.
    pub(super) fn paused_by_us(&self) -> bool {
        matches!(
            self,
            Self::Playing {
                paused_by_us: true,
                ..
            }
        )
    }

    pub(super) fn set_paused_by_us(&mut self, paused: bool) {
        if let Self::Playing { paused_by_us, .. } = self {
            *paused_by_us = paused;
        }
    }

    /// `play()` was asked for just now.
    pub(super) fn note_play(&mut self, now: Instant) {
        if let Self::Playing {
            play_at,
            paused_by_us,
            ..
        } = self
        {
            *play_at = now;
            *paused_by_us = false;
        }
    }

    /// A just-requested `play()` that may not have started yet.
    pub(super) fn play_requested_within(&self, window: Duration, now: Instant) -> bool {
        matches!(self, Self::Playing { play_at, .. }
            if now.saturating_duration_since(*play_at) < window)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STORY: StoryKey = (7, 1);
    const NEXT: StoryKey = (7, 2);

    #[test]
    fn a_clip_that_never_downloads_falls_back_after_the_wait() {
        let start = Instant::now();
        let mut video = StoryVideo::default();
        video.follow(STORY, start);
        // The viewer ticks every 100 ms; ticks must not restart the wait.
        for tick in 1..=70 {
            let now = start + Duration::from_millis(100 * tick);
            video.follow(STORY, now);
            if now.saturating_duration_since(start) <= VIDEO_WAIT {
                assert!(!video.give_up_if_stalled(false, now), "gave up at {tick}");
            }
        }
        assert!(video.give_up_if_stalled(false, start + Duration::from_secs(7)));
        assert!(video.failed_for(Some(STORY)));
        assert!(!video.failed_for(Some(NEXT)));
    }

    #[test]
    fn a_downloading_clip_keeps_waiting() {
        let start = Instant::now();
        let mut video = StoryVideo::default();
        video.follow(STORY, start);
        assert!(!video.give_up_if_stalled(true, start + Duration::from_secs(60)));
        assert!(matches!(video, StoryVideo::Waiting { .. }));
    }

    #[test]
    fn another_story_starts_over_and_a_retry_clears_the_fallback() {
        let start = Instant::now();
        let mut video = StoryVideo::Fallback { key: STORY };
        video.follow(STORY, start);
        assert!(video.failed_for(Some(STORY)), "same story stays failed");
        video.follow(NEXT, start);
        assert!(matches!(video, StoryVideo::Waiting { key: NEXT, .. }));
        let mut failed = StoryVideo::Fallback { key: STORY };
        failed.retry();
        assert!(matches!(failed, StoryVideo::Idle));
    }
}
