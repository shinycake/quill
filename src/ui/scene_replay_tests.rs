//! A cached view replays its last paint, and the window must look as if it
//! had painted again. Each case compares a replayed scene with one painted
//! from scratch by the order a renderer draws them: every overlapping pair
//! keeps its order, and a later view draws over an earlier one.

use gpui_kit::{
    AtlasTextureId, AtlasTextureKind, AtlasTile, Bounds, ContentMask, Corners, Edges, Hsla,
    MonochromeSprite, Path, PathId, PolychromeSprite, Primitive, PrimitiveBatch, Quad,
    ScaledPixels, Scene, Shadow, SubpixelSprite, TileId, TransformationMatrix, Underline, point,
    px, size,
};
use std::collections::HashMap;
use std::ops::Range;

type Rect = Bounds<ScaledPixels>;

const CANVAS: i32 = 64;

/// Seeded, so a failing case reproduces.
struct Dice(u64);

impl Dice {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_add(1).wrapping_mul(0x9e37_79b9_7f4a_7c15))
    }

    fn below(&mut self, n: i32) -> i32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        ((self.0 >> 33) % n.max(1) as u64) as i32
    }

    fn one_in(&mut self, n: i32) -> bool {
        self.below(n) == 0
    }
}

fn rect(x: i32, y: i32, width: i32, height: i32) -> Rect {
    Bounds {
        origin: point(ScaledPixels(x as f32), ScaledPixels(y as f32)),
        size: size(ScaledPixels(width as f32), ScaledPixels(height as f32)),
    }
}

fn canvas() -> Rect {
    rect(0, 0, CANVAS, CANVAS)
}

fn quad(tag: u32, bounds: Rect) -> Primitive {
    primitive(0, tag, bounds, canvas())
}

fn path(tag: u32, bounds: Rect) -> Primitive {
    primitive(6, tag, bounds, canvas())
}

fn sprite(tag: u32, bounds: Rect) -> Primitive {
    primitive(3, tag, bounds, canvas())
}

fn tile(tag: u32) -> AtlasTile {
    AtlasTile {
        texture_id: AtlasTextureId {
            index: tag % 2,
            kind: AtlasTextureKind::Monochrome,
        },
        tile_id: TileId(tag),
        padding: 0,
        bounds: Bounds::default(),
    }
}

/// A primitive of each kind carries `tag` in a field nothing reads, so it
/// can be found again after `Scene::finish` sorted it.
fn primitive(kind: i32, tag: u32, bounds: Rect, clip: Rect) -> Primitive {
    let content_mask = ContentMask { bounds: clip };
    match kind {
        0 => Quad {
            bounds,
            content_mask,
            border_widths: Edges {
                left: ScaledPixels(tag as f32),
                ..Edges::default()
            },
            ..Quad::default()
        }
        .into(),
        1 => Shadow {
            order: 0,
            blur_radius: ScaledPixels(0.),
            bounds,
            corner_radii: Corners::default(),
            content_mask,
            color: Hsla::default(),
            element_bounds: bounds,
            element_corner_radii: Corners::default(),
            inset: 0,
            pad: tag,
        }
        .into(),
        2 => Underline {
            order: 0,
            pad: tag,
            bounds,
            content_mask,
            color: Hsla::default(),
            thickness: ScaledPixels(1.),
            wavy: false.into(),
        }
        .into(),
        3 => MonochromeSprite {
            order: 0,
            pad: tag,
            tile: tile(tag),
            bounds,
            content_mask,
            color: Hsla::default(),
            transformation: TransformationMatrix::unit(),
        }
        .into(),
        4 => SubpixelSprite {
            order: 0,
            pad: tag,
            tile: tile(tag),
            bounds,
            content_mask,
            color: Hsla::default(),
            transformation: TransformationMatrix::unit(),
        }
        .into(),
        5 => PolychromeSprite {
            order: 0,
            pad: tag,
            tile: tile(tag),
            grayscale: false.into(),
            opacity: 1.,
            bounds,
            content_mask,
            corner_radii: Corners::default(),
        }
        .into(),
        _ => {
            let at = |x: ScaledPixels, y: ScaledPixels| point(px(x.0), px(y.0));
            let (left, top) = (bounds.origin.x, bounds.origin.y);
            let (right, bottom) = (left + bounds.size.width, top + bounds.size.height);
            let mut path = Path::new(at(left, top));
            let mark = point(tag as f32, 0.);
            path.push_triangle(
                (at(left, top), at(right, top), at(left, bottom)),
                (mark, mark, mark),
            );
            let mut path = path.scale(1.);
            path.content_mask = content_mask;
            path.into()
        }
    }
}

