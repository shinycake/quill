use super::*;
use crate::calls::engine::i420_to_rgba;
use ntgcalls_sys::{
    NTG_VIDEO_ROTATION_VIDEO_ROTATION_0, NTG_VIDEO_ROTATION_VIDEO_ROTATION_90,
    NTG_VIDEO_ROTATION_VIDEO_ROTATION_180, NTG_VIDEO_ROTATION_VIDEO_ROTATION_270,
};

/// A frame where every plane value differs: a gradient in Y, U and V.
fn gradient(width: u16, height: u16) -> Vec<u8> {
    let (w, h) = (usize::from(width), usize::from(height));
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    let mut planes = Vec::new();
    for y in 0..h {
        for x in 0..w {
            planes.push(((x * 7 + y * 13) % 256) as u8);
        }
    }
    for y in 0..ch {
        for x in 0..cw {
            planes.push(((x * 31 + y * 5 + 40) % 256) as u8);
        }
    }
    for y in 0..ch {
        for x in 0..cw {
            planes.push(((x * 11 + y * 23 + 90) % 256) as u8);
        }
    }
    planes
}

fn swap_to_bgra(mut rgba: Vec<u8>) -> Vec<u8> {
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    rgba
}

#[test]
fn bgra_matches_the_engine_conversion_pixel_for_pixel() {
    for (w, h) in [(64u16, 36u16), (17, 9), (2, 2), (1, 1)] {
        let planes = gradient(w, h);
        let frame = I420::new(&planes, w, h).expect("a frame");
        let expected = swap_to_bgra(
            i420_to_rgba(w, h, &planes, NTG_VIDEO_ROTATION_VIDEO_ROTATION_0).expect("rgba"),
        );
        assert_eq!(i420_to_bgra(&frame).expect("bgra"), expected, "{w}x{h}");
    }
}

#[test]
fn turned_planes_match_the_engine_turning_rgba() {
    let cases = [
        (LiveRotation::R90, NTG_VIDEO_ROTATION_VIDEO_ROTATION_90),
        (LiveRotation::R180, NTG_VIDEO_ROTATION_VIDEO_ROTATION_180),
        (LiveRotation::R270, NTG_VIDEO_ROTATION_VIDEO_ROTATION_270),
    ];
    let (w, h) = (24u16, 10u16);
    let planes = gradient(w, h);
    let frame = I420::new(&planes, w, h).expect("a frame");
    for (rotation, native) in cases {
        let (turned, tw, th) = rotate_planes(&frame, rotation).expect("turned");
        assert_eq!(
            (tw, th),
            if rotation.swaps_sides() {
                (h, w)
            } else {
                (w, h)
            }
        );
        let turned = I420::new(&turned, tw, th).expect("a turned frame");
        let expected = swap_to_bgra(i420_to_rgba(w, h, &planes, native).expect("rgba"));
        assert_eq!(
            i420_to_bgra(&turned).expect("bgra"),
            expected,
            "{rotation:?}"
        );
    }
    assert!(rotate_planes(&frame, LiveRotation::None).is_none());
}

#[test]
fn turning_four_quarters_is_the_identity() {
    let (w, h) = (8u16, 6u16);
    let planes = gradient(w, h);
    let mut current = (planes.clone(), w, h);
    for _ in 0..4 {
        let frame = I420::new(&current.0, current.1, current.2).expect("frame");
        current = rotate_planes(&frame, LiveRotation::R90).expect("turned");
    }
    assert_eq!(current, (planes, w, h));
}

