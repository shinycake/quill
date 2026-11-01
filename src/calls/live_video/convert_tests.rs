use super::*;

/// Phase C2e: I420 planar byte size: Y (w*h) + U (cw*ch) + V (cw*ch).
fn i420_frame_size(width: u16, height: u16) -> usize {
    let (w, h) = (usize::from(width), usize::from(height));
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    w * h + 2 * cw * ch
}

/// The engine's former conversion (`i420_to_rgba`, gone
/// since every feed carries I420 planes), kept as the colour and rotation
/// oracle: an I420 frame to RGBA8 with BT.601 full-range math,
/// then apply the frame rotation. Returns `None` on invalid input so we
/// never render garbage bytes.
fn i420_to_rgba(width: u16, height: u16, yuv: &[u8], rotation: LiveRotation) -> Option<Vec<u8>> {
    if width == 0 || height == 0 || yuv.len() != i420_frame_size(width, height) {
        return None;
    }
    let (w, h) = (usize::from(width), usize::from(height));
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    let (y_plane, uv) = yuv.split_at(w * h);
    let (u_plane, v_plane) = uv.split_at(cw * ch);

    let mut rgba = vec![0u8; w * h * 4];
    for (row, out_row) in rgba.chunks_exact_mut(w * 4).enumerate() {
        for (col, px) in out_row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let chroma = (row / 2) * cw + col / 2;
            let (Some(&y), Some(&u), Some(&v), [pr, pg, pb, pa]) = (
                y_plane.get(row * w + col),
                u_plane.get(chroma),
                v_plane.get(chroma),
                px,
            ) else {
                continue;
            };
            let (y, u, v) = (f32::from(y), f32::from(u), f32::from(v));
            let r = y + 1.402 * (v - 128.0);
            let g = y - 0.344_136 * (u - 128.0) - 0.714_136 * (v - 128.0);
            let b = y + 1.772 * (u - 128.0);
            *pr = r.clamp(0.0, 255.0).round() as u8;
            *pg = g.clamp(0.0, 255.0).round() as u8;
            *pb = b.clamp(0.0, 255.0).round() as u8;
            *pa = 255;
        }
    }
    Some(match rotation {
        LiveRotation::R90 | LiveRotation::R180 | LiveRotation::R270 => {
            rotate_rgba(&rgba, w, h, rotation)
        }
        _ => rgba,
    })
}

/// Phase C2e: rotate an RGBA8 buffer (row-major, `w` x `h`) by the given
/// frame rotation; 90/270 swap the dimensions.
fn rotate_rgba(src: &[u8], w: usize, h: usize, rotation: LiveRotation) -> Vec<u8> {
    let (dw, dh) = match rotation {
        LiveRotation::R90 | LiveRotation::R270 => (h, w),
        _ => (w, h),
    };
    let mut dst = vec![0u8; src.len()];
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = match rotation {
                // 90° clockwise: the left column lands on top.
                LiveRotation::R90 => (h - 1 - y, x),
                LiveRotation::R180 => (w - 1 - x, h - 1 - y),
                LiveRotation::R270 => (y, w - 1 - x),
                _ => (x, y),
            };
            let src_off = (y * w + x) * 4;
            let dst_off = (dy * dw + dx) * 4;
            if let (Some(to), Some(from)) = (
                dst.get_mut(dst_off..dst_off + 4),
                src.get(src_off..src_off + 4),
            ) {
                to.copy_from_slice(from);
            }
        }
    }
    debug_assert_eq!(dw * dh, w * h);
    dst
}

