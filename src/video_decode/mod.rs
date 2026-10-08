//! In-process video playback for Linux and Windows (and, opt-in, macOS
//! development builds): FFmpeg demuxes and decodes on a background thread
//! through the runtime-loaded `quillvideo` shim ([`ffi`]); this module
//! queues the pictures and sound, keeps time and picks the picture to show.
//! Telegram Desktop decodes with FFmpeg on every platform the same way
//! (`media/streaming/media_streaming_*.cpp`). See
//! `docs/decisions/codex-video-cross-platform.md`.
//!
//! - One [`Player`] per clip, one decode thread per player. The thread stays
//!   a few pictures ([`OpenOptions::video_frames`]) and up to
//!   [`AUDIO_HIGH_SECS`] of sound ahead and sleeps otherwise, so memory is
//!   bounded and a paused clip costs nothing.
//! - Pictures come out as BGRA (GPUI's image format), already scaled into
//!   the caller's box and rotated upright.
//! - Time: with sound, the sound card is the master clock (the samples it
//!   took, [`AudioTap`]); without, a wall clock that starts when the first
//!   picture is shown ([`Clock`]). The picture shown is the newest one due.

mod ffi;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Sound the decoder keeps buffered at least (while the picture queue is
/// full it still decodes until this much sound is ready).
pub const AUDIO_LOW_SECS: f64 = 0.5;
/// Sound the decoder never buffers beyond.
pub const AUDIO_HIGH_SECS: f64 = 3.0;
/// The audio clock (samples pulled) runs ahead of what is heard by the
/// output's buffers: the tempo stretcher's block and read-ahead (~50 ms)
/// plus the device period (10-30 ms). Pictures follow what is heard.
pub const AUDIO_OUTPUT_LATENCY: f64 = 0.07;
/// Pictures pace on a smooth wall clock that is pulled back to the sound
/// only when they drift further apart than this (the sound is pulled in
/// bursts of ~20 ms, too coarse to pace pictures directly). Well inside the
/// ±45 ms at which lip sync errors become noticeable.
pub const SYNC_TOLERANCE: f64 = 0.04;
/// A picture is due this much before its time (half a 60 Hz refresh), so
/// it isn't held a whole refresh for rounding.
const DUE_SLACK: f64 = 0.008;
/// The decode thread re-checks for room at least this often (sound is
/// drained on the audio thread, which doesn't wake it).
const IDLE_POLL: Duration = Duration::from_millis(40);

/// Whether the in-process decoder can run here: the `quillvideo` shim and
/// its FFmpeg libraries are installed.
pub fn available() -> bool {
    ffi::availability().is_ok()
}

/// Why [`available`] is false.
pub fn unavailable_reason() -> Option<&'static str> {
    ffi::availability().err()
}

/// The shim's file name on this platform (packaging, diagnostics).
pub fn library_name() -> &'static str {
    ffi::library_name()
}

/// How a clip is opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenOptions {
    /// Pictures are scaled down (never up) to fit this box; 0 = unbounded.
    pub max_width: u32,
    pub max_height: u32,
    /// Decoder threads (0: FFmpeg decides).
    pub threads: u32,
    /// Decode the soundtrack.
    pub want_audio: bool,
    /// Pictures decoded ahead (at least 1).
    pub video_frames: usize,
}

impl OpenOptions {
    /// A muted inline loop in the history, drawn at most `max_edge` px on
    /// its longer side: silent, one decoder thread, a short queue.
    pub fn inline(max_edge: u32) -> Self {
        Self {
            max_width: max_edge,
            max_height: max_edge,
            threads: 1,
            want_audio: false,
            video_frames: 3,
        }
    }

    /// The media viewer: up to 1080p-ish pictures, with sound.
    pub fn viewer() -> Self {
        Self {
            max_width: 1920,
            max_height: 1920,
            threads: 0,
            want_audio: true,
            video_frames: 4,
        }
    }

    /// Bytes the picture queue may hold at most.
    pub fn video_budget_bytes(&self) -> usize {
        let edge = |v: u32| if v == 0 { 4096 } else { v as usize };
        edge(self.max_width) * edge(self.max_height) * 4 * self.video_frames.max(1)
    }
}

