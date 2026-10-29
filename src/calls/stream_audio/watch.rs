//! The self-healing rule: a stream that plays through Quill but sends no
//! sound within 5 s of going live, or whose output stops, gets its output
//! reopened, once.
#![deny(clippy::indexing_slicing)]

use crate::calls::engine::StreamState;
use std::time::{Duration, Instant};

pub const NO_SOUND_LIMIT: Duration = Duration::from_secs(5);

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

#[derive(Debug, Default)]
pub struct PlaybackWatch {
    live_since: Option<Instant>,
    fell_back: bool,
}

impl PlaybackWatch {
    /// Returns a reason the first time the stream should fall back, and
    /// `None` on every call after that.
    pub fn observe(
        &mut self,
        now: Instant,
        state: StreamState,
        health: PlaybackHealth,
    ) -> Option<FallBack> {
        if self.fell_back {
            return None;
        }
        let reason = if health.output_failed {
            Some(FallBack::OutputFailed)
        } else if health.frames > 0 || state != StreamState::Live {
            self.live_since = None;
            None
        } else {
            let since = *self.live_since.get_or_insert(now);
            (now.saturating_duration_since(since) >= NO_SOUND_LIMIT).then_some(FallBack::NoSound)
        };
        self.fell_back = reason.is_some();
        reason
    }
}

#[cfg(test)]
mod tests {
    use super::{FallBack, NO_SOUND_LIMIT, PlaybackHealth, PlaybackWatch};
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
}
