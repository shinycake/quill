//! The app's one audio output device, open only while something plays.
//!
//! Every player connects to one app mixer that never goes away, made with
//! the first device's layout. A device plays it while any player's sound
//! is being pulled ([`Heard`]), and is closed once nothing has played for
//! [`QUIET_FOR`] (tdesktop detaches its device after 500 ms of silence,
//! `kDetachDeviceTimeout`). An open output keeps the audio hardware
//! running: on macOS coreaudiod holds an assertion that keeps the Mac from
//! idle sleep, and the device's threads and buffers stay alive. Whatever
//! needs the device again (a new sound, play, a seek) goes through
//! [`OutputPlayer`], which has it opened on a thread of its own: opening
//! takes 10 to 75 ms on macOS, up to five frames on the UI thread. A
//! player made while no device is up connects to the app mixer once one
//! is. See `docs/decisions/codex-audio-in-process.md`.
#![deny(clippy::indexing_slicing)]

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use quill::advanced::debug_mode::debug_log;
use quill::cpal_stream::StreamErrors;
use rodio::cpal::traits::{DeviceTrait, HostTrait};
use rodio::mixer::{Mixer, MixerSource};
use rodio::queue::SourcesQueueOutput;
use rodio::source::SeekError;
use rodio::{
    ChannelCount, CpalError, DeviceSinkBuilder, DeviceSinkError, MixerDeviceSink, Player,
    SampleRate, Source,
};

#[cfg(test)]
use gate::Gate;

/// How long nothing may play before the device is closed.
const QUIET_FOR: Duration = Duration::from_millis(500);
/// How often an open device is checked for silence.
const LOOK_EVERY: Duration = Duration::from_millis(250);
/// How long after a device failed to open the next sound tries again.
const RETRY_AFTER: Duration = Duration::from_secs(5);
/// Frames the device takes from the app mixer at a time (about 5 ms).
const FEED_FRAMES: usize = 256;
/// How long an alert may wait for the device before it is dropped.
const ALERT_LATE: Duration = Duration::from_secs(2);

/// The app's one audio output, shared by the playback engines, the
/// notification sounds, the call tones and video soundtracks. `None` from
/// [`Self::outlet`] when the machine has no usable device (retried after
/// [`RETRY_AFTER`]).
///
/// The device is the system *default* output, so on macOS (cpal 0.18) its
/// stream follows a new default device by itself. Where the stream cannot
/// follow (Windows reports the change as `StreamInvalidated`), or the
/// device went away, the error callback marks it stale and the output
/// opens the new default: the app mixer, and every player on it, moves
/// over.
#[derive(Clone, Default)]
pub(in crate::ui) struct SharedOutput(Arc<Output>);

#[derive(Default)]
struct Output {
    state: Mutex<OutputState>,
    /// Signalled when a device opening off the calling thread is up or
    /// failed.
    opened: Condvar,
    /// Set by every sound a player pulls, cleared by the watch.
    heard: Arc<AtomicBool>,
    /// Set each time the device asks for sound ([`Feed`]), cleared by the
    /// watch.
    pulled: Arc<AtomicBool>,
    /// Open [`Hold`]s.
    holds: AtomicUsize,
    /// Numbers the players, for their parked sounds ([`Parked`]).
    players: AtomicU64,
    /// Tests: stands in for the app mixer and a device that is always up
    /// (`Some(None)` is "no device").
    #[cfg(test)]
    forced: Option<Option<Mixer>>,
    /// Tests: how many times a player asked for the device.
    #[cfg(test)]
    wakes: AtomicUsize,
    /// Tests: opens devices in place of the system ([`Gate`]).
    #[cfg(test)]
    gate: Option<Arc<Gate>>,
}

#[derive(Default)]
struct OutputState {
    /// Made with the first device's layout, on the thread that opened it,
    /// and kept for the app's life. A device with another layout converts
    /// its sound ([`Feed`]).
    app: Option<AppMixer>,
    /// Players made while no device was up, connected to the app mixer when
    /// one is.
    parked: Vec<Parked>,
    device: Option<Device>,
    failed_at: Option<Instant>,
    /// A watch thread looks after the open device.
    watched: bool,
    /// A device is opening on a thread of its own ([`reopen`]).
    opening: bool,
}

struct AppMixer {
    mixer: Mixer,
    /// Pulled by one device at a time ([`Feed`]).
    sound: Arc<Mutex<MixerSource>>,
}

struct Device {
    /// The open stream; `None` only for a test's stand-in ([`Gate`]).
    _sink: Option<MixerDeviceSink>,
    /// Set from cpal's audio thread when the stream ended.
    stale: Arc<AtomicBool>,
    /// Tests: tells the gate when the device is dropped.
    #[cfg(test)]
    _gate: Option<gate::Dropped>,
}

/// A player's sound, waiting for a device to come up. Dropped with its
/// player, as nothing else would ever play it: a video's soundtrack lets
/// go of the decoder at once (the picture then runs on the wall clock).
struct Parked {
    queue: SourcesQueueOutput,
    player: u64,
}

/// Where players connect: the app mixer.
#[derive(Clone)]
pub(in crate::ui) struct Outlet {
    output: SharedOutput,
}

