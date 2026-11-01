//! NV12 output (`docs/decisions/video-player-nv12.md`): plane packing and
//! the shim's layout checks, the colour matrix pick, the CPU fallback
//! conversion, the player passing the asked layout to its decoder, and the
//! real shim's NV12 pictures (skipped where the shim isn't installed).

use super::super::ffi::{FfiDemuxer, PictureLayout, picture_layout};
use super::super::nv12::BufferPool;
use super::super::{
    BgraPool, Demuxer, Item, MediaInfo, OpenOptions, PixelOutput, Pixels, Player, VideoFrame,
    YuvColor, YuvMatrix, nv12_len, nv12_to_bgra, pack_nv12,
};
use super::{Scripted, decoder, fixture, wait_until};
use std::path::Path;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};

fn limited_709() -> YuvColor {
    YuvColor {
        matrix: YuvMatrix::Bt709,
        full_range: false,
    }
}

#[test]
fn nv12_needs_even_sides() {
    assert_eq!(nv12_len(4, 2), Some(12));
    assert_eq!(nv12_len(1920, 1080), Some(1920 * 1080 * 3 / 2));
    assert_eq!(nv12_len(3, 2), None);
    assert_eq!(nv12_len(4, 1), None);
    assert_eq!(nv12_len(0, 2), None);
}

/// `pack_nv12` into a buffer of the picture's size (12 bytes for the 4x2
/// pictures here when the size is refused).
fn pack(
    luma: &[u8],
    luma_stride: usize,
    chroma: &[u8],
    chroma_stride: usize,
    size: (u32, u32),
    rotation: u32,
) -> Option<(Vec<u8>, u32, u32)> {
    let mut out = vec![0u8; nv12_len(size.0, size.1).unwrap_or(12)];
    let (w, h) = pack_nv12(
        luma,
        luma_stride,
        chroma,
        chroma_stride,
        size,
        rotation,
        &mut out,
    )?;
    Some((out, w, h))
}

#[test]
fn a_buffer_is_packed_into_only_once_no_picture_holds_it() {
    let mut pool = BufferPool::default();
    let luma = [1, 2, 3, 4, 5, 6, 7, 8];
    let chroma = [10, 11, 20, 21];
    let fill = |pool: &mut BufferPool| {
        let mut bytes = pool.take(12);
        let out = Arc::get_mut(&mut bytes).expect("nothing else holds it");
        pack_nv12(&luma, 4, &chroma, 4, (4, 2), 0, out).expect("packs");
        pool.keep(&bytes);
        bytes
    };
    let first = fill(&mut pool);
    // The first picture is still shown: the next one gets its own buffer.
    let second = fill(&mut pool);
    assert!(!Arc::ptr_eq(&first, &second));
    assert_eq!(*first, [1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 20, 21]);
    // Once the first is let go, its buffer is filled again.
    let reused = Arc::as_ptr(&first);
    drop(first);
    let third = fill(&mut pool);
    assert_eq!(Arc::as_ptr(&third), reused);
    // A picture of another size drops the old buffers.
    drop((second, third));
    assert_eq!(pool.take(24).len(), 24);
}

#[test]
fn a_buffer_of_the_wrong_length_is_refused() {
    let mut out = [0u8; 11];
    assert!(pack_nv12(&[0; 8], 4, &[0; 4], 4, (4, 2), 0, &mut out).is_none());
}

#[test]
fn nv12_planes_are_packed_without_their_stride_padding() {
    // 4x2 luma with rows 6 bytes apart; one chroma row of two pairs.
    let luma = [1, 2, 3, 4, 0, 0, 5, 6, 7, 8];
    let chroma = [10, 11, 20, 21];
    let (bytes, w, h) = pack(&luma, 6, &chroma, 6, (4, 2), 0).expect("packs");
    assert_eq!((w, h), (4, 2));
    assert_eq!(bytes, [1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 20, 21]);
}

