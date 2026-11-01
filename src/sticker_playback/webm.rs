//! WebM video stickers (VP9 or VP8, with alpha) to BGRA frames.
//!
//! In process through the bundled decoder (`webm_ffi`, the same FFmpeg
//! libraries the video player loads) when it is installed: no processes, no
//! PNG files, no system `ffmpeg`. Without it (macOS release builds, which
//! play video through AVPlayer and ship no FFmpeg, or a plain `cargo run`)
//! the `ffmpeg` binary decodes as before. Both paths keep the same frame
//! rate and frame count caps, so the playback cache sees the same sizes.
//! docs/decisions/sweep-webm-stickers.md.

use super::webm_ffi::{OpenError, StickerDecoder};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct WebmFrame {
    pub width: u32,
    pub height: u32,
    /// Straight-alpha BGRA, `width * height * 4` bytes (GPUI's
    /// `RenderImage` byte order).
    pub bgra: Vec<u8>,
}

impl WebmFrame {
    /// Centered on a transparent `edge` × `edge` square.
    pub fn into_square(self, edge: u32) -> Vec<u8> {
        if self.width == edge && self.height == edge {
            self.bgra
        } else {
            center_into(&self.bgra, self.width, self.height, edge, edge)
        }
    }
}

/// What a decode gives next. Every frame of a sticker has one size.
pub enum WebmStep {
    Frame(WebmFrame),
    /// The last frame, this many times more.
    Repeat(usize),
}

pub type WebmSteps<'a> = dyn FnMut(WebmStep) -> Result<(), String> + 'a;

/// A sticker's frames gathered in memory, all `width` × `height`.
#[derive(Default)]
pub struct WebmFrames {
    pub width: u32,
    pub height: u32,
    /// Each as [`WebmFrame::bgra`].
    pub frames: Vec<Vec<u8>>,
    pub fps: f64,
}

impl WebmFrames {
    pub(super) fn keep(&mut self, step: WebmStep) {
        match step {
            WebmStep::Frame(frame) => {
                (self.width, self.height) = (frame.width, frame.height);
                self.frames.push(frame.bgra);
            }
            WebmStep::Repeat(times) => {
                if let Some(last) = self.frames.last().cloned() {
                    self.frames.extend(std::iter::repeat_n(last, times));
                }
            }
        }
    }

    /// The first frame as RGBA (the photo editor's sticker item).
    pub fn first_rgba(self) -> Option<(u32, u32, Vec<u8>)> {
        let mut frame = self.frames.into_iter().next()?;
        for pixel in frame.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        Some((self.width, self.height, frame))
    }
}

/// Longest video sticker accepted, in whole seconds (Telegram's limit is
/// three; the old probe-based check allowed up to 30).
const MAX_SECONDS: i64 = 30;
/// Frame rate cap of decoded webm frames: at sticker sizes 24 fps looks the
/// same as the usual 30 and holds a fifth fewer frames.
const MAX_FPS: f64 = 24.0;

/// Frame rate frames are sampled at for a `duration`-second sticker with at
/// most `max_frames` frames: the ffmpeg path's `max / (seconds + 1)`, at
/// most [`MAX_FPS`]. `seconds` is rounded the way the probe rounded it.
pub(super) fn sample_fps(duration: Option<f64>, max_frames: usize) -> Result<f64, String> {
    let Some(duration) = duration.filter(|d| d.is_finite() && *d > 0.0) else {
        return Ok(MAX_FPS);
    };
    let seconds = (duration.round() as i64).max(1);
    if seconds > MAX_SECONDS {
        return Err("Video sticker is too long".into());
    }
    Ok((max_frames.max(1) as f64 / (seconds + 1) as f64).min(MAX_FPS))
}

/// What to do with one decoded picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Placement {
    /// Copies of the previous frame that fill skipped slots first.
    pub(super) repeat_previous: usize,
    /// Keep this picture as the next frame.
    pub(super) take: bool,
}

