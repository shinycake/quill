//! Microphone levels and speaking detection, as tgcalls measures them and
//! Telegram Desktop reads them. No audio I/O here: the level tap
//! (`calls::level_tap`) feeds samples in, the driver and the UI read
//! levels, speaking state and animation values out.
//!
//! The scale is tgcalls' (`group/GroupInstanceCustomImpl.cpp`, the
//! capture sink): the loudest sample of each 4,400-sample window, on the
//! 16-bit scale, divided by 4,000. Without a voice detector tgcalls calls
//! a window "voice" when that level reaches 1.0. tdesktop then treats a
//! level above `kSpeakLevelThreshold` (0.2) as sound
//! (`calls_group_call.cpp`), keeps a participant "sounding" or "speaking"
//! for `kSoundStatusKeptFor` (1,500 ms) after the last such window
//! (`data_group_call.cpp`), and refreshes its speaking send action every
//! `kUpdateSendActionEach` (500 ms).

/// tdesktop `GroupCall::kSpeakLevelThreshold`: a level above this counts
/// as sound.
pub const SPEAK_LEVEL_THRESHOLD: f32 = 0.2;
/// tgcalls: one level per this many samples (92 ms at 48 kHz).
pub const LEVEL_WINDOW_SAMPLES: usize = 4400;
/// tgcalls: `level = peak / 4000` with the peak on the 16-bit scale.
pub const LEVEL_PEAK_DIVISOR: f32 = 4000.0;
/// tgcalls without a voice detector: a window is voice from this level.
pub const VOICE_LEVEL: f32 = 1.0;
/// tdesktop `Data::GroupCall::kSoundStatusKeptFor`.
pub const SOUND_STATUS_KEPT_MS: u64 = 1500;
/// tdesktop `kUpdateSendActionEach`: how often the speaking state is sent
/// while sound continues.
pub const SEND_ACTION_EACH_MS: u64 = 500;
/// tdesktop `kLevelDuration` (`calls_group_members_row.cpp`,
/// `100 + 500 * 0.23`): the blobs follow a new level over this time.
pub const LEVEL_ANIMATION_MS: u64 = 215;
/// tdesktop `kMicTestUpdateInterval`: the settings meter samples the
/// microphone this often.
pub const MIC_TEST_UPDATE_MS: u64 = 100;
/// tdesktop `kMicTestAnimationDuration`: the meter moves to a new value
/// over this time.
pub const MIC_TEST_ANIMATION_MS: u64 = 200;
/// tdesktop `groupCallLevelMeter` (`calls.style`): 44 lines, 3 px wide,
/// 5 px apart, 18 px tall.
pub const METER_LINE_COUNT: usize = 44;
pub const METER_LINE_WIDTH: f32 = 3.0;
pub const METER_LINE_SPACING: f32 = 5.0;
pub const METER_HEIGHT: f32 = 18.0;

/// tgcalls' level for the loudest sample (`-1.0..=1.0`) of a window.
pub fn level_from_peak(peak: f32) -> f32 {
    (peak.abs() * 32768.0 / LEVEL_PEAK_DIVISOR).max(0.0)
}

/// The loudest sample (`0.0..=1.0` of full scale) behind a level: what
/// tdesktop's settings meter shows (`AudioInputTester` reports the
/// peak as a fraction of full scale).
pub fn peak_from_level(level: f32) -> f32 {
    (level * LEVEL_PEAK_DIVISOR / 32768.0).clamp(0.0, 1.0)
}

/// Collects samples into tgcalls' windows and reports one level per
/// window.
#[derive(Debug, Default, Clone)]
pub struct PeakWindow {
    peak: f32,
    count: usize,
}

impl PeakWindow {
    /// Feed samples in `-1.0..=1.0`; `on_level` runs once per completed
    /// window with its level.
    pub fn push(&mut self, samples: &[f32], mut on_level: impl FnMut(f32)) {
        for &sample in samples {
            let sample = sample.abs();
            if sample > self.peak {
                self.peak = sample;
            }
            self.count += 1;
            if self.count >= LEVEL_WINDOW_SAMPLES {
                on_level(level_from_peak(self.peak));
                self.peak = 0.0;
                self.count = 0;
            }
        }
    }
}

/// What a level update means for the speaking state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SpeakingUpdate {
    /// Whether you count as speaking now.
    pub speaking: bool,
    /// A `setGroupCallParticipantIsSpeaking` to send, with its value:
    /// on every change, and every [`SEND_ACTION_EACH_MS`] while sound
    /// continues.
    pub send: Option<bool>,
}