fn clipped(primitive: &Primitive) -> Rect {
    primitive
        .bounds()
        .intersect(&primitive.content_mask().bounds)
}

#[derive(Clone)]
enum Step {
    Draw(Primitive),
    Layer(Rect),
    EndLayer,
}

/// What a view paints, in painter's order.
#[derive(Clone)]
struct View {
    steps: Vec<Step>,
    cached: bool,
}

impl View {
    /// What this view leaves in a scene: the content mask can clip a
    /// primitive away entirely.
    fn drawn_tags(&self) -> impl Iterator<Item = u32> + '_ {
        self.steps.iter().filter_map(|step| match step {
            Step::Draw(primitive) if !clipped(primitive).is_empty() => Some(tag_of(primitive)),
            _ => None,
        })
    }
}

fn paint(scene: &mut Scene, steps: &[Step]) {
    for step in steps {
        match step {
            Step::Draw(primitive) => scene.insert_primitive(primitive.clone()),
            Step::Layer(bounds) => scene.push_layer(*bounds),
            Step::EndLayer => scene.pop_layer(),
        }
    }
}

/// One frame: cached views replay `last`, the rest paint.
fn frame(views: &[View], last: Option<(&Scene, &[Range<usize>])>) -> (Scene, Vec<Range<usize>>) {
    let mut scene = Scene::default();
    let mut ranges = Vec::with_capacity(views.len());
    for (index, view) in views.iter().enumerate() {
        let start = scene.len();
        match last {
            Some((previous, stored)) if view.cached => {
                scene.replay(stored[index].clone(), previous);
            }
            _ => paint(&mut scene, &view.steps),
        }
        ranges.push(start..scene.len());
    }
    scene.finish();
    (scene, ranges)
}

/// Every view painted from scratch.
fn painted(views: &[View]) -> Scene {
    frame(views, None).0
}

/// Tags in the order a renderer draws them.
fn drawn(scene: &Scene) -> Vec<u32> {
    let mut tags = Vec::new();
    for batch in scene.batches() {
        match batch {
            PrimitiveBatch::Shadows(range) => {
                tags.extend(scene.shadows[range].iter().map(|shadow| shadow.pad));
            }
            PrimitiveBatch::Quads(range) => {
                tags.extend(
                    scene.quads[range]
                        .iter()
                        .map(|quad| quad.border_widths.left.0 as u32),
                );
            }
            PrimitiveBatch::Paths(range) => {
                tags.extend(
                    scene.paths[range]
                        .iter()
                        .map(|path| path.vertices[0].st_position.x as u32),
                );
            }
            PrimitiveBatch::Underlines(range) => {
                tags.extend(scene.underlines[range].iter().map(|line| line.pad));
            }
            PrimitiveBatch::MonochromeSprites { range, .. } => {
                tags.extend(scene.monochrome_sprites[range].iter().map(|s| s.pad));
            }
            PrimitiveBatch::SubpixelSprites { range, .. } => {
                tags.extend(scene.subpixel_sprites[range].iter().map(|s| s.pad));
            }
            PrimitiveBatch::PolychromeSprites { range, .. } => {
                tags.extend(scene.polychrome_sprites[range].iter().map(|s| s.pad));
            }
            PrimitiveBatch::Surfaces(_) => {}
        }
    }
    tags
}

fn positions(scene: &Scene) -> HashMap<u32, usize> {
    drawn(scene)
        .into_iter()
        .enumerate()
        .map(|(at, tag)| (tag, at))
        .collect()
}

fn tag_of(primitive: &Primitive) -> u32 {
    match primitive {
        Primitive::Shadow(shadow) => shadow.pad,
        Primitive::Quad(quad) => quad.border_widths.left.0 as u32,
        Primitive::Path(path) => path.vertices[0].st_position.x as u32,
        Primitive::Underline(line) => line.pad,
        Primitive::MonochromeSprite(sprite) => sprite.pad,
        Primitive::SubpixelSprite(sprite) => sprite.pad,
        Primitive::PolychromeSprite(sprite) => sprite.pad,
        Primitive::Surface(_) => 0,
    }
}