/// Picks the decoded pictures that land on a fixed-rate grid (`fps`), as
/// ffmpeg's `fps` filter did: the picture nearest each slot, the previous
/// frame repeated over gaps, as many frames as the duration holds and at
/// most `max` in all.
pub(super) struct FramePlan {
    fps: f64,
    max: usize,
    /// The duration is known: [`tail`](Self::tail) fills up to `max`.
    timed: bool,
    base: Option<i64>,
    last: Option<i64>,
    emitted: usize,
}

impl FramePlan {
    pub(super) fn new(fps: f64, max: usize, duration: Option<f64>) -> Self {
        let fps = if fps.is_finite() && fps > 0.0 {
            fps
        } else {
            MAX_FPS
        };
        let slots = duration
            .filter(|d| d.is_finite() && *d > 0.0)
            .map(|d| usize::try_from(((d * fps).round() as i64).max(1)).unwrap_or(usize::MAX));
        Self {
            fps,
            max: slots.map_or(max, |slots| slots.min(max)),
            timed: slots.is_some(),
            base: None,
            last: None,
            emitted: 0,
        }
    }

    pub(super) fn full(&self) -> bool {
        self.emitted >= self.max
    }

    pub(super) fn place(&mut self, pts: f64) -> Placement {
        const SKIP: Placement = Placement {
            repeat_previous: 0,
            take: false,
        };
        let remaining = self.max.saturating_sub(self.emitted);
        if remaining == 0 {
            return SKIP;
        }
        // `as` saturates for huge values; a NaN time counts as slot 0.
        let raw = (pts.max(0.0) * self.fps).round() as i64;
        let base = *self.base.get_or_insert(raw);
        let slot = raw.saturating_sub(base).max(0);
        let repeat_previous = match self.last {
            Some(last) if slot <= last => return SKIP,
            Some(last) => usize::try_from(slot.saturating_sub(last).saturating_sub(1))
                .unwrap_or(usize::MAX)
                .min(remaining.saturating_sub(1)),
            None => 0,
        };
        self.last = Some(slot);
        self.emitted = self
            .emitted
            .saturating_add(repeat_previous)
            .saturating_add(1);
        Placement {
            repeat_previous,
            take: true,
        }
    }

    /// At the end: copies of the last frame that hold it until the
    /// sticker's duration ends, so the loop keeps the file's length.
    pub(super) fn tail(&mut self) -> usize {
        if !self.timed || self.last.is_none() {
            return 0;
        }
        let repeat = self.max.saturating_sub(self.emitted);
        self.emitted = self.emitted.saturating_add(repeat);
        repeat
    }
}

/// `frame` (`width` × `height` BGRA) centered in a transparent `out_w` ×
/// `out_h` picture, cropped where it is larger.
pub(super) fn center_into(
    frame: &[u8],
    width: u32,
    height: u32,
    out_w: u32,
    out_h: u32,
) -> Vec<u8> {
    let (width, height) = (width as usize, height as usize);
    let (out_w, out_h) = (out_w as usize, out_h as usize);
    let mut out = vec![0u8; out_w.saturating_mul(out_h).saturating_mul(4)];
    let copy_w = width.min(out_w);
    let copy_h = height.min(out_h);
    let (src_x, dst_x) = ((width - copy_w) / 2, (out_w - copy_w) / 2);
    let (src_y, dst_y) = ((height - copy_h) / 2, (out_h - copy_h) / 2);
    for row in 0..copy_h {
        let src = (src_y + row) * width * 4 + src_x * 4;
        let dst = (dst_y + row) * out_w * 4 + dst_x * 4;
        let len = copy_w * 4;
        if let (Some(from), Some(to)) = (
            frame.get(src..src.saturating_add(len)),
            out.get_mut(dst..dst.saturating_add(len)),
        ) {
            to.copy_from_slice(from);
        }
    }
    out
}