/// tdesktop's "last spoke" bookkeeping for your own microphone
/// (`GroupCall::audioLevelsUpdated` + `Data::GroupCall::applyLastSpoke`).
#[derive(Debug, Default, Clone)]
pub struct SpeakingTracker {
    last_sound_ms: Option<u64>,
    last_voice_ms: Option<u64>,
    last_sent: Option<(u64, bool)>,
    /// The latest level, for the UI.
    level: f32,
}

impl SpeakingTracker {
    /// Apply one level window measured at `now_ms`. A muted microphone
    /// passes `muted`, and its levels are ignored as in tdesktop.
    pub fn update(&mut self, level: f32, muted: bool, now_ms: u64) -> SpeakingUpdate {
        let level = if muted { 0.0 } else { level };
        self.level = level;
        if level > SPEAK_LEVEL_THRESHOLD {
            self.last_sound_ms = Some(now_ms);
            if level >= VOICE_LEVEL {
                self.last_voice_ms = Some(now_ms);
            }
        }
        let speaking = self.is_speaking(now_ms);
        let send = match self.last_sent {
            Some((_, was)) if was != speaking => Some(speaking),
            Some((at, true)) if speaking && at + SEND_ACTION_EACH_MS <= now_ms => Some(true),
            Some(_) => None,
            None if speaking => Some(true),
            None => None,
        };
        if let Some(value) = send {
            self.last_sent = Some((now_ms, value));
        }
        SpeakingUpdate { speaking, send }
    }

    /// Sound above the threshold within the kept-for window.
    pub fn is_sounding(&self, now_ms: u64) -> bool {
        self.last_sound_ms
            .is_some_and(|at| at + SOUND_STATUS_KEPT_MS >= now_ms)
    }

    /// Voice within the kept-for window (tdesktop: sounding and voice).
    pub fn is_speaking(&self, now_ms: u64) -> bool {
        self.is_sounding(now_ms)
            && self
                .last_voice_ms
                .is_some_and(|at| at + SOUND_STATUS_KEPT_MS >= now_ms)
    }

    /// The last level applied (0 while muted).
    pub fn level(&self) -> f32 {
        self.level
    }

    /// Forget everything, as when the call ends; the next sound sends
    /// `true` again. Returns whether a final `false` should be sent.
    pub fn reset(&mut self) -> bool {
        let was_speaking = matches!(self.last_sent, Some((_, true)));
        *self = Self::default();
        was_speaking
    }
}

/// A linear move from one level to another over a fixed time (tdesktop
/// `Ui::Animations::Simple` with the default transition).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LevelAnimation {
    from: f32,
    to: f32,
    started_ms: u64,
    duration_ms: u64,
}

impl LevelAnimation {
    pub fn new(level: f32, duration_ms: u64) -> Self {
        Self {
            from: level,
            to: level,
            started_ms: 0,
            duration_ms,
        }
    }

    /// Head for `to` from wherever the animation is at `now_ms`.
    pub fn retarget(&mut self, to: f32, now_ms: u64) {
        if (to - self.to).abs() < f32::EPSILON {
            return;
        }
        self.from = self.value(now_ms);
        self.to = to;
        self.started_ms = now_ms;
    }

    pub fn target(&self) -> f32 {
        self.to
    }

    pub fn value(&self, now_ms: u64) -> f32 {
        if self.duration_ms == 0 {
            return self.to;
        }
        let elapsed = now_ms.saturating_sub(self.started_ms) as f32;
        let t = (elapsed / self.duration_ms as f32).clamp(0.0, 1.0);
        self.from + (self.to - self.from) * t
    }

    pub fn is_running(&self, now_ms: u64) -> bool {
        now_ms.saturating_sub(self.started_ms) < self.duration_ms
            && (self.to - self.from).abs() > f32::EPSILON
    }
}

/// tdesktop `LevelMeter::paintEvent`: line `i` is lit while
/// `(i + 1) / count <= value`.
pub fn meter_lit_lines(value: f32, count: usize) -> usize {
    (0..count)
        .take_while(|i| (*i as f32 + 1.0) / count as f32 <= value)
        .count()
}

/// The meter's full width: `count` lines and the gaps between them.
pub fn meter_width(count: usize) -> f32 {
    if count == 0 {
        return 0.0;
    }
    count as f32 * METER_LINE_WIDTH + (count as f32 - 1.0) * METER_LINE_SPACING
}

