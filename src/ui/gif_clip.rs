//! A true `image/gif` file decoded in process (the `image` crate's GIF
//! decoder, the one GPUI's own `img` uses for the viewer) for the history's
//! looping GIF. A frame that repeats for several ticks is held once with its
//! duration, so a long GIF with still stretches does not keep N copies of
//! the same picture. MP4 "GIFs" play through the video player instead, and
//! so does a file labelled `image/gif` whose bytes are not a GIF.
//!
//! Received files are untrusted: the canvas is bounded before anything is
//! allocated, the decoder runs under `image::Limits`, decoding stops after
//! [`MAX_FRAMES`] frames, [`MAX_TOTAL`] of playing time, [`MAX_HELD`]
//! pictures or [`MAX_BYTES`], and a cancel flag is checked before each
//! frame. A frame that fails to decode ends the clip with what came
//! before it.
#![deny(clippy::indexing_slicing)]

use gpui_kit::RenderImage;
use image::{AnimationDecoder, ImageDecoder, ImageResult, RgbaImage};
use smallvec::SmallVec;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// A GIF's canvas may not be larger than this on a side.
const MAX_CANVAS_SIDE: u32 = 4096;
/// Nor have more pixels than this (16 Mpx, a 64 MB RGBA canvas; a
/// 4096 x 4096 canvas is just over it).
const MAX_CANVAS_PIXELS: u64 = 16_000_000;
/// What the decoder may allocate at once: its canvas, one frame and one
/// composited picture, each at most [`MAX_CANVAS_PIXELS`] RGBA.
const MAX_ALLOC: u64 = 3 * MAX_CANVAS_PIXELS * 4;
/// Pictures are never decoded smaller than this on their longer side.
const MIN_EDGE: u32 = 16;
/// Decoding stops after this many frames read (held or merged).
const MAX_FRAMES: usize = 3000;
/// Decoding stops once the frames read play this long.
const MAX_TOTAL: Duration = Duration::from_secs(300);
/// Decoding stops after this many distinct held frames.
const MAX_HELD: usize = 600;
/// Decoding stops once the held pictures take this many bytes.
const MAX_BYTES: usize = 48 * 1024 * 1024;
/// Delays of 10 ms and less play at 100 ms, as browsers do.
const FAST_DELAY: Duration = Duration::from_millis(10);
const DEFAULT_DELAY: Duration = Duration::from_millis(100);
/// The layer redraws a GIF at most this often (its shortest delay is 20 ms).
const MAX_FPS: u32 = 50;

/// Why a GIF did not decode.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum GifError {
    /// The file could not be read, or its header is broken.
    Open(String),
    /// The bytes are not a GIF (an MP4 labelled `image/gif`): the video
    /// player takes it.
    NotGif,
    /// The canvas is outside [`MAX_CANVAS_SIDE`] / [`MAX_CANVAS_PIXELS`].
    TooLarge,
    /// Stopped before it finished (`stop_animation_playback`).
    Cancelled,
    /// Not one frame decoded.
    Empty,
}

struct Held {
    image: Arc<RenderImage>,
    /// How long this picture shows.
    shown: Duration,
    /// When it stops showing, from the start of the loop.
    ends: Duration,
}

/// What has decoded of a clip so far.
#[derive(Default)]
struct Decoded {
    frames: Vec<Held>,
    /// Every frame is in, so the clip loops.
    complete: bool,
}

/// The distinct pictures of a looping GIF and how long each shows. The
/// decode thread adds them while the row already plays the first ones.
pub(super) struct GifClip {
    decoded: Mutex<Decoded>,
}

impl Drop for GifClip {
    fn drop(&mut self) {
        // Hand the pictures to the atlas sweeper, which keeps any still
        // held elsewhere (the frame being painted).
        let frames = std::mem::take(&mut lock(&self.decoded).frames);
        super::image_budget::retire_all(frames.into_iter().map(|held| held.image));
    }
}

fn lock(decoded: &Mutex<Decoded>) -> MutexGuard<'_, Decoded> {
    decoded.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Pictures collected for a clip, merging a picture equal to the one
/// before, and published to it as they come.
struct ClipBuilder {
    clip: Arc<GifClip>,
    held: usize,
    bytes: usize,
}

impl Default for ClipBuilder {
    fn default() -> Self {
        Self {
            clip: Arc::new(GifClip {
                decoded: Mutex::default(),
            }),
            held: 0,
            bytes: 0,
        }
    }
}

