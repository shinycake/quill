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
//! and lands here, on the UI thread, which alone changes the engine. A seek
//! in a playing track is a new build too, never a seek on the audio thread.
#![deny(clippy::indexing_slicing)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(test)]
use std::sync::mpsc::Receiver;
use std::sync::mpsc::{self, TryRecvError};
use std::time::{Duration, Instant};

use quill::playback::PlaybackClock;

use super::engine_build::{
    BuildEnd, Building, Built, CancelOnDrop, Landing, Track, build_paused_player,
};
use super::tempo::SharedSpeed;
use super::{AudioError, SharedOutput};

/// What the engine is doing right now: a view computed on each call from
/// `Playback` and `Sound`, which own the stage and the build task. Nothing
/// stores it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum EngineView {
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
}

/// How long a simulated seek hangs before it reads as a stalled device.
const SIM_STALL_AFTER: Duration = Duration::from_millis(2500);

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

/// The sound of a device run and whether it is where the track is. One
/// value, so a track and its standing cannot disagree.
#[derive(Default)]
enum Sound {
    /// Nothing loaded.
    #[default]
    Empty,
    /// The sound is the track's: the position reads from it.
    Current(Track),
    /// Seeked while paused: the sound still sits at the old position, so
    /// resuming restarts it at `parked_secs`.
    Stale(Track),
    /// The start or the last build failed: nothing plays, the position is
    /// parked. The track that ran out stays loaded, when there was one.
    Failed(Option<Track>),
}

impl Sound {
    fn track(&self) -> Option<&Track> {
        match self {
            Self::Current(track) | Self::Stale(track) => Some(track),
            Self::Failed(track) => track.as_ref(),
            Self::Empty => None,
        }
    }

    fn is_failed(&self) -> bool {
        matches!(self, Self::Failed(_))
    }

    /// The sound is the track's (or there is none).
    fn is_current(&self) -> bool {
        matches!(self, Self::Empty | Self::Current(_))
    }

    /// Nothing is consuming the track: none loaded, or it ran out.
    fn ended(&self) -> bool {
        self.track().is_none_or(|track| track.player.empty())
    }

    /// A start is on its way: a failure no longer stands.
    fn recovered(self) -> Self {
        match self {
            Self::Failed(Some(track)) => Self::Current(track),
            Self::Failed(None) => Self::Empty,
            other => other,
        }
    }
}

/// What the engine has: the device's sound, the same sound with a build on
/// its way, or the device-less demo run. One value, so no build can run
/// beside a simulation and a landing replaces the sound in one step.
enum Playback {
    Settled(Sound),
    /// A start or seek is being built; `sound` plays on meanwhile.
    Building {
        build: Building,
        sound: Sound,
    },
    Simulated(SimRun),
}

impl Playback {
    fn sound(&self) -> Option<&Sound> {
        match self {
            Self::Settled(sound) | Self::Building { sound, .. } => Some(sound),
            Self::Simulated(_) => None,
        }
    }

    fn track(&self) -> Option<&Track> {
        self.sound().and_then(Sound::track)
    }

    fn building(&self) -> Option<&Building> {
        match self {
            Self::Building { build, .. } => Some(build),
            _ => None,
        }
    }

    fn building_mut(&mut self) -> Option<&mut Building> {
        match self {
            Self::Building { build, .. } => Some(build),
            _ => None,
        }
    }

    fn run(&self) -> Option<&SimRun> {
        match self {
            Self::Simulated(run) => Some(run),
            _ => None,
        }
    }

    fn run_mut(&mut self) -> Option<&mut SimRun> {
        match self {
            Self::Simulated(run) => Some(run),
            _ => None,
        }
    }

    /// The sound, leaving a settled empty one: a running build is dropped
    /// (it is cancelled) and a simulation goes.
    fn take_sound(&mut self) -> Sound {
        match std::mem::replace(self, Self::Settled(Sound::Empty)) {
            Self::Settled(sound) | Self::Building { sound, .. } => sound,
            Self::Simulated(_) => Sound::Empty,
        }
    }

    /// The running build, leaving its sound settled.
    fn take_build(&mut self) -> Option<Building> {
        match std::mem::replace(self, Self::Settled(Sound::Empty)) {
            Self::Building { build, sound } => {
                *self = Self::Settled(sound);
                Some(build)
            }
            other => {
                *self = other;
                None
            }
        }
    }
}

/// Why the last start, resume or seek did not work, and whether the start
/// failure was told to the app yet (`take_start_failed`). Cleared by the
/// next one that works, and by `stop`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fault {
    None,
    Seen(AudioError),
    /// A build that was to be the only sound failed; the app has not been
    /// told.
    Unseen(AudioError),
}

impl Fault {
    fn error(self) -> Option<AudioError> {
        match self {
            Self::None => None,
            Self::Seen(error) | Self::Unseen(error) => Some(error),
        }
    }

