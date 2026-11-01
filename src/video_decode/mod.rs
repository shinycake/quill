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
//!   the caller's box and rotated upright; with [`PixelOutput::Nv12`] the
//!   ones the stream allows come out as NV12 instead ([`nv12`]), for a
//!   renderer that converts on the GPU (Windows).
//! - Time: with sound, the sound card is the master clock (the samples it
//!   took, [`AudioTap`]); without, a wall clock that starts when the first
//!   picture is shown ([`Clock`]). The picture shown is the newest one due.
#![deny(clippy::indexing_slicing)]

mod bgra;
mod decode_loop;
mod ffi;
mod nv12;
mod present;

pub use bgra::{BGRA_KEPT, BgraPool, rotate_bgra};
pub use nv12::{PixelOutput, Pixels, YuvColor, YuvMatrix, nv12_len, nv12_to_bgra, pack_nv12};

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Sound the decoder keeps buffered at least.
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
/// A gap in the sound's timestamps longer than this plays as silence, so
/// the sound after it plays at its time (shorter ones are rounding).
const SOUND_GAP_MIN: f64 = 0.005;
/// A gap longer than this is a broken timestamp, not silence: the sound
/// after it plays at once instead of after hours of silence that would
/// fill the sound queue and stall the pictures.
const SOUND_GAP_MAX: f64 = 5.0;
/// A picture is due this much before its time (half a 60 Hz refresh), so
/// it isn't held a whole refresh for rounding.
const DUE_SLACK: f64 = 0.008;
/// Every change the decode thread waits for (a picture taken, sound
/// drained, a seek, the player dropped) wakes it under its lock, so an idle
/// one (a paused clip, an offscreen inline loop) re-checks on its own only
/// this often, as a safety net.
const IDLE_WAIT: Duration = Duration::from_secs(1);

/// Whether the in-process decoder can run here: the `quillvideo` shim and
/// its FFmpeg libraries are installed.
pub fn available() -> bool {
    ffi::availability().is_ok()
}

/// [`available`] without waiting: `None` until the shim has been loaded
/// (by [`available`] on some other thread), so a render never pays for it.
pub fn available_now() -> Option<bool> {
    ffi::loaded()
}

/// Why [`available`] is false.
pub fn unavailable_reason() -> Option<&'static str> {
    ffi::availability().err()
}

/// The shim's file name on this platform (packaging, diagnostics).
pub fn library_name() -> &'static str {
    ffi::library_name()
}

/// `quill --video-probe <file>`: decode a whole file with the installed
/// decoder and print what came out, with the mean and worst time spent
/// turning a decoded picture into its pixels. The package smoke tests run
/// it against the bundled libraries. `QUILL_VIDEO_PROBE_OUTPUT=nv12` asks
/// for NV12 pictures, as the Linux and Windows player does. Returns the
/// process exit code.
pub fn probe_cli(path: Option<&str>) -> i32 {
    let Some(path) = path else {
        eprintln!("usage: quill --video-probe <file>");
        return 2;
    };
    let options = OpenOptions {
        hardware: true,
        ..OpenOptions::viewer()
    };
    let pool = BgraPool::default();
    let mut demuxer = match ffi::FfiDemuxer::open(Path::new(path), &options) {
        Ok(demuxer) => demuxer.with_bgra_pool(pool.clone()),
        Err(err) => {
            eprintln!("video-probe: {err}");
            return 1;
        }
    };
    if std::env::var("QUILL_VIDEO_PROBE_OUTPUT").is_ok_and(|output| output == "nv12") {
        demuxer.set_output(PixelOutput::Nv12);
    }
    let info = demuxer.info();
    let (mut frames, mut samples) = (0usize, 0usize);
    let mut size = (0, 0);
    let (mut convert_total, mut convert_max, mut format) = (0u64, 0u32, "none");
    loop {
        match demuxer.next() {
            Ok(Some(Item::Video(frame))) => {
                frames += 1;
                size = (frame.width, frame.height);
                convert_total += u64::from(frame.convert_us);
                convert_max = convert_max.max(frame.convert_us);
                format = frame.pixels.trace_name();
                // As the player's images do once they leave the screen.
                if let Pixels::Bgra(bytes) = frame.pixels {
                    pool.give(bytes);
                }
            }
            Ok(Some(Item::Audio(chunk))) => samples += chunk.samples.len(),
            Ok(None) => break,
            Err(err) => {
                eprintln!("video-probe: {err}");
                return 1;
            }
        }
    }
    println!(
        "video={} {}x{} frames={frames} audio={} samples={samples} hardware={} \
         pixels={format} convert_us_mean={} convert_us_max={convert_max}",
        info.video_codec,
        size.0,
        size.1,
        info.audio_codec,
        demuxer.info().hardware,
        convert_total.checked_div(frames as u64).unwrap_or(0),
    );
    i32::from(info.has_video && frames == 0)
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
    /// The picture layout to start with ([`Player::set_output`] changes it).
    pub output: PixelOutput,
    /// Decode the video on the GPU where the platform and codec allow it
    /// (software otherwise, and after a hardware failure).
    pub hardware: bool,
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
            output: PixelOutput::Bgra,
            hardware: false,
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
            output: PixelOutput::Bgra,
            hardware: false,
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
    /// The video decodes in hardware ([`OpenOptions::hardware`]).
    pub hardware: bool,
}

