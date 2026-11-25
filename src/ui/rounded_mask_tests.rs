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

/// A 200 x 200 card at (20, 20) with radius 24, a 40 x 40 child at its
/// top-left corner and a 30 x 30 child in its middle. `clip` picks the
/// card's clip; the corner child counts its clicks.
struct CardView {
    clip: Clip,
    clicks: std::rc::Rc<std::cell::Cell<u32>>,
}

#[derive(Clone, Copy)]
enum Clip {
    Rounded,
    OverflowHidden,
}

impl gpui_kit::Render for CardView {
    fn render(
        &mut self,
        _window: &mut gpui_kit::Window,
        _cx: &mut gpui_kit::Context<Self>,
    ) -> impl gpui_kit::IntoElement {
        use gpui_kit::{
            InteractiveElement, ParentElement, StatefulInteractiveElement, Styled, div, hsla,
        };
        let clicks = self.clicks.clone();
        let card = div()
            .absolute()
            .left(px(20.0))
            .top(px(20.0))
            .w(px(200.0))
            .h(px(200.0))
            .rounded(px(24.0))
            .bg(hsla(0.0, 0.0, 1.0, 1.0));
        let card = match self.clip {
            Clip::Rounded => card.clip_rounded(),
            Clip::OverflowHidden => card.overflow_hidden(),
        };
        div().size_full().child(
            card.child(
                div()
                    .id("corner")
                    .absolute()
                    .left_0()
                    .top_0()
                    .w(px(40.0))
                    .h(px(40.0))
                    .bg(hsla(0.0, 1.0, 0.5, 1.0))
                    .on_click(move |_, _, _| clicks.set(clicks.get() + 1)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(85.0))
                    .top(px(85.0))
                    .w(px(30.0))
                    .h(px(30.0))
                    .bg(hsla(0.6, 1.0, 0.5, 1.0)),
            ),
        )
    }
}

fn card_window(
    cx: &mut gpui_kit::TestAppContext,
    clip: Clip,
) -> (
    gpui_kit::VisualTestContext,
    std::rc::Rc<std::cell::Cell<u32>>,
) {
    use gpui_kit::AppContext;
    use gpui_kit::test::TestWindowExt;
    cx.update(gpui_kit::init);
    let clicks = std::rc::Rc::new(std::cell::Cell::new(0));
    let view_clicks = clicks.clone();
    let handle = cx.open_window(size(px(300.0), px(300.0)), move |window, cx| {
        let view = cx.new(|_| CardView {
            clip,
            clicks: view_clicks,
        });
        gpui_kit::component::Root::new(view, window, cx)
    });
    let mut vcx = gpui_kit::VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    vcx.update(|window, cx| window.render_frame(cx));
    (vcx, clicks)
}

/// The rounded part of the mask of each quad `side` pixels wide.
fn masks_of_width(
    vcx: &mut gpui_kit::VisualTestContext,
    side: f32,
) -> Vec<RoundedMask<ScaledPixels>> {
    vcx.update(|window, _| {
        let width = ScaledPixels(side * window.scale_factor());
        window
            .rendered_scene()
            .quads
            .iter()
            .filter(|quad| quad.bounds.size.width == width)
            .map(|quad| quad.content_mask.rounded)
            .collect()
    })
}

#[gpui_kit::test]
fn clip_rounded_cuts_the_child_at_the_corner_only(cx: &mut gpui_kit::TestAppContext) {
    let (mut vcx, _) = card_window(cx, Clip::Rounded);
    let corner = masks_of_width(&mut vcx, 40.0);
    assert_eq!(corner.len(), 1);
    assert!(corner.iter().all(|mask| !mask.is_none()));
    let middle = masks_of_width(&mut vcx, 30.0);
    assert_eq!(middle.len(), 1);
    assert!(middle.iter().all(RoundedMask::is_none));
    // The card's own fill is not under its own mask.
    let card = masks_of_width(&mut vcx, 200.0);
    assert!(card.iter().all(RoundedMask::is_none));
}

#[gpui_kit::test]
fn overflow_hidden_keeps_its_rectangle(cx: &mut gpui_kit::TestAppContext) {
    let (mut vcx, _) = card_window(cx, Clip::OverflowHidden);
    let corner = masks_of_width(&mut vcx, 40.0);
    assert_eq!(corner.len(), 1);
    assert!(corner.iter().all(RoundedMask::is_none));
}

#[gpui_kit::test]
fn the_cut_away_corner_takes_no_click(cx: &mut gpui_kit::TestAppContext) {
    let (mut vcx, clicks) = card_window(cx, Clip::Rounded);
    // Inside the child, outside the card's rounding: (21, 21).
    vcx.simulate_click(point(px(21.0), px(21.0)), gpui_kit::Modifiers::none());
    assert_eq!(clicks.get(), 0);
    // Inside both: (50, 50).
    vcx.simulate_click(point(px(50.0), px(50.0)), gpui_kit::Modifiers::none());
    assert_eq!(clicks.get(), 1);

    let (mut vcx, clicks) = card_window(cx, Clip::OverflowHidden);
    vcx.simulate_click(point(px(21.0), px(21.0)), gpui_kit::Modifiers::none());
    assert_eq!(clicks.get(), 1);
}