    /// Something worked: the fault clears, except a start failure the app
    /// has not been told yet, which stays until it is.
    fn worked(&mut self) {
        if !matches!(self, Self::Unseen(_)) {
            *self = Self::None;
        }
    }

    /// Something failed in a way the app asks about itself: an untold
    /// start failure stays the one to tell.
    fn seen(&mut self, error: AudioError) {
        if !matches!(self, Self::Unseen(_)) {
            *self = Self::Seen(error);
        }
    }

    /// A new build starts: a start failure it replaces is not told.
    fn build_started(&mut self) {
        if let Self::Unseen(error) = *self {
            *self = Self::Seen(error);
        }
    }
}

/// A simulated track: its clock, and when the first seek that hangs was
/// posted (it ends with the run).
struct SimRun {
    clock: PlaybackClock,
    stalled_since: Option<Duration>,
}

/// One track's player: play, pause, seek, volume and speed.
pub(in crate::ui) struct AudioEngine {
    output: SharedOutput,
    /// The track, its build and its sound: one stage.
    playback: Playback,
    on_built: Option<fn()>,
    clock: EngineClock,
    /// The track, kept after a failed start so Play can try again.
    path: Option<PathBuf>,
    speed: SharedSpeed,
    volume: f32,
    /// Where the track is when no player is consuming it (stopped, failed,
    /// seeked while paused).
    parked_secs: f64,
    /// Why the last start, resume or seek did not work: a log the row's
    /// error line reads, independent of the stage (a failed seek leaves the
    /// old sound playing).
    fault: Fault,
    /// Set by the screenshot demos only.
    simulation: Option<Simulation>,
    /// The time the simulation keeps: the app's clock (`set_sim_time`),
    /// which tests drive; the wall clock until one is set.
    sim_time: Option<Box<dyn Fn() -> Instant>>,
    sim_duration: f64,
    #[cfg(test)]
    build_gate: Option<Arc<std::sync::Mutex<Receiver<()>>>>,
    /// Tests: the next builds panic, as a decoder can.
    #[cfg(test)]
    build_panics: bool,
    #[cfg(test)]
    build_threads: Vec<std::thread::JoinHandle<Option<()>>>,
}

