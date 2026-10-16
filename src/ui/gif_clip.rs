//! A true `image/gif` file decoded in process (the `image` crate's GIF
//! decoder, the one GPUI's own `img` uses for the viewer) for the history's
//! looping GIF. A frame that repeats for several ticks is held once with its
//! duration, so a long GIF with still stretches does not keep N copies of
//! the same picture. MP4 "GIFs" play through the video player instead.
#![deny(clippy::indexing_slicing)]

use gpui_kit::RenderImage;
use image::{AnimationDecoder, RgbaImage};
use smallvec::SmallVec;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// Frames are scaled to fit this many px on the longer side (the history
/// row shows a GIF at most 240 px).
const MAX_EDGE: u32 = 320;
/// A GIF's canvas may not be larger than this on a side.
const MAX_CANVAS: u32 = 8192;
/// Decoding stops after this many distinct held frames.
const MAX_HELD: usize = 600;
/// Decoding stops once the held pictures take this many bytes.
const MAX_BYTES: usize = 48 * 1024 * 1024;
/// Delays of 10 ms and less play at 100 ms, as browsers do.
const FAST_DELAY: Duration = Duration::from_millis(10);
const DEFAULT_DELAY: Duration = Duration::from_millis(100);

struct Held {
    image: Arc<RenderImage>,
    /// When this picture stops showing, from the start of the loop.
    ends: Duration,
}

/// The distinct pictures of a looping GIF and how long each shows.
pub(super) struct GifClip {
    frames: Vec<Held>,
}

/// Pictures collected for a clip, merging a picture equal to the one before.
#[derive(Default)]
struct ClipBuilder {
    pictures: Vec<(RgbaImage, Duration)>,
    bytes: usize,
}

impl ClipBuilder {
    /// Add a picture shown for `delay`; equal to the last, it extends that
    /// one instead. Returns whether more pictures fit.
    fn push(&mut self, picture: RgbaImage, delay: Duration) -> bool {
        if let Some((last, shown)) = self.pictures.last_mut()
            && *last == picture
        {
            *shown += delay;
            return true;
        }
        let size = picture.as_raw().len();
        if self.pictures.len() >= MAX_HELD || self.bytes.saturating_add(size) > MAX_BYTES {
            return false;
        }
        self.bytes += size;
        self.pictures.push((picture, delay));
        true
    }