#[test]
fn nv12_planes_turn_upright_with_the_stream() {
    let luma = [1, 2, 3, 4, 5, 6, 7, 8];
    let chroma = [10, 11, 20, 21];
    let (bytes, w, h) = pack(&luma, 4, &chroma, 4, (4, 2), 90).expect("packs");
    assert_eq!((w, h), (2, 4));
    // Clockwise: the left column becomes the top row; chroma pairs move
    // as pairs.
    assert_eq!(bytes, [5, 1, 6, 2, 7, 3, 8, 4, 10, 11, 20, 21]);
    let (half, w, h) = pack(&luma, 4, &chroma, 4, (4, 2), 180).expect("packs");
    assert_eq!((w, h), (4, 2));
    assert_eq!(half, [8, 7, 6, 5, 4, 3, 2, 1, 20, 21, 10, 11]);
}

#[test]
fn nv12_planes_shorter_than_their_rows_are_refused() {
    let luma = [0u8; 8];
    let chroma = [0u8; 4];
    assert!(pack(&luma, 4, &chroma, 4, (4, 2), 0).is_some());
    // Odd sides.
    assert!(pack(&luma, 4, &chroma, 4, (3, 2), 0).is_none());
    assert!(pack(&luma, 4, &chroma, 4, (4, 1), 0).is_none());
    // A stride narrower than a row.
    assert!(pack(&luma, 3, &chroma, 4, (4, 2), 0).is_none());
    assert!(pack(&luma, 4, &chroma, 3, (4, 2), 0).is_none());
    // Planes shorter than their rows.
    let short_luma = luma.get(..7).unwrap_or_default();
    let short_chroma = chroma.get(..3).unwrap_or_default();
    assert!(pack(short_luma, 4, &chroma, 4, (4, 2), 0).is_none());
    assert!(pack(&luma, 4, short_chroma, 4, (4, 2), 0).is_none());
}

#[test]
fn shim_picture_layouts_are_checked_before_any_read() {
    // BGRA: rows may be padded; the last one stops at its pixels.
    assert_eq!(
        picture_layout(0, (2, 3), (12, 0), (false, true)),
        Ok(PictureLayout::Bgra {
            width: 2,
            height: 3,
            stride: 12,
            len: 12 * 2 + 8,
        })
    );
    assert!(picture_layout(0, (2, 3), (7, 0), (false, true)).is_err());
    assert!(picture_layout(0, (2, 3), (8, 0), (true, true)).is_err());
    // NV12: luma rows, then half as many chroma rows of `width` bytes.
    assert_eq!(
        picture_layout(1, (4, 2), (4, 8), (false, false)),
        Ok(PictureLayout::Nv12 {
            width: 4,
            height: 2,
            stride: 4,
            len: 8,
            chroma_stride: 8,
            chroma_len: 4,
        })
    );
    for (size, strides, nulls) in [
        ((3, 2), (4, 4), (false, false)),  // odd width
        ((4, 3), (4, 4), (false, false)),  // odd height
        ((4, 2), (3, 4), (false, false)),  // luma stride too narrow
        ((4, 2), (4, 3), (false, false)),  // chroma stride too narrow
        ((4, 2), (4, 4), (false, true)),   // no chroma plane
        ((4, 2), (4, 4), (true, false)),   // no luma plane
        ((4, 2), (-4, 4), (false, false)), // negative stride
        ((4, 2), (4, -4), (false, false)), // negative chroma stride
        ((0, 2), (4, 4), (false, false)),  // empty
        ((-4, 2), (4, 4), (false, false)), // negative side
    ] {
        assert!(
            picture_layout(1, size, strides, nulls).is_err(),
            "{size:?} {strides:?} {nulls:?}"
        );
    }
    assert!(picture_layout(7, (4, 2), (16, 0), (false, false)).is_err());
}

#[test]
fn the_shim_matrix_code_picks_the_matrix() {
    assert_eq!(YuvMatrix::from_shim(1, 1080), YuvMatrix::Bt601);
    assert_eq!(YuvMatrix::from_shim(2, 480), YuvMatrix::Bt709);
    // Unknown: the untagged rule, BT.709 from 720 rows up, BT.601 below.
    assert_eq!(YuvMatrix::from_shim(0, 720), YuvMatrix::Bt709);
    assert_eq!(YuvMatrix::from_shim(0, 719), YuvMatrix::Bt601);
    assert_eq!(YuvMatrix::from_shim(99, 2160), YuvMatrix::Bt709);
}

