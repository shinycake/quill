//! The voice-note / music player and the one source of truth for what it is
//! doing. The UI (play button, seek bar, time label, Now Playing) reads
//! [`AudioEngine::state`] and [`AudioEngine::position_secs`] on render and on
//! the tick; nothing mirrors them.
//!
//! The position is the number of frames the mixer actually pulled out of the
//! decoder ([`Counted`]), not wall time. A paused, stalled or failed output
//! therefore stops the position by itself. [`Simulation`] is the explicit
//! device-less mode of the screenshot demos; it keeps time with a
//! [`PlaybackClock`] and is never a fallback for a real playback.
#![deny(clippy::indexing_slicing)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use quill::playback::PlaybackClock;
use rodio::source::SeekError;
use rodio::{ChannelCount, Player, SampleRate, Source};

use super::tempo::{SharedSpeed, Tempo};
use super::{AudioError, SharedOutput, open_decoded};

/// What the engine is doing right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum EngineState {
    /// Nothing loaded.
    Idle,
    /// The mixer is consuming the track.
    Playing,
    /// Loaded and held (by the user, or by a seek that has not been applied).
    Paused,
    /// The track ran out.
    Ended,
    /// The last start could not play (no device, unreadable file, ...).
    Failed,
}

/// Device-less playback for the offline screenshot demos.
#[derive(Clone, Copy, Debug)]
pub(in crate::ui) struct Simulation {
    /// Every start fails, to show the failure state.
    pub(in crate::ui) fail: bool,
    /// Every seek hangs, to show the stalled-device state.
    pub(in crate::ui) stall: bool,
}

/// How long a posted seek may wait on the audio thread before the engine
/// reports that the device isn't responding.
const SEEK_STALL_AFTER: Duration = Duration::from_millis(2500);

/// The engine's time source. Tests drive a manual one so the stall bound
/// needs no sleeping.
#[derive(Debug)]
struct EngineClock {
    origin: Instant,
    #[cfg(test)]
    manual: Option<Arc<AtomicU64>>,
}

impl EngineClock {
    fn new() -> Self {
        Self {
            origin: Instant::now(),
            #[cfg(test)]
            manual: None,
        }
    }

    /// Time since the clock began.
    fn now(&self) -> Duration {
        #[cfg(test)]
        if let Some(millis) = &self.manual {
            return Duration::from_millis(millis.load(Ordering::Relaxed));
        }
        self.origin.elapsed()
    }
}

/// Frames the mixer has taken from a track, as an absolute position.
#[derive(Debug, Default)]
struct Probe {
    frames: AtomicU64,
    rate: AtomicU32,
    /// Tests: stands in for a decoder that cannot seek.
    #[cfg(test)]
    refuse_seek: std::sync::atomic::AtomicBool,
    /// Tests: seeks the audio side has handled, applied or refused.
    #[cfg(test)]
    seeks_handled: AtomicU64,
}

impl Probe {
    fn secs(&self) -> f64 {
        let rate = self.rate.load(Ordering::Relaxed).max(1);
        self.frames.load(Ordering::Relaxed) as f64 / f64::from(rate)
    }
}

/// Frames are published in batches, so the audio thread does one atomic add
/// per few milliseconds of sound.
const PUBLISH_FRAMES: u64 = 256;

/// Counts the frames pulled through it. Sits below the speed stretcher, so
/// the count is in track time whatever the speed.
struct Counted<S: Source> {
    inner: S,
    probe: Arc<Probe>,
    channels: u64,
    samples: u64,
    pending: u64,
}

impl<S: Source> Counted<S> {
    fn new(inner: S, probe: Arc<Probe>) -> Self {
        probe
            .rate
            .store(inner.sample_rate().get(), Ordering::Relaxed);
        probe.frames.store(0, Ordering::Relaxed);
        let channels = u64::from(inner.channels().get());
        Self {
            inner,
            probe,
            channels,
            samples: 0,
            pending: 0,
        }
    }

    fn publish(&mut self) {
        if self.pending > 0 {
            self.probe.frames.fetch_add(self.pending, Ordering::Relaxed);
            self.pending = 0;
        }
    }
}

impl<S: Source> Iterator for Counted<S> {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let sample = self.inner.next();
        if sample.is_some() {
            self.samples += 1;
            if self.samples >= self.channels {
                self.samples = 0;
                self.pending += 1;
                if self.pending >= PUBLISH_FRAMES {
                    self.publish();
                }
            }
        } else {
            self.publish();
        }
        sample
    }
}