impl Outlet {
    pub(in crate::ui) fn player(&self) -> OutputPlayer {
        let (player, queue) = Player::new();
        OutputPlayer {
            player,
            output: self.output.clone(),
            id: self.output.0.players.fetch_add(1, Ordering::Relaxed),
            queue: Mutex::new(Some(queue)),
        }
    }
}

/// A player on the app mixer. Whatever needs the audio thread has the
/// device opened first: with it closed nothing pulls the sound, so a play
/// would stall and a seek would wait for an answer forever.
pub(in crate::ui) struct OutputPlayer {
    player: Player,
    output: SharedOutput,
    /// Names its parked sound ([`Parked`]).
    id: u64,
    /// Connects with its first sound. The mixer reads a player's rate and
    /// channels once, as it connects, for its first 512 samples, and an
    /// empty queue answers 48 kHz mono.
    queue: Mutex<Option<SourcesQueueOutput>>,
}

impl Drop for OutputPlayer {
    fn drop(&mut self) {
        self.output.unpark(self.id);
    }
}

impl OutputPlayer {
    /// Queue `source`, marked so the output stays open while it plays.
    pub(in crate::ui) fn append<S>(&self, source: S)
    where
        S: Source + Send + 'static,
    {
        self.queue(source, None);
    }

    /// [`Self::append`] for an alert, which is pointless late: dropped
    /// unless it starts within [`ALERT_LATE`]. A device that failed to open
    /// would otherwise play it whenever one opens next, over a voice note
    /// minutes later.
    pub(in crate::ui) fn append_alert<S>(&self, source: S)
    where
        S: Source + Send + 'static,
    {
        self.queue(source, Some(Instant::now() + ALERT_LATE));
    }

    fn queue<S>(&self, source: S, starts_by: Option<Instant>)
    where
        S: Source + Send + 'static,
    {
        if !self.player.is_paused() {
            self.output.wake();
        }
        self.player.append(Heard {
            source,
            heard: self.output.0.heard.clone(),
            starts_by,
        });
        let queue = self
            .queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(queue) = queue {
            self.output.connect(queue, self.id);
        }
    }

    /// False when no device opened lately. True when one is up or opening
    /// off the calling thread: the sound starts once it is up, and a device
    /// that fails to open leaves the player [`Self::lost`]. Unpaused either
    /// way, so a device that opens later plays it.
    pub(in crate::ui) fn play(&self) -> bool {
        let woke = self.output.wake();
        self.player.play();
        woke
    }

    /// Playing, with nothing to play it: the device it asked for failed to
    /// open, and none is up or opening.
    pub(in crate::ui) fn lost(&self) -> bool {
        !self.player.is_paused() && !self.player.empty() && !self.output.has_device()
    }

    pub(in crate::ui) fn pause(&self) {
        self.player.pause();
    }

    pub(in crate::ui) fn stop(&self) {
        self.player.stop();
    }

    pub(in crate::ui) fn set_volume(&self, volume: f32) {
        self.player.set_volume(volume);
    }

    pub(in crate::ui) fn is_paused(&self) -> bool {
        self.player.is_paused()
    }

    pub(in crate::ui) fn empty(&self) -> bool {
        self.player.empty()
    }
}

impl SharedOutput {
    /// Tests: an output that hands out `mixer` (or nothing) and has a
    /// device whenever it has a mixer.
    #[cfg(test)]
    pub(in crate::ui) fn for_test(mixer: Option<Mixer>) -> Self {
        Self(Arc::new(Output {
            forced: Some(mixer),
            ..Output::default()
        }))
    }

    /// Tests: an output whose devices open through the returned [`Gate`].
    #[cfg(test)]
    pub(in crate::ui) fn gated() -> (Self, Arc<Gate>) {
        let gate = Arc::new(Gate::new());
        let output = Self(Arc::new(Output {
            gate: Some(gate.clone()),
            ..Output::default()
        }));
        (output, gate)
    }

    /// Tests: players made on this output so far.
    #[cfg(test)]
    pub(in crate::ui) fn players_made(&self) -> u64 {
        self.0.players.load(Ordering::Relaxed)
    }

    /// Where every new sound connects. A closed device starts opening on a
    /// thread of its own, as the sound will need it. `None` only when no
    /// device opened lately.
    pub(in crate::ui) fn outlet(&self) -> Option<Outlet> {
        #[cfg(test)]
        if let Some(forced) = &self.0.forced {
            return forced.is_some().then(|| Outlet {
                output: self.clone(),
            });
        }
        self.wake().then(|| Outlet {
            output: self.clone(),
        })
    }

    /// Connect a new player's sound to the app mixer, or park it until a
    /// device is up: the app mixer may not be made yet, and a sound waiting
    /// there for a device that never comes would outlive its player.
    fn connect(&self, queue: SourcesQueueOutput, player: u64) {
        #[cfg(test)]
        if let Some(Some(mixer)) = &self.0.forced {
            mixer.add(queue);
            return;
        }
        let mut state = self.0.lock();
        if let (Some(app), Some(_)) = (&state.app, &state.device) {
            app.mixer.add(queue);
            return;
        }
        state.parked.push(Parked { queue, player });
    }