/// How far the mute button's halo reaches past the disc for a level
/// (tdesktop's blobs grow from their rest radius to `kMaxLevel` 1.0):
/// 6 px at rest, 20 px at full level.
pub fn halo_reach(level: f32) -> f32 {
    6.0 + 14.0 * level.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_follow_tgcalls_scale() {
        // peak 4000 on the 16-bit scale is level 1.0.
        assert!((level_from_peak(4000.0 / 32768.0) - 1.0).abs() < 1e-4);
        // The speaking threshold is a peak of 800 (about -32 dBFS).
        assert!((level_from_peak(800.0 / 32768.0) - 0.2).abs() < 1e-4);
        assert_eq!(level_from_peak(-0.5), level_from_peak(0.5));
        assert_eq!(level_from_peak(0.0), 0.0);
        assert!((peak_from_level(level_from_peak(0.3)) - 0.3).abs() < 1e-5);
        assert_eq!(peak_from_level(100.0), 1.0);
    }

    #[test]
    fn windows_emit_one_level_per_4400_samples_across_pushes() {
        let mut window = PeakWindow::default();
        let mut levels = Vec::new();
        let quiet = vec![0.01f32; 4000];
        window.push(&quiet, |l| levels.push(l));
        assert!(levels.is_empty(), "no full window yet");
        let mut loud = vec![0.0f32; 1000];
        loud[10] = -0.25;
        window.push(&loud, |l| levels.push(l));
        assert_eq!(levels.len(), 1);
        // The loud sample fell in the first window (sample 4010).
        assert!((levels[0] - level_from_peak(0.25)).abs() < 1e-4);
        // The remaining 600 samples carry into the next window.
        window.push(&vec![0.0f32; 3800], |l| levels.push(l));
        assert_eq!(levels.len(), 2);
        assert_eq!(levels[1], 0.0);
    }

    #[test]
    fn speaking_needs_voice_and_lasts_1500_ms() {
        let mut t = SpeakingTracker::default();
        // Sound below the voice level: sounding, not speaking, nothing sent.
        let u = t.update(0.5, false, 1000);
        assert_eq!(
            u,
            SpeakingUpdate {
                speaking: false,
                send: None
            }
        );
        assert!(t.is_sounding(1000));
        // Voice: speaking, sent once.
        let u = t.update(1.2, false, 1100);
        assert_eq!(
            u,
            SpeakingUpdate {
                speaking: true,
                send: Some(true)
            }
        );
        // Still speaking 1,400 ms later, resent after 500 ms of sound.
        assert_eq!(t.update(1.5, false, 1500).send, None);
        assert_eq!(t.update(1.5, false, 1600).send, Some(true));
        assert!(t.is_speaking(2500));
        // Silence: speaking ends 1,500 ms after the last voice window.
        let u = t.update(0.0, false, 3101);
        assert_eq!(
            u,
            SpeakingUpdate {
                speaking: false,
                send: Some(false)
            }
        );
        assert_eq!(t.update(0.0, false, 3200).send, None);
    }

    #[test]
    fn muted_levels_are_ignored_and_reset_reports_a_pending_false() {
        let mut t = SpeakingTracker::default();
        assert_eq!(t.update(2.0, true, 10), SpeakingUpdate::default());
        assert_eq!(t.level(), 0.0);
        assert!(!t.reset());
        t.update(2.0, false, 20);
        assert_eq!(t.level(), 2.0);
        assert!(t.reset(), "a sent `true` needs a closing `false`");
        assert!(!t.is_speaking(20));
    }

    #[test]
    fn level_animation_moves_linearly_and_retargets_from_its_current_value() {
        let mut a = LevelAnimation::new(0.0, 200);
        assert_eq!(a.value(50), 0.0);
        a.retarget(1.0, 100);
        assert!((a.value(200) - 0.5).abs() < 1e-6);
        assert!(a.is_running(200));
        assert_eq!(a.value(300), 1.0);
        assert!(!a.is_running(300));
        // Retargeting midway starts from the value reached.
        a.retarget(0.0, 100);
        a.retarget(1.0, 100);
        a.retarget(0.0, 200);
        assert!((a.value(200) - 0.5).abs() < 1e-6);
        assert!((a.value(300) - 0.25).abs() < 1e-6);
        assert!((a.value(400) - 0.0).abs() < 1e-6);
        // The same target is not a restart.
        let before = a;
        a.retarget(0.0, 250);
        assert_eq!(a, before);
    }

    #[test]
    fn meter_lines_and_width_follow_tdesktop() {
        assert_eq!(meter_lit_lines(0.0, 44), 0);
        assert_eq!(meter_lit_lines(0.5, 44), 22);
        assert_eq!(meter_lit_lines(1.0, 44), 44);
        assert_eq!(meter_lit_lines(1.7, 44), 44);
        assert_eq!(meter_lit_lines(0.1, 0), 0);
        assert_eq!(meter_width(44), 44.0 * 3.0 + 43.0 * 5.0);
        assert_eq!(meter_width(0), 0.0);
        assert_eq!(halo_reach(0.0), 6.0);
        assert_eq!(halo_reach(3.0), 20.0);
    }
}