#[test]
fn nv12_converts_to_bgra_on_the_cpu() {
    // Left 2x2 block white, right block black, neutral chroma.
    let bytes = [235, 235, 16, 16, 235, 235, 16, 16, 128, 128, 128, 128];
    let bgra = nv12_to_bgra(&bytes, 4, 2, limited_709()).expect("converts");
    assert_eq!(bgra.len(), 4 * 2 * 4);
    assert_eq!(bgra.get(..4), Some(&[255, 255, 255, 255][..]));
    assert_eq!(bgra.get(8..12), Some(&[0, 0, 0, 255][..]));
    // BT.709 red, limited range: R high, G and B low (B, G, R, A order).
    let red = [63, 63, 63, 63, 102, 240];
    let px = nv12_to_bgra(&red, 2, 2, limited_709()).expect("converts");
    let Some(&[b, g, r, a]) = px.get(..4) else {
        panic!("a 2x2 picture has pixels");
    };
    assert!(r >= 252 && g <= 3 && b <= 3 && a == 255, "{b} {g} {r} {a}");
    // The wrong length or an odd side is refused.
    let short = bytes.get(..11).unwrap_or_default();
    assert!(nv12_to_bgra(short, 4, 2, limited_709()).is_none());
    assert!(nv12_to_bgra(&bytes, 3, 2, limited_709()).is_none());
    // A whole frame converts, keeping its time and cost.
    let frame = VideoFrame {
        pts: 1.5,
        width: 4,
        height: 2,
        pixels: Pixels::Nv12 {
            bytes: Arc::from(bytes.to_vec()),
            color: limited_709(),
        },
        convert_us: 7,
    };
    let converted = frame.into_bgra(&BgraPool::default()).expect("converts");
    assert_eq!((converted.pts, converted.convert_us), (1.5, 7));
    assert_eq!(converted.pixels, Pixels::Bgra(bgra));
    // A malformed NV12 frame doesn't.
    let malformed = VideoFrame {
        pts: 0.0,
        width: 4,
        height: 2,
        pixels: Pixels::Nv12 {
            bytes: Arc::from(vec![0u8; 5]),
            color: limited_709(),
        },
        convert_us: 0,
    };
    assert!(malformed.into_bgra(&BgraPool::default()).is_none());
}

/// A scripted clip that records the layouts its player asks for.
struct Recording {
    inner: Scripted,
    outputs: Arc<Mutex<Vec<PixelOutput>>>,
}

impl Demuxer for Recording {
    fn info(&self) -> MediaInfo {
        self.inner.info()
    }

    fn next(&mut self) -> Result<Option<Item>, String> {
        self.inner.next()
    }

    fn seek(&mut self, secs: f64) -> Result<(), String> {
        self.inner.seek(secs)
    }

    fn set_output(&mut self, output: PixelOutput) {
        if let Ok(mut outputs) = self.outputs.lock() {
            outputs.push(output);
        }
    }
}

#[test]
fn the_player_passes_the_asked_layout_to_its_decoder() {
    let outputs = Arc::new(Mutex::new(Vec::new()));
    let recorded = || outputs.lock().map(|o| o.clone()).unwrap_or_default();
    let demuxer = Recording {
        inner: Scripted::new(25.0, None, false, Arc::new(AtomicUsize::new(0))),
        outputs: outputs.clone(),
    };
    let options = OpenOptions {
        output: PixelOutput::Nv12,
        ..OpenOptions::inline(64)
    };
    let mut player = Player::with_demuxer(Box::new(demuxer), options).expect("spawn");
    // Asked once, before the first picture.
    assert!(wait_until(|| player.take_frame().is_some()));
    assert_eq!(recorded(), [PixelOutput::Nv12]);
    assert_eq!(player.output(), PixelOutput::Nv12);
    // Back to BGRA (the renderer stopped drawing NV12): passed on before
    // the next picture is decoded.
    player.set_output(PixelOutput::Bgra);
    assert_eq!(player.output(), PixelOutput::Bgra);
    player.play();
    assert!(wait_until(|| {
        let _ = player.take_frame();
        recorded().len() == 2
    }));
    assert_eq!(recorded(), [PixelOutput::Nv12, PixelOutput::Bgra]);
    // Asking for the layout it already has changes nothing.
    player.set_output(PixelOutput::Bgra);
    assert!(wait_until(|| player.take_frame().is_some()));
    assert_eq!(recorded().len(), 2);
}