impl AudioEngine {
    pub(in crate::ui) fn new(output: SharedOutput) -> Self {
        Self {
            output,
            playback: Playback::Settled(Sound::Empty),
            on_built: None,
            clock: EngineClock::new(),
            path: None,
            speed: SharedSpeed::new(1.0),
            volume: 1.0,
            parked_secs: 0.0,
            fault: Fault::None,
            simulation: None,
            sim_time: None,
            sim_duration: 0.0,
            #[cfg(test)]
            build_gate: None,
            #[cfg(test)]
            build_panics: false,
            #[cfg(test)]
            build_threads: Vec::new(),
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
    /// worked since. In the simulation, a seek that has hung for
    /// `SIM_STALL_AFTER` reads as `DeviceStalled`.
    pub(in crate::ui) fn error(&self) -> Option<AudioError> {
        if self.sim_stalled() {
            return Some(AudioError::DeviceStalled);
        }
        self.fault.error()
    }

    fn sim_stalled(&self) -> bool {
        let now = self.clock.now();
        self.playback
            .run()
            .and_then(|run| run.stalled_since)
            .is_some_and(|since| now.saturating_sub(since) >= SIM_STALL_AFTER)
    }

    fn note(&mut self, result: &Result<(), AudioError>) {
        match result {
            Ok(()) => self.fault.worked(),
            Err(error) => self.fault.seen(*error),
        }
    }

    /// What the track is doing, read from the sound itself.
    pub(in crate::ui) fn state(&self) -> EngineView {
        let sound = match &self.playback {
            Playback::Simulated(run) => {
                let clock = &run.clock;
                return if clock.finished_at(self.sim_now()) {
                    EngineView::Ended
                } else if clock.is_playing() {
                    EngineView::Playing
                } else {
                    EngineView::Paused
                };
            }
            Playback::Building { build, sound } => {
                return if sound.is_failed() {
                    EngineView::Failed
                } else if build.landing == Landing::Held {
                    EngineView::Paused
                } else {
                    EngineView::Loading
                };
            }
            Playback::Settled(sound) => sound,
        };
        match sound {
            Sound::Empty => EngineView::Idle,
            Sound::Failed(_) => EngineView::Failed,
            // Seeked while paused: the sound still sits at the old position
            // (a track that ran out reads empty), and Play restarts it.
            Sound::Stale(_) => EngineView::Paused,
            Sound::Current(track) if track.player.is_paused() => EngineView::Paused,
            Sound::Current(track) if track.player.empty() => EngineView::Ended,
            Sound::Current(_) => EngineView::Playing,
        }
    }

    /// Where the track is, in seconds of the track: frames the mixer took,
    /// or the parked position when nothing is consuming it.
    pub(in crate::ui) fn position_secs(&self) -> f64 {
        match &self.playback {
            Playback::Simulated(run) => run.clock.elapsed_secs_at(self.sim_now()),
            Playback::Building { build, .. } => build.latest_target,
            Playback::Settled(Sound::Current(track)) => track.probe.secs(),
            Playback::Settled(_) => self.parked_secs,
        }
    }

    /// A start or seek is being built; the position is its target until
    /// it lands.
    pub(in crate::ui) fn seek_pending(&self) -> bool {
        self.playback.building().is_some()
    }

    /// Tests: an engine whose clock is the returned handle (milliseconds).
    #[cfg(test)]
    pub(in crate::ui) fn with_manual_clock(mut self) -> (Self, Arc<AtomicU64>) {
        let millis = Arc::new(AtomicU64::new(0));
        self.clock.manual = Some(millis.clone());
        (self, millis)
    }

    #[cfg(test)]
    pub(in crate::ui) fn gate_builds(&mut self) -> mpsc::Sender<()> {
        let (open, gate) = mpsc::channel();
        self.build_gate = Some(Arc::new(std::sync::Mutex::new(gate)));
        open
    }

    /// Tests: wait until every build thread has ended.
    #[cfg(test)]
    fn join_builds(&mut self) {
        for thread in self.build_threads.drain(..) {
            let _ = thread.join();
        }
    }

    #[cfg(test)]
    pub(in crate::ui) fn await_build_done(&mut self) {
        if let Some(building) = self.playback.building_mut() {
            let result = building.built.recv().unwrap_or(Err(AudioError::Open));
            let (done, built) = mpsc::channel();
            let _ = done.send(result);
            building.built = built;
        }
    }

    #[cfg(test)]
    pub(in crate::ui) fn await_landing(&mut self) {
        while let Some(building) = self.playback.building() {
            let result = building.built.recv().unwrap_or(Err(AudioError::Open));
            self.land(result);
        }
    }

    /// Land a finished build: the new sound takes over from the old one,
    /// and the engine's error follows the outcome. A track whose device
    /// failed to open is paused. Called on the UI tick, when a build is done
    /// and before every command; never waits.
    pub(in crate::ui) fn settle(&mut self) {
        self.land_build();
        // Starts and resumes return while the device opens off the UI
        // thread; this is where one that couldn't open shows.
        if let Some(track) = self.playback.track().filter(|t| t.player.lost()) {
            track.player.pause();
            self.fault.seen(AudioError::NoOutput);
        }
    }

    fn spawn_build(&mut self, target: f64, landing: Landing) -> Result<(), AudioError> {
        let path = self.path.clone().ok_or(AudioError::Open)?;
        let outlet = self.output.outlet().ok_or(AudioError::NoOutput)?;
        let (done, built) = mpsc::channel();
        let end = BuildEnd {
            done: Some(done),
            wake: self.on_built,
        };
        let speed = self.speed.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancel = CancelOnDrop(cancelled.clone());
        #[cfg(test)]
        let (gate, panics) = (self.build_gate.clone(), self.build_panics);
        let thread = quill::thread_guard::spawn_guarded("quill-audio-build", move || {
            #[cfg(test)]
            if let Some(gate) = gate {
                let _ = gate.lock().map(|gate| gate.recv());
            }
            #[cfg(test)]
            assert!(!panics, "test: the decoder panicked");
            end.send(build_paused_player(
                &outlet, &path, target, &speed, &cancelled,
            ));
        })
        .map_err(|_| AudioError::Open)?;
        #[cfg(test)]
        self.build_threads.push(thread);
        #[cfg(not(test))]
        drop(thread);
        self.fault.build_started();
        let sound = self.playback.take_sound();
        self.playback = Playback::Building {
            build: Building {
                built_for: target,
                latest_target: target,
                landing,
                built,
                _cancel: cancel,
            },
            sound,
        };
        Ok(())
    }

    fn land_build(&mut self) {
        let Some(building) = self.playback.building() else {
            return;
        };
        match building.built.try_recv() {
            Ok(result) => self.land(result),
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => self.land(Err(AudioError::Open)),
        }
    }

    fn land(&mut self, result: Result<Built, AudioError>) {
        let Some(Building {
            built_for,
            latest_target: target,
            landing,
            ..
        }) = self.playback.take_build()
        else {
            return;
        };
        let redone =
            ((built_for - target).abs() > f64::EPSILON).then(|| self.spawn_build(target, landing));
        match (redone, result) {
            (Some(Ok(())), _) => {}
            (Some(Err(error)), _) | (None, Err(error)) => self.fail_build(error, target),
            (None, Ok(built)) => {
                built.player.set_volume(self.volume);
                let playing = landing == Landing::Held || built.player.play();
                if !playing {
                    built.player.pause();
                }
                self.playback = Playback::Settled(Sound::Current(Track {
                    player: built.player,
                    probe: built.probe,
                }));
                if playing {
                    self.fault.worked();
                } else {
                    self.fault.seen(AudioError::NoOutput);
                }
            }
        }
    }

    fn fail_build(&mut self, error: AudioError, target: f64) {
        let Playback::Settled(sound) = &mut self.playback else {
            return;
        };
        let ended = sound.ended();
        let silent = ended || matches!(sound, Sound::Stale(_));
        if silent {
            self.parked_secs = target;
        }
        // Nothing is playing: the track that ran out stays, failed.
        *sound = match std::mem::take(sound) {
            Sound::Current(track) | Sound::Stale(track) if ended => Sound::Failed(Some(track)),
            Sound::Empty => Sound::Failed(None),
            other => other,
        };
        self.fault = if silent {
            Fault::Unseen(error)
        } else {
            Fault::Seen(error)
        };
    }

    /// A build that was to be the only sound failed since the last call.
    pub(in crate::ui) fn take_start_failed(&mut self) -> bool {
        match self.fault {
            Fault::Unseen(error) => {
                self.fault = Fault::Seen(error);
                true
            }
            _ => false,
        }
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
        if started.is_err() {
            self.playback = Playback::Settled(Sound::Failed(None));
        }
        self.note(&started);
        started
    }

    /// Bring up the sound at `offset` on whichever backend is selected.
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
            self.playback = Playback::Simulated(SimRun {
                clock,
                stalled_since: None,
            });
            return Ok(());
        }
        self.spawn_build(offset, Landing::Playing)
    }