impl<S: Source> Source for Counted<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    fn channels(&self) -> ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        #[cfg(test)]
        self.probe.seeks_handled.fetch_add(1, Ordering::Relaxed);
        #[cfg(test)]
        if self.probe.refuse_seek.load(Ordering::Relaxed) {
            return Err(SeekError::NotSupported {
                underlying_source: "test",
            });
        }
        self.inner.try_seek(pos)?;
        self.samples = 0;
        self.pending = 0;
        let rate = f64::from(self.inner.sample_rate().get());
        self.probe
            .frames
            .store((pos.as_secs_f64() * rate) as u64, Ordering::Relaxed);
        Ok(())
    }
}

/// Seeks posted to one `Player`'s audio side and what came of them.
/// `Player::try_seek` waits for the audio thread, so the UI thread never
/// calls it: a worker does, and this is the only thing the two share.
#[derive(Debug, Default)]
struct SeekBoard {
    inner: Mutex<SeekState>,
}

#[derive(Debug, Default)]
struct SeekState {
    /// The latest seek not yet handed to the audio thread.
    queued: Option<Seek>,
    /// The seek the audio thread has been asked for.
    in_flight: Option<Seek>,
    /// Tests: seeks handed to the audio thread so far.
    #[cfg(test)]
    handed: u64,
    /// A worker is draining `queued`.
    running: bool,
    /// The newest seek's outcome, until the engine takes it.
    done: Option<SeekDone>,
}

/// A posted seek and when (engine clock) the user asked for it.
#[derive(Clone, Copy, Debug)]
struct Seek {
    target: f64,
    posted: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct SeekDone {
    target: f64,
    moved: bool,
}

impl SeekBoard {
    fn lock(&self) -> MutexGuard<'_, SeekState> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Where the sound is headed: a seek not yet confirmed, or one the
    /// decoder refused and the engine has not yet answered with a restart.
    fn target(&self) -> Option<f64> {
        let state = self.lock();
        let pending = state.queued.or(state.in_flight).map(|seek| seek.target);
        pending.or_else(|| {
            state
                .done
                .filter(|done| !done.moved)
                .map(|done| done.target)
        })
    }

    /// The audio thread has held a seek for `SEEK_STALL_AFTER` without
    /// answering: the output isn't pulling.
    fn stalled(&self, now: Duration) -> bool {
        self.lock()
            .in_flight
            .is_some_and(|seek| now.saturating_sub(seek.posted) >= SEEK_STALL_AFTER)
    }

    fn take_done(&self) -> Option<SeekDone> {
        self.lock().done.take()
    }

    /// Queue `target` and make sure a worker will deliver it. Returns at once.
    fn post(self: &Arc<Self>, player: &Arc<Player>, target: f64, now: Duration) {
        let spawn = {
            let mut state = self.lock();
            state.queued = Some(Seek {
                target,
                posted: now,
            });
            state.done = None;
            !std::mem::replace(&mut state.running, true)
        };
        if !spawn {
            return;
        }
        let (board, player) = (self.clone(), player.clone());
        let worker = std::thread::Builder::new()
            .name("quill-audio-seek".into())
            .spawn(move || board.drain(&player));
        if worker.is_err() {
            let mut state = self.lock();
            state.running = false;
            state.queued = None;
            state.done = Some(SeekDone {
                target,
                moved: false,
            });
        }
    }

    /// The worker: hand the newest target to the audio thread, wait there,
    /// repeat until nothing newer was posted.
    fn drain(&self, player: &Player) {
        loop {
            let target = {
                let mut state = self.lock();
                let Some(seek) = state.queued.take() else {
                    state.running = false;
                    return;
                };
                state.in_flight = Some(seek);
                #[cfg(test)]
                {
                    state.handed += 1;
                }
                seek.target
            };
            let moved = player.try_seek(Duration::from_secs_f64(target)).is_ok();
            let mut state = self.lock();
            state.in_flight = None;
            if state.queued.is_none() {
                state.done = Some(SeekDone { target, moved });
            }
        }
    }
}

/// One track's player: play, pause, seek, volume and speed.
pub(in crate::ui) struct AudioEngine {
    output: SharedOutput,
    player: Option<Arc<Player>>,
    /// Seeks in flight on `player`; replaced with the player.
    seeks: Arc<SeekBoard>,
    probe: Arc<Probe>,
    clock: EngineClock,
    /// The track, kept after a failed start so Play can try again.
    path: Option<PathBuf>,
    speed: SharedSpeed,
    volume: f32,
    /// Seeked while paused or failed: the sound still sits at the old
    /// position, so resuming restarts it at `parked_secs`.
    stale: bool,
    /// Where the track is when no player is consuming it (stopped, failed,
    /// seeked while paused).
    parked_secs: f64,
    failed: bool,
    /// Why the last start, resume or seek did not work. Cleared by the next
    /// one that does, and by `stop`. The row's error line reads only this.
    error: Option<AudioError>,
    /// Set by the screenshot demos only.
    simulation: Option<Simulation>,
    sim_clock: Option<PlaybackClock>,
    sim_duration: f64,
    /// Simulation only: when the first seek that hangs was posted.
    sim_stalled_since: Option<Duration>,
}

impl AudioEngine {
    pub(in crate::ui) fn new(output: SharedOutput) -> Self {
        Self {
            output,
            player: None,
            seeks: Arc::default(),
            probe: Arc::new(Probe::default()),
            clock: EngineClock::new(),
            path: None,
            speed: SharedSpeed::new(1.0),
            volume: 1.0,
            stale: false,
            parked_secs: 0.0,
            failed: false,
            error: None,
            simulation: None,
            sim_clock: None,
            sim_duration: 0.0,
            sim_stalled_since: None,
        }
    }

