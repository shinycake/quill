//! Bounded sticker decoding; GPUI owns the display clock.
#![deny(clippy::indexing_slicing)]
use std::io::Read;
use std::path::Path;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

mod thorvg;
mod tint;
mod webm;
mod webm_ffi;
pub use tint::Tint;
pub use webm::WebmFrames;

pub const STICKER_EDGE: usize = 128;
pub const MAX_STICKER_FRAMES: usize = 120;
/// Playback rate cap for stickers and custom emoji drawn in the chat
/// (Lottie files are usually 60 fps). At sticker and emoji sizes 30 fps
/// looks the same and halves the frames held and the redraws.
pub const MAX_PLAYBACK_FPS: f64 = 30.0;

/// How many evenly spaced frames to render of a `total`-frame animation
/// at `fps`: at most `max_frames`, and no more than `max_fps` needs.
pub fn sampled_frame_count(total: usize, fps: f64, max_frames: usize, max_fps: f64) -> usize {
    let by_rate = if fps > max_fps && max_fps > 0.0 {
        (total as f64 * max_fps / fps).ceil() as usize
    } else {
        total
    };
    total.min(by_rate).min(max_frames).max(1)
}

pub struct StickerFrames {
    /// Straight-alpha BGRA, GPUI's RenderImage format.
    pub frames: Vec<Vec<u8>>,
    pub fps: f64,
}

/// A TGS file's Lottie JSON, checked.
struct Tgs {
    json: Vec<u8>,
    fps: f64,
}

fn tgs_json(path: &Path) -> Result<Tgs, String> {
    let file = std::fs::File::open(path).map_err(|_| "Could not read animated sticker")?;
    if file
        .metadata()
        .map_err(|_| "Could not read animated sticker")?
        .len()
        > 1_048_576
    {
        return Err("Animated sticker is too large".into());
    }
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(file)
        .take(8_388_609)
        .read_to_end(&mut bytes)
        .map_err(|_| "Invalid animated sticker compression")?;
    if bytes.len() > 8_388_608 {
        return Err("Animated sticker is too large".into());
    }
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| "Invalid animated sticker")?;
    for dimension in ["w", "h"] {
        if !value
            .get(dimension)
            .and_then(|v| v.as_u64())
            .is_some_and(|n| (1..=4096).contains(&n))
        {
            return Err("Invalid animated sticker size".into());
        }
    }
    // TGS is vector-only. Never let native decoding read external image resources.
    if value
        .get("assets")
        .and_then(|v| v.as_array())
        .is_some_and(|assets| {
            assets
                .iter()
                .any(|asset| asset.get("p").is_some() || asset.get("u").is_some())
        })
    {
        return Err("External images are not allowed in animated stickers".into());
    }
    let fps = value.get("fr").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let start = value.get("ip").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let end = value.get("op").and_then(|v| v.as_f64()).unwrap_or(0.0);
    if !(1.0..=120.0).contains(&fps)
        || !(0.0..=3600.0).contains(&start)
        || end <= start
        || end > 7200.0
        || (end - start) / fps > 30.0
    {
        return Err("Invalid animated sticker duration".into());
    }
    Ok(Tgs { json: bytes, fps })
}

pub fn decode_tgs(path: &Path, cancelled: &AtomicBool) -> Result<StickerFrames, String> {
    decode_tgs_sized(
        path,
        STICKER_EDGE,
        MAX_STICKER_FRAMES,
        MAX_PLAYBACK_FPS,
        cancelled,
    )
}

/// [`decode_tgs`] at `edge` × `edge` px with at most `max_frames` frames
/// played at most `max_fps` (custom emoji decode smaller and shorter).
pub fn decode_tgs_sized(
    path: &Path,
    edge: usize,
    max_frames: usize,
    max_fps: f64,
    cancelled: &AtomicBool,
) -> Result<StickerFrames, String> {
    let tgs = tgs_json(path)?;
    if cancelled.load(Ordering::SeqCst) {
        return Err("Sticker playback cancelled".into());
    }
    let mut lottie = thorvg::Lottie::load(&tgs.json, edge)?;
    let total_frames = lottie.total_frames();
    if !(1.0..=7200.0).contains(&total_frames) || f64::from(total_frames) / tgs.fps > 30.0 {
        return Err("Invalid animated sticker duration".into());
    }
    let total = total_frames as usize;
    let count = sampled_frame_count(total, tgs.fps, max_frames, max_fps);
    let mut frames = Vec::with_capacity(count);
    for index in 0..count {
        if cancelled.load(Ordering::SeqCst) {
            return Err("Sticker playback cancelled".into());
        }
        // Evenly spaced; ThorVG interpolates between whole frames.
        frames.push(lottie.render(index as f32 * total as f32 / count as f32)?);
    }
    Ok(StickerFrames {
        frames,
        fps: tgs.fps * count as f64 / total as f64,
    })
}

/// Video sticker frames at [`STICKER_EDGE`] (see [`decode_webm_sized`]).
pub fn decode_webm(
    path: &Path,
    cache: &Path,
    child: &std::sync::Arc<Mutex<Option<std::process::Child>>>,
    cancelled: &AtomicBool,
) -> Result<WebmFrames, String> {
    decode_webm_sized(
        path,
        cache,
        STICKER_EDGE,
        MAX_STICKER_FRAMES,
        child,
        cancelled,
    )
}

/// A WebM video sticker fit into `edge` × `edge` px, with at most
/// `max_frames` frames. Decoded in process by the bundled decoder when it
/// is installed; otherwise by the `ffmpeg` binary, with PNG frames under
/// `cache` (removed again) and the running process in `child` so the UI
/// can stop it.
pub fn decode_webm_sized(
    path: &Path,
    cache: &Path,
    edge: usize,
    max_frames: usize,
    child: &std::sync::Arc<Mutex<Option<std::process::Child>>>,
    cancelled: &AtomicBool,
) -> Result<WebmFrames, String> {
    if let Some(frames) = webm::decode_in_process(path, edge, max_frames, cancelled)? {
        return Ok(frames);
    }
    webm::decode_with_ffmpeg(path, cache, edge, max_frames, child, cancelled)
}

#[cfg(test)]
mod tests;