/// What the file holds, once opened.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MediaInfo {
    pub duration: Option<f64>,
    pub frame_rate: f64,
    /// Upright output size (after scaling and rotation).
    pub width: u32,
    pub height: u32,
    /// Clockwise degrees the pictures were turned by.
    pub rotation: u32,
    pub has_video: bool,
    pub has_audio: bool,
    pub sample_rate: u32,
    pub channels: u16,
    pub video_codec: String,
    pub audio_codec: String,
}

/// One decoded picture, upright BGRA, tightly packed.
#[derive(Clone, Debug, PartialEq)]
pub struct VideoFrame {
    pub pts: f64,
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

/// Decoded sound: interleaved f32 at the clip's rate and channel count.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioChunk {
    pub pts: f64,
    pub samples: Vec<f32>,
}

/// What a demuxer produces next.
#[derive(Debug)]
pub enum Item {
    Video(VideoFrame),
    Audio(AudioChunk),
}

/// A source of decoded items: the FFmpeg shim, or a scripted one in tests.
pub trait Demuxer: Send {
    fn info(&self) -> MediaInfo;
    /// The next item, `None` at the end.
    fn next(&mut self) -> Result<Option<Item>, String>;
    /// Continue from `secs` (items before it are not returned).
    fn seek(&mut self, secs: f64) -> Result<(), String>;
}

/// Turn a BGRA picture (`stride` bytes per row) upright by `rotation`
/// clockwise degrees, packing rows tightly. Returns the pixels and size.
pub fn rotate_bgra(
    src: &[u8],
    width: u32,
    height: u32,
    stride: usize,
    rotation: u32,
) -> (Vec<u8>, u32, u32) {
    let (w, h) = (width as usize, height as usize);
    let px = |x: usize, y: usize| {
        let at = y * stride + x * 4;
        [src[at], src[at + 1], src[at + 2], src[at + 3]]
    };
    match rotation {
        90 | 180 | 270 => {
            let (ow, oh) = if rotation == 180 { (w, h) } else { (h, w) };
            let mut out = vec![0u8; ow * oh * 4];
            for oy in 0..oh {
                for ox in 0..ow {
                    let (sx, sy) = match rotation {
                        // Clockwise: the left column becomes the top row.
                        90 => (oy, h - 1 - ox),
                        180 => (w - 1 - ox, h - 1 - oy),
                        _ => (w - 1 - oy, ox),
                    };
                    let at = (oy * ow + ox) * 4;
                    out[at..at + 4].copy_from_slice(&px(sx, sy));
                }
            }
            (out, ow as u32, oh as u32)
        }
        _ => {
            let row = w * 4;
            let mut out = Vec::with_capacity(row * h);
            for y in 0..h {
                out.extend_from_slice(&src[y * stride..y * stride + row]);
            }
            (out, width, height)
        }
    }
}

/// A wall clock that can be paused, seeked and sped up. It holds still
/// (`held`) after a seek until the first picture is shown, so decoding the
/// first frame doesn't eat into playback.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clock {
    base: f64,
    anchor: Option<Instant>,
    rate: f64,
    playing: bool,
    held: bool,
}

impl Clock {
    pub fn new() -> Self {
        Self {
            base: 0.0,
            anchor: None,
            rate: 1.0,
            playing: false,
            held: true,
        }
    }

    pub fn position(&self, now: Instant) -> f64 {
        match self.anchor {
            Some(anchor) if self.playing && !self.held => {
                self.base + now.saturating_duration_since(anchor).as_secs_f64() * self.rate
            }
            _ => self.base,
        }
    }

    fn rebase(&mut self, now: Instant) {
        self.base = self.position(now);
        self.anchor = Some(now);
    }

    pub fn play(&mut self, now: Instant) {
        self.rebase(now);
        self.playing = true;
    }