    /// Drop the parked sound of a player that is gone.
    fn unpark(&self, player: u64) {
        let mut state = self.0.lock();
        let gone: Vec<Parked> = state
            .parked
            .extract_if(.., |parked| parked.player == player)
            .collect();
        drop(state);
        // Unlocked: a video's soundtrack lets go of its decoder here.
        drop(gone);
    }

    /// Tests: how many times a player asked for the device.
    #[cfg(test)]
    pub(in crate::ui) fn wakes(&self) -> usize {
        self.0.wakes.load(Ordering::Relaxed)
    }

    /// Tests: wait until no device is opening.
    #[cfg(test)]
    pub(in crate::ui) fn settle_device(&self) {
        let mut state = self.0.lock();
        while state.opening {
            state = self
                .0
                .opened
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Keep the device open, once a sound opened it, until the hold drops.
    pub(in crate::ui) fn hold(&self) -> Hold {
        self.0.holds.fetch_add(1, Ordering::Relaxed);
        Hold(self.0.clone())
    }

    /// Have the device opened, off the calling thread, for a player about
    /// to need it. False when no device opened lately.
    fn wake(&self) -> bool {
        #[cfg(test)]
        if let Some(forced) = &self.0.forced {
            self.0.wakes.fetch_add(1, Ordering::Relaxed);
            return forced.is_some();
        }
        self.0.lock().open(&self.0)
    }

    /// A device is up, or opening.
    fn has_device(&self) -> bool {
        #[cfg(test)]
        if let Some(forced) = &self.0.forced {
            return forced.is_some();
        }
        let state = self.0.lock();
        state.device.is_some() || state.opening
    }
}

impl Output {
    fn lock(&self) -> MutexGuard<'_, OutputState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether the device should stay open: something played since the
    /// last look, a hold is open, or the device didn't ask for sound. A
    /// device that stalls (a reroute, a Bluetooth reconnect) stops asking,
    /// and its silence then says nothing about the players: a player could
    /// be waiting on it, a seek parked for its answer.
    fn sounded(&self) -> bool {
        let heard = self.heard.swap(false, Ordering::Relaxed);
        let pulled = self.pulled.swap(false, Ordering::Relaxed);
        heard || !pulled || self.holds.load(Ordering::Relaxed) > 0
    }
}

/// Keeps the device open, once a sound opened it, while held: between the
/// rings of a call sound that loops, so it isn't reopened for every ring.
pub(in crate::ui) struct Hold(Arc<Output>);

impl Drop for Hold {
    fn drop(&mut self) {
        self.0.holds.fetch_sub(1, Ordering::Relaxed);
    }
}

impl OutputState {
    /// Make sure a live device plays the app mixer, or that one is opening
    /// on a thread of its own, and that a watch closes it once nothing
    /// plays. Never waits for a device. Counts as sound, so the watch gives
    /// a player about to start [`QUIET_FOR`] to do so. False when no device
    /// opened lately.
    fn open(&mut self, output: &Arc<Output>) -> bool {
        output.heard.store(true, Ordering::Relaxed);
        let stale = self.device.take_if(|device| device.is_stale());
        if stale.is_some() {
            debug_log("audio: output: reopening the default output device");
            self.failed_at = None;
        }
        if self.device.is_some() {
            self.watch(output);
            return true;
        }
        if self.opening {
            return true;
        }
        if self.failed_at.is_some_and(|at| at.elapsed() < RETRY_AFTER) {
            return false;
        }
        let opener = output.clone();
        let spawned = quill::thread_guard::spawn_guarded("quill-audio-open", move || {
            reopen(&opener, stale);
        });
        self.opening = spawned.is_ok();
        if !self.opening {
            self.failed_at = Some(Instant::now());
        }
        self.opening
    }

    /// Play the app mixer on a device just opened, with a watch to close
    /// it. The first device's layout makes the app mixer, and the players
    /// parked until a device was up connect now.
    fn install(&mut self, sink: OpenSink, output: &Arc<Output>) {
        let app = self
            .app
            .get_or_insert_with(|| AppMixer::new(sink.channels, sink.rate));
        for parked in self.parked.drain(..) {
            app.mixer.add(parked.queue);
        }
        let feed = Feed::new(app.sound.clone(), output.pulled.clone());
        self.device = Some(sink.play(feed));
        self.failed_at = None;
        self.watch(output);
    }

    /// Make sure a watch looks after the open device; one that failed to
    /// start is tried again, or the device would never close.
    fn watch(&mut self, output: &Arc<Output>) {
        // Tests: a gate's device stays open, as the watch looks in real
        // time.
        #[cfg(test)]
        if output.gate.is_some() {
            return;
        }
        if !self.watched {
            let watched = output.clone();
            let spawned = std::thread::Builder::new()
                .name("quill-audio-idle".into())
                .spawn(move || watch(&watched));
            self.watched = spawned.is_ok();
        }
    }
}

/// Open the default device for the app mixer, on a thread of its own and
/// without holding the output's lock while the device comes up. `stale`
/// is the device it replaces.
fn reopen(output: &Arc<Output>, stale: Option<Device>) {
    let mut reopened = Reopened { output, sink: None };
    // Stopping a stream takes a while too; a panic there still ends the
    // open (`Reopened`).
    drop(stale);
    #[cfg(test)]
    if let Some(gate) = &output.gate {
        reopened.sink = Gate::open(gate);
        return;
    }
    reopened.sink = open_sink();
}

/// Ends an open off the calling thread however it went, a panic included,
/// so a seek waiting for it doesn't wait for ever.
struct Reopened<'a> {
    output: &'a Arc<Output>,
    sink: Option<OpenSink>,
}

impl Drop for Reopened<'_> {
    fn drop(&mut self) {
        let mut state = self.output.lock();
        state.opening = false;
        match self.sink.take() {
            Some(sink) => state.install(sink, self.output),
            None => state.failed_at = Some(Instant::now()),
        }
        drop(state);
        self.output.opened.notify_all();
    }
}