/// Phase C2e: validates the BT.601 conversion math. Neutral chroma
/// passes Y straight through, so Y=235 ~ white and Y=16 ~ black.
/// A saturated case with no clamping on red pins the 1.402 coefficient:
/// Y=64, U=128, V=255 gives R = 64 + 1.402*127 ~ 242.
#[test]
fn i420_to_rgba_known_pixels() {
    let white = i420_to_rgba(2, 2, &[235, 235, 235, 235, 128, 128], LiveRotation::None)
        .expect("valid 2x2 frame");
    assert_eq!(white.len(), 16);
    let (pixels, _) = white.as_chunks::<4>();
    for pixel in pixels {
        assert_pixel_close(pixel, &[235, 235, 235, 255]);
    }
    let black = i420_to_rgba(2, 2, &[16, 16, 16, 16, 128, 128], LiveRotation::None)
        .expect("valid 2x2 frame");
    let (pixels, _) = black.as_chunks::<4>();
    for pixel in pixels {
        assert_pixel_close(pixel, &[16, 16, 16, 255]);
    }
    // I420 planes: Y(2x2) then U(1x1) then V(1x1).
    let saturated = i420_to_rgba(2, 2, &[64, 64, 64, 64, 128, 255], LiveRotation::None)
        .expect("valid 2x2 frame");
    let (pixels, _) = saturated.as_chunks::<4>();
    for pixel in pixels {
        assert_pixel_close(pixel, &[242, 0, 64, 255]);
    }
}

fn assert_pixel_close(actual: &[u8], expected: &[u8; 4]) {
    assert_eq!(actual.len(), 4);
    for (index, (&a, &e)) in actual.iter().zip(expected.iter()).enumerate() {
        assert!(
            (i16::from(a) - i16::from(e)).abs() <= 2,
            "channel {index}: got {a}, want ~{e}"
        );
    }
}

/// Phase C2e: validates we never render garbage bytes.
#[test]
fn i420_to_rgba_rejects_bad_sizes() {
    assert_eq!(i420_to_rgba(2, 2, &[0; 5], LiveRotation::None), None);
    assert_eq!(i420_to_rgba(2, 2, &[0; 7], LiveRotation::None), None);
    assert_eq!(i420_to_rgba(0, 2, &[], LiveRotation::None), None);
    assert_eq!(i420_to_rgba(2, 0, &[], LiveRotation::None), None);
}

/// Phase C2e: validates rotation handling. 2x1 with distinct pixels,
/// rotated 90° clockwise, becomes 1x2 with the left column on top.
#[test]
fn i420_to_rgba_rotation_90() {
    let rgba =
        i420_to_rgba(2, 1, &[235, 16, 128, 128], LiveRotation::R90).expect("valid 2x1 frame");
    assert_eq!(
        rgba,
        vec![235, 235, 235, 255, 16, 16, 16, 255],
        "white pixel (left column) lands on top"
    );
}

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
        let expected = swap_to_bgra(i420_to_rgba(w, h, &planes, LiveRotation::None).expect("rgba"));
        assert_eq!(i420_to_bgra(&frame).expect("bgra"), expected, "{w}x{h}");
    }
}

#[test]
fn turned_planes_match_the_engine_turning_rgba() {
    let cases = [LiveRotation::R90, LiveRotation::R180, LiveRotation::R270];
    let (w, h) = (24u16, 10u16);
    let planes = gradient(w, h);
    let frame = I420::new(&planes, w, h).expect("a frame");
    for rotation in cases {
        let mut turned = Vec::new();
        let (tw, th) = rotate_planes(&frame, rotation, &mut turned).expect("turned");
        assert_eq!(
            (tw, th),
            if rotation.swaps_sides() {
                (h, w)
            } else {
                (w, h)
            }
        );
        let turned = I420::new(&turned, tw, th).expect("a turned frame");
        let expected = swap_to_bgra(i420_to_rgba(w, h, &planes, rotation).expect("rgba"));
        assert_eq!(
            i420_to_bgra(&turned).expect("bgra"),
            expected,
            "{rotation:?}"
        );
    }
    assert!(rotate_planes(&frame, LiveRotation::None, &mut Vec::new()).is_none());
}

#[test]
fn every_size_turns_pixel_for_pixel() {
    for rotation in [LiveRotation::R90, LiveRotation::R180, LiveRotation::R270] {
        for (w, h) in (1..=26).flat_map(|w| (1..=26).map(move |h| (w, h))) {
            let plane: Vec<u8> = (0..w * h).map(|at| (at % 251) as u8).collect();
            let mut out = vec![0u8; w * h];
            rotate_plane(&plane, w, h, rotation, &mut out);
            let dw = if rotation.swaps_sides() { h } else { w };
            for (y, x) in (0..h).flat_map(|y| (0..w).map(move |x| (y, x))) {
                let (dx, dy) = turned(x, y, w, h, rotation);
                assert_eq!(
                    out[dy * dw + dx],
                    plane[y * w + x],
                    "{rotation:?} {w}x{h} at {x},{y}"
                );
            }
        }
    }
}