/// `replayed` draws the same primitives as `reference`, with each
/// overlapping pair the same way round, and each view over the ones before.
#[track_caller]
fn assert_same_picture(case: u64, reference: &Scene, replayed: &Scene, views: &[View]) {
    let visible: Vec<(u32, Rect, usize)> = views
        .iter()
        .enumerate()
        .flat_map(|(view, content)| {
            content.steps.iter().filter_map(move |step| match step {
                Step::Draw(primitive) if !clipped(primitive).is_empty() => {
                    Some((tag_of(primitive), clipped(primitive), view))
                }
                _ => None,
            })
        })
        .collect();
    let (want, got) = (positions(reference), positions(replayed));
    assert_eq!(want.len(), visible.len(), "case {case}: painted scene");
    assert_eq!(got.len(), visible.len(), "case {case}: replayed scene");
    for (at, (a, a_box, a_view)) in visible.iter().enumerate() {
        for (b, b_box, b_view) in &visible[at + 1..] {
            if !a_box.intersects(b_box) {
                continue;
            }
            assert_eq!(
                want[a] < want[b],
                got[a] < got[b],
                "case {case}: {a} and {b} changed places"
            );
            if a_view != b_view {
                assert!(got[a] < got[b], "case {case}: view {b_view} under {a_view}");
            }
        }
    }
}

/// The order each tagged primitive of `scene` was given.
fn orders(scene: &Scene) -> HashMap<u32, u32> {
    let quads = scene.quads.iter();
    quads
        .map(|quad| (quad.border_widths.left.0 as u32, quad.order))
        .chain(scene.shadows.iter().map(|s| (s.pad, s.order)))
        .chain(scene.underlines.iter().map(|s| (s.pad, s.order)))
        .chain(scene.monochrome_sprites.iter().map(|s| (s.pad, s.order)))
        .chain(scene.subpixel_sprites.iter().map(|s| (s.pad, s.order)))
        .chain(scene.polychrome_sprites.iter().map(|s| (s.pad, s.order)))
        .chain(
            scene
                .paths
                .iter()
                .map(|path| (path.vertices[0].st_position.x as u32, path.order)),
        )
        .collect()
}

struct Painter {
    dice: Dice,
    next_tag: u32,
}

impl Painter {
    fn new(seed: u64) -> Self {
        Self {
            dice: Dice::new(seed),
            next_tag: 1,
        }
    }

    fn rect_in(&mut self, area: Rect) -> Rect {
        let (left, top) = (area.origin.x.0 as i32, area.origin.y.0 as i32);
        let (width, height) = (area.size.width.0 as i32, area.size.height.0 as i32);
        let (x, y) = (self.dice.below(width), self.dice.below(height));
        let w = 1 + self.dice.below(width - x);
        let h = 1 + self.dice.below(height - y);
        rect(left + x, top + y, w, h)
    }

    fn primitive_in(&mut self, area: Rect) -> Step {
        let tag = self.next_tag;
        self.next_tag += 1;
        let bounds = self.rect_in(area);
        let clip = if self.dice.one_in(4) {
            self.rect_in(area)
        } else {
            area
        };
        Step::Draw(primitive(self.dice.below(7), tag, bounds, clip))
    }

    fn layer_in(&mut self, area: Rect, depth: u32, steps: &mut Vec<Step>) {
        let bounds = self.rect_in(area);
        steps.push(Step::Layer(bounds));
        for _ in 0..self.dice.below(5) {
            if depth < 2 && self.dice.one_in(4) {
                self.layer_in(bounds, depth + 1, steps);
            } else {
                let step = self.primitive_in(bounds);
                steps.push(step);
            }
        }
        steps.push(Step::EndLayer);
    }

    fn view_in(&mut self, area: Rect) -> View {
        let mut steps = Vec::new();
        for _ in 0..self.dice.below(12) {
            if self.dice.one_in(3) {
                self.layer_in(area, 0, &mut steps);
            } else {
                let step = self.primitive_in(area);
                steps.push(step);
            }
        }
        View {
            steps,
            cached: self.dice.one_in(3) || self.dice.one_in(2),
        }
    }

    fn view(&mut self) -> View {
        let area = self.rect_in(canvas());
        self.view_in(area)
    }
}

/// Merge consecutive views at random into outer views.
fn grouped(ranges: &[Range<usize>], dice: &mut Dice) -> Vec<Range<usize>> {
    let mut groups: Vec<Range<usize>> = Vec::new();
    for (index, range) in ranges.iter().enumerate() {
        match groups.last_mut() {
            Some(group) if index > 0 && dice.one_in(2) => group.end = range.end,
            _ => groups.push(range.clone()),
        }
    }
    groups
}