impl AppMixer {
    fn new(channels: ChannelCount, rate: SampleRate) -> Self {
        let (mixer, sound) = rodio::mixer::mixer(channels, rate);
        Self {
            mixer,
            sound: Arc::new(Mutex::new(sound)),
        }
    }
}

/// A device just opened, not yet playing the app mixer.
struct OpenSink {
    /// `None` only for a test's stand-in ([`Gate`]).
    sink: Option<MixerDeviceSink>,
    channels: ChannelCount,
    rate: SampleRate,
    stale: Arc<AtomicBool>,
    /// Tests: takes the device's side of the app mixer.
    #[cfg(test)]
    gate: Option<Arc<Gate>>,
}

/// Open the default output device.
fn open_sink() -> Option<OpenSink> {
    let stale = Arc::new(AtomicBool::new(false));
    let started = Instant::now();
    let mut sink = open_default_output(output_error_callback(stale.clone())).ok()?;
    debug_log(&format!(
        "audio: output: opened the output device in {} ms on {}",
        started.elapsed().as_millis(),
        std::thread::current().name().unwrap_or("an unnamed thread"),
    ));
    sink.log_on_drop(false);
    let config = sink.config();
    Some(OpenSink {
        channels: config.channel_count(),
        rate: config.sample_rate(),
        sink: Some(sink),
        stale,
        #[cfg(test)]
        gate: None,
    })
}

impl OpenSink {
    fn play(self, feed: Feed) -> Device {
        #[cfg(test)]
        if let Some(gate) = &self.gate {
            gate.plays(feed);
            return Device {
                _sink: None,
                stale: self.stale,
                _gate: Some(gate::Dropped(gate.clone())),
            };
        }
        if let Some(sink) = &self.sink {
            sink.mixer().add(feed);
        }
        Device {
            _sink: self.sink,
            stale: self.stale,
            #[cfg(test)]
            _gate: None,
        }
    }
}

impl Device {
    fn is_stale(&self) -> bool {
        self.stale.load(Ordering::Relaxed)
    }
}

/// Looks after the open device: closes it once nothing has played for
/// [`QUIET_FOR`], and moves the app mixer to the new default device when
/// the stream ended while something plays. Ends with the device.
fn watch(output: &Arc<Output>) {
    let mut quiet = Quiet::default();
    loop {
        std::thread::sleep(LOOK_EVERY);
        if !look(output, &mut quiet, Instant::now()) {
            return;
        }
    }
}

/// One look of the watch at `now`: whether to look again.
fn look(output: &Arc<Output>, quiet: &mut Quiet, now: Instant) -> bool {
    let mut state = output.lock();
    let sounded = output.sounded();
    let Some(device) = &state.device else {
        state.watched = false;
        return false;
    };
    if device.is_stale() {
        // Under the lock, so that players always see a device up or
        // opening: the opener takes the stale one and drops it. The new
        // device gets a watch of its own.
        state.watched = false;
        state.open(output);
        return false;
    }
    if !quiet.long_enough(sounded, now) {
        return true;
    }
    let closed = state.device.take();
    state.watched = false;
    drop(state);
    // Dropped unlocked, as stopping a stream takes a while.
    drop(closed);
    debug_log("audio: output: closing the output device after silence");
    false
}

/// Counts the silence the watch sees.
#[derive(Default)]
struct Quiet {
    since: Option<Instant>,
}

impl Quiet {
    /// Whether nothing has played for [`QUIET_FOR`], given whether anything
    /// did since the last look.
    fn long_enough(&mut self, heard: bool, now: Instant) -> bool {
        if heard {
            self.since = None;
            return false;
        }
        let since = *self.since.get_or_insert(now);
        now.saturating_duration_since(since) >= QUIET_FOR
    }
}

/// A player's sound that marks the output as playing each time it is
/// pulled. A paused player, or one with nothing queued, doesn't pull it.
struct Heard<S> {
    source: S,
    heard: Arc<AtomicBool>,
    /// Ends unplayed when first pulled after this.
    starts_by: Option<Instant>,
}

impl<S: Source> Iterator for Heard<S> {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.starts_by.take().is_some_and(|by| Instant::now() > by) {
            return None;
        }
        let sample = self.source.next();
        if sample.is_some() {
            self.heard.store(true, Ordering::Relaxed);
        }
        sample
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.source.size_hint()
    }
}

impl<S: Source> Source for Heard<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.source.current_span_len()
    }

    fn channels(&self) -> ChannelCount {
        self.source.channels()
    }

    fn sample_rate(&self) -> SampleRate {
        self.source.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.source.total_duration()
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.source.try_seek(pos)
    }
}

