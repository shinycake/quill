//! The app's one audio output device, open only while something plays.
//!
//! Every player connects to one app mixer that never goes away. A device
//! plays it while any player's sound is being pulled ([`Heard`]), and is
//! closed once nothing has played for [`QUIET_FOR`] (tdesktop detaches its
//! device after 500 ms of silence, `kDetachDeviceTimeout`). An open output
//! keeps the audio hardware running: on macOS coreaudiod holds an assertion
//! that keeps the Mac from idle sleep, and the device's threads and buffers
//! stay alive. Whatever needs the device again (a new sound, play, a seek)
//! goes through [`OutputPlayer`], which opens it first. See
//! `docs/decisions/codex-audio-in-process.md`.
#![deny(clippy::indexing_slicing)]

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use quill::advanced::debug_mode::debug_log;
use quill::cpal_stream::StreamErrors;
use rodio::cpal::traits::{DeviceTrait, HostTrait};
use rodio::mixer::{Mixer, MixerSource};
use rodio::source::SeekError;
use rodio::{
    ChannelCount, CpalError, DeviceSinkBuilder, DeviceSinkError, MixerDeviceSink, Player,
    SampleRate, Source,
};

/// How long nothing may play before the device is closed.
const QUIET_FOR: Duration = Duration::from_millis(500);
/// How often an open device is checked for silence.
const LOOK_EVERY: Duration = Duration::from_millis(250);
/// How long after a device failed to open the next sound tries again.
const RETRY_AFTER: Duration = Duration::from_secs(5);
/// Frames the device takes from the app mixer at a time (about 5 ms).
const FEED_FRAMES: usize = 256;

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
    /// Signalled when a device opening off the UI thread is up or failed.
    opened: Condvar,
    /// Set by every sound a player pulls, cleared by the watch.
    heard: Arc<AtomicBool>,
    /// Set each time the device asks for sound ([`Feed`]), cleared by the
    /// watch.
    pulled: Arc<AtomicBool>,
    /// Open [`Hold`]s.
    holds: AtomicUsize,
    /// Tests: stands in for the device (`Some(None)` is "no device").
    #[cfg(test)]
    forced: Option<Option<Mixer>>,
    /// Tests: how many times a player asked for the device.
    #[cfg(test)]
    wakes: AtomicUsize,
}

#[derive(Default)]
struct OutputState {
    /// Made with the first device's layout and kept for the app's life.
    app: Option<AppMixer>,
    device: Option<Device>,
    failed_at: Option<Instant>,
    /// A watch thread looks after the open device.
    watched: bool,
    /// A device is opening off the UI thread ([`reopen`]).
    opening: bool,
}

/// When a closed device has to be open.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Open {
    /// Before returning: the caller needs the audio thread (a play, a seek)
    /// or has to know there is no device.
    Now,
    /// Off the calling thread, for a sound nothing waits on: it is queued
    /// on the app mixer and plays once the device is up. Opening takes 10
    /// to 40 ms on macOS, two frames on the UI thread.
    Soon,
}

struct AppMixer {
    mixer: Mixer,
    /// Pulled by one device at a time ([`Feed`]).
    sound: Arc<Mutex<MixerSource>>,
}

struct Device {
    _sink: MixerDeviceSink,
    /// Set from cpal's audio thread when the stream ended.
    stale: Arc<AtomicBool>,
}

/// Where players connect: the app mixer.
#[derive(Clone)]
pub(in crate::ui) struct Outlet {
    mixer: Mixer,
    output: SharedOutput,
}

impl Outlet {
    pub(in crate::ui) fn player(&self) -> OutputPlayer {
        OutputPlayer {
            player: Player::connect_new(&self.mixer),
            output: self.output.clone(),
        }
    }
}

/// A player on the app mixer. Whatever needs the audio thread opens the
/// device first: with it closed nothing pulls the sound, so a play would
/// stall and a seek would wait for an answer forever.
pub(in crate::ui) struct OutputPlayer {
    player: Player,
    output: SharedOutput,
}

impl OutputPlayer {
    /// Queue `source`, marked so the output stays open while it plays.
    pub(in crate::ui) fn append<S>(&self, source: S)
    where
        S: Source + Send + 'static,
    {
        if !self.player.is_paused() {
            self.output.wake(Open::Soon);
        }
        self.player.append(Heard {
            source,
            heard: self.output.0.heard.clone(),
        });
    }