    /// True while a track is loaded (playing or paused, ended or not).
    pub(in crate::ui) fn is_loaded(&self) -> bool {
        match &self.playback {
            Playback::Settled(sound) => sound.track().is_some(),
            Playback::Building { .. } | Playback::Simulated(_) => true,
        }
    }

    /// The loaded track played to its end.
    pub(in crate::ui) fn is_ended(&self) -> bool {
        self.state() == EngineView::Ended
    }

    pub(in crate::ui) fn pause(&mut self) {
        self.settle();
        let now = self.sim_now();
        if let Some(run) = self.playback.run_mut() {
            run.clock.pause_at(now);
        }
        self.set_landing(Landing::Held);
        if let Some(track) = self.playback.track() {
            track.player.pause();
        }
    }

    /// While a build runs: what its sound does on landing, and the old
    /// sound with it, so the two cannot disagree. No build, no change.
    fn set_landing(&mut self, landing: Landing) {
        let Playback::Building { build, sound } = &mut self.playback else {
            return;
        };
        build.landing = landing;
        let Some(track) = sound.track() else {
            return;
        };
        match landing {
            Landing::Held => track.player.pause(),
            Landing::Playing if !matches!(sound, Sound::Stale(_)) => {
                track.player.play();
            }
            Landing::Playing => {}
        }
    }

    /// A start went out: a failure of the sound no longer stands.
    fn recover_sound(&mut self) {
        let sound = match &mut self.playback {
            Playback::Settled(sound) | Playback::Building { sound, .. } => sound,
            Playback::Simulated(_) => return,
        };
        *sound = std::mem::take(sound).recovered();
    }

    /// Continue from the pause; restarts at `offset_secs` when the sound is
    /// no longer where the track is (seek while paused, failed start), or
    /// from the top when it ended. If that fails the state goes back to what
    /// it was, with the reason in `error`.
    pub(in crate::ui) fn resume(&mut self, offset_secs: f64) -> Result<(), AudioError> {
        self.settle();
        let now = self.sim_now();
        if let Some(run) = self.playback.run_mut() {
            if run.clock.finished_at(now) {
                run.clock.seek_at(0.0, now);
            }
            run.clock.resume_at(now);
            self.fault.worked();
            return Ok(());
        }
        if self.playback.building().is_some() {
            self.set_landing(Landing::Playing);
            self.fault.worked();
            return Ok(());
        }
        let Some(sound) = self.playback.sound() else {
            return Ok(());
        };
        let (ended, current) = (sound.ended(), sound.is_current());
        if !current || ended {
            if self.path.is_none() {
                return Ok(());
            }
            let at = if ended && current {
                0.0
            } else {
                offset_secs.max(0.0)
            };
            let started = self.begin(at);
            if started.is_ok() {
                self.recover_sound();
            }
            self.note(&started);
            return started;
        }
        if let Some(track) = self.playback.track()
            && !track.player.play()
        {
            track.player.pause();
            let woke = Err(AudioError::NoOutput);
            self.note(&woke);
            return woke;
        }
        self.fault.worked();
        Ok(())
    }