    fn finish(self) -> Option<GifClip> {
        let mut ends = Duration::ZERO;
        let frames: Vec<Held> = self
            .pictures
            .into_iter()
            .map(|(mut picture, shown)| {
                ends += shown;
                // RenderImage holds BGRA.
                for pixel in picture.as_chunks_mut::<4>().0 {
                    pixel.swap(0, 2);
                }
                Held {
                    image: Arc::new(RenderImage::new(SmallVec::from_buf([image::Frame::new(
                        picture,
                    )]))),
                    ends,
                }
            })
            .collect();
        (!frames.is_empty()).then_some(GifClip { frames })
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

fn fitted(picture: RgbaImage) -> RgbaImage {
    let (width, height) = picture.dimensions();
    let longer = width.max(height);
    if longer <= MAX_EDGE || longer == 0 {
        return picture;
    }
    let size = |side: u32| {
        let scaled = (u64::from(side) * u64::from(MAX_EDGE) / u64::from(longer)).max(1);
        u32::try_from(scaled).unwrap_or(MAX_EDGE)
    };
    image::imageops::resize(
        &picture,
        size(width),
        size(height),
        image::imageops::FilterType::Triangle,
    )
}

impl GifClip {
    /// Decode the file. Runs off the UI thread.
    pub(super) fn decode(path: &Path) -> Result<Self, String> {
        let file = std::fs::File::open(path).map_err(|err| err.to_string())?;
        let decoder = image::codecs::gif::GifDecoder::new(std::io::BufReader::new(file))
            .map_err(|err| err.to_string())?;
        let (width, height) = image::ImageDecoder::dimensions(&decoder);
        if width == 0 || height == 0 || width > MAX_CANVAS || height > MAX_CANVAS {
            return Err("GIF size is outside the supported range".into());
        }
        let mut builder = ClipBuilder::default();
        for frame in decoder.into_frames() {
            let frame = frame.map_err(|err| err.to_string())?;
            let delay = frame_delay(&frame);
            if !builder.push(fitted(frame.into_buffer()), delay) {
                break;
            }
        }
        builder.finish().ok_or_else(|| "GIF has no frames".into())
    }

    /// How many distinct pictures are held.
    #[cfg(test)]
    pub(super) fn held(&self) -> usize {
        self.frames.len()
    }

    /// Whether the clip never changes (a single held picture).
    pub(super) fn is_still(&self) -> bool {
        self.frames.len() < 2
    }

    /// The loop's length.
    fn total(&self) -> Duration {
        self.frames.last().map_or(Duration::ZERO, |held| held.ends)
    }

    /// The picture showing `elapsed` into the (looping) clip.
    pub(super) fn frame_at(&self, elapsed: Duration) -> Option<&Arc<RenderImage>> {
        let total = self.total().as_nanos();
        let at = elapsed.as_nanos().checked_rem(total).unwrap_or(0);
        self.frames
            .iter()
            .find(|held| held.ends.as_nanos() > at)
            .or(self.frames.last())
            .map(|held| &held.image)
    }

    /// Every held picture, to hand to the atlas sweeper when the clip goes.
    pub(super) fn into_images(self) -> Vec<Arc<RenderImage>> {
        self.frames.into_iter().map(|held| held.image).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{ClipBuilder, GifClip};
    use image::{Delay, Frame, Rgba, RgbaImage};
    use std::sync::Arc;
    use std::time::Duration;

    fn solid(color: [u8; 4]) -> RgbaImage {
        RgbaImage::from_pixel(8, 4, Rgba(color))
    }

    const RED: [u8; 4] = [255, 0, 0, 255];
    const BLUE: [u8; 4] = [0, 0, 255, 255];

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_repeated_picture_is_held_once_with_its_whole_duration() {
        let mut builder = ClipBuilder::default();
        for (color, delay) in [(RED, 100), (RED, 100), (RED, 50), (BLUE, 200), (RED, 100)] {
            assert!(builder.push(solid(color), ms(delay)));
        }
        let clip = builder.finish().expect("clip");
        assert_eq!(clip.held(), 3);
        assert_eq!(clip.total(), ms(550));
        let at = |t| clip.frame_at(ms(t)).cloned().expect("frame");
        // The first red shows for 250 ms, then blue until 450, then red.
        assert!(Arc::ptr_eq(&at(0), &at(249)));
        assert!(!Arc::ptr_eq(&at(249), &at(250)));
        assert!(Arc::ptr_eq(&at(250), &at(449)));
        assert!(!Arc::ptr_eq(&at(449), &at(450)));
        // It loops.
        assert!(Arc::ptr_eq(&at(0), &at(550)));
        assert!(Arc::ptr_eq(&at(250), &at(550 + 300)));
    }

    #[test]
    fn a_decoded_gif_holds_repeated_frames_once() {
        let mut bytes = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
            let frames = [RED, RED, RED, BLUE, BLUE].map(|color| {
                Frame::from_parts(solid(color), 0, 0, Delay::from_numer_denom_ms(100, 1))
            });
            encoder.encode_frames(frames).expect("encode");
        }
        let dir = std::env::temp_dir().join(format!("quill-gif-clip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("held.gif");
        std::fs::write(&path, &bytes).expect("write");
        let clip = GifClip::decode(&path).expect("decode");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(clip.held(), 2);
        assert!(!clip.is_still());
        assert_eq!(clip.total(), ms(500));
    }

    #[test]
    fn the_held_pictures_are_bounded() {
        let mut builder = ClipBuilder::default();
        let mut fitted_all = true;
        for n in 0..super::MAX_HELD + 5 {
            let shade = u8::try_from(n % 250).unwrap_or(0);
            let tint = u8::try_from(n / 250).unwrap_or(0);
            fitted_all &= builder.push(solid([shade, tint, 7, 255]), ms(40));
        }
        assert!(!fitted_all);
        assert_eq!(builder.pictures.len(), super::MAX_HELD);
    }
}