impl ClipBuilder {
    /// Add a picture shown for `delay`; equal to the last, it extends that
    /// one instead. Returns whether more pictures fit.
    fn push(&mut self, mut picture: RgbaImage, delay: Duration) -> bool {
        // RenderImage holds BGRA.
        for pixel in picture.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        let mut decoded = lock(&self.clip.decoded);
        if let Some(last) = decoded.frames.last_mut()
            && last.image.as_bytes(0) == Some(picture.as_raw().as_slice())
        {
            last.shown = last.shown.saturating_add(delay);
            last.ends = last.ends.saturating_add(delay);
            return true;
        }
        let size = picture.as_raw().len();
        if self.held >= MAX_HELD || self.bytes.saturating_add(size) > MAX_BYTES {
            return false;
        }
        self.held += 1;
        self.bytes = self.bytes.saturating_add(size);
        let ends = decoded
            .frames
            .last()
            .map_or(Duration::ZERO, |last| last.ends)
            .saturating_add(delay);
        decoded.frames.push(Held {
            image: Arc::new(RenderImage::new(SmallVec::from_buf([image::Frame::new(
                picture,
            )]))),
            shown: delay,
            ends,
        });
        true
    }

    /// The clip so far, once it has a picture.
    fn started(&self) -> Option<Arc<GifClip>> {
        (self.held > 0).then(|| self.clip.clone())
    }

    fn finish(self) -> Option<Arc<GifClip>> {
        lock(&self.clip.decoded).complete = true;
        self.started()
    }
}

fn frame_delay(frame: &image::Frame) -> Duration {
    let (numer, denom) = frame.delay().numer_denom_ms();
    let millis = u64::from(numer) / u64::from(denom.max(1));
    let delay = Duration::from_millis(millis);
    if delay <= FAST_DELAY {
        DEFAULT_DELAY
    } else {
        delay
    }
}

/// `picture` scaled down (never up) to fit `max_edge` px on its longer side.
fn fitted(picture: RgbaImage, max_edge: u32) -> RgbaImage {
    let (width, height) = picture.dimensions();
    let longer = width.max(height);
    if longer <= max_edge || longer == 0 {
        return picture;
    }
    let size = |side: u32| {
        let scaled = (u64::from(side) * u64::from(max_edge) / u64::from(longer)).max(1);
        u32::try_from(scaled).unwrap_or(max_edge)
    };
    image::imageops::resize(
        &picture,
        size(width),
        size(height),
        image::imageops::FilterType::Triangle,
    )
}

/// Whether a canvas of `width` x `height` may be decoded.
fn canvas_fits(width: u32, height: u32) -> bool {
    width > 0
        && height > 0
        && width <= MAX_CANVAS_SIDE
        && height <= MAX_CANVAS_SIDE
        && u64::from(width) * u64::from(height) <= MAX_CANVAS_PIXELS
}

/// Whether the first bytes are a GIF signature.
fn is_gif_header(head: &[u8]) -> bool {
    head == b"GIF87a" || head == b"GIF89a"
}

/// Read the frames into a clip: stop when `cancel` is set (checked before
/// each frame), at the first frame that fails (keeping the ones before),
/// or at a bound.
fn collect(
    mut frames: impl Iterator<Item = ImageResult<image::Frame>>,
    max_edge: u32,
    cancel: &AtomicBool,
    started: impl FnOnce(Arc<GifClip>),
) -> Result<Arc<GifClip>, GifError> {
    let mut builder = ClipBuilder::default();
    let mut started = Some(started);
    let mut read = 0usize;
    let mut total = Duration::ZERO;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(GifError::Cancelled);
        }
        // A malformed late frame ends the clip; what decoded plays.
        let Some(Ok(frame)) = frames.next() else {
            break;
        };
        read = read.saturating_add(1);
        let delay = frame_delay(&frame);
        total = total.saturating_add(delay);
        let more = builder.push(fitted(frame.into_buffer(), max_edge), delay);
        if let Some(clip) = builder.started()
            && let Some(started) = started.take()
        {
            started(clip);
        }
        if !more {
            break;
        }
        if read >= MAX_FRAMES || total >= MAX_TOTAL {
            break;
        }
    }
    builder.finish().ok_or(GifError::Empty)
}

impl GifClip {
    /// Decode the file at most `max_edge` px on its longer side (the tile
    /// in device pixels, capped at `INLINE_MAX_EDGE`). Runs off the UI
    /// thread; returns early once `cancel` is set. `started` gets the clip
    /// as soon as its first picture is in, to play while the rest decodes.
    pub(super) fn decode(
        path: &Path,
        max_edge: u32,
        cancel: &AtomicBool,
        started: impl FnOnce(Arc<Self>),
    ) -> Result<Arc<Self>, GifError> {
        let mut file = std::fs::File::open(path).map_err(|err| GifError::Open(err.to_string()))?;
        let mut head = [0u8; 6];
        if file.read_exact(&mut head).is_err() || !is_gif_header(&head) {
            return Err(GifError::NotGif);
        }
        file.seek(SeekFrom::Start(0))
            .map_err(|err| GifError::Open(err.to_string()))?;
        Self::decode_from(std::io::BufReader::new(file), max_edge, cancel, started)
    }