#[test]
fn nv12_copies_luma_and_interleaves_chroma_at_any_stride() {
    let (w, h) = (6u16, 4u16);
    let planes = gradient(w, h);
    let frame = I420::new(&planes, w, h).expect("frame");
    let (y_stride, uv_stride) = (8, 8);
    let mut y = vec![0xAA; y_stride * 4];
    let mut uv = vec![0xAA; uv_stride * 2];
    assert!(i420_to_nv12(&frame, &mut y, y_stride, &mut uv, uv_stride));
    for row in 0..4 {
        assert_eq!(
            &y[row * y_stride..row * y_stride + 6],
            &frame.y[row * 6..row * 6 + 6]
        );
        // The padding is left alone.
        assert_eq!(&y[row * y_stride + 6..row * y_stride + 8], &[0xAA, 0xAA]);
    }
    for row in 0..2 {
        for col in 0..3 {
            assert_eq!(uv[row * uv_stride + col * 2], frame.u[row * 3 + col]);
            assert_eq!(uv[row * uv_stride + col * 2 + 1], frame.v[row * 3 + col]);
        }
    }
    // Too small a destination writes nothing past it and says so.
    let mut short = vec![0u8; 10];
    assert!(!i420_to_nv12(&frame, &mut short, 6, &mut uv, uv_stride));
    assert!(!i420_to_nv12(&frame, &mut y, 4, &mut uv, uv_stride));
}

#[test]
fn a_short_or_empty_buffer_is_not_a_frame() {
    assert!(I420::new(&[0; 5], 2, 2).is_none());
    assert!(I420::new(&[], 0, 0).is_none());
    assert!(I420::new(&[0; 6], 2, 2).is_some());
}

#[test]
fn rgba_round_trips_through_i420() {
    let (w, h) = (4u16, 2u16);
    let rgba: Vec<u8> = [[200, 40, 40, 255]; 8].concat();
    let planes = rgba_to_i420(&rgba, w, h).expect("i420");
    let back = i420_to_bgra(&I420::new(&planes, w, h).expect("frame")).expect("bgra");
    for pixel in back.as_chunks::<4>().0 {
        // BGRA within rounding of the red it came from.
        assert!((i32::from(pixel[2]) - 200).abs() <= 2, "{pixel:?}");
        assert!((i32::from(pixel[1]) - 40).abs() <= 2, "{pixel:?}");
        assert!((i32::from(pixel[0]) - 40).abs() <= 2, "{pixel:?}");
    }
    assert!(rgba_to_i420(&rgba, 3, 3).is_none());
}

#[test]
fn the_paused_picture_fits_ninety_pixels_and_turns() {
    let planes = gradient(320, 180);
    let frame = I420::new(&planes, 320, 180).expect("frame");
    let raster = paused_raster(&frame, LiveRotation::None).expect("raster");
    assert_eq!((raster.width, raster.height), (90, 51));
    assert_eq!(raster.bgra.len(), 90 * 51 * 4);
    let turned = paused_raster(&frame, LiveRotation::R90).expect("raster");
    assert_eq!((turned.width, turned.height), (51, 90));
    let tall = gradient(100, 400);
    let raster = paused_raster(
        &I420::new(&tall, 100, 400).expect("frame"),
        LiveRotation::None,
    )
    .expect("raster");
    assert_eq!((raster.width, raster.height), (23, 90));
}

#[test]
fn the_test_picture_is_a_frame_and_moves() {
    let first = gradient_frame(64, 36, 0);
    assert_eq!(first.len(), i420_size(64, 36));
    assert_ne!(first, gradient_frame(64, 36, 10));
    assert_eq!(gradient_frame(7, 5, 3).len(), i420_size(7, 5));
}

/// The median of `runs` timings of `work`, in microseconds.
fn median_us(runs: usize, mut work: impl FnMut()) -> u128 {
    let mut times: Vec<u128> = (0..runs)
        .map(|_| {
            let started = std::time::Instant::now();
            work();
            started.elapsed().as_micros()
        })
        .collect();
    times.sort_unstable();
    times.get(runs / 2).copied().unwrap_or(0)
}

