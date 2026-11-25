//! Draw order of backdrop glass (`BackdropLens`, the vendored GPUI's
//! `scene/backdrop_lens.rs`): a lens draws after what it covers, every lens
//! with no glass under it shares one batch (one copy of the frame), and
//! what is painted over a lens draws on top of it.

use gpui_kit::{
    AtlasTextureId, AtlasTextureKind, AtlasTile, BackdropLens, Background, Bounds, ContentMask,
    LENS_ATLAS_GUTTER, LensBend, LensCopy, LensDetail, LensDroplet, LensFrameContext, LensInk,
    LensInkEntry, LensMaterial, LensPacking, LensReuse, LensStyle, LensTune, LensWork,
    MonochromeSprite, Quad, ScaledPixels, Scene, SpriteAnimationId, TileId, TransformationMatrix,
    bounds, hsla, point, px, size,
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
        droplet: None,
        blob: None,
        detail: LensDetail::Copied,
    }
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Bounds<ScaledPixels> {
    bounds(
        point(ScaledPixels(x), ScaledPixels(y)),
        size(ScaledPixels(w), ScaledPixels(h)),
    )
}

fn everywhere() -> ContentMask<ScaledPixels> {
    ContentMask::rect(rect(0.0, 0.0, 10_000.0, 10_000.0))
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
    scene.push_layer();
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

// The atlas packing every renderer shares (`LensPacking::pack`).

#[test]
fn footprints_are_clipped_to_the_target_and_aligned_in_the_atlas() {
    let packing = LensPacking::pack(
        &[
            lens(-10.5, 20.0, 100.0, 50.0),
            lens(900.0, 700.0, 200.0, 200.0),
        ],
        (1000, 800),
        16384,
    );
    let Some(packing) = packing else {
        panic!("two small footprints fit");
    };
    assert_eq!(
        packing.copies,
        [
            LensCopy {
                from: (0, 20),
                to: (0, 0),
                size: (90, 50),
                density: 1,
            },
            LensCopy {
                from: (900, 700),
                to: (96 + LENS_ATLAS_GUTTER, 0),
                size: (100, 100),
                density: 1,
            },
        ]
    );
    assert_eq!(
        packing.width,
        96 + LENS_ATLAS_GUTTER + 112 + LENS_ATLAS_GUTTER
    );
    assert_eq!(packing.height, 112);
}

#[test]
fn a_full_row_starts_a_shelf() {
    let lenses = [lens(0.0, 0.0, 600.0, 40.0), lens(0.0, 100.0, 600.0, 40.0)];
    let packing = LensPacking::pack(&lenses, (1000, 800), 16384);
    assert_eq!(
        packing.map(|packing| (packing.copies[1].to, packing.height)),
        Some(((0, 48 + LENS_ATLAS_GUTTER), 48 + LENS_ATLAS_GUTTER + 48))
    );
}

#[test]
fn footprints_past_the_largest_atlas_draw_flat() {
    let lenses = [lens(0.0, 0.0, 3000.0, 40.0)];
    assert_eq!(LensPacking::pack(&lenses, (4000, 800), 2048), None);
}

// Stage 5: reused backdrops, inked labels, gooey shapes, sharp loupes.

fn frame_context() -> LensFrameContext {
    LensFrameContext {
        target: (1000, 800),
        atlas: 1,
        sprites: 0,
    }
}

fn page(shade: f32) -> Scene {
    let mut scene = Scene::default();
    scene.insert_primitive(Quad {
        background: Background::from(hsla(0.0, 0.0, shade, 1.0)),
        ..quad(0.0, 0.0, 400.0, 100.0)
    });
    scene.insert_primitive(lens(10.0, 10.0, 80.0, 40.0));
    scene.finish();
    scene
}

/// What each lens of `scene` does this frame, the frame kept as drawn.
fn plan(reuse: &mut LensReuse, scene: &Scene) -> Vec<LensWork> {
    let Some(frame) = reuse.plan(scene, frame_context(), 16384, false) else {
        panic!("the lens fits an atlas");
    };
    reuse.keep(&frame, frame_context());
    frame.slots.iter().map(|slot| slot.work).collect()
}

#[test]
fn a_lens_over_still_content_reuses_its_backdrop_until_it_changes() {
    let mut reuse = LensReuse::default();
    assert_eq!(plan(&mut reuse, &page(0.5)), [LensWork::Copy]);
    assert_eq!(plan(&mut reuse, &page(0.5)), [LensWork::Reuse]);
    assert_eq!(plan(&mut reuse, &page(0.2)), [LensWork::Copy]);
    reuse.forget();
    assert_eq!(plan(&mut reuse, &page(0.2)), [LensWork::Copy]);
}

#[test]
fn an_inked_label_reads_the_glass_under_it_and_none_beside_it() {
    let ink = LensInk {
        on_light: hsla(0.0, 0.0, 0.0, 1.0),
        on_dark: hsla(0.0, 0.0, 1.0, 1.0),
    };
    let glyph = |x: f32| MonochromeSprite {
        order: 0,
        animation: SpriteAnimationId::NONE,
        tile: AtlasTile {
            texture_id: AtlasTextureId {
                index: 0,
                kind: AtlasTextureKind::Monochrome,
            },
            tile_id: TileId(1),
            padding: 0,
            bounds: Default::default(),
        },
        bounds: rect(x, 25.0, 10.0, 10.0),
        content_mask: everywhere(),
        color: hsla(0.0, 0.0, 0.5, 1.0),
        transformation: TransformationMatrix::unit(),
    };
    let mut scene = Scene::default();
    scene.insert_primitive(lens(10.0, 10.0, 80.0, 40.0));
    scene.insert_inked_sprite(glyph(40.0), ink);
    scene.insert_inked_sprite(glyph(300.0), ink);
    scene.finish();
    // Each glyph names its ink entry: the lens it reads.
    let mut read: Vec<(f32, Option<u32>)> = scene
        .monochrome_sprites
        .iter()
        .map(|sprite| {
            let entry = sprite.ink_entry().and_then(|i| scene.inks.get(i));
            assert_eq!(entry.map(|entry| entry.ink), Some(ink));
            (sprite.bounds.origin.x.0, entry.map(|entry| entry.lens))
        })
        .collect();
    read.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert_eq!(
        read,
        [(40.0, Some(0)), (300.0, Some(LensInkEntry::NO_LENS))]
    );
}

#[test]
fn a_droplet_widens_the_glass_and_its_copy() {
    let body = rect(10.0, 10.0, 80.0, 40.0);
    let mut drop = style();
    drop.droplet = Some(LensDroplet {
        offset_x: px(60.0),
        radius: px(20.0),
        softness: px(8.0),
    });
    let plain = BackdropLens::new(body, ScaledPixels(20.0), body, everywhere(), 1.0, &style());
    let gooey = BackdropLens::new(body, ScaledPixels(20.0), body, everywhere(), 1.0, &drop);
    // The droplet's centre sits 60 px right of the body's (50), radius 20,
    // and the gooey bridge can bulge a quarter of its softness past that.
    assert_eq!(gooey.glass_bounds().right(), ScaledPixels(132.0));
    assert_eq!(plain.glass_bounds(), body);
}

#[test]
fn a_sharp_loupe_asks_for_its_density_and_a_copied_one_for_none() {
    let body = rect(10.0, 10.0, 80.0, 80.0);
    let mut sharp = style();
    sharp.detail = LensDetail::Rendered { density: 3 };
    let lens = |style: &LensStyle| {
        BackdropLens::new(body, ScaledPixels(40.0), body, everywhere(), 1.0, style).density
    };
    assert_eq!((lens(&sharp), lens(&style())), (3, 0));
}