    /// Choose the backend for the next start: the real output, or the
    /// device-less demo simulation.
    pub(in crate::ui) fn set_simulation(&mut self, simulation: Option<Simulation>) {
        self.simulation = simulation;
    }

    /// Why the last start, resume or seek failed, if it did and nothing has
    /// worked since. A seek the audio thread has left unanswered for
    /// `SEEK_STALL_AFTER` reads as `DeviceStalled`, derived from the seek
    /// board on every call.
    pub(in crate::ui) fn error(&self) -> Option<AudioError> {
        if self.seek_stalled() {
            return Some(AudioError::DeviceStalled);
        }
        self.error
    }

    fn seek_stalled(&self) -> bool {
        let now = self.clock.now();
        if self.sim_clock.is_some() {
            return self
                .sim_stalled_since
                .is_some_and(|since| now.saturating_sub(since) >= SEEK_STALL_AFTER);
        }
        self.player.is_some() && self.seeks.stalled(now)
    }

    fn note(&mut self, result: &Result<(), AudioError>) {
        self.error = result.as_ref().err().copied();
    }

    /// What the track is doing, read from the sound itself.
    pub(in crate::ui) fn state(&self) -> EngineState {
        if self.failed {
            return EngineState::Failed;
        }
        if let Some(clock) = &self.sim_clock {
            return if clock.finished() {
                EngineState::Ended
            } else if clock.is_playing() {
                EngineState::Playing
            } else {
                EngineState::Paused
            };
        }
        match &self.player {
            None => EngineState::Idle,
            Some(player) if player.is_paused() => EngineState::Paused,
            Some(player) if player.empty() => EngineState::Ended,
            Some(_) => EngineState::Playing,
        }
    }

    /// Where the track is, in seconds of the track: frames the mixer took,
    /// or the parked position when nothing is consuming it.
    pub(in crate::ui) fn position_secs(&self) -> f64 {
        if let Some(clock) = &self.sim_clock {
            return clock.elapsed_secs();
        }
        if self.failed || self.stale || self.player.is_none() {
            return self.parked_secs;
        }
        self.seeks.target().unwrap_or_else(|| self.probe.secs())
    }

    /// A seek is waiting on the audio thread (or on `settle`); the position
    /// is its target until then.
    #[cfg(test)]
    pub(in crate::ui) fn seek_pending(&self) -> bool {
        self.player.is_some() && self.seeks.target().is_some()
    }

    /// Tests: the audio side owes an answer to a seek it was handed.
    #[cfg(test)]
    fn seek_awaits_device(&self) -> bool {
        self.seeks.lock().handed > self.probe.seeks_handled.load(Ordering::Relaxed)
    }

    /// Tests: an engine whose clock is the returned handle (milliseconds).
    #[cfg(test)]
    fn with_manual_clock(mut self) -> (Self, Arc<AtomicU64>) {
        let millis = Arc::new(AtomicU64::new(0));
        self.clock.manual = Some(millis.clone());
        (self, millis)
    }

    /// Answer a finished seek: a decoder that refused to seek in place gets
    /// the track restarted at the target; the engine's error follows the
    /// outcome. Called on the UI tick and before every command; never waits.
    pub(in crate::ui) fn settle(&mut self) {
        let Some(done) = self.seeks.take_done() else {
            return;
        };
        if done.moved {
            self.error = None;
            return;
        }
        let restarted = match self.path.clone() {
            Some(path) => self.begin(&path, done.target),
            None => Ok(()),
        };
        self.note(&restarted);
    }