#[test]
fn turning_again_reuses_the_buffer() {
    let (w, h) = (24u16, 10u16);
    let planes = gradient(w, h);
    let frame = I420::new(&planes, w, h).expect("a frame");
    let mut out = Vec::new();
    rotate_planes(&frame, LiveRotation::R90, &mut out).expect("turned");
    let first = out.as_ptr();
    rotate_planes(&frame, LiveRotation::R90, &mut out).expect("turned");
    assert_eq!(out.as_ptr(), first);
}

#[test]
fn turning_four_quarters_is_the_identity() {
    let (w, h) = (8u16, 6u16);
    let planes = gradient(w, h);
    let mut current = (planes.clone(), w, h);
    for _ in 0..4 {
        let frame = I420::new(&current.0, current.1, current.2).expect("frame");
        let mut turned = Vec::new();
        let (tw, th) = rotate_planes(&frame, LiveRotation::R90, &mut turned).expect("turned");
        current = (turned, tw, th);
    }
    assert_eq!(current, (planes, w, h));
}

#[test]
fn mirroring_reverses_every_row_of_every_plane() {
    let (w, h) = (6u16, 4u16);
    let planes = gradient(w, h);
    let mut mirrored = planes.clone();
    assert!(mirror_planes(&mut mirrored, w, h));
    let src = I420::new(&planes, w, h).expect("frame");
    let out = I420::new(&mirrored, w, h).expect("mirrored frame");
    for (y, row) in out.y.as_chunks::<6>().0.iter().enumerate() {
        for (x, value) in row.iter().enumerate() {
            assert_eq!(Some(value), src.y.get(y * 6 + (5 - x)), "luma {x},{y}");
        }
    }
    for (plane, source) in [(out.u, src.u), (out.v, src.v)] {
        for (y, row) in plane.as_chunks::<3>().0.iter().enumerate() {
            for (x, value) in row.iter().enumerate() {
                assert_eq!(Some(value), source.get(y * 3 + (2 - x)), "chroma {x},{y}");
            }
        }
    }
    // The same picture as BGRA, mirrored pixel for pixel.
    let bgra = i420_to_bgra(&src).expect("bgra");
    let mirrored_bgra = i420_to_bgra(&out).expect("bgra");
    for (row, mirrored_row) in bgra
        .as_chunks::<24>()
        .0
        .iter()
        .zip(mirrored_bgra.as_chunks::<24>().0)
    {
        let mut reversed = row.as_chunks::<4>().0.to_vec();
        reversed.reverse();
        assert_eq!(mirrored_row.as_chunks::<4>().0, reversed.as_slice());
    }
    assert!(mirror_planes(&mut mirrored, w, h));
    assert_eq!(mirrored, planes, "twice is the identity");
}

#[test]
fn mirroring_leaves_a_malformed_frame_alone() {
    let mut short = vec![7u8; i420_size(4, 4) - 1];
    assert!(!mirror_planes(&mut short, 4, 4));
    assert!(short.iter().all(|value| *value == 7));
    assert!(!mirror_planes(&mut [], 0, 0));
}

#[test]
fn a_mirrored_raster_reverses_its_pixels() {
    let mut raster = Raster {
        width: 2,
        height: 1,
        bgra: vec![1, 2, 3, 255, 4, 5, 6, 255],
    };
    mirror_raster(&mut raster);
    assert_eq!(raster.bgra, vec![4, 5, 6, 255, 1, 2, 3, 255]);
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
    let decoded = gradient(w, h);
    let runs = 61;
    // Engine thread, before: copy out of the engine, then I420 to RGBA.
    let old_engine = median_us(runs, || {
        let bytes = black_box(&decoded).to_vec();
        black_box(i420_to_rgba(w, h, &bytes, LiveRotation::None));
    });
    let rgba = i420_to_rgba(w, h, &decoded, LiveRotation::None).expect("rgba");
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
        black_box(black_box(&decoded).to_vec());
    });
    let frame = I420::new(&decoded, w, h).expect("frame");
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
        black_box(i420_to_rgba(tw, th, &tile_planes, LiveRotation::None));
    });
    let tile_rgba = i420_to_rgba(tw, th, &tile_planes, LiveRotation::None).expect("rgba");
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
    let (planes, picture) = (decoded.len(), rgba.len());
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

