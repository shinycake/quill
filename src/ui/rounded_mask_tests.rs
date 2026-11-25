//! Rounded content masks (the vendored GPUI's `scene/rounded_mask.rs`,
//! docs/decisions/rounded-content-masks.md): the nesting rules, corner
//! culling, the point test, and the instance layouts the shaders mirror.

use gpui_kit::{
    Bounds, ContentMask, Corners, Nesting, Pixels, Quad, RoundedMask, ScaledPixels, Scene, point,
    px, size,
};

fn rect(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
    Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
}

fn scaled(x: f32, y: f32, w: f32, h: f32) -> Bounds<ScaledPixels> {
    Bounds::new(
        point(ScaledPixels(x), ScaledPixels(y)),
        size(ScaledPixels(w), ScaledPixels(h)),
    )
}

fn radius(r: f32) -> Corners<Pixels> {
    Corners::all(px(r))
}

/// A card: 0,0 to 400,600 with radius 24.
fn card() -> ContentMask<Pixels> {
    ContentMask::rounded(rect(0.0, 0.0, 400.0, 600.0), radius(24.0))
}

#[test]
fn zero_radii_make_a_plain_rectangle() {
    let mask = ContentMask::rounded(rect(0.0, 0.0, 10.0, 10.0), radius(0.0));
    assert_eq!(mask, ContentMask::rect(rect(0.0, 0.0, 10.0, 10.0)));
    assert!(mask.rounded.is_none());
}

#[test]
fn radii_are_clamped_to_half_the_shorter_side() {
    let mask = ContentMask::rounded(rect(0.0, 0.0, 100.0, 40.0), radius(30.0));
    assert_eq!(mask.rounded.radii, radius(20.0));
}

#[test]
fn rule_1_a_side_without_a_shape_takes_the_other() {
    let viewport = ContentMask::rect(rect(-10.0, -10.0, 500.0, 700.0));
    let (nested, nesting) = viewport.nest_in(&card());
    assert_eq!(nesting, Nesting::Exact);
    assert_eq!(nested.rounded, card().rounded);
    assert_eq!(nested.bounds, rect(0.0, 0.0, 400.0, 600.0));

    let (nested, nesting) = card().nest_in(&viewport);
    assert_eq!(nesting, Nesting::Exact);
    assert_eq!(nested.rounded, card().rounded);
}

#[test]
fn rule_2_an_outer_shape_with_no_corner_in_the_rectangle_gives_way() {
    // A pill in the middle of the card, far from its corners.
    let pill = ContentMask::rounded(rect(100.0, 200.0, 200.0, 48.0), radius(24.0));
    let (nested, nesting) = pill.nest_in(&card());
    assert_eq!(nesting, Nesting::Exact);
    assert_eq!(nested.rounded, pill.rounded);
}

#[test]
fn rule_3_an_inner_shape_with_no_corner_in_the_rectangle_keeps_the_outer() {
    // A rounded shape far larger than the card, its corners outside it.
    let huge = ContentMask::rounded(rect(-1000.0, -1000.0, 3000.0, 3000.0), radius(50.0));
    let (nested, nesting) = huge.nest_in(&card());
    assert_eq!(nesting, Nesting::Exact);
    assert_eq!(nested.rounded, card().rounded);
    assert_eq!(nested.bounds, card().bounds);
}

#[test]
fn rule_4_a_contained_shape_wins_exactly() {
    // Inset 8 with radius 16: the concentric inner corner.
    let inset = ContentMask::rounded(rect(8.0, 8.0, 384.0, 584.0), radius(16.0));
    let (nested, nesting) = inset.nest_in(&card());
    assert_eq!(nesting, Nesting::Exact);
    assert_eq!(nested.rounded, inset.rounded);
}

#[test]
fn rule_5_crossing_shapes_keep_the_inner_and_say_so() {
    // Flush with the card's corner but with a smaller radius: the inner
    // shape pokes out of the card's rounding.
    let crossing = ContentMask::rounded(rect(0.0, 0.0, 200.0, 200.0), radius(4.0));
    let (nested, nesting) = crossing.nest_in(&card());
    assert_eq!(nesting, Nesting::Inexact);
    assert_eq!(nested.rounded, crossing.rounded);
}

#[test]
fn a_viewport_inside_the_card_keeps_only_the_corners_it_reaches() {
    // A scroll viewport below the header: the card's bottom corners still cut.
    let viewport = ContentMask::rect(rect(0.0, 100.0, 400.0, 500.0));
    let (nested, _) = viewport.nest_in(&card());
    assert_eq!(nested.rounded, card().rounded);
    // A viewport in the card's middle: no corner of the card is inside it.
    let middle = ContentMask::rect(rect(0.0, 100.0, 400.0, 300.0));
    let (nested, nesting) = middle.nest_in(&card());
    assert_eq!(nesting, Nesting::Exact);
    assert!(nested.rounded.is_none());
}

#[test]
fn culling_clears_the_shape_away_from_the_corners() {
    let mask = card();
    let row = rect(0.0, 200.0, 400.0, 72.0);
    assert!(mask.culled_for(&row).rounded.is_none());
    let bottom_row = rect(0.0, 560.0, 400.0, 72.0);
    assert_eq!(mask.culled_for(&bottom_row).rounded, mask.rounded);
    // Touching a corner square's edge without sharing its area cuts nothing.
    let below_square = rect(0.0, 24.0, 400.0, 10.0);
    assert!(mask.culled_for(&below_square).rounded.is_none());
    // The part of a primitive outside the rectangle does not count.
    let outside = rect(-100.0, 200.0, 50.0, 500.0);
    assert!(mask.culled_for(&outside).rounded.is_none());
}

