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
//!
//! Opening a file and reaching an offset read the disk, on a long MP3 up to
//! the whole file, so a track is built on a thread of its own ([`Building`])
//! and lands here, on the UI thread, which alone changes the engine.
#![deny(clippy::indexing_slicing)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use quill::playback::PlaybackClock;
use rodio::source::SeekError;
use rodio::{ChannelCount, SampleRate, Source};

use super::tempo::{SharedSpeed, Tempo};
use super::{AudioError, Outlet, OutputPlayer, SharedOutput, open_decoded};

/// What the engine is doing right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum EngineState {
    /// Nothing loaded.
    Idle,
    /// The track is being built and plays when it lands.
    Loading,
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
    /// The sound never comes up, to show a long start.
    pub(in crate::ui) slow: bool,
}

/// How long a posted seek may wait on the audio thread before the engine
/// reports that the device isn't responding.
const SEEK_STALL_AFTER: Duration = Duration::from_millis(2500);

/// How long a start may take before the row says so. Telegram Desktop shows a
/// stream's waiting radial after this long (`kWaitingShowDelay`), and nothing
/// before: a track that is starting shows Pause, at the position asked for.
const LOADING_SHOWS_AFTER: Duration = Duration::from_millis(500);

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
    /// Of `frames`, those the speed stretcher has read but not yet played.
    lag: Arc<AtomicU64>,
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
        let played = self
            .frames
            .load(Ordering::Relaxed)
            .saturating_sub(self.lag.load(Ordering::Relaxed));
        played as f64 / f64::from(rate)
    }
}

/// Frames are published in batches, so the audio thread does one atomic add
/// per few milliseconds of sound.
const PUBLISH_FRAMES: u64 = 256;

/// Counts the frames pulled through it. Sits below the speed stretcher, so
/// the count is in track time whatever the speed, and the stretcher reports
/// how far it has read ahead ([`Probe::lag`]).
struct Counted<S: Source> {
    inner: S,
    probe: Arc<Probe>,
    channels: u64,
    samples: u64,
    pending: u64,
}