/// One decoded picture, upright, tightly packed.
#[derive(Clone, Debug, PartialEq)]
pub struct VideoFrame {
    pub pts: f64,
    pub width: u32,
    pub height: u32,
    pub pixels: Pixels,
    /// Microseconds spent turning the decoded picture into `pixels` (the
    /// shim's conversion or plane copy, plus packing and turning here):
    /// the `QUILL_TRACE_VIDEO` convert column.
    pub convert_us: u32,
}

impl VideoFrame {
    /// A BGRA picture.
    pub fn bgra(pts: f64, width: u32, height: u32, bgra: Vec<u8>) -> Self {
        Self {
            pts,
            width,
            height,
            pixels: Pixels::Bgra(bgra),
            convert_us: 0,
        }
    }

    /// The picture as BGRA: as it is, or converted from NV12 on the CPU
    /// (a renderer that can't draw NV12 after all) into a buffer from
    /// `pool`. `None` when the NV12 bytes don't match the size.
    pub fn into_bgra(self, pool: &BgraPool) -> Option<Self> {
        let pixels = match self.pixels {
            Pixels::Bgra(bytes) => Pixels::Bgra(bytes),
            Pixels::Nv12 { bytes, color } => {
                let len = (self.width as usize)
                    .saturating_mul(self.height as usize)
                    .saturating_mul(4);
                let out = pool.take(len);
                Pixels::Bgra(nv12::nv12_to_bgra_into(
                    &bytes,
                    self.width,
                    self.height,
                    color,
                    out,
                )?)
            }
        };
        Some(Self { pixels, ..self })
    }
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
    /// The next sound, with the pictures it was read past kept undecoded
    /// for [`Demuxer::next`]. `None` when no sound is in reach: it ended,
    /// or too many pictures wait.
    fn next_sound(&mut self) -> Result<Option<AudioChunk>, String> {
        Ok(None)
    }
    /// Continue from `secs` (items before it are not returned).
    fn seek(&mut self, secs: f64) -> Result<(), String>;
    /// The picture layout wanted from the next picture on. Demuxers that
    /// only make BGRA ignore it.
    fn set_output(&mut self, _output: PixelOutput) {}
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

/// The sound shared between the decode thread and the audio output.
struct AudioShared {
    rate: u32,
    channels: u16,
    queue: Mutex<AudioQueue>,
    /// Bumped by every seek: the output drops what it held.
    generation: AtomicU64,
    /// Where the sound of the current generation starts (f64 bits): the
    /// seek target, 0 at open. Stored before `generation` is bumped.
    origin: AtomicU64,
    /// Seqlock-style: the position of the next sample the output will
    /// play (f64 bits), tagged with the generation it belongs to.
    played: AtomicU64,
    played_generation: AtomicU64,
    /// The output let go of the tap: sound is decoded for nobody.
    detached: AtomicBool,
    starved: AtomicBool,
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
        self.origin.store(secs.to_bits(), Ordering::Release);
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