#[test]
fn packed_nv12_is_tight_luma_then_interleaved_chroma() {
    // 4 × 2: luma 0..8, U [100, 101], V [200, 201].
    let mut planes: Vec<u8> = (0..8).collect();
    planes.extend([100, 101, 200, 201]);
    let frame = I420::new(&planes, 4, 2).expect("frame");
    assert_eq!(nv12_packed_len(4, 2), Some(12));
    let mut out = vec![0u8; 12];
    assert!(i420_to_nv12_packed(&frame, &mut out));
    assert_eq!(out, [0, 1, 2, 3, 4, 5, 6, 7, 100, 200, 101, 201]);
    // Wider: each row is exactly the width, no padding between rows.
    let (w, h) = (6u16, 4u16);
    let planes = gradient(w, h);
    let frame = I420::new(&planes, w, h).expect("frame");
    let mut out = vec![0u8; nv12_packed_len(6, 4).expect("even")];
    assert!(i420_to_nv12_packed(&frame, &mut out));
    let (luma, chroma) = out.split_at(24);
    assert_eq!(luma, frame.y);
    for (row, pairs) in chroma.as_chunks::<6>().0.iter().enumerate() {
        for (col, pair) in pairs.as_chunks::<2>().0.iter().enumerate() {
            assert_eq!(*pair, [frame.u[row * 3 + col], frame.v[row * 3 + col]]);
        }
    }
}

#[test]
fn packed_nv12_needs_even_sides_and_the_exact_length() {
    for (w, h) in [(3usize, 2usize), (4, 3), (1, 1), (0, 2), (2, 0)] {
        assert_eq!(nv12_packed_len(w, h), None, "{w}x{h}");
    }
    assert_eq!(nv12_packed_len(1920, 1080), Some(1920 * 1080 * 3 / 2));
    assert_eq!(nv12_packed_len(usize::MAX - 1, 2), None, "overflow");
    let even = gradient(4, 2);
    let even = I420::new(&even, 4, 2).expect("frame");
    for len in [0, 11, 13, 16] {
        let mut out = vec![0u8; len];
        assert!(!i420_to_nv12_packed(&even, &mut out), "{len} bytes");
        assert!(out.iter().all(|byte| *byte == 0), "nothing written");
    }
}

#[test]
fn an_odd_frame_packs_without_its_last_column_and_row() {
    assert_eq!(nv12_packed_size(5, 3), Some((4, 2)));
    assert_eq!(nv12_packed_size(1100, 739), Some((1100, 738)));
    assert_eq!(nv12_packed_size(1920, 1080), Some((1920, 1080)));
    for (w, h) in [(1usize, 1usize), (1, 4), (4, 1), (0, 0)] {
        assert_eq!(nv12_packed_size(w, h), None, "{w}x{h}");
    }
    // 5 × 3: luma rows of 5, chroma 3 × 2 (U 100.., V 200..).
    let mut planes: Vec<u8> = (0..15).collect();
    planes.extend([100, 101, 102, 103, 104, 105]);
    planes.extend([200, 201, 202, 203, 204, 205]);
    let odd = I420::new(&planes, 5, 3).expect("frame");
    // Packed as 4 × 2: the full-size buffer is refused.
    let mut wrong = vec![0u8; 5 * 3 * 3 / 2];
    assert!(!i420_to_nv12_packed(&odd, &mut wrong));
    let mut out = vec![0u8; nv12_packed_len(4, 2).expect("even")];
    assert!(i420_to_nv12_packed(&odd, &mut out));
    // Rows 0 and 1 without column 4; chroma row 0 without sample 2.
    assert_eq!(out, [0, 1, 2, 3, 5, 6, 7, 8, 100, 200, 101, 201]);
}