/// What a 1080p frame costs before and after the live player, per step
/// (`docs/decisions/video-player-live.md`). Run in release:
/// `cargo test --release --no-default-features --lib -- --ignored
/// measure_1080p --nocapture`.
#[test]
#[ignore = "a measurement, not a check"]
fn measure_1080p_frame_costs() {
    use std::hint::black_box;
    let (w, h) = (1920u16, 1080u16);
    let ntgcalls = gradient(w, h);
    let runs = 61;
    // Engine thread, before: copy out of ntgcalls, then I420 to RGBA.
    let old_engine = median_us(runs, || {
        let bytes = black_box(&ntgcalls).to_vec();
        black_box(i420_to_rgba(
            w,
            h,
            &bytes,
            NTG_VIDEO_ROTATION_VIDEO_ROTATION_0,
        ));
    });
    let rgba = i420_to_rgba(w, h, &ntgcalls, NTG_VIDEO_ROTATION_VIDEO_ROTATION_0).expect("rgba");
    // UI side, before (the former `stream_picture::frame_image`, gone with
    // the live backend): clone and swap to BGRA.
    let old_ui = median_us(runs, || {
        let mut bgra = black_box(&rgba).clone();
        for pixel in bgra.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        black_box(bgra);
    });
    // Engine thread, after: the copy only.
    let new_engine = median_us(runs, || {
        black_box(black_box(&ntgcalls).to_vec());
    });
    let frame = I420::new(&ntgcalls, w, h).expect("frame");
    // Background executor, after, macOS: into an NV12 buffer (a pool
    // buffer's rows are 64-byte aligned).
    let stride = usize::from(w).next_multiple_of(64);
    let mut y = vec![0u8; stride * usize::from(h)];
    let mut uv = vec![0u8; stride * usize::from(h).div_ceil(2)];
    let new_nv12 = median_us(runs, || {
        black_box(i420_to_nv12(&frame, &mut y, stride, &mut uv, stride));
    });
    // Background executor, after, Windows and Linux: into BGRA.
    let new_bgra = median_us(runs, || {
        black_box(i420_to_bgra(&frame));
    });
    // A 640 × 360 tile converts on the UI thread, during the draw.
    let (tw, th) = (640u16, 360u16);
    let tile = gradient(tw, th);
    let tile = I420::new(&tile, tw, th).expect("tile");
    let tile_stride = usize::from(tw).next_multiple_of(64);
    let mut tile_y = vec![0u8; tile_stride * usize::from(th)];
    let mut tile_uv = vec![0u8; tile_stride * usize::from(th).div_ceil(2)];
    let tile_nv12 = median_us(runs, || {
        black_box(i420_to_nv12(
            &tile,
            &mut tile_y,
            tile_stride,
            &mut tile_uv,
            tile_stride,
        ));
    });
    let tile_bgra = median_us(runs, || {
        black_box(i420_to_bgra(&tile));
    });
    let tile_planes = gradient(tw, th);
    let old_tile_engine = median_us(runs, || {
        black_box(i420_to_rgba(
            tw,
            th,
            &tile_planes,
            NTG_VIDEO_ROTATION_VIDEO_ROTATION_0,
        ));
    });
    let tile_rgba =
        i420_to_rgba(tw, th, &tile_planes, NTG_VIDEO_ROTATION_VIDEO_ROTATION_0).expect("rgba");
    let old_tile_ui = median_us(runs, || {
        let mut bgra = black_box(&tile_rgba).clone();
        for pixel in bgra.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        black_box(bgra);
    });
    eprintln!(
        "MEASURE 640x360 tile before: engine {old_tile_engine} us, UI {old_tile_ui} us; after: nv12 {tile_nv12} us, bgra {tile_bgra} us"
    );
    let paused = median_us(runs, || {
        black_box(paused_raster(&frame, LiveRotation::None));
    });
    eprintln!("MEASURE 1080p paused picture (90 px, blurred): {paused} us");
    let mb = |bytes: usize| bytes as f64 / 1e6;
    let (planes, picture) = (ntgcalls.len(), rgba.len());
    eprintln!("MEASURE 1080p before: engine {old_engine} us, UI {old_ui} us");
    eprintln!(
        "MEASURE 1080p after: engine {new_engine} us, nv12 {new_nv12} us, bgra {new_bgra} us"
    );
    eprintln!(
        "MEASURE bytes written per frame: before {:.1} MB copy + {:.1} MB RGBA + {:.1} MB BGRA; after {:.1} MB copy + {:.1} MB NV12 (macOS) or {:.1} MB BGRA",
        mb(planes),
        mb(picture),
        mb(picture),
        mb(planes),
        mb(y.len() + uv.len()),
        mb(picture)
    );
}