    pub fn pause(&mut self, now: Instant) {
        self.rebase(now);
        self.playing = false;
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Jump to `secs` and hold there until [`Clock::release`].
    pub fn seek(&mut self, secs: f64, now: Instant) {
        self.base = secs.max(0.0);
        self.anchor = Some(now);
        self.held = true;
    }

    /// Follow an external clock (the sound): jump without holding.
    pub fn follow(&mut self, secs: f64, now: Instant) {
        self.base = secs.max(0.0);
        self.anchor = Some(now);
        self.held = false;
    }

    /// The first picture after a seek is up: time starts running.
    pub fn release(&mut self, now: Instant) {
        if self.held {
            self.anchor = Some(now);
            self.held = false;
        }
    }

    pub fn set_rate(&mut self, rate: f64, now: Instant) {
        self.rebase(now);
        self.rate = if rate.is_finite() && rate > 0.0 {
            rate
        } else {
            1.0
        };
    }
}

impl Default for Clock {
    fn default() -> Self {
        Self::new()
    }
}

/// Pop every picture due at `position` from the front of `queue` and
/// return the newest of them (older due ones are dropped: they're late).
/// With `first`, the front picture is taken even if it isn't due yet.
pub fn take_due(
    queue: &mut VecDeque<VideoFrame>,
    position: f64,
    first: bool,
) -> Option<VideoFrame> {
    let mut picked = if first { queue.pop_front() } else { None };
    while queue
        .front()
        .is_some_and(|frame| frame.pts <= position + DUE_SLACK)
    {
        picked = queue.pop_front();
    }
    picked
}

/// The position pictures pace on: the smooth wall clock, unless what is
/// heard drifted more than [`SYNC_TOLERANCE`] from it (then the sound).
pub fn synced_position(wall: f64, heard: Option<f64>) -> f64 {
    match heard {
        Some(heard) if (wall - heard).abs() > SYNC_TOLERANCE => heard,
        _ => wall,
    }
}

/// Whether the decode thread should decode more now: while pictures are
/// short, or sound is below its low mark; never past the sound's cap.
pub fn wants_more(
    video_queued: Option<(usize, usize)>,
    audio_buffered_secs: Option<f64>,
) -> bool {
    if audio_buffered_secs.is_some_and(|secs| secs >= AUDIO_HIGH_SECS) {
        return false;
    }
    let video_short = video_queued.is_some_and(|(queued, cap)| queued < cap.max(1));
    let audio_low = audio_buffered_secs.is_some_and(|secs| secs < AUDIO_LOW_SECS);
    video_short || audio_low
}

/// The sound shared between the decode thread and the audio output.
struct AudioShared {
    rate: u32,
    channels: u16,
    queue: Mutex<AudioQueue>,
    /// Bumped by every seek: the output drops what it held.
    generation: AtomicU64,
    /// Seqlock-style: the position of the next sample the output will
    /// play (f64 bits), tagged with the generation it belongs to.
    played: AtomicU64,
    played_generation: AtomicU64,
    /// The output let go of the tap: sound is decoded for nobody.
    detached: AtomicBool,
}

#[derive(Default)]
struct AudioQueue {
    chunks: VecDeque<AudioChunk>,
    /// Interleaved samples queued.
    buffered: usize,
}

impl AudioShared {
    fn buffered_secs(&self) -> f64 {
        let buffered = lock(&self.queue).buffered;
        buffered as f64 / f64::from(self.channels.max(1)) / f64::from(self.rate.max(1))
    }

    fn push(&self, chunk: AudioChunk) {
        if chunk.samples.is_empty() || self.detached.load(Ordering::Relaxed) {
            return;
        }
        let mut queue = lock(&self.queue);
        queue.buffered += chunk.samples.len();
        queue.chunks.push_back(chunk);
    }

    fn reset(&self, secs: f64) {
        let mut queue = lock(&self.queue);
        queue.chunks.clear();
        queue.buffered = 0;
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.played.store(secs.to_bits(), Ordering::Release);
        self.played_generation.store(generation, Ordering::Release);
    }

    fn drained(&self) -> bool {
        lock(&self.queue).chunks.is_empty()
    }