/// The app mixer's sound on a device, taken a block at a time. Endless:
/// silence when nothing plays.
struct Feed {
    sound: Arc<Mutex<MixerSource>>,
    pulled: Arc<AtomicBool>,
    channels: ChannelCount,
    rate: SampleRate,
    block: Vec<f32>,
    at: usize,
}

impl Feed {
    fn new(sound: Arc<Mutex<MixerSource>>, pulled: Arc<AtomicBool>) -> Self {
        let (channels, rate) = {
            let sound = sound.lock().unwrap_or_else(PoisonError::into_inner);
            (sound.channels(), sound.sample_rate())
        };
        let len = FEED_FRAMES * usize::from(channels.get());
        Self {
            sound,
            pulled,
            channels,
            rate,
            block: vec![0.0; len],
            at: len,
        }
    }
}

impl Iterator for Feed {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.at >= self.block.len() {
            let mut sound = self.sound.lock().unwrap_or_else(PoisonError::into_inner);
            for slot in &mut self.block {
                *slot = sound.next().unwrap_or(0.0);
            }
            self.at = 0;
            self.pulled.store(true, Ordering::Relaxed);
        }
        let sample = self.block.get(self.at).copied().unwrap_or(0.0);
        self.at += 1;
        Some(sample)
    }
}

impl Source for Feed {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        self.channels
    }

    fn sample_rate(&self) -> SampleRate {
        self.rate
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }

    fn try_seek(&mut self, _pos: Duration) -> Result<(), SeekError> {
        Err(SeekError::NotSupported {
            underlying_source: std::any::type_name::<Self>(),
        })
    }
}

/// rodio's own callback prints every error to stderr, xruns and reroutes
/// included. This one logs through Quill's debug log, rate-limited, and
/// marks the output stale when the stream ended.
fn output_error_callback(stale: Arc<AtomicBool>) -> impl FnMut(CpalError) + Send + Clone + 'static {
    let mut errors = StreamErrors::new("output");
    move |err| {
        if errors.check(err).is_some() {
            stale.store(true, Ordering::Relaxed);
        }
    }
}

/// `DeviceSinkBuilder::open_default_sink` with `on_error` in place of
/// rodio's printing callback: the default device first, then any other
/// output that is not the null device.
fn open_default_output<E>(on_error: E) -> Result<MixerDeviceSink, DeviceSinkError>
where
    E: FnMut(CpalError) + Send + Clone + 'static,
{
    DeviceSinkBuilder::from_default_device()
        .and_then(|builder| builder.with_error_callback(on_error.clone()).open_stream())
        .or_else(|first| {
            let devices = match rodio::cpal::default_host().output_devices() {
                Ok(devices) => devices,
                Err(err) => {
                    debug_log(&format!("audio: output: can't list output devices: {err}"));
                    return Err(first);
                }
            };
            devices
                .filter(|device| {
                    device
                        .description()
                        .is_ok_and(|desc| desc.driver().is_some_and(|driver| driver != "null"))
                })
                .find_map(|device| {
                    DeviceSinkBuilder::from_device(device)
                        .ok()?
                        .with_error_callback(on_error.clone())
                        .open_sink_or_fallback()
                        .ok()
                })
                .ok_or(first)
        })
}

/// Tests: a stand-in for the system's output device.
#[cfg(test)]
mod gate {
    use super::{Feed, OpenSink};
    use rodio::{ChannelCount, SampleRate};
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
    use std::thread::ThreadId;

    /// Each open waits, on the thread that opens it, for the test's answer;
    /// an opened device is a feed the test pulls by hand. The thread that
    /// made the gate plays the UI thread: opening a device there, or
    /// waiting there for one, fails the test.
    pub(in crate::ui) struct Gate {
        ui: ThreadId,
        state: Mutex<GateState>,
        changed: Condvar,
    }

    #[derive(Default)]
    struct GateState {
        /// Answers no open took yet: a device's channels and rate, or none.
        answers: VecDeque<Option<(u16, u32)>>,
        /// Opens asked for so far.
        asked: usize,
        /// The open device's side of the app mixer.
        feed: Option<Feed>,
        /// The open device's stale flag.
        stale: Option<Arc<AtomicBool>>,
        /// Dropping a device waits while set, as stopping a stream takes a
        /// while.
        drops_held: bool,
        /// Devices dropped, or being dropped.
        dropped: usize,
    }

    /// In a device the gate opened: tells the gate when it is dropped.
    pub(super) struct Dropped(pub(super) Arc<Gate>);

    impl Drop for Dropped {
        fn drop(&mut self) {
            let gate = &self.0;
            let mut state = gate.lock();
            state.dropped += 1;
            gate.changed.notify_all();
            while state.drops_held {
                state = gate
                    .changed
                    .wait(state)
                    .unwrap_or_else(PoisonError::into_inner);
            }
        }
    }

    impl Gate {
        pub(super) fn new() -> Self {
            Self {
                ui: std::thread::current().id(),
                state: Mutex::default(),
                changed: Condvar::new(),
            }
        }

        fn lock(&self) -> MutexGuard<'_, GateState> {
            self.state.lock().unwrap_or_else(PoisonError::into_inner)
        }