#[test]
fn a_replay_draws_what_painting_again_would_draw() {
    for case in 0..3_000 {
        let mut painter = Painter::new(case);
        let count = 2 + painter.dice.below(5);
        let mut views: Vec<View> = (0..count).map(|_| painter.view()).collect();
        let (first, ranges) = frame(&views, None);

        // What rendered this frame paints something new.
        for view in views.iter_mut().filter(|view| !view.cached) {
            view.steps = painter.view().steps;
        }
        let (second, ranges) = frame(&views, Some((&first, &ranges)));
        assert_same_picture(case, &painted(&views), &second, &views);

        // Replays of a replay, through outer views holding several of them,
        // and something new painted over the result.
        let mut third = Scene::default();
        for group in grouped(&ranges, &mut painter.dice) {
            third.replay(group, &second);
        }
        let late = View {
            steps: vec![painter.primitive_in(canvas())],
            cached: false,
        };
        paint(&mut third, &late.steps);
        third.finish();
        views.push(late);
        assert_same_picture(case, &painted(&views), &third, &views);
    }
}

/// Views side by side, so no box of one meets a box of another.
fn columns(painter: &mut Painter) -> Vec<View> {
    let columns = 4;
    let width = CANVAS / columns;
    (0..columns)
        .map(|column| View {
            cached: true,
            ..painter.view_in(rect(column * width, 0, width, CANVAS))
        })
        .collect()
}

/// A layer takes an order whether anything draws in it or not, and a replay
/// only knows the orders its primitives stored. So a view that opens with a
/// layer that drew nothing can slide down to order 1 as a whole, and land in
/// other batches, more of them or fewer. The rest batch as they painted.
#[test]
fn views_that_do_not_touch_batch_as_if_painted_again() {
    let mut unmoved = 0;
    for case in 0..200 {
        let mut painter = Painter::new(case);
        let views = columns(&mut painter);
        let (first, ranges) = frame(&views, None);
        let (second, _) = frame(&views, Some((&first, &ranges)));
        let reference = painted(&views);
        let (want, got) = (orders(&reference), orders(&second));

        let mut moved = false;
        for view in &views {
            let tags: Vec<u32> = view.drawn_tags().collect();
            let (Some(was), Some(now)) = (
                tags.iter().map(|tag| want[tag]).min(),
                tags.iter().map(|tag| got[tag]).min(),
            ) else {
                continue;
            };
            assert!(now == was || now == 1, "case {case}: {was} became {now}");
            moved |= now != was;
            for tag in &tags {
                assert_eq!(got[tag] - now, want[tag] - was, "case {case}");
            }
        }
        if !moved {
            unmoved += 1;
            let labels = |scene: &Scene| scene.batches().map(|b| b.label()).collect::<Vec<_>>();
            assert_eq!(labels(&reference), labels(&second), "case {case}");
        }
    }
    assert!(unmoved > 100, "only {unmoved} cases compared their batches");
}

#[test]
fn a_view_replayed_frame_after_frame_keeps_its_orders() {
    for case in 0..200 {
        let mut painter = Painter::new(case);
        let views: Vec<View> = (0..4)
            .map(|_| View {
                cached: true,
                ..painter.view()
            })
            .collect();
        let (mut last, mut ranges) = frame(&views, None);
        (last, ranges) = frame(&views, Some((&last, &ranges)));
        let settled = orders(&last);
        for _ in 0..3 {
            (last, ranges) = frame(&views, Some((&last, &ranges)));
            assert_eq!(orders(&last), settled, "case {case}");
        }
    }
}

/// A scene holding `steps`, finished as a window's last frame is.
fn last_frame(steps: &[Primitive]) -> Scene {
    let mut scene = Scene::default();
    for primitive in steps {
        scene.insert_primitive(primitive.clone());
    }
    scene.finish();
    scene
}

#[test]
fn what_is_painted_after_a_replay_sorts_above_all_of_it() {
    let stack: Vec<Primitive> = (1..=3).map(|tag| quad(tag, rect(0, 0, 10, 10))).collect();
    let first = last_frame(&stack);
    let mut second = Scene::default();
    second.replay(0..stack.len(), &first);
    second.insert_primitive(quad(4, rect(5, 5, 10, 10)));
    second.finish();
    assert_eq!(drawn(&second), [1, 2, 3, 4]);
}