#[test]
fn contains_follows_the_corners() {
    let mask = card();
    assert!(mask.contains(&point(px(200.0), px(300.0))));
    // The cut-away top-left corner.
    assert!(!mask.contains(&point(px(1.0), px(1.0))));
    // Just inside the arc on the diagonal (24 − 24/√2 ≈ 7.03).
    assert!(mask.contains(&point(px(8.0), px(8.0))));
    // Straight edges are whole.
    assert!(mask.contains(&point(px(0.5), px(300.0))));
    assert!(!mask.contains(&point(px(401.0), px(300.0))));
}

#[test]
fn inserting_a_primitive_culls_its_mask() {
    let mask = card().scale(2.0);
    let mut scene = Scene::default();
    for bounds in [
        scaled(0.0, 400.0, 800.0, 144.0),
        scaled(0.0, 1120.0, 800.0, 80.0),
    ] {
        scene.insert_primitive(Quad {
            bounds,
            content_mask: mask,
            ..Quad::default()
        });
    }
    let masks: Vec<RoundedMask<ScaledPixels>> = scene
        .quads
        .iter()
        .map(|quad| quad.content_mask.rounded)
        .collect();
    assert_eq!(masks.len(), 2);
    assert!(masks.first().is_some_and(RoundedMask::is_none));
    assert_eq!(masks.get(1), Some(&mask.rounded));
}

#[test]
fn scaling_scales_both_parts() {
    let scaled_mask = card().scale(2.0);
    assert_eq!(scaled_mask.bounds, scaled(0.0, 0.0, 800.0, 1200.0));
    assert_eq!(scaled_mask.rounded.bounds, scaled(0.0, 0.0, 800.0, 1200.0));
    assert_eq!(scaled_mask.rounded.radii, Corners::all(ScaledPixels(48.0)));
}

/// `// QUILL-LAYOUT <struct> size=<bytes> mask=<offset>` lines of a shader.
fn layouts(source: &str) -> Vec<(String, usize, usize)> {
    source
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("// QUILL-LAYOUT ")?;
            let mut words = rest.split_whitespace();
            let name = words.next()?.to_string();
            let size = words.next()?.strip_prefix("size=")?.parse().ok()?;
            let mask = words.next()?.strip_prefix("mask=")?.parse().ok()?;
            Some((name, size, mask))
        })
        .collect()
}

#[test]
fn shader_layouts_match_the_rust_structs() {
    use gpui_kit::{
        BackdropLens, MonochromeSprite, PathPaint, PolychromeSprite, Shadow, SubpixelSprite,
        Underline,
    };
    use std::mem::{offset_of, size_of};
    let rust = |name: &str| -> Option<(usize, usize)> {
        Some(match name {
            "Quad" => (size_of::<Quad>(), offset_of!(Quad, content_mask)),
            "Shadow" => (size_of::<Shadow>(), offset_of!(Shadow, content_mask)),
            "Underline" => (size_of::<Underline>(), offset_of!(Underline, content_mask)),
            "MonochromeSprite" => (
                size_of::<MonochromeSprite>(),
                offset_of!(MonochromeSprite, content_mask),
            ),
            "SubpixelSprite" => (
                size_of::<SubpixelSprite>(),
                offset_of!(SubpixelSprite, content_mask),
            ),
            "PolychromeSprite" => (
                size_of::<PolychromeSprite>(),
                offset_of!(PolychromeSprite, content_mask),
            ),
            "PathPaint" => (size_of::<PathPaint>(), offset_of!(PathPaint, content_mask)),
            "BackdropLens" => (
                size_of::<BackdropLens>(),
                offset_of!(BackdropLens, content_mask),
            ),
            _ => return None,
        })
    };
    let shaders = [
        (
            "shaders.metal",
            include_str!("../../third_party/gpui-pre-apple/src/shaders.metal"),
            7,
        ),
        (
            "shaders.wgsl",
            include_str!("../../third_party/gpui-pre-wgpu/src/shaders.wgsl"),
            6,
        ),
        (
            "shaders_webgl.wgsl",
            include_str!("../../third_party/gpui-pre-wgpu/src/shaders_webgl.wgsl"),
            6,
        ),
        (
            "shaders_subpixel.wgsl",
            include_str!("../../third_party/gpui-pre-wgpu/src/shaders_subpixel.wgsl"),
            1,
        ),
        (
            "shaders_lens.wgsl",
            include_str!("../../third_party/gpui-pre-wgpu/src/shaders_lens.wgsl"),
            1,
        ),
        (
            "shaders.hlsl",
            include_str!("../../third_party/gpui-pre-windows/src/shaders.hlsl"),
            7,
        ),
        (
            "shaders_lens.hlsl",
            include_str!("../../third_party/gpui-pre-windows/src/shaders_lens.hlsl"),
            1,
        ),
    ];
    for (file, source, count) in shaders {
        let found = layouts(source);
        assert_eq!(found.len(), count, "{file}: QUILL-LAYOUT lines");
        for (name, size, mask) in found {
            assert_eq!(rust(&name), Some((size, mask)), "{file}: {name}");
        }
    }
    assert_eq!(size_of::<ContentMask<ScaledPixels>>(), 48);
    assert_eq!(size_of::<RoundedMask<ScaledPixels>>(), 32);
}
