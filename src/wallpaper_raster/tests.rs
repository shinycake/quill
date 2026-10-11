use super::*;
use crate::telegram::envelope::BackgroundFill;

fn rgb_at(raster: &Raster, x: u32, y: u32) -> [u8; 3] {
    let [b, g, r, _] = raster.pixel(x, y);
    [r, g, b]
}

#[test]
fn solid_fills_paint_one_colour() {
    let fill = FillSampler::new(&BackgroundFill::Solid(0x123456));
    let out = compose_fill(&fill, 4, 3);
    assert_eq!(out.bgra.len(), 4 * 3 * 4);
    assert_eq!(out.pixel(3, 2), [0x56, 0x34, 0x12, 255]);
}

#[test]
fn linear_gradients_run_corner_to_corner_like_lib_ui() {
    // Rotation 0: top to bottom.
    let down = FillSampler::new(&BackgroundFill::Gradient {
        top: 0x000000,
        bottom: 0xffffff,
        angle: 0,
    });
    let out = compose_fill(&down, 2, 100);
    assert!(rgb_at(&out, 0, 0)[0] < 5);
    assert!(rgb_at(&out, 1, 99)[0] > 250);
    assert_eq!(rgb_at(&out, 0, 50), rgb_at(&out, 1, 50), "rows are flat");
    // Rotation 45: from the top-right to the bottom-left corner.
    let diag = FillSampler::new(&BackgroundFill::Gradient {
        top: 0x000000,
        bottom: 0xffffff,
        angle: 45,
    });
    assert!(diag.sample(1.0, 0.0)[0] < 1.0);
    assert!(diag.sample(0.0, 1.0)[0] > 254.0);
    let mid = diag.sample(0.5, 0.5)[0];
    assert!((mid - 127.5).abs() < 1.0, "{mid}");
    // Rotation 180: bottom to top.
    let up = FillSampler::new(&BackgroundFill::Gradient {
        top: 0x000000,
        bottom: 0xffffff,
        angle: 180,
    });
    assert!(up.sample(0.5, 1.0)[0] < 1.0);
}

#[test]
fn freeform_gradients_blend_all_four_colours() {
    let colors = vec![0xff0000, 0x00ff00, 0x0000ff, 0xffff00];
    let fill = FillSampler::new(&BackgroundFill::Freeform(colors.clone()));
    assert!(matches!(fill, FillSampler::Freeform { side: 64, .. }));
    // The phase-0 points: colour 0 sits at (0.8, 0.9) (y flipped).
    let near_first = fill.sample(0.8, 0.9);
    assert!(near_first[0] > near_first[2], "{near_first:?}");
    let out = compose_fill(&fill, 32, 32);
    let distinct: std::collections::HashSet<[u8; 3]> = (0..32)
        .flat_map(|y| (0..32).map(move |x| (x, y)))
        .map(|(x, y)| rgb_at(&out, x, y))
        .collect();
    assert!(distinct.len() > 50, "a smooth blend, not flat");
    // Two colours fall back to the linear gradient, one to a solid.
    assert!(matches!(
        FillSampler::new(&BackgroundFill::Freeform(vec![1, 2])),
        FillSampler::Linear { .. }
    ));
    assert_eq!(
        FillSampler::new(&BackgroundFill::Freeform(vec![0x010203])),
        FillSampler::Solid([1.0, 2.0, 3.0])
    );
}

#[test]
fn vertical_bubble_gradients_interpolate_between_stops() {
    let colors = [0x000000, 0x646464, 0xc8c8c8];
    assert_eq!(vertical_color_at(&colors, 0.0), 0x000000);
    assert_eq!(vertical_color_at(&colors, 0.5), 0x646464);
    assert_eq!(vertical_color_at(&colors, 1.0), 0xc8c8c8);
    assert_eq!(vertical_color_at(&colors, 0.25), 0x323232);
    assert_eq!(vertical_color_at(&[0xabcdef], 0.7), 0xabcdef);
}

#[test]
fn soft_light_with_black_darkens_light_fills_gently() {
    let mix = PatternMix::new(&[0xffffff], 100, false);
    assert!(!mix.light && !mix.inverted);
    let table = MixTable::new(mix);
    // No coverage: the fill is untouched.
    for dst in [0u8, 77, 200, 255] {
        assert_eq!(table.mix(dst, 0), dst);
    }
    // Full black coverage at full opacity: cb * cb (W3C soft light with a
    // black source), so 200 -> ~157 and white stays white.
    let darker = table.mix(200, 255);
    assert!((150..=160).contains(&darker), "{darker}");
    assert_eq!(table.mix(255, 255), 255);
    // Half intensity moves halfway.
    let half = MixTable::new(PatternMix::new(&[0xffffff], 50, false)).mix(200, 255);
    assert!((174..=180).contains(&half), "{half}");
}

#[test]
fn dark_fills_get_a_white_pattern_that_lightens() {
    let mix = PatternMix::new(&[0x101010, 0x202020], 60, false);
    assert!(mix.light);
    let table = MixTable::new(mix);
    assert!(table.mix(40, 255) > 40);
    assert_eq!(table.mix(40, 0), 40);
    assert!(!pattern_is_light(&[0xffffff], false));
    assert!(!pattern_is_light(&[0x101010], true), "inverted stays black");
}