    /// False when there is no device now; unpaused either way, so a device
    /// that opens later plays it.
    pub(in crate::ui) fn play(&self) -> bool {
        let woke = self.output.wake(Open::Now);
        self.player.play();
        woke
    }

    /// Waits for the audio thread to take the seek; fails at once when
    /// there is no device.
    pub(in crate::ui) fn try_seek(&self, pos: Duration) -> Result<(), SeekError> {
        if !self.output.wake(Open::Now) {
            return Err(SeekError::Other(Arc::new(std::io::Error::other(
                "no audio output device",
            ))));
        }
        self.player.try_seek(pos)
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
    /// Tests: an output that hands out `mixer` (or nothing) instead of
    /// opening a device.
    #[cfg(test)]
    pub(in crate::ui) fn for_test(mixer: Option<Mixer>) -> Self {
        Self(Arc::new(Output {
            forced: Some(mixer),
            ..Output::default()
        }))
    }

    /// The app mixer, with the device open to play it. Every new sound
    /// connects here.
    pub(in crate::ui) fn outlet(&self) -> Option<Outlet> {
        self.outlet_when(Open::Now)
    }

    /// [`Self::outlet`] for a sound nothing waits on (a notification, a
    /// call tone): a closed device opens off the calling thread. `None`
    /// only when no device opened lately.
    pub(in crate::ui) fn outlet_soon(&self) -> Option<Outlet> {
        self.outlet_when(Open::Soon)
    }

    fn outlet_when(&self, when: Open) -> Option<Outlet> {
        #[cfg(test)]
        if let Some(forced) = &self.0.forced {
            return forced.clone().map(|mixer| Outlet {
                mixer,
                output: self.clone(),
            });
        }
        let mut state = self.0.settled(when);
        if !state.open(&self.0, when) {
            return None;
        }
        let app = state.app.as_ref()?;
        Some(Outlet {
            mixer: app.mixer.clone(),
            output: self.clone(),
        })
    }

    /// Tests: how many times a player asked for the device.
    #[cfg(test)]
    pub(in crate::ui) fn wakes(&self) -> usize {
        self.0.wakes.load(Ordering::Relaxed)
    }

    /// Keep the device open, once a sound opened it, until the hold drops.
    pub(in crate::ui) fn hold(&self) -> Hold {
        self.0.holds.fetch_add(1, Ordering::Relaxed);
        Hold(self.0.clone())
    }

    /// Open the device for a player about to need it. False when there is
    /// none.
    fn wake(&self, when: Open) -> bool {
        #[cfg(test)]
        if let Some(forced) = &self.0.forced {
            self.0.wakes.fetch_add(1, Ordering::Relaxed);
            return forced.is_some();
        }
        self.0.settled(when).open(&self.0, when)
    }
}

impl Output {
    fn lock(&self) -> MutexGuard<'_, OutputState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The state, after a device opening off the UI thread is up or failed
    /// when the caller needs the device [`Open::Now`].
    fn settled(&self, when: Open) -> MutexGuard<'_, OutputState> {
        let mut state = self.lock();
        while when == Open::Now && state.opening {
            state = self
                .opened
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
        state
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
    /// Make sure a live device plays the app mixer, and that a watch closes
    /// it once nothing plays. Counts as sound, so the watch gives a player
    /// about to start [`QUIET_FOR`] to do so. The first device always opens
    /// here: its layout makes the app mixer.
    fn open(&mut self, output: &Arc<Output>, when: Open) -> bool {
        output.heard.store(true, Ordering::Relaxed);
        if self.device.as_ref().is_some_and(Device::is_stale) {
            debug_log("audio: output: reopening the default output device");
            self.device = None;
            self.failed_at = None;
        }
        if self.device.is_some() || self.opening {
            return true;
        }
        if self.failed_at.is_some_and(|at| at.elapsed() < RETRY_AFTER) {
            return false;
        }
        if let (Some(app), Open::Soon) = (&self.app, when) {
            let (sound, opener) = (app.sound.clone(), output.clone());
            let spawned = quill::thread_guard::spawn_guarded("quill-audio-open", move || {
                reopen(&opener, &sound);
            });
            if spawned.is_ok() {
                self.opening = true;
                return true;
            }
        }
        let Some(sink) = open_sink() else {
            self.failed_at = Some(Instant::now());
            return false;
        };
        let app = self.app.get_or_insert_with(|| AppMixer::for_device(&sink));
        let device = sink.play(&app.sound, &output.pulled);
        self.install(device, output);
        true
    }

    /// Play on `device`, with a watch to close it.
    fn install(&mut self, device: Device, output: &Arc<Output>) {
        self.device = Some(device);
        self.failed_at = None;
        if !self.watched {
            let watched = output.clone();
            let spawned = std::thread::Builder::new()
                .name("quill-audio-idle".into())
                .spawn(move || watch(&watched));
            self.watched = spawned.is_ok();
        }
    }
}

/// Open the default device for the app mixer's `sound`, off the UI thread,
/// without holding the output's lock while the device comes up.
fn reopen(output: &Arc<Output>, sound: &Arc<Mutex<MixerSource>>) {
    let mut reopened = Reopened {
        output,
        device: None,
    };
    reopened.device = open_sink().map(|sink| sink.play(sound, &output.pulled));
}

/// Ends an open off the UI thread however it went, a panic included, so a
/// play waiting for it doesn't wait for ever.
struct Reopened<'a> {
    output: &'a Arc<Output>,
    device: Option<Device>,
}

impl Drop for Reopened<'_> {
    fn drop(&mut self) {
        let mut state = self.output.lock();
        state.opening = false;
        match self.device.take() {
            Some(device) => state.install(device, self.output),
            None => state.failed_at = Some(Instant::now()),
        }
        drop(state);
        self.output.opened.notify_all();
    }
}