    /// Where the output is, if it has played anything since the last seek.
    fn played(&self) -> Option<f64> {
        let generation = self.generation.load(Ordering::Acquire);
        let before = self.played_generation.load(Ordering::Acquire);
        let value = f64::from_bits(self.played.load(Ordering::Acquire));
        let after = self.played_generation.load(Ordering::Acquire);
        (before == generation && after == generation).then_some(value)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The clip's sound as a stream of samples for an audio output (the UI
/// wraps it in a rodio source). Pulling a sample advances the audio clock;
/// when the decoder is behind it yields silence without advancing.
/// Dropping it detaches the sound (the player falls back to a wall clock).
pub struct AudioTap {
    shared: Arc<AudioShared>,
    wake: Arc<Shared>,
    chunk: Vec<f32>,
    at: usize,
    chunk_pts: f64,
    generation: u64,
    /// Whether the last sample was real sound (not underrun silence).
    sounding: bool,
}

impl AudioTap {
    pub fn channels(&self) -> u16 {
        self.shared.channels.max(1)
    }

    pub fn sample_rate(&self) -> u32 {
        self.shared.rate.max(1)
    }

    /// Whether the last sample pulled was decoded sound.
    pub fn sounding(&self) -> bool {
        self.sounding
    }

    /// The next interleaved sample (silence while starved).
    pub fn next_sample(&mut self) -> f32 {
        let generation = self.shared.generation.load(Ordering::Acquire);
        if generation != self.generation {
            self.generation = generation;
            self.chunk.clear();
            self.at = 0;
        }
        if self.at >= self.chunk.len() {
            let next = {
                let mut queue = lock(&self.shared.queue);
                // A seek between the check above and here: start over.
                if self.shared.generation.load(Ordering::Acquire) != self.generation {
                    None
                } else {
                    let chunk = queue.chunks.pop_front();
                    if let Some(chunk) = &chunk {
                        queue.buffered = queue.buffered.saturating_sub(chunk.samples.len());
                    }
                    chunk
                }
            };
            match next {
                Some(chunk) => {
                    self.chunk_pts = chunk.pts;
                    self.chunk = chunk.samples;
                    self.at = 0;
                    self.wake.wake.notify_one();
                }
                None => {
                    self.sounding = false;
                    return 0.0;
                }
            }
        }
        let sample = self.chunk[self.at];
        self.at += 1;
        let channels = usize::from(self.channels());
        if self.at % channels == 0 {
            let played = self.chunk_pts
                + (self.at / channels) as f64 / f64::from(self.sample_rate());
            self.shared.played.store(played.to_bits(), Ordering::Release);
            self.shared
                .played_generation
                .store(self.generation, Ordering::Release);
        }
        self.sounding = true;
        sample
    }
}

impl Drop for AudioTap {
    fn drop(&mut self) {
        self.shared.detached.store(true, Ordering::Relaxed);
        lock(&self.shared.queue).chunks.clear();
        self.wake.wake.notify_one();
    }
}

/// State shared with the decode thread.
struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}

#[derive(Default)]
struct State {
    info: Option<MediaInfo>,
    /// The sound, once the file is open and has some (and it was asked for).
    audio: Option<Arc<AudioShared>>,
    /// Its output end, until the player hands it out.
    tap: Option<AudioTap>,
    video: VecDeque<VideoFrame>,
    video_cap: usize,
    seek: Option<f64>,
    /// Bumped by every seek: items decoded for an older one are dropped.
    epoch: u64,
    eof: bool,
    error: Option<String>,
    stop: bool,
}

/// One clip: a decode thread plus the clock and the picture queue.
pub struct Player {
    shared: Arc<Shared>,
    clock: Clock,
    /// No picture shown since the last seek (or open).
    first: bool,
    /// The newest picture handed out, for the end position.
    last_pts: Option<f64>,
    ended: bool,
}

impl Player {
    /// Open `path` with the FFmpeg shim on a new decode thread. Returns at
    /// once; the file opens on that thread ([`Player::info`] and
    /// [`Player::take_audio_tap`] become available then, or
    /// [`Player::error`]).
    pub fn open(path: &Path, options: OpenOptions) -> Result<Self, String> {
        ffi::availability().map_err(str::to_string)?;
        let path: PathBuf = path.to_path_buf();
        let opts = options.clone();
        Self::spawn(
            move || ffi::FfiDemuxer::open(&path, &opts).map(|d| Box::new(d) as Box<dyn Demuxer>),
            options,
        )
    }

    /// Run a player over any demuxer (tests use scripted ones).
    pub fn with_demuxer(demuxer: Box<dyn Demuxer>, options: OpenOptions) -> Self {
        Self::spawn(move || Ok(demuxer), options).expect("spawn the decode thread")
    }