    /// Move to `offset_secs`. While playing, a build thread opens the track
    /// at the target and `settle` swaps it in, so the decoder's seek never
    /// runs on the audio thread; the old sound plays on meanwhile, the
    /// position reads the target, and a failed build keeps the old sound
    /// with the reason in `error`. While paused it catches up on resume.
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
        if let Some(run) = self.playback.run_mut() {
            run.clock.seek_at(offset, now);
            if self.simulation.is_some_and(|simulation| simulation.stall) {
                run.stalled_since.get_or_insert(self.clock.now());
            }
            return Ok(());
        }
        if let Some(building) = self.playback.building_mut() {
            building.latest_target = offset;
            return Ok(());
        }
        if !playing {
            if let Playback::Settled(sound) = &mut self.playback {
                *sound = match std::mem::take(sound) {
                    Sound::Current(track) => Sound::Stale(track),
                    other => other,
                };
            }
            self.parked_secs = offset;
            return Ok(());
        }
        if self.path.is_none() {
            return Ok(());
        }
        self.begin(offset)
    }

    pub(in crate::ui) fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        if let Some(track) = self.playback.track() {
            track.player.set_volume(self.volume);
        }
    }

    /// Takes effect on the running sound at its next 20 ms block.
    pub(in crate::ui) fn set_speed(&mut self, speed: f64) {
        self.speed.set(speed);
        let now = self.sim_now();
        if let Some(run) = self.playback.run_mut() {
            run.clock.set_rate_at(speed, now);
        }
    }

    pub(in crate::ui) fn stop(&mut self) {
        self.parked_secs = self.position_secs();
        self.playback = Playback::Settled(Sound::Empty);
        self.path = None;
        self.fault = Fault::None;
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
    use super::{AudioEngine, AudioError, EngineView, Simulation};
    use crate::ui::audio::SharedOutput;
    use rodio::mixer::MixerSource;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;

    /// An engine on a fake output, plus the "device" that pulls from it.
    fn rig() -> (AudioEngine, MixerSource) {
        let (output, source) = device();
        (AudioEngine::new(output), source)
    }

    fn start_landed(
        engine: &mut AudioEngine,
        path: &Path,
        offset: f64,
        volume: f32,
        speed: f64,
        duration: f64,
    ) {
        engine.start(path, offset, volume, speed, duration).unwrap();
        engine.await_landing();
    }

    #[test]
    fn start_without_an_output_is_failed_and_keeps_the_offset() {
        let mut engine = AudioEngine::new(SharedOutput::for_test(None));
        let path = track("no-output.wav", 2);
        assert!(engine.start(&path, 1.5, 1.0, 1.0, 2.0).is_err());
        assert_eq!(engine.state(), EngineView::Failed);
        assert!((engine.position_secs() - 1.5).abs() < 1e-9);
    }

    #[test]
    fn start_of_a_missing_file_fails_when_the_build_lands() {
        let (mut engine, _device) = rig();
        let path = std::env::temp_dir().join("quill-engine-test-missing.wav");
        engine.start(&path, 1.5, 1.0, 1.0, 2.0).unwrap();
        assert_eq!(engine.state(), EngineView::Loading);
        engine.await_landing();
        assert_eq!(engine.state(), EngineView::Failed);
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
        assert_eq!(engine.state(), EngineView::Loading);
        assert!(engine.is_loaded());
        assert!((engine.position_secs() - 2.0).abs() < 1e-9);
        engine.settle();
        assert_eq!(engine.state(), EngineView::Loading, "still reading");
        open.send(()).unwrap();
        engine.await_landing();
        assert_eq!(engine.state(), EngineView::Playing);
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
        assert_eq!(engine.state(), EngineView::Paused);
        engine.resume(1.0).unwrap();
        assert_eq!(engine.state(), EngineView::Loading);
        engine.pause();
        open.send(()).unwrap();
        engine.await_landing();
        assert_eq!(engine.state(), EngineView::Paused);
        pull(&mut device, usize::try_from(RATE).unwrap());
        assert!((engine.position_secs() - 1.0).abs() < 0.05);
        engine.resume(1.0).unwrap();
        assert_eq!(engine.state(), EngineView::Playing);
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
        assert_eq!(engine.state(), EngineView::Loading);
        drop(open);
        engine.await_landing();
        assert_eq!(engine.state(), EngineView::Playing);
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
        assert_eq!(engine.state(), EngineView::Paused);
        drop(open);
        engine.await_landing();
        assert_eq!(engine.state(), EngineView::Paused);
        assert!((engine.position_secs() - 4.0).abs() < 0.05);
    }

    #[test]
    fn a_stop_while_loading_lands_nothing() {
        let (mut engine, _device) = rig();
        let open = engine.gate_builds();
        let path = track("loading-stop.wav", 4);
        engine.start(&path, 1.0, 1.0, 1.0, 4.0).unwrap();
        engine.stop();
        assert_eq!(engine.state(), EngineView::Idle);
        assert!(!engine.is_loaded());
        drop(open);
        engine.settle();
        assert_eq!(engine.state(), EngineView::Idle);
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
        engine.await_landing();
        pull(&mut device, usize::try_from(RATE).unwrap());
        let secs = engine.position_secs();
        assert!((2.9..=3.3).contains(&secs), "got {secs}");
    }

    #[test]
    fn a_build_thread_that_died_fails_the_start() {
        let (mut engine, _device) = rig();
        let _open = engine.gate_builds();
        let path = track("loading-died.wav", 4);
        engine.start(&path, 1.0, 1.0, 1.0, 4.0).unwrap();
        if let Some(building) = engine.playback.building_mut() {
            building.built = mpsc::channel().1;
        }
        engine.settle();
        assert_eq!(engine.state(), EngineView::Failed);
        assert_eq!(engine.error(), Some(AudioError::Open));
    }

    static PANICKED_BUILD_WAKES: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn a_build_thread_that_panics_still_wakes_the_app_to_fail_the_start() {
        let (mut engine, _device) = rig();
        engine.set_on_built(|| {
            PANICKED_BUILD_WAKES.fetch_add(1, Ordering::SeqCst);
        });
        engine.build_panics = true;
        let path = track("loading-panics.wav", 4);
        engine.start(&path, 1.0, 1.0, 1.0, 4.0).unwrap();
        engine.join_builds();
        assert_eq!(PANICKED_BUILD_WAKES.load(Ordering::SeqCst), 1);
        engine.settle();
        assert_eq!(engine.state(), EngineView::Failed);
        assert!(engine.take_start_failed(), "the user is told");
    }

    #[test]
    fn a_good_start_after_a_failed_one_reports_no_failure() {
        let (mut engine, _device) = rig();
        let open = engine.gate_builds();
        let path = track("loading-after-died.wav", 4);
        engine.start(&path, 1.0, 1.0, 1.0, 4.0).unwrap();
        if let Some(building) = engine.playback.building_mut() {
            building.built = mpsc::channel().1;
        }
        engine.settle();
        assert_eq!(engine.state(), EngineView::Failed);
        engine.resume(1.0).unwrap();
        drop(open);
        engine.await_landing();
        assert_eq!(engine.state(), EngineView::Playing);
        assert!(!engine.take_start_failed());
    }

    #[test]
    fn a_replay_after_the_end_that_cannot_open_fails_and_does_not_end() {
        let (mut engine, mut device) = rig();
        let path = track("replay-missing.wav", 1);
        start_landed(&mut engine, &path, 0.0, 1.0, 1.0, 1.0);
        pull(&mut device, usize::try_from(RATE).unwrap() * 2);
        assert_eq!(engine.state(), EngineView::Ended);
        std::fs::remove_file(&path).unwrap();
        engine.resume(0.0).unwrap();
        engine.await_landing();
        assert_eq!(engine.state(), EngineView::Failed);
        assert!(!engine.is_ended(), "the tick must not take it for the end");
        assert_eq!(engine.error(), Some(AudioError::Open));
        assert!(engine.take_start_failed(), "the user is told");
    }

    #[test]
    fn a_stopped_or_replaced_build_makes_no_player() {
        let (output, _device) = device();
        let mut engine = AudioEngine::new(output.clone());
        let open = engine.gate_builds();
        let first = track("cancel-first.wav", 4);
        let second = track("cancel-second.wav", 4);
        engine.start(&first, 2.0, 1.0, 1.0, 4.0).unwrap();
        engine.stop();
        engine.start(&first, 2.0, 1.0, 1.0, 4.0).unwrap();
        engine.start(&second, 2.0, 1.0, 1.0, 4.0).unwrap();
        drop(open);
        engine.join_builds();
        engine.settle();
        assert_eq!(engine.state(), EngineView::Playing);
        assert_eq!(output.players_made(), 1, "only the build that landed");
    }

    #[test]
    fn a_started_track_plays_and_a_pause_holds_it() {
        let (mut engine, mut device) = rig();
        let path = track("play-pause.wav", 4);
        start_landed(&mut engine, &path, 0.0, 1.0, 1.0, 4.0);
        assert_eq!(engine.state(), EngineView::Playing);
        engine.pause();
        assert_eq!(engine.state(), EngineView::Paused);
        pull(&mut device, 4000);
        assert!(engine.position_secs() < 0.05);
    }

    #[test]
    fn position_follows_the_frames_the_device_consumed() {
        let (mut engine, mut device) = rig();
        let path = track("position.wav", 4);
        start_landed(&mut engine, &path, 0.0, 1.0, 1.0, 4.0);
        assert!(engine.position_secs() < 0.05, "no wall-clock advance");
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
        assert_eq!(engine.state(), EngineView::Ended);
    }

    #[test]
    fn a_seek_on_a_track_that_ran_out_parks_it_instead_of_ending_it() {
        let (mut engine, mut device) = rig();
        let path = track("ran-out-seek.wav", 1);
        start_landed(&mut engine, &path, 0.0, 1.0, 1.0, 1.0);
        pull(&mut device, usize::try_from(RATE).unwrap() * 2);
        assert!(engine.is_ended());
        engine.seek(0.0, false).unwrap();
        assert_eq!(engine.state(), EngineView::Paused);
        assert!(!engine.is_ended(), "the tick does not end a parked track");
        engine.resume(0.0).unwrap();
        engine.await_landing();
        assert_eq!(engine.state(), EngineView::Playing);
    }

    #[test]
    fn a_hold_while_loading_pauses_the_old_sound_and_a_resume_plays_it() {
        let (mut engine, mut device) = rig();
        let path = track("hold-old.wav", 6);
        start_landed(&mut engine, &path, 0.0, 1.0, 1.0, 6.0);
        let open = engine.gate_builds();
        engine.seek(3.0, true).unwrap();
        engine.pause();
        assert_eq!(engine.state(), EngineView::Paused);
        pull(&mut device, usize::try_from(RATE).unwrap());
        assert!(engine.position_secs() - 3.0 < 1e-9, "the target is shown");
        engine.resume(3.0).unwrap();
        assert_eq!(engine.state(), EngineView::Loading);
        drop(open);
        engine.await_landing();
        assert_eq!(engine.state(), EngineView::Playing);
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
        assert_eq!(engine.state(), EngineView::Playing);
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
        assert_eq!(engine.state(), EngineView::Playing);
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
        assert_eq!(engine.state(), EngineView::Paused);
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
        assert_eq!(engine.state(), EngineView::Loading);
        engine.await_landing();
        assert_eq!(engine.state(), EngineView::Paused);
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
        }));
        assert!(engine.start(&path, 1.0, 1.0, 1.0, 4.0).is_err());
        assert_eq!(engine.state(), EngineView::Failed);
        engine.set_simulation(None);
        engine.resume(1.0).unwrap();
        engine.await_landing();
        assert_eq!(engine.state(), EngineView::Playing);
        assert!((engine.position_secs() - 1.0).abs() < 0.05);
    }

    #[test]
    fn stop_keeps_the_last_position() {
        let (mut engine, mut device) = rig();
        let path = track("stop.wav", 4);
        start_landed(&mut engine, &path, 0.0, 1.0, 1.0, 4.0);
        pull(&mut device, usize::try_from(RATE).unwrap());
        engine.stop();
        assert_eq!(engine.state(), EngineView::Idle);
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
        start_landed(&mut engine, &path, 3.0, 1.0, 1.0, 10.0);
        assert_eq!(engine.state(), EngineView::Playing);
        engine.pause();
        assert_eq!(engine.state(), EngineView::Paused);
        assert!((engine.position_secs() - 3.0).abs() < 0.05);
        engine.seek(12.0, false).unwrap();
        engine.resume(12.0).unwrap();
        assert_eq!(engine.state(), EngineView::Ended);
    }
    /// Land the build a seek started, as the tick would.
    fn land_seek(engine: &mut AudioEngine) {
        engine.await_landing();
        engine.settle();
        assert!(!engine.seek_pending(), "the seek landed");
    }

    #[test]
    fn a_seek_while_playing_lands_without_the_device_pulling() {
        let (mut engine, mut device) = rig();
        let path = track("seek-stalled.wav", 6);
        start_landed(&mut engine, &path, 0.0, 1.0, 1.0, 6.0);
        pull(&mut device, usize::try_from(RATE).unwrap());
        // Nothing pulls from here on: the seek is a build, so the audio
        // thread has no part in it.
        engine.seek(4.0, true).unwrap();
        engine.seek(5.0, true).unwrap();
        assert!(engine.seek_pending());
        assert_eq!(engine.state(), EngineView::Loading);
        assert!(
            (engine.position_secs() - 5.0).abs() < 1e-9,
            "the newest target"
        );
        land_seek(&mut engine);
        assert_eq!(engine.state(), EngineView::Playing);
        let landed = engine.position_secs();
        assert!((landed - 5.0).abs() < 1e-9, "got {landed}");
        assert_eq!(engine.error(), None);
        pull(&mut device, usize::try_from(RATE).unwrap() / 2);
        let later = engine.position_secs();
        assert!(
            (5.3..=5.7).contains(&later),
            "plays on from it, got {later}"
        );
    }

    #[test]
    fn a_seek_while_playing_is_built_off_the_ui_thread_and_a_pause_holds_it() {
        let (mut engine, mut device) = rig();
        let path = track("restart-paused.wav", 6);
        start_landed(&mut engine, &path, 0.0, 1.0, 1.0, 6.0);
        pull(&mut device, usize::try_from(RATE).unwrap());
        let open = engine.gate_builds();
        engine.seek(4.0, true).unwrap();
        engine.pause();
        assert_eq!(engine.state(), EngineView::Paused);
        open.send(()).unwrap();
        land_seek(&mut engine);
        assert_eq!(engine.state(), EngineView::Paused, "the pause survives");
        let secs = engine.position_secs();
        assert!((secs - 4.0).abs() < 1e-9, "got {secs}");
        assert_eq!(engine.error(), None);
    }

    #[test]
    fn a_seek_while_a_seek_builds_retargets_it() {
        let (mut engine, mut device) = rig();
        let path = track("restart-retarget.wav", 8);
        start_landed(&mut engine, &path, 0.0, 1.0, 1.0, 8.0);
        pull(&mut device, usize::try_from(RATE).unwrap());
        let open = engine.gate_builds();
        engine.seek(6.0, true).unwrap();
        engine.seek(2.0, true).unwrap();
        assert!(
            (engine.position_secs() - 2.0).abs() < 1e-9,
            "the newest target at once"
        );
        // The first build lands at 6 s, finds the newer target and builds
        // again.
        open.send(()).unwrap();
        open.send(()).unwrap();
        land_seek(&mut engine);
        let secs = engine.position_secs();
        assert!((secs - 2.0).abs() < 1e-9, "got {secs}");
        assert_eq!(engine.error(), None);
    }

    #[test]
    fn a_failed_seek_keeps_the_sound_playing_with_the_engines_error() {
        let (mut engine, mut device) = rig();
        let path = track("seek-fails.wav", 6);
        start_landed(&mut engine, &path, 0.0, 1.0, 1.0, 6.0);
        pull(&mut device, usize::try_from(RATE).unwrap());
        std::fs::remove_file(&path).unwrap();
        engine.seek(5.0, true).unwrap();
        assert!(
            (engine.position_secs() - 5.0).abs() < 1e-9,
            "pending target"
        );
        land_seek(&mut engine);
        assert_eq!(engine.error(), Some(AudioError::Open));
        assert_eq!(engine.state(), EngineView::Playing);
        let secs = engine.position_secs();
        assert!(
            (0.95..=1.2).contains(&secs),
            "back at the sound, got {secs}"
        );
    }

    /// What a saved position resumes from: the sound the device played,
    /// not what the speed stretcher read ahead of it. It may trail what was
    /// played by up to a block, never run past it: a resume from there
    /// replays a moment instead of skipping one.
    #[test]
    fn position_trails_what_was_played_by_at_most_a_block_at_1x_and_1_5x() {
        const RATE: u32 = 48_000;
        for speed in [1.0, 1.5] {
            let (output, mut device) = device_at(RATE);
            let mut engine = AudioEngine::new(output);
            let path = track_at("played.wav", 4, RATE);
            start_landed(&mut engine, &path, 0.0, 1.0, speed, 4.0);
            pull(&mut device, usize::try_from(RATE).unwrap() / 2);
            let played = 0.5 * speed;
            let block = 0.04 * speed;
            let secs = engine.position_secs();
            assert!(
                secs <= played + 1e-9,
                "at {speed}x played {played} s, position read {secs}: past the sound"
            );
            assert!(
                played - secs <= block,
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
        assert_eq!(engine.state(), EngineView::Failed);
        engine.stop();
        assert_eq!(engine.error(), None);
    }

    #[test]
    fn a_failed_seek_is_cleared_by_the_next_good_one() {
        let (mut engine, mut device) = rig();
        let path = track("error-seek.wav", 1);
        start_landed(&mut engine, &path, 0.0, 1.0, 1.0, 1.0);
        pull(&mut device, usize::try_from(RATE).unwrap() * 2);
        assert_eq!(engine.state(), EngineView::Ended);
        std::fs::remove_file(&path).unwrap();
        engine.seek(0.5, true).unwrap();
        engine.await_landing();
        assert_eq!(engine.error(), Some(AudioError::Open));
        track("error-seek.wav", 1);
        engine.seek(0.5, true).unwrap();
        engine.await_landing();
        assert_eq!(engine.error(), None);
        assert_eq!(engine.state(), EngineView::Playing);
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
        engine.await_landing();
        assert_eq!(engine.error(), Some(AudioError::Open));
        assert_eq!(engine.state(), EngineView::Paused);
        track("error-resume.wav", 4);
        engine.resume(2.0).unwrap();
        engine.await_landing();
        assert_eq!(engine.error(), None);
        assert_eq!(engine.state(), EngineView::Playing);
    }
}