impl AppMixer {
    /// A mixer with `sink`'s channels and rate.
    fn for_device(sink: &OpenSink) -> Self {
        let config = sink.sink.config();
        let (mixer, sound) = rodio::mixer::mixer(config.channel_count(), config.sample_rate());
        Self {
            mixer,
            sound: Arc::new(Mutex::new(sound)),
        }
    }
}

/// A device just opened, not yet playing the app mixer.
struct OpenSink {
    sink: MixerDeviceSink,
    stale: Arc<AtomicBool>,
}

/// Open the default output device.
fn open_sink() -> Option<OpenSink> {
    let stale = Arc::new(AtomicBool::new(false));
    let started = Instant::now();
    let mut sink = open_default_output(output_error_callback(stale.clone())).ok()?;
    debug_log(&format!(
        "audio: output: opened the output device in {} ms",
        started.elapsed().as_millis()
    ));
    sink.log_on_drop(false);
    Some(OpenSink { sink, stale })
}

impl OpenSink {
    fn play(self, sound: &Arc<Mutex<MixerSource>>, pulled: &Arc<AtomicBool>) -> Device {
        self.sink
            .mixer()
            .add(Feed::new(sound.clone(), pulled.clone()));
        Device {
            _sink: self.sink,
            stale: self.stale,
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
        let mut state = output.lock();
        let sounded = output.sounded();
        let Some(device) = &state.device else {
            state.watched = false;
            return;
        };
        let stale = device.is_stale();
        if !stale && !quiet.long_enough(sounded, Instant::now()) {
            continue;
        }
        // Dropped after the lock: stopping a device takes a while.
        let closed = state.device.take();
        state.watched = false;
        if stale {
            debug_log("audio: output: reopening the default output device");
            // The new device gets a watch of its own.
            state.open(output, Open::Soon);
        } else {
            debug_log("audio: output: closing the output device after silence");
        }
        drop(state);
        drop(closed);
        return;
    }
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
}

impl<S: Source> Iterator for Heard<S> {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
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

#[cfg(test)]
mod tests {
    use super::{Feed, Heard, QUIET_FOR, Quiet, SharedOutput};
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
        assert!(player.try_seek(Duration::from_millis(50)).is_err());
    }

    #[test]
    fn a_marked_sound_still_seeks_and_reports_its_layout() {
        let (channels, rate) = mono(8000);
        let mut heard = Heard {
            source: SamplesBuffer::new(channels, rate, vec![0.5; 8000]),
            heard: Arc::new(AtomicBool::new(false)),
        };
        assert_eq!(heard.channels(), channels);
        assert_eq!(heard.sample_rate(), rate);
        assert_eq!(heard.total_duration(), Some(Duration::from_secs(1)));
        heard.try_seek(Duration::from_millis(500)).unwrap();
        assert_eq!(heard.by_ref().count(), 4000);
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