        pub(super) fn not_on_ui(&self, what: &str) {
            assert_ne!(
                std::thread::current().id(),
                self.ui,
                "{what} on the UI thread"
            );
        }

        pub(super) fn open(gate: &Arc<Self>) -> Option<OpenSink> {
            gate.not_on_ui("opened the device");
            let mut state = gate.lock();
            state.asked += 1;
            gate.changed.notify_all();
            loop {
                if let Some(answer) = state.answers.pop_front() {
                    let (channels, rate) = answer?;
                    let stale = Arc::new(AtomicBool::new(false));
                    state.stale = Some(stale.clone());
                    return Some(OpenSink {
                        sink: None,
                        channels: ChannelCount::new(channels).unwrap(),
                        rate: SampleRate::new(rate).unwrap(),
                        stale,
                        gate: Some(gate.clone()),
                    });
                }
                state = gate
                    .changed
                    .wait(state)
                    .unwrap_or_else(PoisonError::into_inner);
            }
        }

        pub(super) fn plays(&self, feed: Feed) {
            self.lock().feed = Some(feed);
        }

        /// Answer the next open: a 48 kHz stereo device, or none.
        pub(in crate::ui) fn answer(&self, opens: bool) {
            self.answer_with(opens.then_some((2, 48_000)));
        }

        /// Answer the next open with a device of `channels` at `rate`.
        pub(in crate::ui) fn answer_device(&self, channels: u16, rate: u32) {
            self.answer_with(Some((channels, rate)));
        }

        fn answer_with(&self, answer: Option<(u16, u32)>) {
            self.lock().answers.push_back(answer);
            self.changed.notify_all();
        }

        /// Opens asked for so far.
        pub(in crate::ui) fn asked(&self) -> usize {
            self.lock().asked
        }

        /// The open device's stream ends, as when the default output
        /// changes.
        pub(in crate::ui) fn go_stale(&self) {
            if let Some(stale) = &self.lock().stale {
                stale.store(true, Ordering::Relaxed);
            }
        }

        /// Make dropping a device wait (`true`) until let go (`false`).
        pub(in crate::ui) fn hold_drops(&self, held: bool) {
            self.lock().drops_held = held;
            self.changed.notify_all();
        }

        /// Wait until `count` devices are dropped, or being dropped.
        pub(in crate::ui) fn wait_dropped(&self, count: usize) {
            let mut state = self.lock();
            while state.dropped < count {
                state = self
                    .changed
                    .wait(state)
                    .unwrap_or_else(PoisonError::into_inner);
            }
        }

        /// The open device takes `samples` from the app mixer (none with
        /// no device).
        pub(in crate::ui) fn pull(&self, samples: usize) -> Vec<f32> {
            self.lock()
                .feed
                .as_mut()
                .map(|feed| feed.by_ref().take(samples).collect())
                .unwrap_or_default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Feed, Heard, QUIET_FOR, Quiet, SharedOutput, look};
    use rodio::buffer::SamplesBuffer;
    use rodio::mixer::mixer;
    use rodio::{ChannelCount, SampleRate, Source};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    fn mono(rate: u32) -> (ChannelCount, SampleRate) {
        (
            ChannelCount::new(1).unwrap(),
            SampleRate::new(rate).unwrap(),
        )
    }

    #[test]
    fn the_device_closes_after_half_a_second_of_silence() {
        let mut quiet = Quiet::default();
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        assert!(!quiet.long_enough(true, at(0)));
        // The first silent look starts the count.
        assert!(!quiet.long_enough(false, at(250)));
        assert!(!quiet.long_enough(false, at(500)));
        assert!(quiet.long_enough(false, at(250) + QUIET_FOR));
    }

    #[test]
    fn a_sound_in_between_restarts_the_count() {
        let mut quiet = Quiet::default();
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        assert!(!quiet.long_enough(false, at(0)));
        assert!(!quiet.long_enough(false, at(400)));
        assert!(!quiet.long_enough(true, at(600)));
        assert!(!quiet.long_enough(false, at(800)));
        assert!(!quiet.long_enough(false, at(1200)));
        assert!(quiet.long_enough(false, at(1300)));
    }

    #[test]
    fn a_new_player_keeps_its_sounds_rate_from_the_first_sample() {
        let (channels, rate) = mono(8000);
        let (mixer, mut device) = mixer(channels, rate);
        let player = SharedOutput::for_test(Some(mixer))
            .outlet()
            .unwrap()
            .player();
        let ramp: Vec<f32> = (0..1024u16).map(f32::from).collect();
        player.append(SamplesBuffer::new(channels, rate, ramp.clone()));
        let heard: Vec<f32> = device.by_ref().take(ramp.len()).collect();
        assert_eq!(heard, ramp);
    }

    #[test]
    fn a_new_player_keeps_its_sounds_channels_from_the_first_sample() {
        let channels = ChannelCount::new(2).unwrap();
        let rate = SampleRate::new(48_000).unwrap();
        let (mixer, mut device) = mixer(channels, rate);
        let player = SharedOutput::for_test(Some(mixer))
            .outlet()
            .unwrap()
            .player();
        let left_right: Vec<f32> = [1.0, -1.0].repeat(512);
        player.append(SamplesBuffer::new(channels, rate, left_right.clone()));
        let heard: Vec<f32> = device.by_ref().take(left_right.len()).collect();
        assert_eq!(heard, left_right);
    }