/// Every picture of `path` in `output`, straight from the shim.
fn decode_as(path: &Path, options: &OpenOptions, output: PixelOutput) -> Vec<VideoFrame> {
    let mut demuxer = FfiDemuxer::open(path, options).expect("open fixture");
    demuxer.set_output(output);
    let mut frames = Vec::new();
    while let Some(item) = demuxer.next().expect("decode fixture") {
        if let Item::Video(frame) = item {
            frames.push(frame);
        }
    }
    frames
}

/// Mean per-channel difference of two BGRA pictures (alpha left out).
fn mean_difference(a: &[u8], b: &[u8]) -> f64 {
    let mut sum = 0u64;
    let mut count = 0u64;
    for (a, b) in a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0) {
        for (x, y) in a.iter().zip(b).take(3) {
            sum += u64::from(x.abs_diff(*y));
            count += 1;
        }
    }
    sum as f64 / count.max(1) as f64
}

#[test]
fn ffmpeg_hands_over_nv12_that_matches_its_bgra() {
    if !decoder() {
        return;
    }
    let clip = fixture("clip-h264-aac.mp4");
    let bgra_frames = decode_as(&clip, &OpenOptions::viewer(), PixelOutput::Bgra);
    let nv12_frames = decode_as(&clip, &OpenOptions::viewer(), PixelOutput::Nv12);
    assert!(!bgra_frames.is_empty());
    assert_eq!(nv12_frames.len(), bgra_frames.len());
    for (nv12, bgra) in nv12_frames.into_iter().zip(&bgra_frames) {
        assert_eq!((nv12.width, nv12.height), (96, 64));
        let Pixels::Nv12 { bytes, color } = &nv12.pixels else {
            panic!("an h264 yuv420p picture comes out as NV12");
        };
        assert_eq!(bytes.len(), 96 * 64 * 3 / 2);
        // An untagged SD clip in limited range.
        assert_eq!(
            *color,
            YuvColor {
                matrix: YuvMatrix::Bt601,
                full_range: false
            }
        );
        assert!((nv12.pts - bgra.pts).abs() < 1e-9);
        let reference = bgra.pixels.bgra().expect("bgra");
        let wrong = nv12_to_bgra(bytes, 96, 64, limited_709()).expect("converts");
        let wrong_mean = mean_difference(&wrong, reference);
        let converted = nv12.into_bgra(&BgraPool::default()).expect("converts");
        let mean = mean_difference(converted.pixels.bgra().expect("bgra"), reference);
        // The same picture: the CPU conversion takes the nearest chroma
        // sample where swscale interpolates, so testsrc2's sharp colour
        // edges differ a little (about 4 of 255 on average here) ...
        assert!(mean < 6.0, "mean difference {mean}");
        // ... while reading it with the other matrix is about twice as far.
        assert!(wrong_mean > mean * 1.5, "{mean} vs BT.709 {wrong_mean}");
    }
}