    /// Play `path` from `offset_secs`, replacing whatever played.
    /// `duration_secs` is only used by the demo simulation.
    pub(in crate::ui) fn start(
        &mut self,
        path: &Path,
        offset_secs: f64,
        volume: f32,
        speed: f64,
        duration_secs: f64,
    ) -> Result<(), AudioError> {
        self.stop();
        self.volume = volume.clamp(0.0, 1.0);
        self.speed.set(speed);
        self.path = Some(path.to_path_buf());
        self.sim_duration = duration_secs;
        let offset = offset_secs.max(0.0);
        self.parked_secs = offset;
        let started = self.begin(path, offset);
        self.failed = started.is_err();
        self.note(&started);
        started
    }

    /// Bring up the sound at `offset` on whichever backend is selected.
    fn begin(&mut self, path: &Path, offset: f64) -> Result<(), AudioError> {
        if let Some(simulation) = self.simulation {
            if simulation.fail {
                return Err(AudioError::NoOutput);
            }
            let mut clock = PlaybackClock::new(self.sim_duration);
            clock.set_rate(f64::from(self.speed.get()));
            clock.seek(offset);
            clock.resume();
            self.sim_clock = Some(clock);
            return Ok(());
        }
        let (player, probe) = self.build(path, offset)?;
        if let Some(old) = self.player.replace(player) {
            old.stop();
        }
        self.probe = probe;
        self.seeks = Arc::default();
        self.stale = false;
        Ok(())
    }

    /// A playing `Player` on `path` at `offset`; the current one is left
    /// alone, so a failure here changes nothing.
    fn build(&self, path: &Path, offset: f64) -> Result<(Arc<Player>, Arc<Probe>), AudioError> {
        let mixer = self.output.mixer().ok_or(AudioError::NoOutput)?;
        let probe = Arc::new(Probe::default());
        let decoded = open_decoded(path)?;
        let mut source = Tempo::new(Counted::new(decoded, probe.clone()), self.speed.clone());
        if offset > 0.05 {
            // A source that can't seek starts from the top, and the probe
            // says so.
            let _ = source.try_seek(Duration::from_secs_f64(offset));
        }
        let player = Player::connect_new(&mixer);
        player.set_volume(self.volume);
        player.append(source);
        Ok((Arc::new(player), probe))
    }

    /// True while a track is loaded (playing or paused, ended or not).
    pub(in crate::ui) fn is_loaded(&self) -> bool {
        self.player.is_some() || self.sim_clock.is_some()
    }

    /// The loaded track played to its end.
    pub(in crate::ui) fn is_ended(&self) -> bool {
        self.state() == EngineState::Ended
    }

    pub(in crate::ui) fn pause(&mut self) {
        self.settle();
        if let Some(clock) = self.sim_clock.as_mut() {
            clock.pause();
        }
        if let Some(player) = &self.player {
            player.pause();
        }
    }

    /// Continue from the pause; restarts at `offset_secs` when the sound is
    /// no longer where the track is (seek while paused, failed start), or
    /// from the top when it ended. On an error the state is what it was.
    pub(in crate::ui) fn resume(&mut self, offset_secs: f64) -> Result<(), AudioError> {
        self.settle();
        if let Some(clock) = self.sim_clock.as_mut() {
            if clock.finished() {
                clock.seek(0.0);
            }
            clock.resume();
            self.error = None;
            return Ok(());
        }
        let ended = self.player.as_ref().is_none_or(|p| p.empty());
        if self.failed || self.stale || ended {
            let Some(path) = self.path.clone() else {
                return Ok(());
            };
            let at = if ended && !self.stale && !self.failed {
                0.0
            } else {
                offset_secs.max(0.0)
            };
            let started = self.begin(&path, at);
            if started.is_ok() {
                self.failed = false;
            }
            self.note(&started);
            return started;
        }
        if let Some(player) = &self.player {
            player.play();
        }
        self.error = None;
        Ok(())
    }

    /// Move to `offset_secs`. While playing the seek is posted to the audio
    /// side and this returns at once: the position reads the target until
    /// the audio thread confirms, and a decoder that can't seek in place gets
    /// the track restarted by `settle` (a failure lands in `error`). While
    /// paused it catches up on resume.
    pub(in crate::ui) fn seek(
        &mut self,
        offset_secs: f64,
        playing: bool,
    ) -> Result<(), AudioError> {
        self.settle();
        let moved = self.seek_to(offset_secs, playing);
        self.note(&moved);
        moved
    }