    /// What is heard, if the output has played anything since the last seek.
    fn heard(&self) -> Option<f64> {
        self.played()
            .map(|played| (played - AUDIO_OUTPUT_LATENCY).max(0.0))
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    crate::locks::lock_or_recover(mutex)
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
    /// Where the sound played so far ends, since the last seek.
    end_pts: f64,
    /// Taken off the chunks' timestamps since a broken one, so the sound
    /// stays on the pictures' clock.
    pts_shift: f64,
    /// Silent samples owed before `chunk`, for a gap in the sound.
    gap: usize,
    gap_at: usize,
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
            let origin = f64::from_bits(self.shared.origin.load(Ordering::Acquire));
            self.end_pts = origin;
            self.pts_shift = 0.0;
            self.gap = 0;
        }
        if self.gap_at < self.gap {
            return self.next_gap_sample();
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
                    self.start_chunk(chunk);
                    // Through the state lock: the decode thread is then
                    // either before its room check or waiting, never between.
                    drop(lock(&self.wake.state));
                    self.wake.wake.notify_one();
                    if self.gap > 0 {
                        return self.next_gap_sample();
                    }
                }
                None => {
                    self.set_sounding(false);
                    return 0.0;
                }
            }
        }
        let Some(&sample) = self.chunk.get(self.at) else {
            return 0.0;
        };
        self.at += 1;
        let channels = usize::from(self.channels());
        if self.at.is_multiple_of(channels) {
            let played =
                self.chunk_pts + (self.at / channels) as f64 / f64::from(self.sample_rate());
            self.publish_played(played);
        }
        self.set_sounding(true);
        sample
    }

    /// Take `chunk` as the next sound, after the silence of any gap
    /// between where the sound played so far ends and where it starts.
    fn start_chunk(&mut self, chunk: AudioChunk) {
        let channels = usize::from(self.channels());
        let rate = f64::from(self.sample_rate());
        let mut pts = chunk.pts - self.pts_shift;
        let gap = pts - self.end_pts;
        if !gap.is_finite() || gap.abs() > SOUND_GAP_MAX {
            // A broken timestamp: the sound goes on where it was, and so do
            // the chunks stamped after it.
            if chunk.pts.is_finite() {
                self.pts_shift = chunk.pts - self.end_pts;
            }
            pts = self.end_pts;
        }
        self.gap = if gap > SOUND_GAP_MIN && gap <= SOUND_GAP_MAX {
            ((gap * rate).round() as usize).saturating_mul(channels)
        } else {
            0
        };
        self.gap_at = 0;
        let frames = chunk.samples.len() / channels;
        self.end_pts = pts + frames as f64 / rate;
        self.chunk_pts = pts;
        self.chunk = chunk.samples;
        self.at = 0;
    }

    fn next_gap_sample(&mut self) -> f32 {
        self.gap_at += 1;
        let channels = usize::from(self.channels());
        if self.gap_at.is_multiple_of(channels) {
            let left = (self.gap - self.gap_at) / channels;
            let played = self.chunk_pts - left as f64 / f64::from(self.sample_rate());
            self.publish_played(played);
        }
        self.set_sounding(true);
        0.0
    }

    fn publish_played(&self, played: f64) {
        self.shared
            .played
            .store(played.to_bits(), Ordering::Release);
        self.shared
            .played_generation
            .store(self.generation, Ordering::Release);
    }

    fn set_sounding(&mut self, sounding: bool) {
        if self.sounding != sounding {
            self.sounding = sounding;
            self.shared.starved.store(!sounding, Ordering::Release);
        }
    }
}