    /// Only a player that pulls its sound counts as playing: not one that
    /// is paused, nor one whose sound ran out.
    #[test]
    fn only_a_playing_player_marks_the_output() {
        let (channels, rate) = mono(8000);
        let (mixer, mut device) = mixer(channels, rate);
        let output = SharedOutput::for_test(Some(mixer));
        let outlet = output.outlet().unwrap();
        let heard = |device: &mut rodio::mixer::MixerSource| {
            output.0.heard.store(false, Ordering::Relaxed);
            for _ in 0..400 {
                device.next();
            }
            output.0.heard.load(Ordering::Relaxed)
        };

        let player = outlet.player();
        player.pause();
        player.append(SamplesBuffer::new(channels, rate, vec![0.0; 200]));
        assert!(!heard(&mut device), "paused");
        assert!(player.play());
        assert!(heard(&mut device), "playing, even a silent sound");
        assert!(!heard(&mut device), "ran out");
    }

    /// Playing, seeking and a new sound on a playing player need the
    /// audio thread, so each opens the device first; a sound queued on a
    /// paused player waits for its play.
    #[test]
    fn what_needs_the_device_opens_it() {
        let (channels, rate) = mono(8000);
        let (mixer, _device) = mixer(channels, rate);
        let output = SharedOutput::for_test(Some(mixer));
        let wakes = || output.0.wakes.load(Ordering::Relaxed);
        let player = output.outlet().unwrap().player();
        player.pause();
        player.append(SamplesBuffer::new(channels, rate, vec![0.5; 800]));
        assert_eq!(wakes(), 0);
        assert!(player.play());
        assert_eq!(wakes(), 1);
        player.append(SamplesBuffer::new(channels, rate, vec![0.5; 800]));
        assert_eq!(wakes(), 2);
    }

    /// With no device a play says so, and a seek fails at once instead of
    /// waiting for an audio thread that isn't there.
    #[test]
    fn without_a_device_play_and_seek_give_up_at_once() {
        let (channels, rate) = mono(8000);
        let (mixer, _device) = mixer(channels, rate);
        let mut player = SharedOutput::for_test(Some(mixer))
            .outlet()
            .unwrap()
            .player();
        player.pause();
        player.append(SamplesBuffer::new(channels, rate, vec![0.5; 800]));
        // The device went away after the player was made.
        player.output = SharedOutput::for_test(None);
        assert!(!player.play());
        assert!(!player.is_paused(), "a device that opens later plays it");
    }

    /// The first sound of the app's life is queued on the app mixer at
    /// once; the device opens on a thread of its own, then plays it.
    #[test]
    fn the_first_sound_plays_once_the_device_opens_off_the_ui_thread() {
        let (channels, rate) = mono(8000);
        let (output, gate) = SharedOutput::gated();
        let player = output.outlet().unwrap().player();
        player.append(SamplesBuffer::new(channels, rate, vec![0.5; 800]));
        assert!(player.play(), "the device is opening");
        assert!(gate.pull(100).is_empty(), "not up yet");
        gate.answer(true);
        output.settle_device();
        // 0.2 s of a 48 kHz stereo device, twice the sound.
        let played = gate.pull(19_200);
        assert!(played.iter().any(|s| *s > 0.4), "the sound plays");
        assert_eq!(gate.asked(), 1, "one open for the outlet, sound and play");
    }

    /// The app mixer runs at the first device's layout, so a sound at the
    /// device's rate reaches it sample for sample, with no conversion.
    #[test]
    fn the_app_mixer_takes_the_first_devices_layout() {
        let (channels, rate) = mono(44_100);
        let (output, gate) = SharedOutput::gated();
        let player = output.outlet().unwrap().player();
        let ramp: Vec<f32> = (0..441).map(|i| i as f32 / 1000.0).collect();
        player.append(SamplesBuffer::new(channels, rate, ramp.clone()));
        assert!(player.play());
        gate.answer_device(1, 44_100);
        output.settle_device();
        assert_eq!(gate.pull(ramp.len()), ramp);
    }

    /// A silent sound that says when it is dropped.
    struct Tracked(Arc<AtomicBool>);

    impl Iterator for Tracked {
        type Item = f32;

        fn next(&mut self) -> Option<f32> {
            Some(0.0)
        }
    }

    impl Source for Tracked {
        fn current_span_len(&self) -> Option<usize> {
            None
        }

        fn channels(&self) -> ChannelCount {
            mono(8000).0
        }

        fn sample_rate(&self) -> SampleRate {
            mono(8000).1
        }

        fn total_duration(&self) -> Option<Duration> {
            None
        }
    }