impl<S: Source> Counted<S> {
    /// `start_secs` is where `inner` already sits.
    fn new(inner: S, probe: Arc<Probe>, start_secs: f64) -> Self {
        let rate = inner.sample_rate().get();
        probe.rate.store(rate, Ordering::Relaxed);
        probe
            .frames
            .store((start_secs * f64::from(rate)) as u64, Ordering::Relaxed);
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
    /// The track is gone: a worker that wakes from a stalled device exits
    /// without reading or writing anything else.
    cancelled: AtomicBool,
    /// Live seek workers across the engine's tracks.
    workers: Arc<AtomicUsize>,
    /// Tests: how long (ms) the worker takes to notice the audio thread's
    /// answer, as on a loaded machine.
    #[cfg(test)]
    answer_delay_ms: AtomicU64,
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

/// Counts a running seek worker for as long as it lives.
struct WorkerGuard(Arc<AtomicUsize>);

impl WorkerGuard {
    fn new(count: &Arc<AtomicUsize>) -> Self {
        count.fetch_add(1, Ordering::SeqCst);
        Self(count.clone())
    }
}

impl Drop for WorkerGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl SeekBoard {
    fn new(workers: Arc<AtomicUsize>) -> Self {
        Self {
            inner: Mutex::default(),
            cancelled: AtomicBool::new(false),
            workers,
            #[cfg(test)]
            answer_delay_ms: AtomicU64::new(0),
        }
    }

    /// Tell the worker, if one is parked on a stalled device, to leave as
    /// soon as the device answers.
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

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
    fn post(self: &Arc<Self>, player: &Arc<OutputPlayer>, target: f64, now: Duration) {
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
        let guard = WorkerGuard::new(&self.workers);
        let worker = std::thread::Builder::new()
            .name("quill-audio-seek".into())
            .spawn(move || {
                let _guard = guard;
                board.drain(&player);
            });
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
    fn drain(&self, player: &OutputPlayer) {
        loop {
            if self.is_cancelled() {
                return;
            }
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
            #[cfg(test)]
            std::thread::sleep(Duration::from_millis(
                self.answer_delay_ms.load(Ordering::Relaxed),
            ));
            if self.is_cancelled() {
                return;
            }
            let mut state = self.lock();
            state.in_flight = None;
            if state.queued.is_none() {
                state.done = Some(SeekDone { target, moved });
            }
        }
    }
}

/// A freshly built player and the probe counting its frames.
struct Built {
    player: Arc<OutputPlayer>,
    probe: Arc<Probe>,
}

/// One loaded track: its player, its position probe and its seek worker.
/// Dropping it stops the sound and tells a seek worker parked on a stalled
/// device to leave when the device answers, so a replaced track can't leave
/// anything running or touch the next track's state.
struct Track {
    player: Arc<OutputPlayer>,
    seeks: Arc<SeekBoard>,
    probe: Arc<Probe>,
}

impl Track {
    fn new(built: Built, workers: &Arc<AtomicUsize>) -> Self {
        Self {
            player: built.player,
            seeks: Arc::new(SeekBoard::new(workers.clone())),
            probe: built.probe,
        }
    }
}

impl Drop for Track {
    fn drop(&mut self) {
        self.seeks.cancel();
        self.player.stop();
    }
}

/// A track building on a thread of its own: a start or a resume, or a restart
/// at a seek the decoder refused, whose old sound plays on until the swap.
/// Until it lands the engine reads `Loading` (`Paused` with `hold`) and the
/// position reads `target`.
struct Building {
    /// Where the thread was asked to build.
    at: f64,
    /// The newest position asked for. A build that lands elsewhere is redone.
    target: f64,
    /// Land paused.
    hold: bool,
    /// When the wait began (engine clock).
    asked: Duration,
    built: Receiver<Result<Built, AudioError>>,
}

/// A paused `Player` on `path` at `offset`. Reads the file, up to all of it,
/// so it runs off the UI thread ([`Building`]).
fn build_player(
    outlet: &Outlet,
    path: &Path,
    offset: f64,
    speed: &SharedSpeed,
) -> Result<Built, AudioError> {
    let probe = Arc::new(Probe::default());
    let mut decoded = open_decoded(path)?;
    // Seeked below `Counted`, so its `try_seek` answers only the player's
    // seeks. A source that can't seek starts from the top.
    let start = if offset > 0.05 && decoded.try_seek(Duration::from_secs_f64(offset)).is_ok() {
        offset
    } else {
        0.0
    };
    let source = Tempo::new(Counted::new(decoded, probe.clone(), start), speed.clone())
        .reporting_lag(probe.lag.clone());
    let player = outlet.player();
    player.pause();
    player.append(source);
    Ok(Built {
        player: Arc::new(player),
        probe,
    })
}

/// One track's player: play, pause, seek, volume and speed.
pub(in crate::ui) struct AudioEngine {
    output: SharedOutput,
    track: Option<Track>,
    /// Seek workers alive across all tracks.
    workers: Arc<AtomicUsize>,
    /// A track building off the UI thread.
    building: Option<Building>,
    /// Called on the building thread when a build is done, so it lands now
    /// and not at the next tick.
    on_built: Option<fn()>,
    /// A build that would have been the only sound failed. Taken by the app.
    start_failed: bool,
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
    /// The time the simulation keeps: the app's clock (`set_sim_time`),
    /// which tests drive; the wall clock until one is set.
    sim_time: Option<Box<dyn Fn() -> Instant>>,
    sim_duration: f64,
    /// Simulation only: when the first seek that hangs was posted.
    sim_stalled_since: Option<Duration>,
    /// Tests: holds a build at its start ([`Self::gate_builds`]).
    #[cfg(test)]
    build_gate: Option<Arc<Mutex<Receiver<()>>>>,
}

impl AudioEngine {
    pub(in crate::ui) fn new(output: SharedOutput) -> Self {
        Self {
            output,
            track: None,
            workers: Arc::default(),
            building: None,
            on_built: None,
            start_failed: false,
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
            sim_time: None,
            sim_duration: 0.0,
            sim_stalled_since: None,
            #[cfg(test)]
            build_gate: None,
        }
    }

    /// Choose the backend for the next start: the real output, or the
    /// device-less demo simulation.
    pub(in crate::ui) fn set_simulation(&mut self, simulation: Option<Simulation>) {
        self.simulation = simulation;
    }

    /// Keep the simulation's time with `now`, the app's clock.
    pub(in crate::ui) fn set_sim_time(&mut self, now: impl Fn() -> Instant + 'static) {
        self.sim_time = Some(Box::new(now));
    }

    /// Call `wake` on the building thread each time a build is done.
    pub(in crate::ui) fn set_on_built(&mut self, wake: fn()) {
        self.on_built = Some(wake);
    }

    fn sim_now(&self) -> Instant {
        self.sim_time
            .as_ref()
            .map_or_else(Instant::now, |now| now())
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
        self.track.as_ref().is_some_and(|t| t.seeks.stalled(now))
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
            return if clock.finished_at(self.sim_now()) {
                EngineState::Ended
            } else if clock.is_playing() {
                if self.simulation.is_some_and(|simulation| simulation.slow) {
                    EngineState::Loading
                } else {
                    EngineState::Playing
                }
            } else {
                EngineState::Paused
            };
        }
        if let Some(building) = &self.building {
            return if building.hold {
                EngineState::Paused
            } else {
                EngineState::Loading
            };
        }
        match self.track.as_ref().map(|t| &t.player) {
            None => EngineState::Idle,
            Some(player) if player.is_paused() => EngineState::Paused,
            Some(player) if player.empty() => EngineState::Ended,
            Some(_) => EngineState::Playing,
        }
    }

    /// A start has been `Loading` for long enough to show.
    pub(in crate::ui) fn loading_shown(&self) -> bool {
        let waited = |building: &Building| self.clock.now().saturating_sub(building.asked);
        self.state() == EngineState::Loading
            && (self.sim_clock.is_some()
                || self.building.as_ref().is_some_and(|b| waited(b) >= LOADING_SHOWS_AFTER))
    }

    /// Where the track is, in seconds of the track: frames the mixer took,
    /// or the parked position when nothing is consuming it.
    pub(in crate::ui) fn position_secs(&self) -> f64 {
        if let Some(clock) = &self.sim_clock {
            return clock.elapsed_secs_at(self.sim_now());
        }
        if let Some(building) = &self.building {
            return building.target;
        }
        let Some(track) = self.track.as_ref().filter(|_| !self.failed && !self.stale) else {
            return self.parked_secs;
        };
        track.seeks.target().unwrap_or_else(|| track.probe.secs())
    }

    /// A seek is waiting on the audio thread (or on `settle`); the position
    /// is its target until then.
    #[cfg(test)]
    pub(in crate::ui) fn seek_pending(&self) -> bool {
        self.building.is_some() || self.track.as_ref().is_some_and(|t| t.seeks.target().is_some())
    }

    /// Tests: the audio side owes an answer to a seek it was handed.
    #[cfg(test)]
    fn seek_awaits_device(&self) -> bool {
        self.track
            .as_ref()
            .is_some_and(|t| t.seeks.lock().handed > t.probe.seeks_handled.load(Ordering::Relaxed))
    }

    /// Tests: seek workers alive across all tracks.
    #[cfg(test)]
    fn live_workers(&self) -> usize {
        self.workers.load(Ordering::SeqCst)
    }

    /// Tests: the current track's seek board and probe.
    #[cfg(test)]
    fn track_parts(&self) -> Option<(Arc<SeekBoard>, Arc<Probe>)> {
        self.track
            .as_ref()
            .map(|t| (t.seeks.clone(), t.probe.clone()))
    }

    /// Tests: an engine whose clock is the returned handle (milliseconds).
    #[cfg(test)]
    fn with_manual_clock(mut self) -> (Self, Arc<AtomicU64>) {
        let millis = Arc::new(AtomicU64::new(0));
        self.clock.manual = Some(millis.clone());
        (self, millis)
    }

    /// Tests: builds wait for a message on the returned sender, or for it
    /// to drop.
    #[cfg(test)]
    fn gate_builds(&mut self) -> mpsc::Sender<()> {
        let (open, gate) = mpsc::channel();
        self.build_gate = Some(Arc::new(Mutex::new(gate)));
        open
    }

    /// Tests: wait for the build to finish, and leave it to land.
    #[cfg(test)]
    pub(in crate::ui) fn await_built(&mut self) {
        if let Some(building) = self.building.as_mut() {
            let result = building.built.recv().unwrap_or(Err(AudioError::Open));
            let (done, built) = mpsc::channel();
            let _ = done.send(result);
            building.built = built;
        }
    }

    /// Tests: wait for the track being built to land, a redone build
    /// included.
    #[cfg(test)]
    pub(in crate::ui) fn await_build(&mut self) {
        while let Some(building) = &self.building {
            let result = building.built.recv().unwrap_or(Err(AudioError::Open));
            self.land(result);
        }
    }

    /// Land a finished build, and answer a finished seek: a decoder that
    /// refused to seek in place gets the track rebuilt at the target, and
    /// the new sound takes over when it lands; the engine's error follows
    /// the outcome. A track whose device failed to open is paused. Called
    /// on the UI tick, when a build is done and before every command; never
    /// waits.
    pub(in crate::ui) fn settle(&mut self) {
        self.land_build();
        // Starts and resumes return while the device opens off the UI
        // thread; this is where one that couldn't open shows.
        if let Some(track) = self.track.as_ref().filter(|t| t.player.lost()) {
            track.player.pause();
            self.error = Some(AudioError::NoOutput);
        }
        let Some(done) = self.track.as_ref().and_then(|t| t.seeks.take_done()) else {
            return;
        };
        if done.moved {
            self.error = None;
            return;
        }
        self.restart_at(done.target);
    }

    /// Ask for the track at `target` after a refused seek. A build already
    /// under way just gets the newer target.
    fn restart_at(&mut self, target: f64) {
        if let Some(building) = self.building.as_mut() {
            building.target = target;
            return;
        }
        let hold = self.track.as_ref().is_some_and(|t| t.player.is_paused());
        let asked = self.build(target, hold, self.clock.now());
        self.note(&asked);
    }

    /// Have the track built at `target` off the UI thread. Fails only when
    /// the build can't begin; what comes of it lands in [`Self::land`].
    fn build(&mut self, target: f64, hold: bool, asked: Duration) -> Result<(), AudioError> {
        let path = self.path.clone().ok_or(AudioError::Open)?;
        let outlet = self.output.outlet().ok_or(AudioError::NoOutput)?;
        let (done, built) = mpsc::channel();
        let (speed, on_built) = (self.speed.clone(), self.on_built);
        #[cfg(test)]
        let gate = self.build_gate.clone();
        quill::thread_guard::spawn_guarded("quill-audio-build", move || {
            #[cfg(test)]
            if let Some(gate) = gate {
                let _ = gate.lock().map(|gate| gate.recv());
            }
            let _ = done.send(build_player(&outlet, &path, target, &speed));
            if let Some(wake) = on_built {
                wake();
            }
        })
        .map_err(|_| AudioError::Open)?;
        self.building = Some(Building {
            at: target,
            target,
            hold,
            asked,
            built,
        });
        Ok(())
    }

    fn land_build(&mut self) {
        let Some(building) = &self.building else {
            return;
        };
        match building.built.try_recv() {
            Ok(result) => self.land(result),
            Err(TryRecvError::Empty) => {}
            // The thread panicked on the file.
            Err(TryRecvError::Disconnected) => self.land(Err(AudioError::Open)),
        }
    }

    /// The new sound takes over from the old one, if any. A failure leaves
    /// what was there; with no sound left the track has failed.
    fn land(&mut self, result: Result<Built, AudioError>) {
        let Some(Building {
            at,
            target,
            hold,
            asked,
            ..
        }) = self.building.take()
        else {
            return;
        };
        let redone = ((at - target).abs() > f64::EPSILON).then(|| self.build(target, hold, asked));
        match (redone, result) {
            (Some(Ok(())), _) => {}
            (Some(Err(error)), _) | (None, Err(error)) => self.fail_build(error, target),
            (None, Ok(built)) => {
                built.player.set_volume(self.volume);
                let playing = hold || built.player.play();
                if !playing {
                    built.player.pause();
                }
                self.track = Some(Track::new(built, &self.workers));
                self.stale = false;
                self.error = (!playing).then_some(AudioError::NoOutput);
            }
        }
    }

    fn fail_build(&mut self, error: AudioError, target: f64) {
        let silent = self.track.is_none() || self.stale;
        if silent {
            self.parked_secs = target;
        }
        self.failed = self.track.is_none();
        self.start_failed = silent;
        self.error = Some(error);
    }

    /// A build that was to be the only sound failed since the last call.
    pub(in crate::ui) fn take_start_failed(&mut self) -> bool {
        std::mem::take(&mut self.start_failed)
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
        let started = self.begin(offset);
        self.failed = started.is_err();
        self.note(&started);
        started
    }

    /// Bring up the sound at `offset` on whichever backend is selected. The
    /// current sound is left alone until the new one lands.
    fn begin(&mut self, offset: f64) -> Result<(), AudioError> {
        if let Some(simulation) = self.simulation {
            if simulation.fail {
                return Err(AudioError::NoOutput);
            }
            let now = self.sim_now();
            let mut clock = PlaybackClock::new(self.sim_duration);
            clock.set_rate_at(f64::from(self.speed.get()), now);
            clock.seek_at(offset, now);
            clock.resume_at(now);
            self.sim_clock = Some(clock);
            return Ok(());
        }
        self.build(offset, false, self.clock.now())
    }

    /// True while a track is loaded (playing or paused, ended or not).
    pub(in crate::ui) fn is_loaded(&self) -> bool {
        self.track.is_some() || self.building.is_some() || self.sim_clock.is_some()
    }

    /// The loaded track played to its end.
    pub(in crate::ui) fn is_ended(&self) -> bool {
        self.state() == EngineState::Ended
    }

    pub(in crate::ui) fn pause(&mut self) {
        self.settle();
        let now = self.sim_now();
        if let Some(clock) = self.sim_clock.as_mut() {
            clock.pause_at(now);
        }
        if let Some(building) = self.building.as_mut() {
            building.hold = true;
        }
        if let Some(track) = &self.track {
            track.player.pause();
        }
    }

    /// Continue from the pause; restarts at `offset_secs` when the sound is
    /// no longer where the track is (seek while paused, failed start), or
    /// from the top when it ended. If that fails the state goes back to what
    /// it was, with the reason in `error`.
    pub(in crate::ui) fn resume(&mut self, offset_secs: f64) -> Result<(), AudioError> {
        self.settle();
        let now = self.sim_now();
        if let Some(clock) = self.sim_clock.as_mut() {
            if clock.finished_at(now) {
                clock.seek_at(0.0, now);
            }
            clock.resume_at(now);
            self.error = None;
            return Ok(());
        }
        if let Some(building) = self.building.as_mut() {
            building.hold = false;
            self.error = None;
            if let Some(track) = self.track.as_ref().filter(|_| !self.stale) {
                track.player.play();
            }
            return Ok(());
        }
        let ended = self.track.as_ref().is_none_or(|t| t.player.empty());
        if self.failed || self.stale || ended {
            if self.path.is_none() {
                return Ok(());
            }
            let at = if ended && !self.stale && !self.failed {
                0.0
            } else {
                offset_secs.max(0.0)
            };
            let started = self.begin(at);
            if started.is_ok() {
                self.failed = false;
            }
            self.note(&started);
            return started;
        }
        if let Some(track) = &self.track
            && !track.player.play()
        {
            track.player.pause();
            let woke = Err(AudioError::NoOutput);
            self.note(&woke);
            return woke;
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
        let now = self.sim_now();
        if let Some(clock) = self.sim_clock.as_mut() {
            clock.seek_at(offset, now);
            if self.simulation.is_some_and(|simulation| simulation.stall) {
                self.sim_stalled_since.get_or_insert(self.clock.now());
            }
            return Ok(());
        }
        if let Some(building) = self.building.as_mut() {
            building.target = offset;
            return Ok(());
        }
        if !playing {
            self.stale = true;
            self.parked_secs = offset;
            return Ok(());
        }
        if let Some(track) = self.track.as_ref().filter(|t| !t.player.empty()) {
            track.seeks.post(&track.player, offset, self.clock.now());
            return Ok(());
        }
        if self.path.is_none() {
            return Ok(());
        }
        self.begin(offset)
    }

    pub(in crate::ui) fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        if let Some(track) = &self.track {
            track.player.set_volume(self.volume);
        }
    }

    /// Takes effect on the running sound at its next 20 ms block.
    pub(in crate::ui) fn set_speed(&mut self, speed: f64) {
        self.speed.set(speed);
        let now = self.sim_now();
        if let Some(clock) = self.sim_clock.as_mut() {
            clock.set_rate_at(speed, now);
        }
    }

    pub(in crate::ui) fn stop(&mut self) {
        self.parked_secs = self.position_secs();
        self.track = None;
        self.building = None;
        self.start_failed = false;
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

    fn wav(seconds: u32, rate: u32) -> Vec<u8> {
        let samples = rate * seconds;
        let data_len = samples * 2;
        let mut out = Vec::new();
        out.extend(b"RIFF");
        out.extend((36 + data_len).to_le_bytes());
        out.extend(b"WAVEfmt ");
        out.extend(16u32.to_le_bytes());
        out.extend(1u16.to_le_bytes());
        out.extend(1u16.to_le_bytes());
        out.extend(rate.to_le_bytes());
        out.extend((rate * 2).to_le_bytes());
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
        track_at(name, seconds, RATE)
    }

    pub(in crate::ui) fn track_at(name: &str, seconds: u32, rate: u32) -> PathBuf {
        let dir = quill::scratch::process_dir("quill-engine-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, wav(seconds, rate)).unwrap();
        path
    }

    /// A fake output plus the "device" that pulls frames from it.
    pub(in crate::ui) fn device() -> (SharedOutput, MixerSource) {
        device_at(RATE)
    }

    pub(in crate::ui) fn device_at(rate: u32) -> (SharedOutput, MixerSource) {
        let (mixer, source): (Mixer, MixerSource) = mixer(
            ChannelCount::new(1).unwrap(),
            SampleRate::new(rate).unwrap(),
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
    use super::test_support::{RATE, device, device_at, pull, track, track_at};
    use super::{AudioEngine, AudioError, EngineState, SeekBoard, Simulation};
    use crate::ui::audio::SharedOutput;
    use rodio::mixer::MixerSource;
    use std::path::{Path, PathBuf};
    use std::sync::mpsc;

    /// An engine on a fake output, plus the "device" that pulls from it.
    fn rig() -> (AudioEngine, MixerSource) {
        let (output, source) = device();
        (AudioEngine::new(output), source)
    }

    /// `start`, through to its sound landing.
    fn start_landed(
        engine: &mut AudioEngine,
        path: &Path,
        offset: f64,
        volume: f32,
        speed: f64,
        duration: f64,
    ) {
        engine.start(path, offset, volume, speed, duration).unwrap();
        engine.await_build();
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
    fn start_of_a_missing_file_fails_when_the_build_lands() {
        let (mut engine, _device) = rig();
        let path = std::env::temp_dir().join("quill-engine-test-missing.wav");
        engine.start(&path, 1.5, 1.0, 1.0, 2.0).unwrap();
        assert_eq!(engine.state(), EngineState::Loading);
        engine.await_build();
        assert_eq!(engine.state(), EngineState::Failed);
        assert_eq!(engine.error(), Some(AudioError::Open));
        assert!((engine.position_secs() - 1.5).abs() < 1e-9);
        assert!(engine.take_start_failed());
        assert!(!engine.take_start_failed(), "said once");
    }

    #[test]
    fn a_start_returns_before_the_file_is_read_and_lands_playing() {
        let (mut engine, mut device) = rig();
        let open = engine.gate_builds();
        let path = track("loading.wav", 4);
        engine.start(&path, 2.0, 1.0, 1.0, 4.0).unwrap();
        assert_eq!(engine.state(), EngineState::Loading);
        assert!(engine.is_loaded());
        assert!((engine.position_secs() - 2.0).abs() < 1e-9);
        engine.settle();
        assert_eq!(engine.state(), EngineState::Loading, "still reading");
        open.send(()).unwrap();
        engine.await_build();
        assert_eq!(engine.state(), EngineState::Playing);
        pull(&mut device, usize::try_from(RATE).unwrap());
        let secs = engine.position_secs();
        assert!((2.9..=3.3).contains(&secs), "got {secs}");
    }

    #[test]
    fn a_pause_while_loading_lands_held_and_a_resume_plays_it() {
        let (mut engine, mut device) = rig();
        let open = engine.gate_builds();
        let path = track("loading-pause.wav", 4);
        engine.start(&path, 1.0, 1.0, 1.0, 4.0).unwrap();
        engine.pause();
        assert_eq!(engine.state(), EngineState::Paused);
        engine.resume(1.0).unwrap();
        assert_eq!(engine.state(), EngineState::Loading);
        engine.pause();
        open.send(()).unwrap();
        engine.await_build();
        assert_eq!(engine.state(), EngineState::Paused);
        pull(&mut device, usize::try_from(RATE).unwrap());
        assert!((engine.position_secs() - 1.0).abs() < 0.05);
        engine.resume(1.0).unwrap();
        assert_eq!(engine.state(), EngineState::Playing);
        pull(&mut device, usize::try_from(RATE).unwrap());
        let secs = engine.position_secs();
        assert!((1.9..=2.3).contains(&secs), "got {secs}");
    }

    #[test]
    fn a_seek_while_loading_lands_at_the_newest_position() {
        let (mut engine, mut device) = rig();
        let open = engine.gate_builds();
        let path = track("loading-seek.wav", 6);
        engine.start(&path, 1.0, 1.0, 1.0, 6.0).unwrap();
        engine.seek(5.0, true).unwrap();
        engine.seek(3.0, true).unwrap();
        assert!((engine.position_secs() - 3.0).abs() < 1e-9);
        assert_eq!(engine.state(), EngineState::Loading);
        drop(open);
        engine.await_build();
        assert_eq!(engine.state(), EngineState::Playing);
        pull(&mut device, usize::try_from(RATE).unwrap());
        let secs = engine.position_secs();
        assert!((3.9..=4.3).contains(&secs), "got {secs}");
    }

    #[test]
    fn a_seek_while_held_and_loading_stays_paused_at_the_target() {
        let (mut engine, _device) = rig();
        let open = engine.gate_builds();
        let path = track("loading-held-seek.wav", 6);
        engine.start(&path, 1.0, 1.0, 1.0, 6.0).unwrap();
        engine.pause();
        engine.seek(4.0, false).unwrap();
        assert_eq!(engine.state(), EngineState::Paused);
        drop(open);
        engine.await_build();
        assert_eq!(engine.state(), EngineState::Paused);
        assert!((engine.position_secs() - 4.0).abs() < 0.05);
    }

    #[test]
    fn a_stop_while_loading_lands_nothing() {
        let (mut engine, _device) = rig();
        let open = engine.gate_builds();
        let path = track("loading-stop.wav", 4);
        engine.start(&path, 1.0, 1.0, 1.0, 4.0).unwrap();
        engine.stop();
        assert_eq!(engine.state(), EngineState::Idle);
        assert!(!engine.is_loaded());
        drop(open);
        engine.settle();
        assert_eq!(engine.state(), EngineState::Idle);
        assert_eq!(engine.error(), None);
    }

    #[test]
    fn a_track_started_while_another_loads_replaces_it() {
        let (mut engine, mut device) = rig();
        let open = engine.gate_builds();
        let first = track("loading-first.wav", 4);
        let second = track("loading-second.wav", 4);
        engine.start(&first, 1.0, 1.0, 1.0, 4.0).unwrap();
        engine.start(&second, 2.0, 1.0, 1.0, 4.0).unwrap();
        drop(open);
        engine.await_build();
        pull(&mut device, usize::try_from(RATE).unwrap());
        let secs = engine.position_secs();
        assert!((2.9..=3.3).contains(&secs), "got {secs}");
    }

    /// A panic on the file ends the thread with no answer.
    #[test]
    fn a_build_thread_that_died_fails_the_start() {
        let (mut engine, _device) = rig();
        let _open = engine.gate_builds();
        let path = track("loading-died.wav", 4);
        engine.start(&path, 1.0, 1.0, 1.0, 4.0).unwrap();
        if let Some(building) = engine.building.as_mut() {
            building.built = mpsc::channel().1;
        }
        engine.settle();
        assert_eq!(engine.state(), EngineState::Failed);
        assert_eq!(engine.error(), Some(AudioError::Open));
    }

    #[test]
    fn a_short_start_shows_nothing_and_a_long_one_shows_loading() {
        let (engine, _device) = rig();
        let (mut engine, clock) = engine.with_manual_clock();
        let open = engine.gate_builds();
        let path = track("loading-long.wav", 4);
        engine.start(&path, 1.0, 1.0, 1.0, 4.0).unwrap();
        clock.store(499, std::sync::atomic::Ordering::Relaxed);
        assert!(!engine.loading_shown());
        clock.store(500, std::sync::atomic::Ordering::Relaxed);
        assert!(engine.loading_shown());
        engine.pause();
        assert!(!engine.loading_shown(), "a held start shows Play");
        engine.resume(1.0).unwrap();
        assert!(engine.loading_shown());
        open.send(()).unwrap();
        engine.await_build();
        assert!(!engine.loading_shown());
    }

    #[test]
    fn a_started_track_plays_and_a_pause_holds_it() {
        let (mut engine, mut device) = rig();
        let path = track("play-pause.wav", 4);
        start_landed(&mut engine, &path, 0.0, 1.0, 1.0, 4.0);
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
        start_landed(&mut engine, &path, 0.0, 1.0, 1.0, 4.0);
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
        start_landed(&mut engine, &path, 0.0, 1.0, 2.0, 4.0);
        pull(&mut device, usize::try_from(RATE).unwrap());
        let secs = engine.position_secs();
        assert!((1.8..=2.5).contains(&secs), "got {secs}");
    }

    #[test]
    fn a_start_offset_is_where_the_position_begins() {
        let (mut engine, _device) = rig();
        let path = track("offset.wav", 4);
        start_landed(&mut engine, &path, 2.0, 1.0, 1.0, 4.0);
        let secs = engine.position_secs();
        assert!((secs - 2.0).abs() < 0.05, "got {secs}");
    }

    #[test]
    fn a_track_that_runs_out_is_ended() {
        let (mut engine, mut device) = rig();
        let path = track("ended.wav", 1);
        start_landed(&mut engine, &path, 0.0, 1.0, 1.0, 1.0);
        pull(&mut device, usize::try_from(RATE).unwrap() * 2);
        assert_eq!(engine.state(), EngineState::Ended);
    }

    /// The output closes while nothing plays, so a resume opens it before
    /// the track plays on.
    #[test]
    fn resuming_a_paused_track_opens_the_output() {
        let (output, mut device) = device();
        let mut engine = AudioEngine::new(output.clone());
        let path = track("resume-opens.wav", 4);
        start_landed(&mut engine, &path, 0.0, 1.0, 1.0, 4.0);
        engine.pause();
        let asked = output.wakes();
        engine.resume(0.0).unwrap();
        assert_eq!(output.wakes(), asked + 1);
        assert_eq!(engine.state(), EngineState::Playing);
        pull(&mut device, 4000);
        assert!(engine.position_secs() > 0.4, "{}", engine.position_secs());
    }

    /// A start returns while the device opens off the UI thread, showing
    /// the track as playing; the sound plays once the device is up.
    #[test]
    fn a_track_starts_while_the_device_opens_off_the_ui_thread() {
        let (output, gate) = SharedOutput::gated();
        let mut engine = AudioEngine::new(output.clone());
        let path = track("opening.wav", 4);
        start_landed(&mut engine, &path, 0.0, 1.0, 1.0, 4.0);
        engine.settle();
        assert_eq!(engine.state(), EngineState::Playing);
        assert_eq!(engine.error(), None, "opening is no error");
        gate.answer(true);
        output.settle_device();
        // One second of the 48 kHz stereo mix.
        gate.pull(96_000);
        let secs = engine.position_secs();
        assert!((0.9..=1.3).contains(&secs), "got {secs}");
    }

    /// A device that fails to open after the start returned pauses the
    /// track and says why.
    #[test]
    fn a_device_that_fails_to_open_pauses_the_track() {
        let (output, gate) = SharedOutput::gated();
        let mut engine = AudioEngine::new(output.clone());
        let path = track("open-fails.wav", 4);
        start_landed(&mut engine, &path, 1.0, 1.0, 1.0, 4.0);
        gate.answer(false);
        output.settle_device();
        engine.settle();
        assert_eq!(engine.state(), EngineState::Paused);
        assert_eq!(engine.error(), Some(AudioError::NoOutput));
    }

    #[test]
    fn resume_failure_leaves_the_track_paused() {
        let (mut engine, _device) = rig();
        let path = track("resume-fails.wav", 4);
        start_landed(&mut engine, &path, 0.0, 1.0, 1.0, 4.0);
        engine.pause();
        engine.seek(2.0, false).unwrap();
        std::fs::remove_file(&path).unwrap();
        engine.resume(2.0).unwrap();
        assert_eq!(engine.state(), EngineState::Loading);
        engine.await_build();
        assert_eq!(engine.state(), EngineState::Paused);
        assert!((engine.position_secs() - 2.0).abs() < 0.05);
        assert!(engine.take_start_failed());
    }

    #[test]
    fn resume_after_a_failed_start_retries_the_track() {
        let (mut engine, _device) = rig();
        let path = track("retry.wav", 4);
        engine.set_simulation(Some(Simulation {
            fail: true,
            stall: false,
            slow: false,
        }));
        assert!(engine.start(&path, 1.0, 1.0, 1.0, 4.0).is_err());
        assert_eq!(engine.state(), EngineState::Failed);
        engine.set_simulation(None);
        engine.resume(1.0).unwrap();
        engine.await_build();
        assert_eq!(engine.state(), EngineState::Playing);
        assert!((engine.position_secs() - 1.0).abs() < 0.05);
    }

    #[test]
    fn stop_keeps_the_last_position() {
        let (mut engine, mut device) = rig();
        let path = track("stop.wav", 4);
        start_landed(&mut engine, &path, 0.0, 1.0, 1.0, 4.0);
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
            slow: false,
        }));
        let path = PathBuf::from("unused.wav");
        start_landed(&mut engine, &path, 3.0, 1.0, 1.0, 10.0);
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
        let (handed, answered) = engine.track_parts().map_or((0, 0), |(board, probe)| {
            let answered = probe
                .seeks_handled
                .load(std::sync::atomic::Ordering::Relaxed);
            (board.lock().handed, answered)
        });
        panic!(
            "the seek never settled: handed {handed}, answered {answered}, restart building {}",
            engine.building.is_some()
        );
    }

    /// Pull one frame at a time, with no tick, until the device has answered
    /// the seek on `board`.
    fn pull_until_answered(board: &SeekBoard, device: &mut MixerSource) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while board.lock().done.is_none() {
            assert!(
                std::time::Instant::now() < deadline,
                "the device never answered the seek"
            );
            pull(device, 1);
            std::thread::yield_now();
        }
    }

    #[test]
    fn a_seek_returns_at_once_while_the_device_is_not_pulling() {
        let (mut engine, mut device) = rig();
        let path = track("seek-stalled.wav", 6);
        start_landed(&mut engine, &path, 0.0, 1.0, 1.0, 6.0);
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
        start_landed(&mut engine, &path, 0.0, 1.0, 1.0, 6.0);
        pull(&mut device, usize::try_from(RATE).unwrap());
        engine.seek(5.0, true).unwrap();
        // The worker hands the seek over; the device pulls nothing.
        wait_until("the seek reaches the device", || {
            engine.seek_awaits_device()
        });
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
        start_landed(&mut engine, &path, 0.0, 1.0, 1.0, 6.0);
        for step in 0..8u64 {
            clock.store(step * 1000, std::sync::atomic::Ordering::Relaxed);
            engine.seek(1.0 + step as f64 * 0.5, true).unwrap();
            pull_until_settled(&mut engine, &mut device);
            assert_eq!(engine.error(), None, "step {step}");
        }
    }

    /// Spin until `done` holds, failing the test instead of hanging it.
    fn wait_until(what: &str, done: impl Fn() -> bool) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while std::time::Instant::now() < deadline {
            if done() {
                return;
            }
            std::thread::yield_now();
        }
        panic!("never happened: {what}");
    }

    /// Pull one frame at a time until `done` holds (bounded).
    fn pull_until(
        engine: &mut AudioEngine,
        device: &mut MixerSource,
        what: &str,
        done: impl Fn(&AudioEngine) -> bool,
    ) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while std::time::Instant::now() < deadline {
            pull(device, 1);
            engine.settle();
            if done(engine) {
                return;
            }
            std::thread::yield_now();
        }
        panic!("never happened: {what}");
    }

    /// The current track's seek worker takes 600 ms to notice an answer, as
    /// a thread does on a machine under load. The position must not depend
    /// on how soon that happens.
    fn slow_workers(engine: &AudioEngine) {
        if let Some((board, _)) = engine.track_parts() {
            board
                .answer_delay_ms
                .store(600, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Like `pull_until`, but the device is pulled only while a seek the
    /// audio side owes an answer for is outstanding, on the current track or
    /// on `replaced`, a track the engine let go of. Pulling on every turn
    /// would let the sound run on while a worker thread waits to be
    /// scheduled, and the position would drift past the seek's target by as
    /// much as the machine is loaded.
    fn pull_while_awaited(
        engine: &mut AudioEngine,
        device: &mut MixerSource,
        replaced: (&SeekBoard, &super::Probe),
        what: &str,
        done: impl Fn(&AudioEngine) -> bool,
    ) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while std::time::Instant::now() < deadline {
            let replaced_awaits = replaced.0.lock().handed
                > replaced
                    .1
                    .seeks_handled
                    .load(std::sync::atomic::Ordering::Relaxed);
            if engine.seek_awaits_device() || replaced_awaits {
                pull(device, 1);
            }
            engine.settle();
            if done(engine) {
                return;
            }
            std::thread::yield_now();
        }
        panic!("never happened: {what}");
    }

    #[test]
    fn a_replaced_stalled_track_lets_its_worker_go_without_touching_state() {
        let (engine, mut device) = rig();
        let (mut engine, clock) = engine.with_manual_clock();
        let first = track("stall-first.wav", 6);
        let second = track("stall-second.wav", 6);
        start_landed(&mut engine, &first, 0.0, 1.0, 1.0, 6.0);
        pull(&mut device, usize::try_from(RATE).unwrap());
        slow_workers(&engine);
        engine.seek(5.0, true).unwrap();
        wait_until("the seek reaches the device", || {
            engine.seek_awaits_device()
        });
        clock.store(2500, std::sync::atomic::Ordering::Relaxed);
        assert_eq!(engine.error(), Some(AudioError::DeviceStalled));
        assert_eq!(engine.live_workers(), 1);
        let (old_board, old_probe) = engine.track_parts().unwrap();

        // A new track starts while the device is still silent, and is
        // scrubbed too: both workers are parked.
        start_landed(&mut engine, &second, 0.0, 1.0, 1.0, 6.0);
        assert_eq!(engine.error(), None, "the new track starts clean");
        slow_workers(&engine);
        engine.seek(3.0, true).unwrap();
        wait_until("both workers parked", || engine.live_workers() >= 2);

        // The device wakes: the old worker leaves without writing anything,
        // the new one delivers its seek, and no thread stays behind.
        pull_while_awaited(
            &mut engine,
            &mut device,
            (&old_board, &old_probe),
            "workers gone",
            |engine| engine.live_workers() == 0 && !engine.seek_pending(),
        );
        assert!(
            old_board.lock().done.is_none(),
            "the stopped track reports nothing"
        );
        assert_eq!(engine.error(), None);
        let secs = engine.position_secs();
        assert!((2.95..=3.3).contains(&secs), "got {secs}");
    }

    #[test]
    fn dropping_a_track_stops_its_sound_and_cancels_its_board() {
        let (mut engine, mut device) = rig();
        let path = track("drop-track.wav", 4);
        start_landed(&mut engine, &path, 0.0, 1.0, 1.0, 4.0);
        pull(&mut device, usize::try_from(RATE).unwrap());
        // A seek the device never answers parks a worker.
        engine.seek(2.0, true).unwrap();
        wait_until("a worker is parked", || engine.live_workers() == 1);
        let (board, _) = engine.track_parts().unwrap();
        assert!(!board.is_cancelled());
        engine.stop();
        assert!(board.is_cancelled());
        pull_until(&mut engine, &mut device, "worker gone", |engine| {
            engine.live_workers() == 0
        });
    }

    #[test]
    fn a_refused_seek_restarts_off_the_ui_thread_and_a_pause_holds_it() {
        let (mut engine, mut device) = rig();
        let path = track("restart-paused.wav", 6);
        start_landed(&mut engine, &path, 0.0, 1.0, 1.0, 6.0);
        pull(&mut device, usize::try_from(RATE).unwrap());
        let (_, probe) = engine.track_parts().unwrap();
        probe
            .refuse_seek
            .store(true, std::sync::atomic::Ordering::Relaxed);
        engine.seek(4.0, true).unwrap();
        engine.pause();
        pull_until(&mut engine, &mut device, "restart landed", |engine| {
            !engine.seek_pending()
        });
        assert_eq!(engine.state(), EngineState::Paused, "the pause survives");
        let secs = engine.position_secs();
        assert!((3.95..=4.2).contains(&secs), "got {secs}");
        assert_eq!(engine.error(), None);
    }

    #[test]
    fn a_seek_while_a_restart_builds_retargets_it() {
        let (mut engine, mut device) = rig();
        let path = track("restart-retarget.wav", 8);
        start_landed(&mut engine, &path, 0.0, 1.0, 1.0, 8.0);
        pull(&mut device, usize::try_from(RATE).unwrap());
        let (board, probe) = engine.track_parts().unwrap();
        probe
            .refuse_seek
            .store(true, std::sync::atomic::Ordering::Relaxed);
        engine.seek(6.0, true).unwrap();
        pull_until_answered(&board, &mut device);
        // No tick has run, so this seek's own settle starts the restart and
        // the seek lands on it while it builds.
        engine.seek(2.0, true).unwrap();
        assert!(engine.building.is_some(), "the restart is building");
        assert!(
            (engine.position_secs() - 2.0).abs() < 1e-9,
            "the newest target at once"
        );
        pull_until_settled(&mut engine, &mut device);
        let secs = engine.position_secs();
        assert!((1.95..=2.3).contains(&secs), "got {secs}");
        assert_eq!(engine.error(), None);
    }

    #[test]
    fn a_seek_after_a_restart_built_moves_the_new_track() {
        let (mut engine, mut device) = rig();
        let path = track("restart-built-then-seek.wav", 8);
        start_landed(&mut engine, &path, 0.0, 1.0, 1.0, 8.0);
        pull(&mut device, usize::try_from(RATE).unwrap());
        let (board, probe) = engine.track_parts().unwrap();
        probe
            .refuse_seek
            .store(true, std::sync::atomic::Ordering::Relaxed);
        engine.seek(6.0, true).unwrap();
        pull_until_answered(&board, &mut device);
        engine.settle();
        engine.await_built();
        // This seek's settle lands the rebuilt track, and the seek goes to
        // the new track in place.
        engine.seek(2.0, true).unwrap();
        assert!(engine.building.is_none(), "the restart landed");
        assert!((engine.position_secs() - 2.0).abs() < 1e-9);
        pull_until_settled(&mut engine, &mut device);
        let secs = engine.position_secs();
        assert!((1.95..=2.3).contains(&secs), "got {secs}");
        assert_eq!(engine.error(), None);
    }

    #[test]
    fn a_failed_seek_keeps_the_previous_state_and_the_engines_error() {
        let (mut engine, mut device) = rig();
        let path = track("seek-fails.wav", 6);
        start_landed(&mut engine, &path, 0.0, 1.0, 1.0, 6.0);
        pull(&mut device, usize::try_from(RATE).unwrap());
        // The decoder refuses; the restart that follows can't open the file
        // either.
        engine
            .track_parts()
            .map(|(_, probe)| probe)
            .unwrap()
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
        start_landed(&mut engine, &path, 0.0, 1.0, 1.0, 6.0);
        pull(&mut device, usize::try_from(RATE).unwrap());
        engine
            .track_parts()
            .map(|(_, probe)| probe)
            .unwrap()
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
        start_landed(&mut engine, &path, 0.0, 1.0, 1.0, 6.0);
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

    /// What a saved position resumes from: the sound the device played,
    /// not what the speed stretcher read ahead of it. At 48 kHz, as devices
    /// run: the mixer converts a new player's first 512 samples from its
    /// still-empty queue's 48 kHz, so an 8 kHz rig pulls 340 frames more
    /// than it plays.
    #[test]
    fn position_is_what_was_played_within_a_block_at_1x_and_1_5x() {
        const RATE: u32 = 48_000;
        for speed in [1.0, 1.5] {
            let (output, mut device) = device_at(RATE);
            let mut engine = AudioEngine::new(output);
            let path = track_at("played.wav", 4, RATE);
            engine.start(&path, 0.0, 1.0, speed, 4.0).unwrap();
            pull(&mut device, usize::try_from(RATE).unwrap() / 2);
            let played = 0.5 * speed;
            let block = 0.04 * speed;
            let secs = engine.position_secs();
            assert!(
                (secs - played).abs() <= block,
                "at {speed}x played {played} s, position read {secs}"
            );
        }
    }

    #[test]
    fn a_speed_change_mid_track_keeps_track_time() {
        let (mut engine, mut device) = rig();
        let path = track("speed-change.wav", 8);
        start_landed(&mut engine, &path, 0.0, 1.0, 1.0, 8.0);
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
        start_landed(&mut engine, &path, 0.0, 1.0, 1.0, 1.0);
        pull(&mut device, usize::try_from(RATE).unwrap() * 2);
        assert_eq!(engine.state(), EngineState::Ended);
        std::fs::remove_file(&path).unwrap();
        engine.seek(0.5, true).unwrap();
        engine.await_build();
        assert_eq!(engine.error(), Some(AudioError::Open));
        track("error-seek.wav", 1);
        engine.seek(0.5, true).unwrap();
        engine.await_build();
        assert_eq!(engine.error(), None);
        assert_eq!(engine.state(), EngineState::Playing);
    }

    #[test]
    fn a_failed_resume_error_goes_away_with_the_next_resume_that_works() {
        let (mut engine, _device) = rig();
        let path = track("error-resume.wav", 4);
        start_landed(&mut engine, &path, 0.0, 1.0, 1.0, 4.0);
        engine.pause();
        engine.seek(2.0, false).unwrap();
        std::fs::remove_file(&path).unwrap();
        engine.resume(2.0).unwrap();
        engine.await_build();
        assert_eq!(engine.error(), Some(AudioError::Open));
        assert_eq!(engine.state(), EngineState::Paused);
        track("error-resume.wav", 4);
        engine.resume(2.0).unwrap();
        engine.await_build();
        assert_eq!(engine.error(), None);
        assert_eq!(engine.state(), EngineState::Playing);
    }
}