    fn seek_to(&mut self, offset_secs: f64, playing: bool) -> Result<(), AudioError> {
        let offset = offset_secs.max(0.0);
        if let Some(clock) = self.sim_clock.as_mut() {
            clock.seek(offset);
            if self.simulation.is_some_and(|simulation| simulation.stall) {
                self.sim_stalled_since.get_or_insert(self.clock.now());
            }
            return Ok(());
        }
        if !playing {
            self.stale = true;
            self.parked_secs = offset;
            return Ok(());
        }
        if let Some(player) = self.player.as_ref().filter(|p| !p.empty()) {
            self.seeks.post(player, offset, self.clock.now());
            return Ok(());
        }
        let Some(path) = self.path.clone() else {
            return Ok(());
        };
        self.begin(&path, offset)
    }

    pub(in crate::ui) fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        if let Some(player) = &self.player {
            player.set_volume(self.volume);
        }
    }

    /// Takes effect on the running sound at its next 20 ms block.
    pub(in crate::ui) fn set_speed(&mut self, speed: f64) {
        self.speed.set(speed);
        if let Some(clock) = self.sim_clock.as_mut() {
            clock.set_rate(speed);
        }
    }

    pub(in crate::ui) fn stop(&mut self) {
        self.parked_secs = self.position_secs();
        if let Some(player) = self.player.take() {
            player.stop();
        }
        self.seeks = Arc::default();
        self.sim_clock = None;
        self.sim_stalled_since = None;
        self.path = None;
        self.stale = false;
        self.failed = false;
        self.error = None;
    }
}

#[cfg(test)]
pub(in crate::ui) mod test_support {
    use super::super::SharedOutput;
    use rodio::mixer::{Mixer, MixerSource, mixer};
    use rodio::{ChannelCount, SampleRate};
    use std::path::PathBuf;

    pub(in crate::ui) const RATE: u32 = 8000;

    fn wav(seconds: u32) -> Vec<u8> {
        let samples = RATE * seconds;
        let data_len = samples * 2;
        let mut out = Vec::new();
        out.extend(b"RIFF");
        out.extend((36 + data_len).to_le_bytes());
        out.extend(b"WAVEfmt ");
        out.extend(16u32.to_le_bytes());
        out.extend(1u16.to_le_bytes());
        out.extend(1u16.to_le_bytes());
        out.extend(RATE.to_le_bytes());
        out.extend((RATE * 2).to_le_bytes());
        out.extend(2u16.to_le_bytes());
        out.extend(16u16.to_le_bytes());
        out.extend(b"data");
        out.extend(data_len.to_le_bytes());
        for i in 0..samples {
            let s = ((i % 80) as i16 - 40) * 200;
            out.extend(s.to_le_bytes());
        }
        out
    }

    /// A mono 8 kHz wav of `seconds` in the temp dir.
    pub(in crate::ui) fn track(name: &str, seconds: u32) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("quill-engine-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, wav(seconds)).unwrap();
        path
    }

    /// A fake output plus the "device" that pulls frames from it.
    pub(in crate::ui) fn device() -> (SharedOutput, MixerSource) {
        let (mixer, source): (Mixer, MixerSource) = mixer(
            ChannelCount::new(1).unwrap(),
            SampleRate::new(RATE).unwrap(),
        );
        (SharedOutput::for_test(Some(mixer)), source)
    }