/// Decode in process, one frame at a time into `each`; the frames a second.
/// `Ok(None)` when the decoder is not installed here.
pub(super) fn decode_in_process(
    path: &Path,
    edge: usize,
    max_frames: usize,
    cancelled: &AtomicBool,
    each: &mut WebmSteps<'_>,
) -> Result<Option<f64>, String> {
    let edge = u32::try_from(edge).unwrap_or(u32::MAX);
    let mut decoder = match StickerDecoder::open(path, edge) {
        Ok(decoder) => decoder,
        Err(OpenError::Unavailable) => return Ok(None),
        Err(OpenError::Failed(reason)) => {
            return Err(format!("Could not read video sticker ({reason})"));
        }
    };
    let info = decoder.info();
    let max_frames = max_frames.max(1);
    let fps = sample_fps(info.duration, max_frames)?;
    let mut plan = FramePlan::new(fps, max_frames, info.duration);
    let mut any = false;
    while !plan.full() {
        if cancelled.load(Ordering::SeqCst) {
            return Err("Sticker playback cancelled".into());
        }
        let Some(pts) = decoder.next()? else {
            break;
        };
        let placement = plan.place(pts);
        if placement.repeat_previous > 0 {
            each(WebmStep::Repeat(placement.repeat_previous))?;
        }
        if !placement.take {
            continue;
        }
        let (width, height, bgra) = decoder.pixels()?;
        let bgra = if (width, height) == (info.width, info.height) {
            bgra
        } else {
            // The stream changed size: keep every frame one size.
            center_into(&bgra, width, height, info.width, info.height)
        };
        each(WebmStep::Frame(WebmFrame {
            width: info.width,
            height: info.height,
            bgra,
        }))?;
        any = true;
    }
    let tail = plan.tail();
    if tail > 0 {
        each(WebmStep::Repeat(tail))?;
    }
    if !any {
        return Err("Video sticker has no frames".into());
    }
    Ok(Some(fps))
}

/// Decode with the `ffmpeg` binary: PNG frames under `cache`, read back one
/// at a time into `each` and removed; the frames a second. The fallback
/// when the bundled decoder is not installed.
pub(super) fn decode_with_ffmpeg(
    path: &Path,
    cache: &Path,
    edge: usize,
    max_frames: usize,
    child: &std::sync::Arc<std::sync::Mutex<Option<std::process::Child>>>,
    cancelled: &AtomicBool,
    each: &mut WebmSteps<'_>,
) -> Result<f64, String> {
    let probe =
        crate::video::probe_with_ffprobe(path, false).ok_or("Could not read video sticker")?;
    let max_frames = max_frames.max(1);
    let fps = sample_fps(Some(f64::from(probe.duration)), max_frames)?;
    // VP9's native ffmpeg decoder drops alpha; libvpx preserves the sticker surface.
    let read = crate::video::extract_frames(
        path,
        cache,
        fps,
        i32::try_from(edge).unwrap_or(i32::MAX),
        i32::try_from(max_frames).unwrap_or(i32::MAX),
        Some((child, cancelled)),
        Some("libvpx-vp9"),
    )
    .and_then(|paths| read_png_frames(&paths, cancelled, each));
    let _ = std::fs::remove_dir_all(cache);
    read?;
    Ok(fps)
}

/// The PNG frames as BGRA into `each`, all the size of the first.
#[cfg(feature = "ui")]
fn read_png_frames(
    paths: &[std::path::PathBuf],
    cancelled: &AtomicBool,
    each: &mut WebmSteps<'_>,
) -> Result<(), String> {
    let mut size = None;
    for path in paths {
        if cancelled.load(Ordering::SeqCst) {
            return Err("Sticker playback cancelled".into());
        }
        let rgba = image::open(path).map_err(|e| e.to_string())?.into_rgba8();
        let (width, height) = rgba.dimensions();
        let mut bgra = rgba.into_raw();
        for pixel in bgra.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        let (out_w, out_h) = *size.get_or_insert((width, height));
        each(WebmStep::Frame(WebmFrame {
            width: out_w,
            height: out_h,
            bgra: if (width, height) == (out_w, out_h) {
                bgra
            } else {
                center_into(&bgra, width, height, out_w, out_h)
            },
        }))?;
    }
    if size.is_none() {
        return Err("Video sticker has no frames".into());
    }
    Ok(())
}

/// Without the UI build there is no PNG reader.
#[cfg(not(feature = "ui"))]
fn read_png_frames(
    _paths: &[std::path::PathBuf],
    _cancelled: &AtomicBool,
    _each: &mut WebmSteps<'_>,
) -> Result<(), String> {
    Err("Video stickers need the bundled decoder".into())
}

#[cfg(test)]
mod tests;