impl Drop for AudioTap {
    fn drop(&mut self) {
        self.shared.detached.store(true, Ordering::Relaxed);
        lock(&self.shared.queue).chunks.clear();
        // Through the state lock, as in `next_sample`: a decode thread that
        // saw the sound attached is then already waiting, and gets the wake.
        drop(lock(&self.wake.state));
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
    /// The picture layout asked for; the decode thread passes it on.
    output: PixelOutput,
    seek: Option<f64>,
    /// The player's clock. It lives here so the decode thread can pace
    /// pictures nobody paints on it.
    clock: Clock,
    /// Bumped by every seek: items decoded for an older one are dropped.
    epoch: u64,
    eof: bool,
    error: Option<String>,
    stop: bool,
}

/// One clip: a decode thread plus the clock and the picture queue.
pub struct Player {
    shared: Arc<Shared>,
    /// No picture shown since the last seek (or open).
    first: bool,
    /// The newest picture handed out, for the end position.
    last_pts: Option<f64>,
    ended: bool,
    bgra: BgraPool,
    /// The decode thread. Dropping the player detaches it (it exits within
    /// one decode step); tests join it.
    #[cfg(test)]
    thread: Option<std::thread::JoinHandle<Option<()>>>,
}

/// Dropped as the decode thread ends. When that was not a stop request, it
/// records an error if none was set (a panic, or an unexpected return),
/// marks the end, and releases the picture queue and the sound end.
struct ExitReport(Arc<Shared>);

impl Drop for ExitReport {
    fn drop(&mut self) {
        let (tap, audio) = {
            let mut state = lock(&self.0.state);
            if state.stop {
                return;
            }
            if state.error.is_none() {
                state.error = Some("Video playback stopped unexpectedly".to_string());
            }
            state.eof = true;
            state.video.clear();
            (state.tap.take(), state.audio.take())
        };
        // Outside the lock: the tap's drop locks it again.
        drop(tap);
        drop(audio);
    }
}

impl Player {
    /// Open `path` with the FFmpeg shim on a new decode thread. Returns at
    /// once; the file opens on that thread ([`Player::info`] and
    /// [`Player::take_audio_tap`] become available then, or
    /// [`Player::error`]).
    pub fn open(path: &Path, options: OpenOptions) -> Result<Self, String> {
        // Only a load that already failed is reported here: a first load
        // (a dlopen of 20 to 90 ms) happens on the decode thread instead
        // of the caller's, which is the UI thread for the players.
        if ffi::loaded() == Some(false) {
            ffi::availability().map_err(str::to_string)?;
        }
        let path: PathBuf = path.to_path_buf();
        let opts = options.clone();
        let bgra = BgraPool::default();
        let pool = bgra.clone();
        Self::spawn(
            move || {
                ffi::FfiDemuxer::open(&path, &opts)
                    .map(|d| Box::new(d.with_bgra_pool(pool)) as Box<dyn Demuxer>)
            },
            options,
            bgra,
        )
    }

    /// Run a player over any demuxer (tests use scripted ones).
    pub fn with_demuxer(demuxer: Box<dyn Demuxer>, options: OpenOptions) -> Result<Self, String> {
        Self::spawn(move || Ok(demuxer), options, BgraPool::default())
    }