#[test]
fn packed_nv12_of_a_turned_and_mirrored_frame_matches_the_planes() {
    // 4 × 2 turned a quarter (to 2 × 4), then mirrored: the planes the
    // live picture packs, checked pixel by pixel against the turn rule
    // and a row reversal.
    let (w, h) = (4u16, 2u16);
    let planes = gradient(w, h);
    let frame = I420::new(&planes, w, h).expect("frame");
    let mut turned = Vec::new();
    let (tw, th) = rotate_planes(&frame, LiveRotation::R90, &mut turned).expect("turned");
    assert_eq!((tw, th), (2, 4));
    assert!(mirror_planes(&mut turned, tw, th));
    let shown = I420::new(&turned, tw, th).expect("turned frame");
    let mut out = vec![0u8; nv12_packed_len(2, 4).expect("even")];
    assert!(i420_to_nv12_packed(&shown, &mut out));
    let (luma, chroma) = out.split_at(8);
    // Turned 90: (x, y) lands at (h - 1 - y, x); mirrored: x' = tw - 1 - x.
    for y in 0..2usize {
        for x in 0..4usize {
            let (tx, ty) = (1 - y, x);
            let mx = 1 - tx;
            assert_eq!(luma[ty * 2 + mx], frame.y[y * 4 + x], "luma {x},{y}");
        }
    }
    // Chroma: the 2 × 1 grid turned to 1 × 2 (the left block on top); a
    // single column mirrors to itself.
    assert_eq!(chroma, [frame.u[0], frame.v[0], frame.u[1], frame.v[1]]);
}

/// What a live frame costs on Windows before (I420 to BGRA, uploaded to
/// the atlas) and after (I420 to a packed NV12 surface frame), on this
/// machine: the pure functions only
/// (`docs/decisions/video-player-live-nv12.md`). Run in release:
/// `cargo test --release --no-default-features --lib -- --ignored
/// measure_windows_live_nv12 --nocapture`.
#[test]
#[ignore = "a measurement, not a check"]
fn measure_windows_live_nv12() {
    use std::hint::black_box;
    use std::sync::Arc;
    let runs = 61;
    for (w, h) in [(1920u16, 1080u16), (1280, 720), (640, 360)] {
        let planes = gradient(w, h);
        let frame = I420::new(&planes, w, h).expect("frame");
        let len = nv12_packed_len(usize::from(w), usize::from(h)).expect("even");
        // Before: one BGRA buffer a frame.
        let bgra = median_us(runs, || {
            black_box(i420_to_bgra(black_box(&frame)));
        });
        // After, a new buffer a frame (no pool).
        let fresh = median_us(runs, || {
            let mut bytes: Arc<[u8]> = std::iter::repeat_n(0u8, len).collect();
            let out = Arc::get_mut(&mut bytes).expect("unique");
            black_box(i420_to_nv12_packed(black_box(&frame), out));
            black_box(bytes);
        });
        // After, a pooled buffer filled again.
        let mut pooled: Arc<[u8]> = std::iter::repeat_n(0u8, len).collect();
        let reused = median_us(runs, || {
            let out = Arc::get_mut(&mut pooled).expect("unique");
            black_box(i420_to_nv12_packed(black_box(&frame), out));
        });
        // The CPU fallback when the renderer refuses the surface.
        let color = crate::video_decode::YuvColor {
            matrix: crate::video_decode::YuvMatrix::Bt601,
            full_range: true,
        };
        let fallback = median_us(runs, || {
            black_box(crate::video_decode::nv12_to_bgra(
                black_box(&pooled),
                u32::from(w),
                u32::from(h),
                color,
            ));
        });
        let bgra_bytes = usize::from(w) * usize::from(h) * 4;
        eprintln!(
            "MEASURE {w}x{h} before: bgra {bgra} us ({bgra_bytes} B); after: nv12 fresh {fresh} us, nv12 pooled {reused} us ({len} B); refused fallback nv12->bgra {fallback} us"
        );
    }
}