    /// The device consumes `samples` mono frames.
    pub(in crate::ui) fn pull(device: &mut MixerSource, samples: usize) {
        for _ in 0..samples {
            device.next();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{RATE, device, pull, track};
    use super::{AudioEngine, AudioError, EngineState, Simulation};
    use crate::ui::audio::SharedOutput;
    use rodio::mixer::MixerSource;
    use std::path::PathBuf;

    /// An engine on a fake output, plus the "device" that pulls from it.
    fn rig() -> (AudioEngine, MixerSource) {
        let (output, source) = device();
        (AudioEngine::new(output), source)
    }

    #[test]
    fn start_without_an_output_is_failed_and_keeps_the_offset() {
        let mut engine = AudioEngine::new(SharedOutput::for_test(None));
        let path = track("no-output.wav", 2);
        assert!(engine.start(&path, 1.5, 1.0, 1.0, 2.0).is_err());
        assert_eq!(engine.state(), EngineState::Failed);
        assert!((engine.position_secs() - 1.5).abs() < 1e-9);
    }

    #[test]
    fn start_of_a_missing_file_is_failed() {
        let (mut engine, _device) = rig();
        let path = std::env::temp_dir().join("quill-engine-test-missing.wav");
        assert!(engine.start(&path, 0.0, 1.0, 1.0, 2.0).is_err());
        assert_eq!(engine.state(), EngineState::Failed);
    }

    #[test]
    fn a_started_track_plays_and_a_pause_holds_it() {
        let (mut engine, mut device) = rig();
        let path = track("play-pause.wav", 4);
        engine.start(&path, 0.0, 1.0, 1.0, 4.0).unwrap();
        assert_eq!(engine.state(), EngineState::Playing);
        engine.pause();
        assert_eq!(engine.state(), EngineState::Paused);
        pull(&mut device, 4000);
        assert!(engine.position_secs() < 0.05);
    }

    #[test]
    fn position_follows_the_frames_the_device_consumed() {
        let (mut engine, mut device) = rig();
        let path = track("position.wav", 4);
        engine.start(&path, 0.0, 1.0, 1.0, 4.0).unwrap();
        assert!(engine.position_secs() < 0.05, "no wall-clock advance");
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(engine.position_secs() < 0.05, "time passing moves nothing");
        pull(&mut device, usize::try_from(RATE).unwrap());
        let one_second = engine.position_secs();
        assert!((0.9..=1.3).contains(&one_second), "got {one_second}");
        engine.pause();
        pull(&mut device, usize::try_from(RATE).unwrap());
        assert!(
            (engine.position_secs() - one_second).abs() < 0.05,
            "a held output consumes nothing"
        );
    }

    #[test]
    fn position_is_in_track_time_at_double_speed() {
        let (mut engine, mut device) = rig();
        let path = track("double.wav", 4);
        engine.start(&path, 0.0, 1.0, 2.0, 4.0).unwrap();
        pull(&mut device, usize::try_from(RATE).unwrap());
        let secs = engine.position_secs();
        assert!((1.8..=2.5).contains(&secs), "got {secs}");
    }

    #[test]
    fn a_start_offset_is_where_the_position_begins() {
        let (mut engine, _device) = rig();
        let path = track("offset.wav", 4);
        engine.start(&path, 2.0, 1.0, 1.0, 4.0).unwrap();
        let secs = engine.position_secs();
        assert!((secs - 2.0).abs() < 0.05, "got {secs}");
    }

    #[test]
    fn a_track_that_runs_out_is_ended() {
        let (mut engine, mut device) = rig();
        let path = track("ended.wav", 1);
        engine.start(&path, 0.0, 1.0, 1.0, 1.0).unwrap();
        pull(&mut device, usize::try_from(RATE).unwrap() * 2);
        assert_eq!(engine.state(), EngineState::Ended);
    }

    #[test]
    fn resume_failure_leaves_the_track_paused() {
        let (mut engine, _device) = rig();
        let path = track("resume-fails.wav", 4);
        engine.start(&path, 0.0, 1.0, 1.0, 4.0).unwrap();
        engine.pause();
        engine.seek(2.0, false).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(engine.resume(2.0).is_err());
        assert_eq!(engine.state(), EngineState::Paused);
        assert!((engine.position_secs() - 2.0).abs() < 0.05);
    }

    #[test]
    fn resume_after_a_failed_start_retries_the_track() {
        let (mut engine, _device) = rig();
        let path = track("retry.wav", 4);
        engine.set_simulation(Some(Simulation {
            fail: true,
            stall: false,
        }));
        assert!(engine.start(&path, 1.0, 1.0, 1.0, 4.0).is_err());
        assert_eq!(engine.state(), EngineState::Failed);
        engine.set_simulation(None);
        engine.resume(1.0).unwrap();
        assert_eq!(engine.state(), EngineState::Playing);
        assert!((engine.position_secs() - 1.0).abs() < 0.05);
    }

    #[test]
    fn stop_keeps_the_last_position() {
        let (mut engine, mut device) = rig();
        let path = track("stop.wav", 4);
        engine.start(&path, 0.0, 1.0, 1.0, 4.0).unwrap();
        pull(&mut device, usize::try_from(RATE).unwrap());
        engine.stop();
        assert_eq!(engine.state(), EngineState::Idle);
        assert!((0.9..=1.3).contains(&engine.position_secs()));
    }

    #[test]
    fn the_simulation_keeps_its_own_time() {
        let mut engine = AudioEngine::new(SharedOutput::for_test(None));
        engine.set_simulation(Some(Simulation {
            fail: false,
            stall: false,
        }));
        let path = PathBuf::from("unused.wav");
        engine.start(&path, 3.0, 1.0, 1.0, 10.0).unwrap();
        assert_eq!(engine.state(), EngineState::Playing);
        engine.pause();
        assert_eq!(engine.state(), EngineState::Paused);
        assert!((engine.position_secs() - 3.0).abs() < 0.05);
        engine.seek(12.0, false).unwrap();
        engine.resume(12.0).unwrap();
        assert_eq!(engine.state(), EngineState::Ended);
    }
    /// The audio thread answers a posted seek only when the device pulls.
    /// Pull one frame at a time, and only while a seek the audio side owes an
    /// answer for is outstanding, so the position cannot drift past what the
    /// seek itself set whatever the worker's scheduling.
    fn pull_until_settled(engine: &mut AudioEngine, device: &mut MixerSource) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while std::time::Instant::now() < deadline {
            if engine.seek_awaits_device() {
                pull(device, 1);
            }
            engine.settle();
            if !engine.seek_pending() {
                return;
            }
            std::thread::yield_now();
        }
        panic!("the seek never settled");
    }

    #[test]
    fn a_seek_returns_at_once_while_the_device_is_not_pulling() {
        let (mut engine, mut device) = rig();
        let path = track("seek-stalled.wav", 6);
        engine.start(&path, 0.0, 1.0, 1.0, 6.0).unwrap();
        pull(&mut device, usize::try_from(RATE).unwrap());
        // Nothing pulls from here on. Before the fix this call waited for
        // the audio thread forever.
        engine.seek(4.0, true).unwrap();
        engine.seek(5.0, true).unwrap();
        assert!(engine.seek_pending());
        assert_eq!(engine.state(), EngineState::Playing);
        assert!(
            (engine.position_secs() - 5.0).abs() < 1e-9,
            "the newest target"
        );
        assert_eq!(engine.error(), None);
        pull_until_settled(&mut engine, &mut device);
        assert!(!engine.seek_pending());
        let confirmed = engine.position_secs();
        assert!((4.95..=5.2).contains(&confirmed), "got {confirmed}");
        assert_eq!(engine.error(), None);
    }

    #[test]
    fn a_seek_the_device_never_answers_reads_as_a_stalled_device() {
        let (engine, mut device) = rig();
        let (mut engine, clock) = engine.with_manual_clock();
        let path = track("seek-stall-reported.wav", 6);
        engine.start(&path, 0.0, 1.0, 1.0, 6.0).unwrap();
        pull(&mut device, usize::try_from(RATE).unwrap());
        engine.seek(5.0, true).unwrap();
        // The worker hands the seek over; the device pulls nothing.
        while !engine.seek_awaits_device() {
            std::thread::yield_now();
        }
        clock.store(2499, std::sync::atomic::Ordering::Relaxed);
        assert_eq!(engine.error(), None, "inside the bound");
        clock.store(2500, std::sync::atomic::Ordering::Relaxed);
        assert_eq!(engine.error(), Some(AudioError::DeviceStalled));
        assert_eq!(engine.state(), EngineState::Playing);
        assert!((engine.position_secs() - 5.0).abs() < 1e-9);
        // The device wakes up: the report goes away on its own.
        pull_until_settled(&mut engine, &mut device);
        assert_eq!(engine.error(), None);
    }

    #[test]
    fn scrubbing_against_a_live_device_never_reads_as_stalled() {
        let (engine, mut device) = rig();
        let (mut engine, clock) = engine.with_manual_clock();
        let path = track("seek-scrub-live.wav", 6);
        engine.start(&path, 0.0, 1.0, 1.0, 6.0).unwrap();
        for step in 0..8u64 {
            clock.store(step * 1000, std::sync::atomic::Ordering::Relaxed);
            engine.seek(1.0 + step as f64 * 0.5, true).unwrap();
            pull_until_settled(&mut engine, &mut device);
            assert_eq!(engine.error(), None, "step {step}");
        }
    }

    #[test]
    fn a_failed_seek_keeps_the_previous_state_and_the_engines_error() {
        let (mut engine, mut device) = rig();
        let path = track("seek-fails.wav", 6);
        engine.start(&path, 0.0, 1.0, 1.0, 6.0).unwrap();
        pull(&mut device, usize::try_from(RATE).unwrap());
        // The decoder refuses; the restart that follows can't open the file
        // either.
        engine
            .probe
            .refuse_seek
            .store(true, std::sync::atomic::Ordering::Relaxed);
        std::fs::remove_file(&path).unwrap();
        engine.seek(5.0, true).unwrap();
        assert!(
            (engine.position_secs() - 5.0).abs() < 1e-9,
            "pending target"
        );
        pull_until_settled(&mut engine, &mut device);
        assert_eq!(engine.error(), Some(AudioError::Open));
        assert_eq!(engine.state(), EngineState::Playing);
        let secs = engine.position_secs();
        assert!(
            (0.95..=1.2).contains(&secs),
            "back at the sound, got {secs}"
        );
    }

    #[test]
    fn a_refused_seek_restarts_the_track_at_the_target() {
        let (mut engine, mut device) = rig();
        let path = track("seek-restarts.wav", 6);
        engine.start(&path, 0.0, 1.0, 1.0, 6.0).unwrap();
        pull(&mut device, usize::try_from(RATE).unwrap());
        engine
            .probe
            .refuse_seek
            .store(true, std::sync::atomic::Ordering::Relaxed);
        engine.seek(4.0, true).unwrap();
        pull_until_settled(&mut engine, &mut device);
        assert_eq!(engine.error(), None);
        assert_eq!(engine.state(), EngineState::Playing);
        let secs = engine.position_secs();
        assert!(
            (3.95..=4.2).contains(&secs),
            "restarted at the target, got {secs}"
        );
    }

    #[test]
    fn an_in_place_seek_while_playing_moves_the_position() {
        let (mut engine, mut device) = rig();
        let path = track("seek-playing.wav", 6);
        engine.start(&path, 0.0, 1.0, 1.0, 6.0).unwrap();
        pull(&mut device, usize::try_from(RATE).unwrap());
        engine.seek(4.0, true).unwrap();
        pull_until_settled(&mut engine, &mut device);
        assert_eq!(engine.state(), EngineState::Playing);
        let after_seek = engine.position_secs();
        assert!((3.95..=4.2).contains(&after_seek), "got {after_seek}");
        let before = engine.position_secs();
        pull(&mut device, usize::try_from(RATE).unwrap() / 2);
        let later = engine.position_secs();
        assert!(
            later - before > 0.3 && later - before < 0.7,
            "got {before} then {later}"
        );
    }

    #[test]
    fn a_speed_change_mid_track_keeps_track_time() {
        let (mut engine, mut device) = rig();
        let path = track("speed-change.wav", 8);
        engine.start(&path, 0.0, 1.0, 1.0, 8.0).unwrap();
        pull(&mut device, usize::try_from(RATE).unwrap());
        let before = engine.position_secs();
        assert!((0.9..=1.3).contains(&before), "got {before}");
        engine.set_speed(2.0);
        pull(&mut device, usize::try_from(RATE).unwrap());
        let after = engine.position_secs();
        assert!((2.7..=3.6).contains(&after), "got {after}");
        assert!(after > before, "time never runs backwards");
    }

    #[test]
    fn the_engine_keeps_why_it_failed_until_something_works() {
        let mut engine = AudioEngine::new(SharedOutput::for_test(None));
        let path = track("error-kept.wav", 2);
        assert_eq!(engine.error(), None);
        assert!(engine.start(&path, 0.0, 1.0, 1.0, 2.0).is_err());
        assert_eq!(engine.error(), Some(AudioError::NoOutput));
        assert_eq!(engine.state(), EngineState::Failed);
        engine.stop();
        assert_eq!(engine.error(), None);
    }

    #[test]
    fn a_failed_seek_is_cleared_by_the_next_good_one() {
        let (mut engine, mut device) = rig();
        let path = track("error-seek.wav", 1);
        engine.start(&path, 0.0, 1.0, 1.0, 1.0).unwrap();
        pull(&mut device, usize::try_from(RATE).unwrap() * 2);
        assert_eq!(engine.state(), EngineState::Ended);
        std::fs::remove_file(&path).unwrap();
        assert!(engine.seek(0.5, true).is_err());
        assert_eq!(engine.error(), Some(AudioError::Open));
        track("error-seek.wav", 1);
        engine.seek(0.5, true).unwrap();
        assert_eq!(engine.error(), None);
        assert_eq!(engine.state(), EngineState::Playing);
    }

    #[test]
    fn a_failed_resume_error_goes_away_with_the_next_resume_that_works() {
        let (mut engine, _device) = rig();
        let path = track("error-resume.wav", 4);
        engine.start(&path, 0.0, 1.0, 1.0, 4.0).unwrap();
        engine.pause();
        engine.seek(2.0, false).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(engine.resume(2.0).is_err());
        assert_eq!(engine.error(), Some(AudioError::Open));
        assert_eq!(engine.state(), EngineState::Paused);
        track("error-resume.wav", 4);
        engine.resume(2.0).unwrap();
        assert_eq!(engine.error(), None);
        assert_eq!(engine.state(), EngineState::Playing);
    }
}