    fn spawn(
        opener: impl FnOnce() -> Result<Box<dyn Demuxer>, String> + Send + 'static,
        options: OpenOptions,
        bgra: BgraPool,
    ) -> Result<Self, String> {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                video_cap: options.video_frames.max(1),
                output: options.output,
                ..State::default()
            }),
            wake: Condvar::new(),
        });
        let thread_shared = shared.clone();
        let want_audio = options.want_audio;
        #[cfg_attr(not(test), allow(unused_variables))]
        let thread = crate::thread_guard::spawn_guarded("quill-video", move || {
            // Runs on every exit, unwinding included: a decode thread that
            // ends without being asked to leaves the player in its error
            // state instead of "loading" forever.
            let _exit = ExitReport(thread_shared.clone());
            match opener() {
                Ok(demuxer) => decode_loop::run(demuxer, thread_shared, want_audio),
                Err(err) => {
                    let mut state = lock(&thread_shared.state);
                    state.error = Some(err);
                    state.eof = true;
                }
            }
        })
        .map_err(|err| err.to_string())?;
        Ok(Self {
            shared,
            first: true,
            last_pts: None,
            ended: false,
            bgra,
            #[cfg(test)]
            thread: Some(thread),
        })
    }

    /// Where the clip's BGRA pictures come from: hand each buffer back
    /// once its picture is off screen.
    pub fn bgra_pool(&self) -> &BgraPool {
        &self.bgra
    }

    /// Wait for the decode thread to end (tests); true when it returned
    /// without panicking.
    #[cfg(test)]
    fn join_thread(&mut self) -> Option<bool> {
        let handle = self.thread.take()?;
        Some(matches!(handle.join(), Ok(Some(()))))
    }

    fn clock(&self) -> Clock {
        lock(&self.shared.state).clock
    }

    /// Change the clock, and wake the decode thread, which paces on it.
    fn with_clock(&self, change: impl FnOnce(&mut Clock)) {
        change(&mut lock(&self.shared.state).clock);
        self.shared.wake.notify_all();
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

    /// The picture layout to decode into from the next picture on.
    /// Pictures already queued keep theirs.
    pub fn set_output(&self, output: PixelOutput) {
        let changed = {
            let mut state = lock(&self.shared.state);
            std::mem::replace(&mut state.output, output) != output
        };
        if changed {
            self.shared.wake.notify_all();
        }
    }

    /// The picture layout asked for.
    pub fn output(&self) -> PixelOutput {
        lock(&self.shared.state).output
    }

    pub fn play(&mut self) {
        if self.ended {
            self.seek(0.0);
        }
        self.with_clock(|clock| clock.play(Instant::now()));
    }

    pub fn pause(&mut self) {
        let now = Instant::now();
        if !self.clock().held {
            let position = self.position_at(now);
            self.with_clock(|clock| clock.follow(position, now));
        }
        self.with_clock(|clock| clock.pause(now));
    }

    /// Playing, and not run out.
    pub fn is_playing(&mut self) -> bool {
        self.update_ended(Instant::now());
        self.clock().is_playing() && !self.ended
    }

    /// Ran out at the end (until the next seek).
    pub fn ended(&mut self) -> bool {
        self.update_ended(Instant::now());
        self.ended
    }

    pub fn set_rate(&mut self, rate: f64) {
        self.with_clock(|clock| clock.set_rate(rate, Instant::now()));
    }

    /// Show `secs` next: pictures and sound restart there.
    pub fn seek(&mut self, secs: f64) {
        let secs = match self.duration() {
            Some(total) => secs.clamp(0.0, total),
            None => secs.max(0.0),
        };
        {
            let mut state = lock(&self.shared.state);
            state.seek = Some(secs);
            state.epoch += 1;
            state.video.clear();
            state.eof = false;
            // Under the lock: the decode thread must not judge pictures of
            // the new position against where the sound was before the seek.
            if let Some(audio) = &state.audio {
                audio.reset(secs);
            }
        }
        self.shared.wake.notify_all();
        self.with_clock(|clock| clock.seek(secs, Instant::now()));
        self.first = true;
        self.ended = false;
        self.last_pts = None;
    }

    fn heard(&self) -> Option<f64> {
        let audio = self.audio()?;
        if audio.starved.load(Ordering::Acquire) {
            return None;
        }
        audio.heard()
    }

    fn position_at(&self, now: Instant) -> f64 {
        let clock = self.clock();
        let wall = clock.position(now);
        let position = if clock.held || !clock.is_playing() {
            wall
        } else {
            synced_position(wall, self.heard())
        };
        match self.duration() {
            Some(total) => position.clamp(0.0, total),
            None => position.max(0.0),
        }
    }

    /// The playback position in seconds.
    pub fn position(&self) -> f64 {
        self.position_at(Instant::now())
    }

    fn update_ended(&mut self, now: Instant) {
        if self.ended {
            return;
        }
        // A clip nobody paints keeps its last picture queued; once the
        // clock has passed it, it would only be shown late.
        // Held for a first picture after a seek that nobody paints, while
        // the sound plays on past it: the decode thread already paces the
        // pictures on the sound (`paint_floor`), so the clock follows it.
        let clock = self.clock();
        if clock.held
            && clock.is_playing()
            && let Some(heard) = self.heard().filter(|&heard| heard > clock.position(now))
        {
            self.with_clock(|clock| clock.follow(heard, now));
        }
        let position = self.position_at(now);
        // Running without a picture shown since the seek: released above.
        let running = !self.first || !self.clock().held;
        let drained = {
            let state = lock(&self.shared.state);
            let passed = |last: &VideoFrame| running && last.pts <= position + DUE_SLACK;
            state.eof && state.video.back().is_none_or(passed)
        };
        if !drained {
            return;
        }
        let no_picture_coming = self.first;
        if no_picture_coming {
            self.with_clock(|clock| clock.release(now));
        }
        let sound_done = self.audio().is_none_or(|audio| audio.drained());
        let end = self.duration().or(self.last_pts);
        let reached =
            no_picture_coming || end.is_none_or(|end| self.position_at(now) + DUE_SLACK >= end);
        if sound_done && reached {
            // Settle the clock on the end so the position reads full.
            let end = end.unwrap_or_else(|| self.position_at(now));
            self.with_clock(|clock| {
                clock.follow(end, now);
                clock.pause(now);
            });
            self.ended = true;
        }
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

#[cfg(test)]
mod tests;
