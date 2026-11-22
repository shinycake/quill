//! Draw order of backdrop glass (`BackdropLens`, the vendored GPUI's
//! `scene/backdrop_lens.rs`): a lens draws after what it covers, every lens
//! with no glass under it shares one batch (one copy of the frame), and
//! what is painted over a lens draws on top of it.

use gpui_kit::{
    BackdropLens, Background, Bounds, ContentMask, LensBend, LensMaterial, LensStyle, LensTune,
    Quad, ScaledPixels, Scene, bounds, hsla, point, px, size,
};

fn style() -> LensStyle {
    LensStyle {
        unit: 0.0,
        bevel: 50.0,
        band: None,
        shadow: 0.0,
        shadow_softness: 34.0,
        shadow_offset: 22.0,
        strength: 1.0,
        color: hsla(0.0, 0.0, 1.0, 1.0),
        surface: hsla(0.6, 0.5, 0.5, 1.0),
        material: LensMaterial::Clear {
            bend: LensBend::Edge {
                refract: 30.0,
                magnify: 0.1,
                dispersion: 0.1,
                blur_center: 0.35,
                blur_edge: 2.2,
            },
            frost: 0.0,
            blur: px(0.0),
            blur_mix: 1.0,
        },
        border: None,
        tune: LensTune::default(),
        light: point(0.0, -1.0),
        light_mix: 0.0,
    }
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Bounds<ScaledPixels> {
    bounds(
        point(ScaledPixels(x), ScaledPixels(y)),
        size(ScaledPixels(w), ScaledPixels(h)),
    )
}

fn everywhere() -> ContentMask<ScaledPixels> {
    ContentMask {
        bounds: rect(0.0, 0.0, 10_000.0, 10_000.0),
    }
}

fn lens(x: f32, y: f32, w: f32, h: f32) -> BackdropLens {
    let body = rect(x, y, w, h);
    BackdropLens::new(
        body,
        ScaledPixels(h / 2.0),
        body,
        everywhere(),
        1.0,
        &style(),
    )
}

fn quad(x: f32, y: f32, w: f32, h: f32) -> Quad {
    Quad {
        bounds: rect(x, y, w, h),
        content_mask: everywhere(),
        background: Background::from(hsla(0.0, 0.0, 0.5, 1.0)),
        ..Default::default()
    }
}

fn batches(scene: &Scene) -> Vec<String> {
    scene.batches().map(|batch| batch.label()).collect()
}

#[test]
fn lenses_with_no_glass_under_them_share_one_batch() {
    let mut scene = Scene::default();
    // A row under both lenses, then [lens, label over it, lens].
    scene.insert_primitive(quad(0.0, 0.0, 400.0, 100.0));
    scene.insert_primitive(lens(10.0, 10.0, 80.0, 40.0));
    scene.insert_primitive(quad(20.0, 20.0, 30.0, 10.0));
    scene.insert_primitive(lens(200.0, 10.0, 80.0, 40.0));
    scene.finish();
    assert_eq!(batches(&scene), ["quads (1)", "lenses (2)", "quads (1)"]);
    assert_eq!(scene.lenses[0].order, scene.lenses[1].order);
}

#[test]
fn glass_over_glass_takes_the_next_band() {
    let mut scene = Scene::default();
    scene.insert_primitive(quad(0.0, 0.0, 400.0, 100.0));
    scene.insert_primitive(lens(10.0, 10.0, 80.0, 40.0));
    scene.insert_primitive(lens(50.0, 20.0, 80.0, 40.0));
    scene.finish();
    assert_eq!(batches(&scene), ["quads (1)", "lenses (1)", "lenses (1)"]);
    assert!(scene.lenses[1].order > scene.lenses[0].order);
}

#[test]
fn content_painted_after_a_lens_draws_above_it() {
    let mut scene = Scene::default();
    scene.insert_primitive(lens(10.0, 10.0, 80.0, 40.0));
    scene.insert_primitive(quad(20.0, 20.0, 30.0, 10.0));
    scene.finish();
    assert!(scene.quads[0].order > scene.lenses[0].order);
    assert_eq!(batches(&scene), ["lenses (1)", "quads (1)"]);
}

#[test]
fn content_beside_a_lens_keeps_its_place() {
    let mut scene = Scene::default();
    scene.insert_primitive(lens(10.0, 10.0, 80.0, 40.0));
    scene.insert_primitive(quad(300.0, 300.0, 30.0, 10.0));
    scene.finish();
    assert_eq!(scene.quads[0].order, 1);
}

#[test]
fn a_lens_in_a_paint_layer_draws_as_its_fallback_quad() {
    let mut scene = Scene::default();
    scene.push_layer(rect(0.0, 0.0, 400.0, 400.0));
    scene.insert_primitive(lens(10.0, 10.0, 80.0, 40.0));
    scene.pop_layer();
    scene.finish();
    assert!(scene.lenses.is_empty());
    assert_eq!(batches(&scene), ["quads (1)"]);
    assert_eq!(scene.quads[0].bounds, rect(10.0, 10.0, 80.0, 40.0));
}

#[test]
fn a_lens_above_the_last_band_draws_as_its_fallback_quad() {
    let mut scene = Scene::default();
    for _ in 0..256 {
        scene.insert_primitive(lens(10.0, 10.0, 80.0, 40.0));
    }
    scene.finish();
    assert_eq!(scene.lenses.len(), 255);
    assert_eq!(scene.quads.len(), 1);
}

#[test]
fn a_replayed_lens_is_banded_against_what_is_under_it_now() {
    let mut first = Scene::default();
    first.insert_primitive(quad(0.0, 0.0, 400.0, 100.0));
    let start = first.len();
    first.insert_primitive(lens(10.0, 10.0, 80.0, 40.0));
    first.insert_primitive(quad(20.0, 20.0, 30.0, 10.0));
    let end = first.len();
    first.finish();

    let mut next = Scene::default();
    next.insert_primitive(quad(0.0, 0.0, 400.0, 100.0));
    next.replay(start..end, &first);
    next.insert_primitive(lens(200.0, 10.0, 80.0, 40.0));
    next.finish();
    assert_eq!(batches(&next), ["quads (1)", "lenses (2)", "quads (1)"]);
}

#[test]
fn the_footprint_leaves_room_for_the_shadow() {
    let body = bounds(point(px(100.0), px(100.0)), size(px(80.0), px(40.0)));
    let mut style = style();
    let flat = style.footprint(body);
    assert_eq!((flat.top(), flat.bottom()), (px(84.0), px(156.0)));
    style.shadow = 0.12;
    let shadowed = style.footprint(body);
    assert_eq!((shadowed.left(), shadowed.top()), (px(84.0), px(82.0)));
    assert_eq!(shadowed.bottom(), px(192.0));
}

#[test]
fn the_fallback_quad_is_the_body_in_the_glass_colour() {
    let quad = lens(10.0, 10.0, 80.0, 40.0).fallback_quad();
    assert_eq!(quad.bounds, rect(10.0, 10.0, 80.0, 40.0));
    assert_eq!(quad.corner_radii.top_left, ScaledPixels(20.0));
}

#[test]
fn a_lens_does_not_grow_every_recorded_primitive() {
    assert!(std::mem::size_of::<gpui_kit::Primitive>() <= std::mem::size_of::<Quad>() + 16);
}