    impl Drop for Tracked {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Relaxed);
        }
    }

    /// A sound waiting for a device is dropped with its player at once,
    /// not kept for whenever one opens: a video's soundtrack lets go of
    /// the decoder, whose picture then runs on the wall clock.
    #[test]
    fn a_sound_waiting_for_a_device_goes_with_its_player() {
        let (output, _gate) = SharedOutput::gated();
        let dropped = Arc::new(AtomicBool::new(false));
        let player = output.outlet().unwrap().player();
        player.append(Tracked(dropped.clone()));
        assert!(!dropped.load(Ordering::Relaxed));
        drop(player);
        assert!(dropped.load(Ordering::Relaxed));
    }

    /// A stale device (the default output changed) is replaced with no
    /// moment where a playing player has no device up or opening: it would
    /// read as lost, and a voice note would stop with "no audio output
    /// device". The new device plays on.
    #[test]
    fn a_stale_device_is_replaced_without_losing_its_players() {
        let (channels, rate) = mono(8000);
        let (output, gate) = SharedOutput::gated();
        let player = output.outlet().unwrap().player();
        player.append(SamplesBuffer::new(channels, rate, vec![0.5; 8000]));
        gate.answer(true);
        output.settle_device();
        gate.go_stale();
        gate.hold_drops(true);
        // The watch's look, on a thread of its own as in the app.
        let watched = output.0.clone();
        std::thread::spawn(move || look(&watched, &mut Quiet::default(), Instant::now()));
        // The stale stream is being stopped.
        gate.wait_dropped(1);
        assert!(!player.lost());
        gate.hold_drops(false);
        gate.answer(true);
        output.settle_device();
        assert!(!player.lost());
        assert!(gate.pull(9600).iter().any(|s| *s > 0.4), "plays on");
    }

    /// A playing player whose device failed to open says so, so its owner
    /// can stop waiting for it; a paused one waits for its play.
    #[test]
    fn a_player_whose_device_failed_to_open_is_lost() {
        let (channels, rate) = mono(8000);
        let (output, gate) = SharedOutput::gated();
        let outlet = output.outlet().unwrap();
        let playing = outlet.player();
        playing.append(SamplesBuffer::new(channels, rate, vec![0.5; 800]));
        let paused = outlet.player();
        paused.pause();
        paused.append(SamplesBuffer::new(channels, rate, vec![0.5; 800]));
        assert!(!playing.lost(), "its device is opening");
        gate.answer(false);
        output.settle_device();
        assert!(playing.lost());
        assert!(!paused.lost());
    }

    #[test]
    fn a_marked_sound_still_seeks_and_reports_its_layout() {
        let (channels, rate) = mono(8000);
        let mut heard = Heard {
            source: SamplesBuffer::new(channels, rate, vec![0.5; 8000]),
            heard: Arc::new(AtomicBool::new(false)),
            starts_by: None,
        };
        assert_eq!(heard.channels(), channels);
        assert_eq!(heard.sample_rate(), rate);
        assert_eq!(heard.total_duration(), Some(Duration::from_secs(1)));
        heard.try_seek(Duration::from_millis(500)).unwrap();
        assert_eq!(heard.by_ref().count(), 4000);
    }

    /// An alert that couldn't start in time is dropped unplayed; one that
    /// starts in time plays in full.
    #[test]
    fn a_late_alert_is_dropped() {
        let (channels, rate) = mono(8000);
        let alert = |starts_by| Heard {
            source: SamplesBuffer::new(channels, rate, vec![0.5; 800]),
            heard: Arc::new(AtomicBool::new(false)),
            starts_by: Some(starts_by),
        };
        let now = Instant::now();
        assert_eq!(
            alert(now.checked_sub(Duration::from_millis(1)).unwrap()).count(),
            0
        );
        assert_eq!(alert(now + Duration::from_secs(3600)).count(), 800);
    }

    /// The device side plays what the app mixer has and silence after it,
    /// without ever ending.
    #[test]
    fn the_feed_plays_the_app_mixer_then_silence() {
        let (channels, rate) = mono(8000);
        let (mixer, sound) = mixer(channels, rate);
        let pulled = Arc::new(AtomicBool::new(false));
        let mut feed = Feed::new(Arc::new(Mutex::new(sound)), pulled.clone());
        assert_eq!(feed.channels(), channels);
        assert_eq!(feed.sample_rate(), rate);
        mixer.add(SamplesBuffer::new(channels, rate, vec![0.25; 300]));
        let played: Vec<f32> = feed.by_ref().take(1000).collect();
        assert_eq!(played.iter().filter(|s| **s == 0.25).count(), 300);
        assert!(played.iter().skip(300).all(|s| *s == 0.0));
        assert!(feed.next().is_some());
        assert!(pulled.load(Ordering::Relaxed), "the device asked");
    }

    #[test]
    fn a_hold_counts_as_sound_until_dropped() {
        let output = SharedOutput::default();
        let look = || {
            output.0.pulled.store(true, Ordering::Relaxed);
            output.0.sounded()
        };
        assert!(!look());
        let hold = output.hold();
        assert!(look());
        assert!(look(), "every look while held");
        drop(hold);
        assert!(!look());
    }

    /// A device that stopped asking for sound (stalled) is not closed for
    /// its silence: a player may be waiting on it.
    #[test]
    fn a_stalled_device_is_not_closed_for_its_silence() {
        let output = SharedOutput::default();
        output.0.pulled.store(true, Ordering::Relaxed);
        assert!(!output.0.sounded(), "asked for sound, heard none");
        assert!(output.0.sounded(), "didn't ask since");
    }
}