    fn spawn(
        opener: impl FnOnce() -> Result<Box<dyn Demuxer>, String> + Send + 'static,
        options: OpenOptions,
    ) -> Result<Self, String> {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                video_cap: options.video_frames.max(1),
                ..State::default()
            }),
            wake: Condvar::new(),
        });
        let thread_shared = shared.clone();
        let want_audio = options.want_audio;
        std::thread::Builder::new()
            .name("quill-video".into())
            .spawn(move || match opener() {
                Ok(demuxer) => decode_loop(demuxer, thread_shared, want_audio),
                Err(err) => {
                    let mut state = lock(&thread_shared.state);
                    state.error = Some(err);
                    state.eof = true;
                }
            })
            .map_err(|err| err.to_string())?;
        Ok(Self {
            shared,
            clock: Clock::new(),
            first: true,
            last_pts: None,
            ended: false,
        })
    }

    /// What the file holds, once the decode thread has opened it.
    pub fn info(&self) -> Option<MediaInfo> {
        lock(&self.shared.state).info.clone()
    }

    pub fn duration(&self) -> Option<f64> {
        self.info().and_then(|info| info.duration)
    }

    /// Why the clip can't play (open or decode failure).
    pub fn error(&self) -> Option<String> {
        lock(&self.shared.state).error.clone()
    }

    /// The clip's sound for an audio output, once (when it was asked for
    /// and the file has some). Whoever takes it must keep it pulled or drop
    /// it: undrained sound stalls decoding at [`AUDIO_HIGH_SECS`].
    pub fn take_audio_tap(&mut self) -> Option<AudioTap> {
        lock(&self.shared.state).tap.take()
    }

    fn audio(&self) -> Option<Arc<AudioShared>> {
        lock(&self.shared.state)
            .audio
            .clone()
            .filter(|audio| !audio.detached.load(Ordering::Relaxed))
    }

    /// Whether the soundtrack is playing out (it is the clock).
    pub fn has_sound(&self) -> bool {
        self.audio().is_some()
    }

    pub fn play(&mut self) {
        if self.ended {
            self.seek(0.0);
        }
        self.clock.play(Instant::now());
    }

    pub fn pause(&mut self) {
        let now = Instant::now();
        if !self.clock.held {
            let position = self.position_at(now);
            self.clock.follow(position, now);
        }
        self.clock.pause(now);
    }

    /// Playing, and not run out.
    pub fn is_playing(&mut self) -> bool {
        self.update_ended();
        self.clock.is_playing() && !self.ended
    }

    pub fn set_rate(&mut self, rate: f64) {
        self.clock.set_rate(rate, Instant::now());
    }

    /// Show `secs` next: pictures and sound restart there.
    pub fn seek(&mut self, secs: f64) {
        let secs = match self.duration() {
            Some(total) => secs.clamp(0.0, total),
            None => secs.max(0.0),
        };
        let audio = {
            let mut state = lock(&self.shared.state);
            state.seek = Some(secs);
            state.epoch += 1;
            state.video.clear();
            state.eof = false;
            state.audio.clone()
        };
        if let Some(audio) = audio {
            audio.reset(secs);
        }
        self.shared.wake.notify_all();
        self.clock.seek(secs, Instant::now());
        self.first = true;
        self.ended = false;
        self.last_pts = None;
    }

    /// What the output is playing to the ear, while the sound is the clock
    /// (before it starts and after it ran out the wall clock carries on).
    fn heard(&self) -> Option<f64> {
        let audio = self.audio()?;
        if self.sound_ran_out(&audio) {
            return None;
        }
        audio
            .played()
            .map(|played| (played - AUDIO_OUTPUT_LATENCY).max(0.0))
    }

    fn position_at(&self, now: Instant) -> f64 {
        let wall = self.clock.position(now);
        let position = if self.clock.held || !self.clock.is_playing() {
            wall
        } else {
            synced_position(wall, self.heard())
        };
        match self.duration() {
            Some(total) => position.clamp(0.0, total),
            None => position.max(0.0),
        }
    }

    fn sound_ran_out(&self, audio: &AudioShared) -> bool {
        lock(&self.shared.state).eof && audio.drained()
    }

    /// The playback position in seconds.
    pub fn position(&self) -> f64 {
        self.position_at(Instant::now())
    }

    fn update_ended(&mut self) {
        if self.ended {
            return;
        }
        let (drained, has_video) = {
            let state = lock(&self.shared.state);
            (
                state.eof && state.video.is_empty(),
                state.info.as_ref().is_some_and(|info| info.has_video),
            )
        };
        if !drained {
            return;
        }
        let now = Instant::now();
        // Done once the last picture was shown and its time came (a clip's
        // last frame stays up for its duration), and the sound played out.
        let sound_done = self.audio().is_none_or(|audio| audio.drained());
        let picture_done = !has_video || !self.first;
        let end = self.duration().or(self.last_pts);
        let reached = end.is_none_or(|end| self.position_at(now) + DUE_SLACK >= end);
        if sound_done && picture_done && reached {
            // Settle the clock on the end so the position reads full.
            let end = end.unwrap_or_else(|| self.position_at(now));
            self.clock.follow(end, now);
            self.clock.pause(now);
            self.ended = true;
        }
    }

    /// The picture to show now, when it changed since the last call.
    pub fn take_frame(&mut self) -> Option<VideoFrame> {
        let now = Instant::now();
        // With sound, pull the wall clock back to it when they drifted
        // apart (the sound is the master; the wall clock only smooths it).
        if !self.clock.held && self.clock.is_playing() {
            let wall = self.clock.position(now);
            let synced = synced_position(wall, self.heard());
            if synced != wall {
                self.clock.follow(synced, now);
            }
        }
        let position = self.position_at(now);
        let picked = {
            let mut state = lock(&self.shared.state);
            take_due(&mut state.video, position, self.first)
        };
        if let Some(frame) = &picked {
            self.shared.wake.notify_all();
            if self.first {
                self.first = false;
                self.clock.release(now);
            }
            self.last_pts = Some(frame.pts);
        }
        picked
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let tap = {
            let mut state = lock(&self.shared.state);
            state.stop = true;
            state.tap.take()
        };
        // An untaken sound end goes too (its drop wakes the thread).
        drop(tap);
        // The thread exits within one decode step; don't block the UI on it.
        self.shared.wake.notify_all();
    }
}