#[test]
fn a_replay_is_one_box_in_the_bounds_tree() {
    let first = last_frame(&[
        quad(1, rect(0, 0, 4, 4)),
        quad(2, rect(0, 0, 4, 4)),
        quad(3, rect(20, 20, 4, 4)),
    ]);
    let mut second = Scene::default();
    second.replay(0..3, &first);
    // Between the corners: over nothing the view drew, but inside its box.
    second.insert_primitive(quad(4, rect(10, 10, 4, 4)));
    let top = second.quads.iter().map(|quad| quad.order).max();
    assert_eq!(second.quads.last().map(|quad| quad.order), top);
    assert_eq!(top, Some(3));
}

#[test]
fn a_replay_starts_above_what_it_covers() {
    let first = last_frame(&[quad(1, rect(0, 0, 10, 10)), quad(2, rect(0, 0, 10, 10))]);
    let mut second = Scene::default();
    second.insert_primitive(quad(3, rect(5, 5, 10, 10)));
    second.insert_primitive(quad(4, rect(5, 5, 10, 10)));
    second.replay(0..2, &first);
    second.finish();
    assert_eq!(drawn(&second), [3, 4, 1, 2]);
}

#[test]
fn a_view_replayed_inside_a_layer_takes_the_layers_order() {
    let mut first = Scene::default();
    first.push_layer(canvas());
    let start = first.len();
    first.insert_primitive(quad(1, rect(0, 0, 10, 10)));
    first.insert_primitive(quad(2, rect(20, 0, 10, 10)));
    let end = first.len();
    first.pop_layer();
    first.finish();

    let mut second = Scene::default();
    second.insert_primitive(quad(3, canvas()));
    second.push_layer(canvas());
    second.replay(start..end, &first);
    second.pop_layer();
    let orders: Vec<u32> = second.quads.iter().map(|quad| quad.order).collect();
    assert_eq!(orders, [1, 2, 2]);
}

#[test]
fn a_layer_inside_a_replay_keeps_its_one_order() {
    let mut first = Scene::default();
    first.push_layer(rect(0, 0, 40, 10));
    first.insert_primitive(sprite(1, rect(0, 0, 10, 10)));
    first.insert_primitive(sprite(2, rect(20, 0, 10, 10)));
    first.pop_layer();
    let operations = first.len();
    first.finish();

    let mut second = Scene::default();
    second.insert_primitive(quad(3, canvas()));
    second.replay(0..operations, &first);
    assert_eq!(second.len(), operations + 1);
    let orders: Vec<u32> = second.monochrome_sprites.iter().map(|s| s.order).collect();
    assert_eq!(orders, [2, 2]);
}

#[test]
fn a_glyph_past_its_line_stays_under_what_follows() {
    let mut first = Scene::default();
    first.push_layer(rect(0, 0, 10, 10));
    first.insert_primitive(sprite(1, rect(8, 8, 6, 6)));
    first.pop_layer();
    let operations = first.len();
    first.finish();

    let mut second = Scene::default();
    second.replay(0..operations, &first);
    second.insert_primitive(quad(2, rect(12, 12, 4, 4)));
    second.finish();
    assert_eq!(drawn(&second), [1, 2]);
}

#[test]
fn replayed_paths_get_ids_for_their_new_place() {
    let first = last_frame(&[path(1, rect(0, 0, 8, 8)), path(2, rect(20, 0, 8, 8))]);
    let mut second = Scene::default();
    second.insert_primitive(path(3, rect(40, 0, 8, 8)));
    second.replay(0..2, &first);
    let ids: Vec<PathId> = second.paths.iter().map(|path| path.id).collect();
    assert_eq!(ids, [PathId(0), PathId(1), PathId(2)]);
}

#[test]
fn a_view_with_nothing_but_empty_layers_replays_as_it_painted() {
    let mut first = Scene::default();
    first.push_layer(canvas());
    first.pop_layer();
    let operations = first.len();
    first.finish();

    let mut second = Scene::default();
    second.replay(0..operations, &first);
    second.replay(operations..operations, &first);
    assert_eq!(second.len(), operations);
    second.insert_primitive(quad(1, canvas()));
    assert_eq!(second.quads.first().map(|quad| quad.order), Some(1));
}

#[test]
fn a_refused_paint_stays_out_of_a_replay() {
    let first = last_frame(&[quad(1, rect(0, 0, 10, 10))]);

    let mut suppressed = Scene::default();
    suppressed.refuse_paint_for_test(false, true);
    suppressed.replay(0..1, &first);
    assert!(suppressed.quads.is_empty());

    let mut filtered = Scene::default();
    filtered.refuse_paint_for_test(true, false);
    filtered.replay(0..1, &first);
    assert_eq!(filtered.len(), 0);
}
