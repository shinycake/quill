//! The self-healing rule: a stream that plays through Quill but sends no
//! sound within 5 s of going live, or whose output stops, gets its output
//! reopened, once. A reopen that fails (the device is not back yet) is
//! tried again a few times, [`REOPEN_RETRY`] apart.
#![deny(clippy::indexing_slicing)]

use crate::calls::engine::StreamState;
use std::time::{Duration, Instant};

pub const NO_SOUND_LIMIT: Duration = Duration::from_secs(5);
/// How long after a failed reopen the next one is tried.
pub const REOPEN_RETRY: Duration = Duration::from_secs(5);
/// Failed reopens before the watch gives up.
const MAX_REOPEN_ATTEMPTS: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlaybackHealth {
    pub frames: u64,
    /// The loudest sample so far (absolute), so silent frames show.
    pub peak: u16,
    pub output_failed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallBack {
    NoSound,
    OutputFailed,
}

/// Where the watch is: looking, reopened (nothing more to do), waiting to
/// retry a reopen that failed, or out of attempts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WatchStage {
    Watching {
        live_since: Option<Instant>,
    },
    Reopened {
        attempts: u8,
        reason: FallBack,
    },
    Retry {
        attempts: u8,
        at: Instant,
        reason: FallBack,
    },
    GaveUp,
}

impl Default for WatchStage {
    fn default() -> Self {
        Self::Watching { live_since: None }
    }
}

#[derive(Debug, Default)]
pub struct PlaybackWatch {
    stage: WatchStage,
}

impl PlaybackWatch {
    /// Returns a reason the first time the stream should fall back, and
    /// `None` after that, unless the reopen failed
    /// ([`Self::reopen_failed`]) and its retry is due.
    pub fn observe(
        &mut self,
        now: Instant,
        state: StreamState,
        health: PlaybackHealth,
    ) -> Option<FallBack> {
        let live_since = match &mut self.stage {
            WatchStage::Reopened { .. } | WatchStage::GaveUp => return None,
            WatchStage::Retry {
                attempts,
                at,
                reason,
            } => {
                if now < *at {
                    return None;
                }
                let (attempts, reason) = (*attempts, *reason);
                self.stage = WatchStage::Reopened { attempts, reason };
                return Some(reason);
            }
            WatchStage::Watching { live_since } => live_since,
        };
        let reason = if health.output_failed {
            Some(FallBack::OutputFailed)
        } else if health.frames > 0 || state != StreamState::Live {
            *live_since = None;
            None
        } else {
            let since = *live_since.get_or_insert(now);
            (now.saturating_duration_since(since) >= NO_SOUND_LIMIT).then_some(FallBack::NoSound)
        };
        if let Some(reason) = reason {
            self.stage = WatchStage::Reopened {
                attempts: 0,
                reason,
            };
        }
        reason
    }

