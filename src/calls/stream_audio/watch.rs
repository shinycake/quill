//! The self-healing rule: a stream that plays through Quill but sends no
//! sound within 5 s of going live, or whose output stops, goes back to
//! ntgcalls' own device playback, once. A wrong assumption about external
//! playback must never leave a stream silent.
#![deny(clippy::indexing_slicing)]

use std::time::{Duration, Instant};

/// How long a live stream may go without a playback frame.
pub const NO_SOUND_LIMIT: Duration = Duration::from_secs(5);

/// What the output side reports for one stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlaybackHealth {
    /// Playback frames received since the output opened.
    pub frames: u64,
    /// The output device stopped for good.
    pub output_failed: bool,
}

/// Why a stream went back to device playback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallBack {
    NoSound,
    OutputFailed,
}

/// One stream's watch, fed on every driver poll.
#[derive(Debug, Default)]
pub struct PlaybackWatch {
    live_since: Option<Instant>,
    fell_back: bool,
}

impl PlaybackWatch {
    /// `live`: segments are flowing (the stream is not just connecting
    /// or waiting for a broadcaster, when no sound is expected). Returns
    /// a reason the first time the stream should fall back, and `None`
    /// on every call after that.
    pub fn observe(&mut self, now: Instant, live: bool, health: PlaybackHealth) -> Option<FallBack> {
        if self.fell_back {
            return None;
        }
        let reason = if health.output_failed {
            Some(FallBack::OutputFailed)
        } else if health.frames > 0 {
            None
        } else if !live {
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
    use std::time::{Duration, Instant};

    const SILENT: PlaybackHealth = PlaybackHealth {
        frames: 0,
        output_failed: false,
    };

    #[test]
    fn five_silent_live_seconds_fall_back_once() {
        let start = Instant::now();
        let mut watch = PlaybackWatch::default();
        assert_eq!(watch.observe(start, true, SILENT), None);
        let almost = start + NO_SOUND_LIMIT - Duration::from_millis(1);
        assert_eq!(watch.observe(almost, true, SILENT), None);
        assert_eq!(
            watch.observe(start + NO_SOUND_LIMIT, true, SILENT),
            Some(FallBack::NoSound)
        );
        assert_eq!(watch.observe(start + NO_SOUND_LIMIT * 3, true, SILENT), None, "once");
    }

    #[test]
    fn the_clock_starts_when_the_stream_goes_live() {
        let start = Instant::now();
        let mut watch = PlaybackWatch::default();
        // Ten seconds of connecting, or of nobody broadcasting.
        assert_eq!(watch.observe(start + Duration::from_secs(10), false, SILENT), None);
        let live = start + Duration::from_secs(11);
        assert_eq!(watch.observe(live, true, SILENT), None);
        assert_eq!(watch.observe(live + Duration::from_secs(4), true, SILENT), None);
        // Going back to waiting restarts the clock.
        assert_eq!(watch.observe(live + Duration::from_secs(4), false, SILENT), None);
        let again = live + Duration::from_secs(6);
        assert_eq!(watch.observe(again, true, SILENT), None);
        assert_eq!(watch.observe(again + NO_SOUND_LIMIT, true, SILENT), Some(FallBack::NoSound));
    }

    #[test]
    fn sound_keeps_the_stream_on_quills_output() {
        let start = Instant::now();
        let mut watch = PlaybackWatch::default();
        let playing = PlaybackHealth {
            frames: 3,
            output_failed: false,
        };
        assert_eq!(watch.observe(start, true, playing), None);
        assert_eq!(watch.observe(start + Duration::from_secs(60), true, playing), None);
    }

    #[test]
    fn a_stopped_output_falls_back_at_once() {
        let start = Instant::now();
        let mut watch = PlaybackWatch::default();
        let failed = PlaybackHealth {
            frames: 500,
            output_failed: true,
        };
        assert_eq!(watch.observe(start, true, failed), Some(FallBack::OutputFailed));
        assert_eq!(watch.observe(start, true, failed), None);
    }
}