fn decode_loop(mut demuxer: Box<dyn Demuxer>, shared: Arc<Shared>, want_audio: bool) {
    let info = demuxer.info();
    let has_video = info.has_video;
    {
        let mut state = lock(&shared.state);
        if want_audio && info.has_audio {
            let audio = Arc::new(AudioShared {
                rate: info.sample_rate,
                channels: info.channels.max(1),
                queue: Mutex::new(AudioQueue::default()),
                generation: AtomicU64::new(0),
                played: AtomicU64::new(0f64.to_bits()),
                played_generation: AtomicU64::new(u64::MAX),
                detached: AtomicBool::new(false),
            });
            state.tap = Some(AudioTap {
                shared: audio.clone(),
                wake: shared.clone(),
                chunk: Vec::new(),
                at: 0,
                chunk_pts: 0.0,
                generation: 0,
                sounding: false,
            });
            state.audio = Some(audio);
        }
        state.info = Some(info);
    }
    loop {
        let mut state = lock(&shared.state);
        if state.stop {
            return;
        }
        if let Some(target) = state.seek.take() {
            let epoch = state.epoch;
            drop(state);
            let result = demuxer.seek(target);
            let mut state = lock(&shared.state);
            if state.epoch == epoch
                && let Err(err) = result
            {
                state.error = Some(err);
                state.eof = true;
            }
            continue;
        }
        let audio = state
            .audio
            .clone()
            .filter(|audio| !audio.detached.load(Ordering::Relaxed));
        let video_queued = has_video.then_some((state.video.len(), state.video_cap));
        let audio_buffered = audio.as_ref().map(|audio| audio.buffered_secs());
        if state.eof || !wants_more(video_queued, audio_buffered) {
            let (guard, _) = shared
                .wake
                .wait_timeout(state, IDLE_POLL)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            drop(guard);
            continue;
        }
        let epoch = state.epoch;
        drop(state);
        let item = demuxer.next();
        let mut state = lock(&shared.state);
        if state.epoch != epoch || state.seek.is_some() || state.stop {
            continue;
        }
        match item {
            Ok(Some(Item::Video(frame))) => {
                if has_video {
                    state.video.push_back(frame);
                }
            }
            Ok(Some(Item::Audio(chunk))) => {
                drop(state);
                if let Some(audio) = audio {
                    audio.push(chunk);
                }
            }
            Ok(None) => state.eof = true,
            Err(err) => {
                state.error = Some(err);
                state.eof = true;
            }
        }
    }
}

#[cfg(test)]
mod tests;