#[test]
fn inverted_patterns_show_the_fill_through_the_pattern_only() {
    let mix = PatternMix::new(&[0x808080], -40, false);
    assert!(mix.inverted && mix.intensity == 40);
    let table = MixTable::new(mix);
    assert_eq!(table.mix(200, 0), 0, "black outside the pattern");
    assert_eq!(table.mix(200, 255), 80, "fill * intensity inside");
    assert!(PatternMix::new(&[0], 30, true).inverted);
}

#[test]
fn pattern_tiles_keep_their_aspect_and_centre_odd_columns() {
    // A portrait pattern (1440 x 2960) in a 900 px tall area.
    assert_eq!(pattern_tile_size(1440.0, 2960.0, 900), (438, 900));
    assert_eq!(pattern_tile_size(512.0, 512.0, 300), (300, 300));
    assert_eq!(pattern_tile_size(0.0, 10.0, 300), (1, 1));
    assert_eq!(pattern_columns(500, 500), 1);
    assert_eq!(pattern_columns(900, 500), 3);
    assert_eq!(pattern_columns(1600, 500), 5);
    assert_eq!(pattern_columns(100, 0), 1);
}

#[test]
fn composed_patterns_centre_a_tile_and_repeat_down() {
    // A 4 x 2 tile, covered on its left half.
    let tile = Mask {
        width: 4,
        height: 2,
        alpha: vec![255, 255, 0, 0, 255, 255, 0, 0],
    };
    let fill = FillSampler::new(&BackgroundFill::Solid(0x808080));
    let table = MixTable::new(PatternMix::new(&[0x808080], -100, false));
    // 6 wide: 3 columns of 4 = 12, shift (6 - 12) / 2 = -3.
    let out = compose_pattern(&fill, &table, &tile, 6, 4);
    let row: Vec<u8> = (0..6).map(|x| rgb_at(&out, x, 0)[0]).collect();
    // x = 0 is tile column 3 (clear), x = 1 and 2 are columns 0 and 1.
    assert_eq!(row, vec![0, 128, 128, 0, 0, 128]);
    // Rows repeat every tile height.
    assert_eq!(rgb_at(&out, 1, 2), rgb_at(&out, 1, 0));
}

#[test]
fn cover_crops_the_centre_like_tdesktop() {
    // A wide image in a square area: the middle is cut out.
    let (from, to) = cover_rects(100, 100, 400, 200);
    assert_eq!(
        from,
        PixelRect {
            x: 100,
            y: 0,
            width: 200,
            height: 200
        }
    );
    assert_eq!((to.x, to.y, to.width, to.height), (0, 0, 100, 100));
    // A tall image in a wide area keeps the full width.
    let (from, to) = cover_rects(300, 100, 300, 600);
    assert_eq!(from.width, 300);
    assert_eq!(from.height, 100);
    assert_eq!(from.y, 250);
    assert_eq!((to.width, to.height), (300, 100));
}

#[test]
fn long_images_are_trimmed_and_big_ones_fit() {
    assert_eq!(long_image_crop(100, 100), None);
    assert_eq!(
        long_image_crop(5000, 100),
        Some(PixelRect {
            x: 500,
            y: 0,
            width: 4000,
            height: 100
        })
    );
    assert_eq!(fit_within(1800, 900, 900), (900, 450));
    assert_eq!(fit_within(400, 300, 900), (400, 300));
}

#[test]
fn blur_spreads_a_spike_and_keeps_flat_areas() {
    let mut flat = Raster::black(20, 20);
    for px in flat.bgra.as_chunks_mut::<4>().0.iter_mut() {
        px[..3].copy_from_slice(&[90, 90, 90]);
    }
    let before = flat.clone();
    blur(&mut flat, BLUR_RADIUS);
    assert_eq!(flat, before);

    let mut spike = Raster::black(31, 31);
    let at = (15 * 31 + 15) * 4;
    spike.bgra[at..at + 3].copy_from_slice(&[255, 255, 255]);
    blur(&mut spike, 8);
    let centre = spike.pixel(15, 15)[0];
    let side = spike.pixel(18, 15)[0];
    let far = spike.pixel(28, 15)[0];
    assert!(centre < 255 && centre > side, "{centre} {side}");
    assert_eq!(far, 0);
    assert_eq!(spike.pixel(0, 0)[3], 255, "stays opaque");
}

#[test]
fn tiles_repeat_from_the_corner_and_blits_clip() {
    let mut source = Raster::black(2, 2);
    source.bgra[0] = 10; // (0, 0) blue
    let tiled = tile(&source, 5, 3);
    assert_eq!(tiled.pixel(0, 0)[0], 10);
    assert_eq!(tiled.pixel(2, 2)[0], 10);
    assert_eq!(tiled.pixel(1, 0)[0], 0);
    assert_eq!(tiled.pixel(4, 0)[0], 10);

    let mut target = Raster::black(3, 3);
    let mut white = Raster::black(2, 2);
    for px in white.bgra.as_chunks_mut::<4>().0.iter_mut() {
        px[0] = 255;
    }
    blit(&mut target, &white, 2, -1);
    assert_eq!(target.pixel(2, 0)[0], 255);
    assert_eq!(target.pixel(1, 0)[0], 0);
    assert_eq!(target.pixel(2, 1)[0], 0);
}

#[test]
fn rgba_input_becomes_opaque_bgra() {
    let raster = Raster::from_rgba(1, 1, vec![1, 2, 3, 0]);
    assert_eq!(raster.pixel(0, 0), [3, 2, 1, 255]);
    assert_eq!(raster.byte_size(), 4);
}