#[test]
fn ffmpeg_nv12_scales_and_turns_upright() {
    if !decoder() {
        return;
    }
    // Scaled into the box: swscale resizes straight into NV12.
    let scaled = decode_as(
        &fixture("clip-h264-aac.mp4"),
        &OpenOptions::inline(48),
        PixelOutput::Nv12,
    );
    assert!(!scaled.is_empty());
    for frame in &scaled {
        assert_eq!((frame.width, frame.height), (48, 32));
        assert!(matches!(frame.pixels, Pixels::Nv12 { .. }));
        assert_eq!(frame.pixels.len(), 48 * 32 * 3 / 2);
    }
    // A rotated clip: both planes are turned upright.
    let rotated = decode_as(
        &fixture("clip-rotated.mp4"),
        &OpenOptions::viewer(),
        PixelOutput::Nv12,
    );
    assert!(!rotated.is_empty());
    for frame in &rotated {
        assert_eq!((frame.width, frame.height), (64, 96));
        assert!(matches!(frame.pixels, Pixels::Nv12 { .. }));
        assert_eq!(frame.pixels.len(), 64 * 96 * 3 / 2);
    }
    // VP9 in WebM decodes to yuv420p too.
    let vp9 = decode_as(
        &fixture("clip-vp9.webm"),
        &OpenOptions::viewer(),
        PixelOutput::Nv12,
    );
    assert!(!vp9.is_empty());
    assert!(
        vp9.iter()
            .all(|frame| matches!(frame.pixels, Pixels::Nv12 { .. }))
    );
}

/// The decoder-side cost of each layout on one clip, for the decision
/// doc's measurements (`QUILL_VIDEO_CLIP`, default `site/media/hero-1920.mp4`):
/// `cargo test --release --no-default-features --lib nv12_conversion_cost
/// -- --ignored --nocapture`.
#[test]
#[ignore = "a measurement, not a check"]
fn nv12_conversion_cost() {
    if !decoder() {
        return;
    }
    let clip = std::env::var_os("QUILL_VIDEO_CLIP")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("site/media/hero-1920.mp4"));
    for output in [PixelOutput::Bgra, PixelOutput::Nv12] {
        let started = std::time::Instant::now();
        let frames = decode_as(&clip, &OpenOptions::viewer(), output);
        let wall = started.elapsed();
        let mut convert: Vec<u32> = frames.iter().map(|frame| frame.convert_us).collect();
        convert.sort_unstable();
        let at = |q: f64| {
            convert
                .get(((convert.len() as f64 * q) as usize).min(convert.len().saturating_sub(1)))
                .copied()
                .unwrap_or(0)
        };
        let bytes = frames.first().map_or(0, |frame| frame.pixels.len());
        let (w, h) = frames.first().map_or((0, 0), |f| (f.width, f.height));
        println!(
            "{output:?}: {} pictures {w}x{h}, {bytes} bytes each, convert us p50={} p90={} \
             max={}, decode wall {:.0} ms ({:.2} ms a picture)",
            frames.len(),
            at(0.5),
            at(0.9),
            at(1.0),
            wall.as_secs_f64() * 1000.0,
            wall.as_secs_f64() * 1000.0 / frames.len().max(1) as f64,
        );
    }
}

fn next_picture(demuxer: &mut FfiDemuxer) -> Option<VideoFrame> {
    loop {
        match demuxer.next().expect("decode fixture") {
            Some(Item::Video(frame)) => return Some(frame),
            Some(Item::Audio(_)) => continue,
            None => return None,
        }
    }
}

#[test]
fn ffmpeg_switches_layout_mid_stream() {
    if !decoder() {
        return;
    }
    let mut demuxer = FfiDemuxer::open(&fixture("clip-h264-aac.mp4"), &OpenOptions::viewer())
        .expect("open fixture");
    // BGRA until asked otherwise.
    let first = next_picture(&mut demuxer).expect("a picture");
    assert!(matches!(first.pixels, Pixels::Bgra(_)));
    demuxer.set_output(PixelOutput::Nv12);
    let second = next_picture(&mut demuxer).expect("a picture");
    assert!(matches!(second.pixels, Pixels::Nv12 { .. }));
    demuxer.set_output(PixelOutput::Bgra);
    let third = next_picture(&mut demuxer).expect("a picture");
    assert!(matches!(third.pixels, Pixels::Bgra(_)));
    assert_eq!(third.pixels.len(), 96 * 64 * 4);
}