    fn decode_from(
        reader: impl std::io::BufRead + Seek,
        max_edge: u32,
        cancel: &AtomicBool,
        started: impl FnOnce(Arc<Self>),
    ) -> Result<Arc<Self>, GifError> {
        let mut decoder = image::codecs::gif::GifDecoder::new(reader)
            .map_err(|err| GifError::Open(err.to_string()))?;
        let (width, height) = decoder.dimensions();
        if !canvas_fits(width, height) {
            return Err(GifError::TooLarge);
        }
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(MAX_CANVAS_SIDE);
        limits.max_image_height = Some(MAX_CANVAS_SIDE);
        limits.max_alloc = Some(MAX_ALLOC);
        decoder.set_limits(limits).map_err(|_| GifError::TooLarge)?;
        let max_edge = max_edge.clamp(MIN_EDGE, super::inline_video::INLINE_MAX_EDGE);
        collect(decoder.into_frames(), max_edge, cancel, started)
    }

    /// How many distinct pictures are held.
    #[cfg(test)]
    pub(super) fn held(&self) -> usize {
        lock(&self.decoded).frames.len()
    }

    /// The longer side of the held pictures, in pixels.
    #[cfg(test)]
    fn edge(&self) -> u32 {
        lock(&self.decoded).frames.first().map_or(0, |held| {
            let size = held.image.size(0);
            let side = |pixels: i32| u32::try_from(pixels).unwrap_or(0);
            side(size.width.0).max(side(size.height.0))
        })
    }

    /// Whether the clip never changes (fully read, one held picture).
    #[cfg(test)]
    pub(super) fn is_still(&self) -> bool {
        let decoded = lock(&self.decoded);
        decoded.complete && decoded.frames.len() < 2
    }

    /// The loop's length.
    #[cfg(test)]
    fn total(&self) -> Duration {
        Self::loop_length(&lock(&self.decoded))
    }

    fn loop_length(decoded: &Decoded) -> Duration {
        decoded
            .frames
            .last()
            .map_or(Duration::ZERO, |held| held.ends)
    }

    /// The picture showing `elapsed` into the (looping) clip. Past the
    /// pictures decoded so far, the last one holds until more come.
    pub(super) fn frame_at(&self, elapsed: Duration) -> Option<Arc<RenderImage>> {
        let decoded = lock(&self.decoded);
        let total = Self::loop_length(&decoded).as_nanos();
        let at = if decoded.complete {
            elapsed.as_nanos().checked_rem(total).unwrap_or(0)
        } else {
            elapsed.as_nanos()
        };
        decoded
            .frames
            .iter()
            .find(|held| held.ends.as_nanos() > at)
            .or(decoded.frames.last())
            .map(|held| held.image.clone())
    }

    /// How often the picture can change: one redraw per shortest held
    /// picture, at most [`MAX_FPS`]; 0 for a still clip. While decoding,
    /// at least the default delay's rate, for the pictures still to come.
    fn fps(&self) -> u32 {
        let decoded = lock(&self.decoded);
        let shortest = decoded.frames.iter().map(|held| held.shown).min();
        let shortest = match (decoded.complete, shortest) {
            (true, _) if decoded.frames.len() < 2 => return 0,
            (true, Some(shortest)) => shortest,
            (false, Some(shortest)) => shortest.min(DEFAULT_DELAY),
            (_, None) => DEFAULT_DELAY,
        };
        let millis = shortest.as_millis().max(1);
        u32::try_from(1000u128.div_ceil(millis))
            .unwrap_or(MAX_FPS)
            .clamp(1, MAX_FPS)
    }
}

/// The playing GIF of a history row: its clip and when it started, so the
/// picture is picked when the row (or its animation layer) draws.
#[derive(Clone)]
pub(crate) struct GifPlay {
    clip: Arc<GifClip>,
    started: Instant,
}

impl GifPlay {
    pub(super) fn new(clip: Arc<GifClip>, started: Instant) -> Self {
        Self { clip, started }
    }

    /// A screenshot capture shows the first picture, so it is the same
    /// every run.
    fn frozen() -> bool {
        #[cfg(feature = "demo-capture")]
        if std::env::var_os("QUILL_DEMO_CAPTURE").is_some() {
            return true;
        }
        false
    }

    /// The picture due now.
    pub(super) fn frame_now(&self) -> Option<Arc<RenderImage>> {
        let elapsed = if Self::frozen() {
            Duration::ZERO
        } else {
            self.started.elapsed()
        };
        self.clip.frame_at(elapsed)
    }

    /// The redraw rate the GIF's own frame cadence asks for (0 while it
    /// cannot change).
    pub(super) fn fps(&self) -> u32 {
        if Self::frozen() { 0 } else { self.clip.fps() }
    }
}

#[cfg(test)]
mod tests;