    /// The reopen the last [`Self::observe`] asked for failed: try again
    /// after [`REOPEN_RETRY`], a few times, then leave the stream alone.
    pub fn reopen_failed(&mut self, now: Instant) {
        if let WatchStage::Reopened { attempts, reason } = self.stage {
            let attempts = attempts.saturating_add(1);
            self.stage = if attempts >= MAX_REOPEN_ATTEMPTS {
                WatchStage::GaveUp
            } else {
                WatchStage::Retry {
                    attempts,
                    at: now + REOPEN_RETRY,
                    reason,
                }
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FallBack, NO_SOUND_LIMIT, PlaybackHealth, PlaybackWatch, REOPEN_RETRY};
    use crate::calls::engine::StreamState::{Connecting, Live, NoStream};
    use std::time::{Duration, Instant};

    const SILENT: PlaybackHealth = PlaybackHealth {
        frames: 0,
        peak: 0,
        output_failed: false,
    };

    #[test]
    fn five_silent_live_seconds_fall_back_once() {
        let start = Instant::now();
        let mut watch = PlaybackWatch::default();
        assert_eq!(watch.observe(start, Live, SILENT), None);
        let almost = start + Duration::from_millis(4_999);
        assert_eq!(watch.observe(almost, Live, SILENT), None);
        assert_eq!(
            watch.observe(start + NO_SOUND_LIMIT, Live, SILENT),
            Some(FallBack::NoSound)
        );
        assert_eq!(
            watch.observe(start + NO_SOUND_LIMIT * 3, Live, SILENT),
            None,
            "once"
        );
    }

    #[test]
    fn the_clock_starts_when_the_stream_goes_live() {
        let start = Instant::now();
        let mut watch = PlaybackWatch::default();
        assert_eq!(
            watch.observe(start + Duration::from_secs(10), Connecting, SILENT),
            None
        );
        let live = start + Duration::from_secs(11);
        assert_eq!(watch.observe(live, Live, SILENT), None);
        assert_eq!(
            watch.observe(live + Duration::from_secs(4), Live, SILENT),
            None
        );
        assert_eq!(
            watch.observe(live + Duration::from_secs(4), NoStream, SILENT),
            None
        );
        let again = live + Duration::from_secs(6);
        assert_eq!(watch.observe(again, Live, SILENT), None);
        assert_eq!(
            watch.observe(again + NO_SOUND_LIMIT, Live, SILENT),
            Some(FallBack::NoSound)
        );
    }

    #[test]
    fn sound_keeps_the_stream_on_quills_output() {
        let start = Instant::now();
        let mut watch = PlaybackWatch::default();
        let playing = PlaybackHealth {
            frames: 3,
            peak: 0,
            output_failed: false,
        };
        assert_eq!(watch.observe(start, Live, playing), None);
        assert_eq!(
            watch.observe(start + Duration::from_secs(60), Live, playing),
            None
        );
    }

    #[test]
    fn a_reopened_output_gets_its_own_five_seconds() {
        let start = Instant::now();
        let mut watch = PlaybackWatch::default();
        let playing = PlaybackHealth {
            frames: 40,
            peak: 0,
            output_failed: false,
        };
        assert_eq!(watch.observe(start, Live, SILENT), None);
        assert_eq!(
            watch.observe(start + Duration::from_secs(1), Live, playing),
            None
        );
        let reopened = start + Duration::from_secs(60);
        assert_eq!(watch.observe(reopened, Live, SILENT), None);
        assert_eq!(
            watch.observe(reopened + Duration::from_millis(4_999), Live, SILENT),
            None
        );
        assert_eq!(
            watch.observe(reopened + NO_SOUND_LIMIT, Live, SILENT),
            Some(FallBack::NoSound)
        );
    }

    #[test]
    fn a_stopped_output_falls_back_at_once() {
        let start = Instant::now();
        let mut watch = PlaybackWatch::default();
        let failed = PlaybackHealth {
            frames: 500,
            peak: 0,
            output_failed: true,
        };
        assert_eq!(
            watch.observe(start, Live, failed),
            Some(FallBack::OutputFailed)
        );
        assert_eq!(watch.observe(start, Live, failed), None);
    }

    #[test]
    fn a_failed_reopen_is_tried_again_a_few_times_then_dropped() {
        let start = Instant::now();
        let failed = PlaybackHealth {
            output_failed: true,
            ..SILENT
        };
        let mut watch = PlaybackWatch::default();
        assert_eq!(
            watch.observe(start, Live, failed),
            Some(FallBack::OutputFailed)
        );
        watch.reopen_failed(start);
        assert_eq!(
            watch.observe(start + REOPEN_RETRY - Duration::from_millis(1), Live, failed),
            None,
            "not before the retry is due"
        );
        let second = start + REOPEN_RETRY;
        assert_eq!(
            watch.observe(second, Live, failed),
            Some(FallBack::OutputFailed)
        );
        watch.reopen_failed(second);
        let third = second + REOPEN_RETRY;
        assert_eq!(
            watch.observe(third, Live, failed),
            Some(FallBack::OutputFailed)
        );
        watch.reopen_failed(third);
        assert_eq!(
            watch.observe(third + REOPEN_RETRY * 10, Live, failed),
            None,
            "out of attempts"
        );
    }

    #[test]
    fn a_reopen_that_worked_is_not_asked_again() {
        let start = Instant::now();
        let failed = PlaybackHealth {
            output_failed: true,
            ..SILENT
        };
        let mut watch = PlaybackWatch::default();
        assert!(watch.observe(start, Live, failed).is_some());
        assert_eq!(watch.observe(start + REOPEN_RETRY * 4, Live, failed), None);
    }
}
