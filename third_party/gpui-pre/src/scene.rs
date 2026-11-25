// Modified by the Quill project (2026) from gpui-pre 0.3.9 (Apache-2.0):
// `SurfaceFrame` (RGBA16F and NV12, `YuvColor`) and the non-Apple `PaintSurface::frame`;
// primitives a window's paint filter refuses never enter the scene; a replayed
// range enters the bounds tree once; backdrop glass (`BackdropLens`, banded
// orders; stage 5: frame plans, label inks, re-rendered backdrops).
// See third_party/gpui-pre/QUILL-CHANGES.md.
// todo("windows"): remove
#![cfg_attr(windows, allow(dead_code))]

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    AtlasTextureId, AtlasTile, Background, Bounds, ContentMask, Corners, Edges, HitboxId, Hsla,
    NativeViewId, Path, PathTiles, Pixels, Point, Radians, ScaledPixels, Size, TimedTransformation,
    bounds_tree::BoundsTree,
};
mod backdrop_frame;
mod backdrop_lens;
mod backdrop_sharp;
mod rounded_mask;
pub use backdrop_frame::*;
pub(crate) use backdrop_frame::{UnderItem, primitives_under_ranked};
pub use backdrop_lens::*;
pub use backdrop_sharp::SharpBackdrop;
pub use rounded_mask::{MaskUnit, Nesting, RoundedMask};
pub(crate) use backdrop_sharp::{GlyphKey, GlyphSource, SlotSpace};

use smallvec::SmallVec;
use std::{
    cmp::Ordering, collections::VecDeque, fmt::Debug, iter::Peekable, ops::Range, rc::Rc, slice,
    time::Duration,
};

#[expect(missing_docs)]
pub type DrawOrder = u32;

/// A boolean stored as a `u32` so that GPU-facing structs contain no
/// compiler-inserted padding bytes, which would be undefined behavior to
/// reinterpret as `&[u8]` when writing instance buffers. Guaranteed to be
/// `0` or `1` by construction; shaders read it as a `u32`/`uint`.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct PaddedBool32(u32);

impl From<bool> for PaddedBool32 {
    fn from(value: bool) -> Self {
        PaddedBool32(value as u32)
    }
}

impl PaddedBool32 {
    /// The wrapped boolean.
    pub fn as_bool(self) -> bool {
        self.0 != 0
    }
}

struct Layer {
    primitives: SmallVec<[Range<usize>; 1]>,
    bounds: Bounds<ScaledPixels>,
    max_kind: PrimitiveKind,
    max_shared_kind: PrimitiveKind,
    /// Whether a native view was painted in the layer, which then can't be
    /// drawn as one: what was painted after the view has to cover it.
    has_native_view: bool,
}

#[derive(Default)]
#[expect(missing_docs)]
pub struct Scene {
    pub(crate) paint_operations: Vec<PaintOperation>,
    primitive_bounds: BoundsTree<ScaledPixels>,
    layer_stack: Vec<usize>,
    layers: VecDeque<Layer>,
    pub shadows: Vec<Shadow>,
    pub quads: Vec<Quad>,
    pub paths: Vec<PaintedPath>,
    pub underlines: Vec<Underline>,
    pub monochrome_sprites: Vec<MonochromeSprite>,
    pub subpixel_sprites: Vec<SubpixelSprite>,
    pub polychrome_sprites: Vec<PolychromeSprite>,
    pub surfaces: Vec<PaintSurface>,
    pub native_views: Vec<PaintNativeView>,
    animations: Vec<SpriteAnimation>,
    /// Quill: backdrop glass, drawn over what the frame holds under it.
    pub lenses: Vec<BackdropLens>,
    /// QUILL (glass stage 5): the lens and colours of each inked label. After
    /// `finish`, an inked `MonochromeSprite` has `ink_entry` = its entry here.
    pub inks: Vec<LensInkEntry>,
    /// QUILL (glass stage 5): the inks labels were painted with; an inked
    /// sprite carries its request until `finish`.
    pub(crate) ink_requests: Vec<LensInk>,
    /// QUILL (glass stage 5): what lies under each `LensDetail::Rendered` lens,
    /// drawn again finer (`Window` builds them as the frame ends).
    pub sharp_backdrops: Vec<SharpBackdrop>,
    /// QUILL (glass stage 5): how to rasterize sprites again, recorded while a
    /// window has re-rendered backdrops.
    pub(crate) glyph_sources: collections::FxHashMap<GlyphKey, GlyphSource>,
    /// Quill: the sort keys of `finish`, kept so it allocates nothing once
    /// the scene has grown.
    sort_keys: Vec<(u64, u32)>,
    /// Quill: the window's paint filter refuses the tags being painted
    /// now, so primitives inserted meanwhile are dropped
    /// (`Window::set_paint_filter`).
    pub(crate) suppressed: bool,
    /// Quill: this scene is drawn under a paint filter, so cached
    /// primitives are not replayed (they were never filtered).
    pub(crate) filtered: bool,
    /// QUILL: how many masks were nested this frame with a loss of the outer
    /// mask's rounding (rule 5 of docs/decisions/rounded-content-masks.md).
    pub inexact_rounded_masks: u32,
}

#[expect(missing_docs)]
impl Scene {
    pub fn clear(&mut self) {
        self.paint_operations.clear();
        self.primitive_bounds.clear();
        self.layer_stack.clear();
        self.layers.clear();
        self.paths.clear();
        self.shadows.clear();
        self.quads.clear();
        self.underlines.clear();
        self.monochrome_sprites.clear();
        self.subpixel_sprites.clear();
        self.polychrome_sprites.clear();
        self.surfaces.clear();
        self.native_views.clear();
        self.animations.clear();
        self.lenses.clear();
        self.inks.clear();
        self.ink_requests.clear();
        self.sharp_backdrops.clear();
        self.glyph_sources.clear();
        self.sort_keys.clear();
        self.suppressed = false;
        self.filtered = false;
        self.inexact_rounded_masks = 0;
    }

    /// QUILL: records a mask nested with the loss of the outer mask's
    /// rounding (rule 5), and says so once in a debug build.
    pub(crate) fn note_inexact_rounded_mask(&mut self) {
        self.inexact_rounded_masks = self.inexact_rounded_masks.saturating_add(1);
        #[cfg(debug_assertions)]
        {
            static LOGGED: std::sync::Once = std::sync::Once::new();
            LOGGED.call_once(|| {
                log::warn!(
                    "a rounded content mask crossed another one's corner; the outer rounding is lost there (docs/decisions/rounded-content-masks.md, rule 5)"
                );
            });
        }
    }

    /// Inserts a sprite that [`Self::advance_animations`] keeps moving. The
    /// sprite's color is taken as its color before the animation's alpha.
    pub(crate) fn insert_animated_sprite(
        &mut self,
        mut sprite: MonochromeSprite,
        mut animation: SpriteAnimation,
    ) {
        sprite.animation = SpriteAnimationId::animation(self.animations.len());
        animation.base_color = sprite.color;
        if self.try_insert_primitive(sprite.into()) && sprite.animation != SpriteAnimationId::NONE {
            self.animations.push(animation);
        }
    }

    /// How long the window may wait between presents of this scene while only
    /// its animations need a frame: `None` when nothing in it animates, and
    /// zero when an animation wants every frame.
    pub(crate) fn animation_frame_interval(&self) -> Option<Duration> {
        self.animations
            .iter()
            .map(|animation| {
                animation
                    .transformation
                    .min_frame_interval()
                    .unwrap_or_default()
            })
            .min()
    }

    /// Moves every animated sprite to its pose at `elapsed` time on the app's
    /// synced animation clock, without the scene being rebuilt.
    pub(crate) fn advance_animations(&mut self, elapsed: Duration) {
        for animation in &self.animations {
            let Some(sprite) = self.monochrome_sprites.get_mut(animation.sprite_index) else {
                continue;
            };
            let pose = animation.transformation.at(elapsed);
            sprite.transformation = pose.into_matrix(animation.center, animation.scale_factor);
            sprite.color = animation.base_color.opacity(pose.alpha_factor());
        }
    }

    pub fn len(&self) -> usize {
        self.paint_operations.len()
    }

    pub fn push_layer(&mut self) {
        self.layer_stack.push(self.layers.len());
        self.layers.push_back(Layer {
            primitives: SmallVec::new(),
            bounds: Bounds::default(),
            max_kind: PrimitiveKind::Shadow,
            max_shared_kind: PrimitiveKind::NativeView,
            has_native_view: false,
        });
        self.paint_operations.push(PaintOperation::StartLayer);
    }

    pub fn pop_layer(&mut self) {
        self.layer_stack.pop();
        if self.layer_stack.is_empty() {
            while let Some(layer) = self.layers.pop_front() {
                self.insert_layer(layer);
            }
        }
        self.paint_operations.push(PaintOperation::EndLayer);
    }

    fn insert_layer(&mut self, layer: Layer) {
        if layer.primitives.is_empty() {
            return;
        }
        if layer.has_native_view {
            self.insert_layer_around_native_views(layer);
            return;
        }
        let order =
            self.primitive_bounds
                .insert(layer.bounds, layer.max_kind, layer.max_shared_kind);
        for index in layer.primitives.into_iter().flatten() {
            // QUILL: the recorded primitive keeps its order for `replay_as_one`.
            if let Some(PaintOperation::Primitive(primitive)) = self.paint_operations.get_mut(index)
            {
                primitive.set_order(order);
                let primitive = primitive.clone();
                self.push_primitive(primitive, order);
            }
        }
    }

    /// Inserts a layer that a native view was painted in.
    ///
    /// A layer's primitives share one place in the drawing order, where they
    /// are drawn by kind and not in the order they were painted. A native
    /// view erases what is drawn before it, so the layer is inserted in parts:
    /// what was painted before each view, the view, and what was painted after.
    fn insert_layer_around_native_views(&mut self, layer: Layer) {
        let mut part = Vec::new();
        for index in layer.primitives.into_iter().flatten() {
            let Some(PaintOperation::Primitive(primitive)) = self.paint_operations.get(index) else {
                continue;
            };
            let primitive = primitive.clone();
            if primitive.kind() == PrimitiveKind::NativeView {
                self.insert_layer_part(std::mem::take(&mut part));
                self.insert_layer_part(vec![(index, primitive)]);
            } else {
                part.push((index, primitive));
            }
        }
        self.insert_layer_part(part);
    }

    /// QUILL: each primitive comes with the index of its paint operation, which
    /// is given the order too.
    fn insert_layer_part(&mut self, primitives: Vec<(usize, Primitive)>) {
        let Some((_, first)) = primitives.first() else {
            return;
        };
        let mut bounds = first.clipped_bounds();
        let mut max_kind = PrimitiveKind::Shadow;
        let mut max_shared_kind = PrimitiveKind::NativeView;
        for (_, primitive) in &primitives {
            let kind = primitive.kind();
            bounds = bounds.join(&primitive.clipped_bounds());
            max_kind = max_kind.max(kind);
            max_shared_kind = max_shared_kind.min(kind.max_shared_kind());
        }
        let order = self
            .primitive_bounds
            .insert(bounds, max_kind, max_shared_kind);
        for (index, primitive) in primitives {
            if let Some(PaintOperation::Primitive(recorded)) = self.paint_operations.get_mut(index)
            {
                recorded.set_order(order);
            }
            self.push_primitive(primitive, order);
        }
    }

    pub fn insert_primitive(&mut self, primitive: impl Into<Primitive>) {
        self.try_insert_primitive(primitive.into());
    }

    /// Returns whether the primitive was kept, rather than clipped away (or,
    /// QUILL, refused by the window's paint filter).
    fn try_insert_primitive(&mut self, primitive: Primitive) -> bool {
        if self.suppressed {
            return false;
        }
        let clipped_bounds = primitive.clipped_bounds();
        if clipped_bounds.is_empty() {
            return false;
        }
        // QUILL: a primitive nowhere near a rounded mask's corner keeps the
        // fast path (docs/decisions/rounded-content-masks.md, corner culling).
        let mut primitive = primitive;
        primitive.cull_rounded_mask(&clipped_bounds);

        if let Primitive::Lens(lens) = primitive {
            return self.insert_lens(lens, clipped_bounds);
        }

        let kind = primitive.kind();
        if let Some(&layer) = self.layer_stack.last()
            && let Some(layer) = self.layers.get_mut(layer)
        {
            layer.bounds = layer.bounds.join(&clipped_bounds);
            layer.max_kind = layer.max_kind.max(kind);
            layer.max_shared_kind = layer.max_shared_kind.min(kind.max_shared_kind());
            layer.has_native_view |= kind == PrimitiveKind::NativeView;
            let index = self.paint_operations.len();
            match layer.primitives.last_mut() {
                Some(range) if range.end == index => range.end += 1,
                _ => layer.primitives.push(index..index + 1),
            }
            self.paint_operations
                .push(PaintOperation::Primitive(primitive));
        } else {
            let order = self
                .primitive_bounds
                .insert(clipped_bounds, kind, kind.max_shared_kind());
            self.record_primitive(primitive, order);
        }
        true
    }

    /// QUILL: draws `primitive` at `order` and records it with that order,
    /// which a replay of its range shifts as a whole (`replay_as_one`).
    fn record_primitive(&mut self, mut primitive: Primitive, order: DrawOrder) {
        primitive.set_order(order);
        if let Primitive::Lens(lens) = &mut primitive {
            lens.id = u32::try_from(self.lenses.len()).unwrap_or(u32::MAX);
        }
        self.push_primitive(primitive.clone(), order);
        self.paint_operations
            .push(PaintOperation::Primitive(primitive));
    }

    /// QUILL: a lens takes the next band above everything it covers, so it
    /// is drawn after what is under it and every lens with no glass under
    /// it shares one batch. Inside a paint layer (one order for all) or
    /// above the last band it can't be ordered so, and draws as its
    /// fallback quad.
    fn insert_lens(&mut self, lens: Box<BackdropLens>, clipped_bounds: Bounds<ScaledPixels>) -> bool {
        let order = if self.layer_stack.is_empty() {
            self.primitive_bounds
                .insert_band(clipped_bounds, LENS_BAND_BITS, PrimitiveKind::Lens)
        } else {
            None
        };
        match order {
            Some(order) => {
                self.record_primitive(Primitive::Lens(lens), order);
                true
            }
            None => self.try_insert_primitive(lens.fallback_quad().into()),
        }
    }

    fn push_primitive(&mut self, primitive: Primitive, order: DrawOrder) {
        match primitive {
            Primitive::Shadow(shadow) => self.shadows.push(Shadow { order, ..shadow }),
            Primitive::Quad(quad) => self.quads.push(Quad { order, ..quad }),
            Primitive::Path(mut path) => {
                path.order = order;
                self.paths.push(path);
            }
            Primitive::Underline(underline) => {
                self.underlines.push(Underline { order, ..underline })
            }
            Primitive::MonochromeSprite(sprite) => self
                .monochrome_sprites
                .push(MonochromeSprite { order, ..sprite }),
            Primitive::SubpixelSprite(sprite) => self
                .subpixel_sprites
                .push(SubpixelSprite { order, ..sprite }),
            Primitive::PolychromeSprite(sprite) => self
                .polychrome_sprites
                .push(PolychromeSprite { order, ..sprite }),
            Primitive::Surface(mut surface) => {
                surface.order = order;
                self.surfaces.push(surface);
            }
            Primitive::NativeView(native_view) => self.native_views.push(PaintNativeView {
                order,
                ..native_view
            }),
            Primitive::Lens(lens) => {
                let id = u32::try_from(self.lenses.len()).unwrap_or(u32::MAX);
                self.lenses.push(BackdropLens { order, id, ..*lens });
            }
        }
    }

    pub fn replay(&mut self, range: Range<usize>, prev_scene: &Scene) {
        if self.filtered {
            return;
        }
        let Some(operations) = prev_scene.paint_operations.get(range) else {
            return;
        };
        // QUILL: a lens is banded against what is under it now, so a range
        // holding one goes in primitive by primitive.
        let has_lens = operations
            .iter()
            .any(|operation| matches!(operation, PaintOperation::Primitive(Primitive::Lens(_))));
        if self.suppressed || !self.layer_stack.is_empty() || has_lens {
            self.repaint_operations(operations, prev_scene);
        } else {
            self.replay_as_one(operations, prev_scene);
        }
    }

    /// Inserts the operations again one by one, as painting them would.
    fn repaint_operations(&mut self, operations: &[PaintOperation], prev_scene: &Scene) {
        for operation in operations {
            match operation {
                // Animation ids index the scene they were inserted into.
                // QUILL (glass stage 5): an inked label keeps its ink, and a
                // glyph keeps how to rasterize it again.
                PaintOperation::Primitive(Primitive::MonochromeSprite(sprite)) => {
                    self.copy_glyph_source(prev_scene, &sprite.tile, &sprite.bounds);
                    let plain = MonochromeSprite {
                        animation: SpriteAnimationId::NONE,
                        ..*sprite
                    };
                    if let Some(animation) = sprite
                        .animation
                        .animation_index()
                        .and_then(|index| prev_scene.animations.get(index))
                    {
                        self.insert_animated_sprite(plain, *animation);
                    } else if let Some(ink) = sprite
                        .animation
                        .ink_request_index()
                        .and_then(|index| prev_scene.ink_requests.get(index))
                    {
                        self.insert_inked_sprite(plain, *ink);
                    } else {
                        self.insert_primitive(plain);
                    }
                }
                PaintOperation::Primitive(Primitive::PolychromeSprite(sprite)) => {
                    self.copy_glyph_source(prev_scene, &sprite.tile, &sprite.bounds);
                    self.insert_primitive(*sprite);
                }
                PaintOperation::Primitive(primitive) => self.insert_primitive(primitive.clone()),
                PaintOperation::StartLayer => self.push_layer(),
                PaintOperation::EndLayer => self.pop_layer(),
            }
        }
    }

    /// QUILL: the range enters the bounds tree as one box, standing for the
    /// orders it drew at, one above whatever it meets now, and every
    /// primitive moves by the same amount. The recorded orders were worked
    /// out by the frame that painted them.
    fn replay_as_one(&mut self, operations: &[PaintOperation], prev_scene: &Scene) {
        let mut extent: Option<(Bounds<ScaledPixels>, DrawOrder, DrawOrder, PrimitiveKind)> =
            None;
        for operation in operations {
            let PaintOperation::Primitive(primitive) = operation else {
                continue;
            };
            let (bounds, order, kind) = (
                primitive.clipped_bounds(),
                primitive.order(),
                primitive.kind(),
            );
            extent = Some(match extent {
                None => (bounds, order, order, kind),
                Some((union, lowest, highest, top_kind)) => {
                    let top_kind = match order.cmp(&highest) {
                        Ordering::Greater => kind,
                        Ordering::Equal => top_kind.max(kind),
                        Ordering::Less => top_kind,
                    };
                    (
                        union.union(&bounds),
                        lowest.min(order),
                        highest.max(order),
                        top_kind,
                    )
                }
            });
        }
        let shift = extent.map_or(0, |(bounds, lowest, highest, top_kind)| {
            self.primitive_bounds
                .insert_span(bounds, highest.saturating_sub(lowest), top_kind)
                .wrapping_sub(lowest)
        });
        let first_operation = self.paint_operations.len();
        for operation in operations {
            match operation {
                // Animation ids index the scene they were inserted into.
                // QUILL (glass stage 5): an inked label keeps its ink, and a
                // glyph keeps how to rasterize it again.
                PaintOperation::Primitive(Primitive::MonochromeSprite(sprite)) => {
                    self.copy_glyph_source(prev_scene, &sprite.tile, &sprite.bounds);
                    let mut sprite = *sprite;
                    if let Some(index) = sprite.animation.animation_index() {
                        sprite.animation = SpriteAnimationId::NONE;
                        if let Some(animation) = prev_scene.animations.get(index) {
                            let id = SpriteAnimationId::animation(self.animations.len());
                            if id != SpriteAnimationId::NONE {
                                sprite.animation = id;
                                self.animations.push(SpriteAnimation {
                                    base_color: sprite.color,
                                    ..*animation
                                });
                            }
                        }
                    } else if let Some(index) = sprite.animation.ink_request_index() {
                        sprite.animation = match prev_scene.ink_requests.get(index) {
                            Some(ink) => self.ink_request(*ink),
                            None => SpriteAnimationId::NONE,
                        };
                    }
                    sprite.order = sprite.order.wrapping_add(shift);
                    self.paint_operations
                        .push(PaintOperation::Primitive(Primitive::MonochromeSprite(sprite)));
                }
                PaintOperation::Primitive(Primitive::PolychromeSprite(sprite)) => {
                    self.copy_glyph_source(prev_scene, &sprite.tile, &sprite.bounds);
                    let mut sprite = *sprite;
                    sprite.order = sprite.order.wrapping_add(shift);
                    self.paint_operations
                        .push(PaintOperation::Primitive(Primitive::PolychromeSprite(sprite)));
                }
                PaintOperation::Primitive(primitive) => {
                    let mut primitive = primitive.clone();
                    primitive.set_order(primitive.order().wrapping_add(shift));
                    self.paint_operations
                        .push(PaintOperation::Primitive(primitive));
                }
                PaintOperation::StartLayer => self.paint_operations.push(PaintOperation::StartLayer),
                PaintOperation::EndLayer => self.paint_operations.push(PaintOperation::EndLayer),
            }
        }
        // Primitives that share an order and a kind draw in the order they
        // reached the scene, which for a layer is when the outermost layer
        // around it ended: so push them as painting did.
        for index in deferred_layer_sequence(operations) {
            if let Some(PaintOperation::Primitive(primitive)) =
                self.paint_operations.get(first_operation + index)
            {
                let primitive = primitive.clone();
                let order = primitive.order();
                self.push_primitive(primitive, order);
            }
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn refuse_paint_for_test(&mut self, filtered: bool, suppressed: bool) {
        self.filtered = filtered;
        self.suppressed = suppressed;
    }

    pub fn finish(&mut self) {
        debug_assert!(self.layer_stack.is_empty(), "unbalanced scene layers");
        // QUILL: the key buffer is the scene's, so a frame allocates none.
        let keys = &mut self.sort_keys;
        sort_in_place(&mut self.shadows, keys, |shadow| (shadow.order, 0));
        sort_in_place(&mut self.quads, keys, |quad| (quad.order, 0));
        self.paths.sort_by_key(|path| path.order);
        sort_in_place(&mut self.underlines, keys, |underline| (underline.order, 0));
        sort_in_place(&mut self.monochrome_sprites, keys, |sprite| {
            (sprite.order, sprite.tile.texture_id.index)
        });
        sort_in_place(&mut self.subpixel_sprites, keys, |sprite| {
            (sprite.order, sprite.tile.texture_id.index)
        });
        sort_in_place(&mut self.polychrome_sprites, keys, |sprite| {
            (sprite.order, sprite.tile.texture_id.index)
        });
        self.surfaces.sort_by_key(|surface| surface.order);
        self.native_views
            .sort_by_key(|native_view| native_view.order);
        sort_in_place(&mut self.lenses, keys, |lens| (lens.order, 0));
        self.keep_first_painted_native_views();

        // Sorting moved the sprites, so animations learn where theirs ended up.
        // QUILL: the sprite then carries no animation, which the renderers,
        // reading the same slot as a label's ink, take as none.
        if !self.animations.is_empty() {
            for (index, sprite) in self.monochrome_sprites.iter_mut().enumerate() {
                if let Some(animation) = sprite.animation.animation_index() {
                    if let Some(animation) = self.animations.get_mut(animation) {
                        animation.sprite_index = index;
                    }
                    sprite.animation = SpriteAnimationId::NONE;
                }
            }
        }
        self.resolve_inks();
    }

    /// QUILL (glass stage 5): paints a label sprite whose colour the renderer
    /// picks from the glass under it (`ink`). Where there is no glass, or
    /// the renderer can't ink it, it draws in its own colour.
    pub fn insert_inked_sprite(&mut self, mut sprite: MonochromeSprite, ink: LensInk) {
        if self.suppressed {
            return;
        }
        sprite.animation = self.ink_request(ink);
        self.insert_primitive(sprite);
    }

    /// The tag of a sprite inked with `ink`, which names its request.
    fn ink_request(&mut self, ink: LensInk) -> SpriteAnimationId {
        if self.ink_requests.last() != Some(&ink) {
            self.ink_requests.push(ink);
        }
        SpriteAnimationId::ink_request(self.ink_requests.len().saturating_sub(1))
    }

    /// Each inked label's lens: the highest lens below it whose glass holds
    /// its centre. Rewrites the sprites' tag from request to entry.
    fn resolve_inks(&mut self) {
        self.inks.clear();
        if self.ink_requests.is_empty() {
            return;
        }
        let glass: Vec<(DrawOrder, Bounds<ScaledPixels>)> = self
            .lenses
            .iter()
            .map(|lens| (lens.order, lens.glass_bounds()))
            .collect();
        let mut entries: collections::FxHashMap<(usize, u32), usize> = Default::default();
        for sprite in &mut self.monochrome_sprites {
            let Some(request) = sprite.animation.ink_request_index() else {
                continue;
            };
            let Some(ink) = self.ink_requests.get(request).copied() else {
                sprite.animation = SpriteAnimationId::NONE;
                continue;
            };
            let center = sprite.bounds.center();
            let lens = glass
                .iter()
                .rposition(|(order, bounds)| *order < sprite.order && bounds.contains(&center))
                .and_then(|i| u32::try_from(i).ok())
                .unwrap_or(LensInkEntry::NO_LENS);
            let inks = &mut self.inks;
            let entry = *entries.entry((request, lens)).or_insert_with(|| {
                inks.push(LensInkEntry { lens, ink });
                inks.len()
            });
            sprite.animation = SpriteAnimationId::ink_entry(entry);
        }
    }

    /// QUILL (glass stage 5): a hash of an inked sprite's colours (0 if it is not
    /// inked), after `finish`.
    pub(crate) fn ink_hash(&self, sprite: &MonochromeSprite) -> u64 {
        use std::hash::{DefaultHasher, Hasher};
        let Some(entry) = sprite.ink_entry().and_then(|i| self.inks.get(i)) else {
            return 0;
        };
        let mut hasher = DefaultHasher::new();
        for color in [entry.ink.on_light, entry.ink.on_dark] {
            for v in [color.h, color.s, color.l, color.a] {
                hasher.write_u32(v.to_bits());
            }
        }
        hasher.write_u32(entry.lens);
        hasher.finish()
    }

    /// QUILL (glass stage 5): records how to rasterize the sprite just painted at
    /// `tile` and `bounds` again.
    pub(crate) fn record_glyph_source(
        &mut self,
        tile: &AtlasTile,
        bounds: &Bounds<ScaledPixels>,
        source: GlyphSource,
    ) {
        self.glyph_sources
            .insert(GlyphKey::new(tile, bounds), source);
    }

    pub(crate) fn glyph_source(
        &self,
        tile: &AtlasTile,
        bounds: &Bounds<ScaledPixels>,
    ) -> Option<&GlyphSource> {
        self.glyph_sources.get(&GlyphKey::new(tile, bounds))
    }

    fn copy_glyph_source(
        &mut self,
        prev_scene: &Scene,
        tile: &AtlasTile,
        bounds: &Bounds<ScaledPixels>,
    ) {
        if prev_scene.glyph_sources.is_empty() {
            return;
        }
        if let Some(source) = prev_scene.glyph_source(tile, bounds) {
            self.record_glyph_source(tile, bounds, source.clone());
        }
    }

    /// Drops every occurrence of a native view but the one painted first,
    /// because a platform view can only be in one place.
    ///
    /// That is the occurrence whose hitbox takes the view's mouse input. It
    /// isn't always the first in the drawing order, which depends on what each
    /// occurrence overlaps.
    fn keep_first_painted_native_views(&mut self) {
        let native_views = &self.native_views;
        let is_repeated = native_views.iter().enumerate().any(|(index, native_view)| {
            native_views[..index]
                .iter()
                .any(|earlier| earlier.id == native_view.id)
        });
        if !is_repeated {
            return;
        }
        let mut first_painted: Vec<(NativeViewId, HitboxId)> = Vec::new();
        for operation in &self.paint_operations {
            if let PaintOperation::Primitive(Primitive::NativeView(native_view)) = operation
                && !first_painted.iter().any(|(id, _)| *id == native_view.id)
            {
                first_painted.push((native_view.id, native_view.hitbox_id));
            }
        }
        self.native_views
            .retain(|native_view| first_painted.contains(&(native_view.id, native_view.hitbox_id)));
    }

    #[cfg_attr(
        all(
            any(target_os = "linux", target_os = "freebsd"),
            not(any(feature = "x11", feature = "wayland"))
        ),
        allow(dead_code)
    )]
    pub fn batches(&self) -> impl Iterator<Item = PrimitiveBatch> + '_ {
        BatchIterator {
            shadows_start: 0,
            shadows_iter: self.shadows.iter().peekable(),
            quads_start: 0,
            quads_iter: self.quads.iter().peekable(),
            paths_start: 0,
            paths_iter: self.paths.iter().peekable(),
            underlines_start: 0,
            underlines_iter: self.underlines.iter().peekable(),
            monochrome_sprites_start: 0,
            monochrome_sprites_iter: self.monochrome_sprites.iter().peekable(),
            subpixel_sprites_start: 0,
            subpixel_sprites_iter: self.subpixel_sprites.iter().peekable(),
            polychrome_sprites_start: 0,
            polychrome_sprites_iter: self.polychrome_sprites.iter().peekable(),
            surfaces_start: 0,
            surfaces_iter: self.surfaces.iter().peekable(),
            native_views_start: 0,
            native_views_iter: self.native_views.iter().peekable(),
            lenses_start: 0,
            lenses_iter: self.lenses.iter().peekable(),
        }
    }
}

/// Sorts small `(key, index)` pairs instead of the primitives themselves, then moves each
/// primitive into place once. Primitives are large, so a comparison sort that shuffles them
/// directly spends most of its time copying. Ties keep insertion order, as `sort_by_key`
/// does, because the index is part of what is sorted.
fn sort_in_place<T: Copy>(
    primitives: &mut [T],
    keys: &mut Vec<(u64, u32)>,
    key: impl Fn(&T) -> (u32, u32),
) {
    if primitives.is_sorted_by_key(&key) {
        return;
    }
    keys.clear();
    keys.extend(primitives.iter().enumerate().map(|(index, primitive)| {
        let (major, minor) = key(primitive);
        ((major as u64) << 32 | minor as u64, index as u32)
    }));
    keys.sort_unstable();

    // `keys[target].1` is the source index for `target`. Follow each cycle of that
    // permutation, marking visited slots by pointing them at themselves.
    for start in 0..primitives.len() {
        if keys[start].1 as usize == start {
            continue;
        }
        let displaced = primitives[start];
        let mut target = start;
        loop {
            let source = keys[target].1 as usize;
            keys[target].1 = target as u32;
            if source == start {
                primitives[target] = displaced;
                break;
            }
            primitives[target] = primitives[source];
            target = source;
        }
    }
}

/// The depth step between consecutive quads in a frame with `quad_count` quads.
pub fn quad_depth_scale(quad_count: usize) -> f32 {
    let denominator = (quad_count.min((1 << 24) - 1) + 1).next_power_of_two();
    1.0 / denominator as f32
}

/// Maps a quad's index in [`Scene::quads`] to depth, with greater values closer.
pub fn quad_depth(quad_id: u32, scale: f32) -> f32 {
    ((quad_id + 1) as f32 * scale).min(1.0)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Default)]
#[cfg_attr(
    all(
        any(target_os = "linux", target_os = "freebsd"),
        not(any(feature = "x11", feature = "wayland"))
    ),
    allow(dead_code)
)]
pub(crate) enum PrimitiveKind {
    Shadow,
    #[default]
    Quad,
    Path,
    Underline,
    MonochromeSprite,
    SubpixelSprite,
    PolychromeSprite,
    Surface,
    NativeView,
    /// QUILL: backdrop glass, which takes a band of its own (never shared).
    Lens,
}

impl PrimitiveKind {
    /// We guarantee that overlapping primitives are drawn in paint order.
    /// But, to improve GPU draw batching, the renderer sorts primitives that
    /// share a draw order by kind, then by atlas texture, and only then in
    /// paint order. So, a primitive can only share the draw order of the
    /// topmost primitive it overlaps if that primitive's kind is at most the
    /// one returned here. Otherwise, it needs a new draw order.
    fn max_shared_kind(self) -> Self {
        match self {
            Self::MonochromeSprite => Self::Underline,
            Self::SubpixelSprite => Self::MonochromeSprite,
            Self::PolychromeSprite => Self::SubpixelSprite,
            kind => kind,
        }
    }
}

/// QUILL: the indices of `operations`' primitives in the order painting
/// them puts them in the scene. A primitive in a layer goes in when the
/// outermost layer around it ends, layer by layer in the order they started
/// (`Scene::pop_layer`); any other goes in at once.
fn deferred_layer_sequence(operations: &[PaintOperation]) -> Vec<usize> {
    let mut sequence = Vec::with_capacity(operations.len());
    let mut layers: Vec<Vec<usize>> = Vec::new();
    let mut open: Vec<usize> = Vec::new();
    for (index, operation) in operations.iter().enumerate() {
        match operation {
            PaintOperation::StartLayer => {
                open.push(layers.len());
                layers.push(Vec::new());
            }
            PaintOperation::EndLayer => {
                open.pop();
                if open.is_empty() {
                    sequence.extend(layers.drain(..).flatten());
                }
            }
            PaintOperation::Primitive(_) => match open.last().and_then(|&l| layers.get_mut(l)) {
                Some(layer) => layer.push(index),
                None => sequence.push(index),
            },
        }
    }
    // A range that ends inside a layer.
    sequence.extend(layers.into_iter().flatten());
    sequence
}

pub(crate) enum PaintOperation {
    Primitive(Primitive),
    StartLayer,
    EndLayer,
}

#[derive(Clone)]
#[expect(missing_docs)]
pub enum Primitive {
    Shadow(Shadow),
    Quad(Quad),
    Path(PaintedPath),
    Underline(Underline),
    MonochromeSprite(MonochromeSprite),
    SubpixelSprite(SubpixelSprite),
    PolychromeSprite(PolychromeSprite),
    Surface(PaintSurface),
    NativeView(PaintNativeView),
    /// QUILL: backdrop glass, boxed so the rare lens does not grow every
    /// primitive the frame records.
    Lens(Box<BackdropLens>),
}

#[expect(missing_docs)]
impl Primitive {
    pub(crate) fn kind(&self) -> PrimitiveKind {
        match self {
            Primitive::Shadow(_) => PrimitiveKind::Shadow,
            Primitive::Quad(_) => PrimitiveKind::Quad,
            Primitive::Path(_) => PrimitiveKind::Path,
            Primitive::Underline(_) => PrimitiveKind::Underline,
            Primitive::MonochromeSprite(_) => PrimitiveKind::MonochromeSprite,
            Primitive::SubpixelSprite(_) => PrimitiveKind::SubpixelSprite,
            Primitive::PolychromeSprite(_) => PrimitiveKind::PolychromeSprite,
            Primitive::Surface(_) => PrimitiveKind::Surface,
            Primitive::NativeView(_) => PrimitiveKind::NativeView,
            Primitive::Lens(_) => PrimitiveKind::Lens,
        }
    }

    pub fn bounds(&self) -> &Bounds<ScaledPixels> {
        match self {
            Primitive::Shadow(shadow) => &shadow.bounds,
            Primitive::Quad(quad) => &quad.bounds,
            Primitive::Path(path) => &path.bounds,
            Primitive::Underline(underline) => &underline.bounds,
            Primitive::MonochromeSprite(sprite) => &sprite.bounds,
            Primitive::SubpixelSprite(sprite) => &sprite.bounds,
            Primitive::PolychromeSprite(sprite) => &sprite.bounds,
            Primitive::Surface(surface) => &surface.bounds,
            Primitive::NativeView(native_view) => &native_view.bounds,
            Primitive::Lens(lens) => &lens.bounds,
        }
    }

    fn transformation(&self) -> TransformationMatrix {
        match self {
            Primitive::MonochromeSprite(sprite) => sprite.transformation,
            Primitive::SubpixelSprite(sprite) => sprite.transformation,
            _ => TransformationMatrix::unit(),
        }
    }

    fn clipped_bounds(&self) -> Bounds<ScaledPixels> {
        self.transformation()
            .transform_bounds(*self.bounds())
            .intersect(&self.content_mask().bounds)
    }

    /// QUILL: clears the rounded shape of the primitive's mask when the
    /// primitive (clipped to the mask's rectangle) reaches none of its corners.
    fn cull_rounded_mask(&mut self, clipped_bounds: &Bounds<ScaledPixels>) {
        let mask = match self {
            Primitive::Shadow(shadow) => &mut shadow.content_mask,
            Primitive::Quad(quad) => &mut quad.content_mask,
            Primitive::Path(path) => &mut path.paint.content_mask,
            Primitive::Underline(underline) => &mut underline.content_mask,
            Primitive::MonochromeSprite(sprite) => &mut sprite.content_mask,
            Primitive::SubpixelSprite(sprite) => &mut sprite.content_mask,
            Primitive::PolychromeSprite(sprite) => &mut sprite.content_mask,
            Primitive::Surface(surface) => &mut surface.content_mask,
            Primitive::NativeView(native_view) => &mut native_view.content_mask,
            Primitive::Lens(lens) => &mut lens.content_mask,
        };
        if !mask.rounded.is_none() {
            *mask = mask.culled_for(clipped_bounds);
        }
    }

    pub fn content_mask(&self) -> &ContentMask<ScaledPixels> {
        match self {
            Primitive::Shadow(shadow) => &shadow.content_mask,
            Primitive::Quad(quad) => &quad.content_mask,
            Primitive::Path(path) => &path.paint.content_mask,
            Primitive::Underline(underline) => &underline.content_mask,
            Primitive::MonochromeSprite(sprite) => &sprite.content_mask,
            Primitive::SubpixelSprite(sprite) => &sprite.content_mask,
            Primitive::PolychromeSprite(sprite) => &sprite.content_mask,
            Primitive::Surface(surface) => &surface.content_mask,
            Primitive::NativeView(native_view) => &native_view.content_mask,
            Primitive::Lens(lens) => &lens.content_mask,
        }
    }

    /// QUILL: the order the primitive was drawn at, which a recorded
    /// primitive keeps for `Scene::replay`.
    fn order(&self) -> DrawOrder {
        match self {
            Primitive::Shadow(shadow) => shadow.order,
            Primitive::Quad(quad) => quad.order,
            Primitive::Path(path) => path.order,
            Primitive::Underline(underline) => underline.order,
            Primitive::MonochromeSprite(sprite) => sprite.order,
            Primitive::SubpixelSprite(sprite) => sprite.order,
            Primitive::PolychromeSprite(sprite) => sprite.order,
            Primitive::Surface(surface) => surface.order,
            Primitive::NativeView(native_view) => native_view.order,
            Primitive::Lens(lens) => lens.order,
        }
    }

    fn set_order(&mut self, order: DrawOrder) {
        match self {
            Primitive::Shadow(shadow) => shadow.order = order,
            Primitive::Quad(quad) => quad.order = order,
            Primitive::Path(path) => path.order = order,
            Primitive::Underline(underline) => underline.order = order,
            Primitive::MonochromeSprite(sprite) => sprite.order = order,
            Primitive::SubpixelSprite(sprite) => sprite.order = order,
            Primitive::PolychromeSprite(sprite) => sprite.order = order,
            Primitive::Surface(surface) => surface.order = order,
            Primitive::NativeView(native_view) => native_view.order = order,
            Primitive::Lens(lens) => lens.order = order,
        }
    }
}

#[cfg_attr(
    all(
        any(target_os = "linux", target_os = "freebsd"),
        not(any(feature = "x11", feature = "wayland"))
    ),
    allow(dead_code)
)]
struct BatchIterator<'a> {
    shadows_start: usize,
    shadows_iter: Peekable<slice::Iter<'a, Shadow>>,
    quads_start: usize,
    quads_iter: Peekable<slice::Iter<'a, Quad>>,
    paths_start: usize,
    paths_iter: Peekable<slice::Iter<'a, PaintedPath>>,
    underlines_start: usize,
    underlines_iter: Peekable<slice::Iter<'a, Underline>>,
    monochrome_sprites_start: usize,
    monochrome_sprites_iter: Peekable<slice::Iter<'a, MonochromeSprite>>,
    subpixel_sprites_start: usize,
    subpixel_sprites_iter: Peekable<slice::Iter<'a, SubpixelSprite>>,
    polychrome_sprites_start: usize,
    polychrome_sprites_iter: Peekable<slice::Iter<'a, PolychromeSprite>>,
    surfaces_start: usize,
    surfaces_iter: Peekable<slice::Iter<'a, PaintSurface>>,
    native_views_start: usize,
    native_views_iter: Peekable<slice::Iter<'a, PaintNativeView>>,
    lenses_start: usize,
    lenses_iter: Peekable<slice::Iter<'a, BackdropLens>>,
}

impl<'a> Iterator for BatchIterator<'a> {
    type Item = PrimitiveBatch;

    fn next(&mut self) -> Option<Self::Item> {
        let mut orders_and_kinds = [
            (
                self.shadows_iter.peek().map(|s| s.order),
                PrimitiveKind::Shadow,
            ),
            (self.quads_iter.peek().map(|q| q.order), PrimitiveKind::Quad),
            (
                self.paths_iter.peek().map(|path| path.order),
                PrimitiveKind::Path,
            ),
            (
                self.underlines_iter.peek().map(|u| u.order),
                PrimitiveKind::Underline,
            ),
            (
                self.monochrome_sprites_iter.peek().map(|s| s.order),
                PrimitiveKind::MonochromeSprite,
            ),
            (
                self.subpixel_sprites_iter.peek().map(|s| s.order),
                PrimitiveKind::SubpixelSprite,
            ),
            (
                self.polychrome_sprites_iter.peek().map(|s| s.order),
                PrimitiveKind::PolychromeSprite,
            ),
            (
                self.surfaces_iter.peek().map(|s| s.order),
                PrimitiveKind::Surface,
            ),
            (
                self.native_views_iter
                    .peek()
                    .map(|native_view| native_view.order),
                PrimitiveKind::NativeView,
            ),
            (
                self.lenses_iter.peek().map(|lens| lens.order),
                PrimitiveKind::Lens,
            ),
        ];
        orders_and_kinds.sort_by_key(|(order, kind)| (order.unwrap_or(u32::MAX), *kind));

        let first = orders_and_kinds[0];
        let second = orders_and_kinds[1];
        let (batch_kind, max_order_and_kind) = if first.0.is_some() {
            (first.1, (second.0.unwrap_or(u32::MAX), second.1))
        } else {
            return None;
        };

        match batch_kind {
            PrimitiveKind::Shadow => {
                let shadows_start = self.shadows_start;
                let mut shadows_end = shadows_start + 1;
                self.shadows_iter.next();
                while self
                    .shadows_iter
                    .next_if(|shadow| (shadow.order, batch_kind) < max_order_and_kind)
                    .is_some()
                {
                    shadows_end += 1;
                }
                self.shadows_start = shadows_end;
                Some(PrimitiveBatch::Shadows(shadows_start..shadows_end))
            }
            PrimitiveKind::Quad => {
                let quads_start = self.quads_start;
                let mut quads_end = quads_start + 1;
                self.quads_iter.next();
                while self
                    .quads_iter
                    .next_if(|quad| (quad.order, batch_kind) < max_order_and_kind)
                    .is_some()
                {
                    quads_end += 1;
                }
                self.quads_start = quads_end;
                Some(PrimitiveBatch::Quads(quads_start..quads_end))
            }
            PrimitiveKind::Path => {
                let paths_start = self.paths_start;
                let mut paths_end = paths_start + 1;
                self.paths_iter.next();
                while self
                    .paths_iter
                    .next_if(|path| (path.order, batch_kind) < max_order_and_kind)
                    .is_some()
                {
                    paths_end += 1;
                }
                self.paths_start = paths_end;
                Some(PrimitiveBatch::Paths(paths_start..paths_end))
            }
            PrimitiveKind::Underline => {
                let underlines_start = self.underlines_start;
                let mut underlines_end = underlines_start + 1;
                self.underlines_iter.next();
                while self
                    .underlines_iter
                    .next_if(|underline| (underline.order, batch_kind) < max_order_and_kind)
                    .is_some()
                {
                    underlines_end += 1;
                }
                self.underlines_start = underlines_end;
                Some(PrimitiveBatch::Underlines(underlines_start..underlines_end))
            }
            PrimitiveKind::MonochromeSprite => {
                let texture_id = self.monochrome_sprites_iter.peek().unwrap().tile.texture_id;
                let sprites_start = self.monochrome_sprites_start;
                let mut sprites_end = sprites_start + 1;
                self.monochrome_sprites_iter.next();
                while self
                    .monochrome_sprites_iter
                    .next_if(|sprite| {
                        (sprite.order, batch_kind) < max_order_and_kind
                            && sprite.tile.texture_id == texture_id
                    })
                    .is_some()
                {
                    sprites_end += 1;
                }
                self.monochrome_sprites_start = sprites_end;
                Some(PrimitiveBatch::MonochromeSprites {
                    texture_id,
                    range: sprites_start..sprites_end,
                })
            }
            PrimitiveKind::SubpixelSprite => {
                let texture_id = self.subpixel_sprites_iter.peek().unwrap().tile.texture_id;
                let sprites_start = self.subpixel_sprites_start;
                let mut sprites_end = sprites_start + 1;
                self.subpixel_sprites_iter.next();
                while self
                    .subpixel_sprites_iter
                    .next_if(|sprite| {
                        (sprite.order, batch_kind) < max_order_and_kind
                            && sprite.tile.texture_id == texture_id
                    })
                    .is_some()
                {
                    sprites_end += 1;
                }
                self.subpixel_sprites_start = sprites_end;
                Some(PrimitiveBatch::SubpixelSprites {
                    texture_id,
                    range: sprites_start..sprites_end,
                })
            }
            PrimitiveKind::PolychromeSprite => {
                let texture_id = self.polychrome_sprites_iter.peek().unwrap().tile.texture_id;
                let sprites_start = self.polychrome_sprites_start;
                let mut sprites_end = sprites_start + 1;
                self.polychrome_sprites_iter.next();
                while self
                    .polychrome_sprites_iter
                    .next_if(|sprite| {
                        (sprite.order, batch_kind) < max_order_and_kind
                            && sprite.tile.texture_id == texture_id
                    })
                    .is_some()
                {
                    sprites_end += 1;
                }
                self.polychrome_sprites_start = sprites_end;
                Some(PrimitiveBatch::PolychromeSprites {
                    texture_id,
                    range: sprites_start..sprites_end,
                })
            }
            PrimitiveKind::Surface => {
                let surfaces_start = self.surfaces_start;
                let mut surfaces_end = surfaces_start + 1;
                self.surfaces_iter.next();
                while self
                    .surfaces_iter
                    .next_if(|surface| (surface.order, batch_kind) < max_order_and_kind)
                    .is_some()
                {
                    surfaces_end += 1;
                }
                self.surfaces_start = surfaces_end;
                Some(PrimitiveBatch::Surfaces(surfaces_start..surfaces_end))
            }
            PrimitiveKind::NativeView => {
                let native_views_start = self.native_views_start;
                let mut native_views_end = native_views_start + 1;
                self.native_views_iter.next();
                while self
                    .native_views_iter
                    .next_if(|native_view| (native_view.order, batch_kind) < max_order_and_kind)
                    .is_some()
                {
                    native_views_end += 1;
                }
                self.native_views_start = native_views_end;
                Some(PrimitiveBatch::NativeViews(
                    native_views_start..native_views_end,
                ))
            }
            PrimitiveKind::Lens => {
                let lenses_start = self.lenses_start;
                let mut lenses_end = lenses_start + 1;
                // A batch is one band: a lens in a higher band covers one of
                // these, so it needs a copy of the frame they are drawn into.
                let band = self.lenses_iter.next().map(|lens| lens.order);
                while self
                    .lenses_iter
                    .next_if(|lens| {
                        Some(lens.order) == band && (lens.order, batch_kind) < max_order_and_kind
                    })
                    .is_some()
                {
                    lenses_end += 1;
                }
                self.lenses_start = lenses_end;
                Some(PrimitiveBatch::Lenses(lenses_start..lenses_end))
            }
        }
    }
}

#[derive(Debug)]
#[cfg_attr(
    all(
        any(target_os = "linux", target_os = "freebsd"),
        not(any(feature = "x11", feature = "wayland"))
    ),
    allow(dead_code)
)]
#[allow(missing_docs)]
pub enum PrimitiveBatch {
    Shadows(Range<usize>),
    Quads(Range<usize>),
    Paths(Range<usize>),
    Underlines(Range<usize>),
    MonochromeSprites {
        texture_id: AtlasTextureId,
        range: Range<usize>,
    },
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    SubpixelSprites {
        texture_id: AtlasTextureId,
        range: Range<usize>,
    },
    PolychromeSprites {
        texture_id: AtlasTextureId,
        range: Range<usize>,
    },
    Surfaces(Range<usize>),
    NativeViews(Range<usize>),
    /// QUILL: backdrop glass. Lenses of one batch share an order, so none
    /// covers another.
    Lenses(Range<usize>),
}

impl PrimitiveBatch {
    #[expect(missing_docs)]
    pub fn label(&self) -> String {
        match self {
            Self::Shadows(range) => format!("shadows ({})", range.len()),
            Self::Quads(range) => format!("quads ({})", range.len()),
            Self::Paths(range) => format!("paths ({})", range.len()),
            Self::Underlines(range) => format!("underlines ({})", range.len()),
            Self::MonochromeSprites { texture_id, range } => {
                format!(
                    "monochrome sprites ({}) on atlas {}",
                    range.len(),
                    texture_id.index
                )
            }
            Self::SubpixelSprites { texture_id, range } => {
                format!(
                    "subpixel sprites ({}) on atlas {}",
                    range.len(),
                    texture_id.index
                )
            }
            Self::PolychromeSprites { texture_id, range } => {
                format!(
                    "polychrome sprites ({}) on atlas {}",
                    range.len(),
                    texture_id.index
                )
            }
            Self::Surfaces(range) => format!("surfaces ({})", range.len()),
            Self::NativeViews(range) => format!("native views ({})", range.len()),
            Self::Lenses(range) => format!("lenses ({})", range.len()),
        }
    }
}

#[derive(Default, Debug, Copy, Clone)]
#[repr(C)]
#[expect(missing_docs)]
pub struct Quad {
    pub order: DrawOrder,
    pub border_style: BorderStyle,
    pub bounds: Bounds<ScaledPixels>,
    pub content_mask: ContentMask<ScaledPixels>,
    pub background: Background,
    pub border_color: Hsla,
    pub corner_radii: Corners<ScaledPixels>,
    pub border_widths: Edges<ScaledPixels>,
}

impl From<Quad> for Primitive {
    fn from(quad: Quad) -> Self {
        Primitive::Quad(quad)
    }
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
#[expect(missing_docs)]
pub struct Underline {
    pub order: DrawOrder,
    pub pad: u32, // align to 8 bytes
    pub bounds: Bounds<ScaledPixels>,
    pub content_mask: ContentMask<ScaledPixels>,
    pub color: Hsla,
    pub thickness: ScaledPixels,
    pub wavy: PaddedBool32,
}

impl From<Underline> for Primitive {
    fn from(underline: Underline) -> Self {
        Primitive::Underline(underline)
    }
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
#[expect(missing_docs)]
pub struct Shadow {
    pub order: DrawOrder,
    pub blur_radius: ScaledPixels,
    pub bounds: Bounds<ScaledPixels>,
    pub corner_radii: Corners<ScaledPixels>,
    pub content_mask: ContentMask<ScaledPixels>,
    pub color: Hsla,
    pub shape_bounds: Bounds<ScaledPixels>,
    pub shape_corner_radii: Corners<ScaledPixels>,
    pub inset: PaddedBool32,
    pub pad: u32, // align to 8 bytes
}

impl From<Shadow> for Primitive {
    fn from(shadow: Shadow) -> Self {
        Primitive::Shadow(shadow)
    }
}

/// The style of a border.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[repr(C)]
pub enum BorderStyle {
    /// A solid border.
    #[default]
    Solid = 0,
    /// A dashed border.
    Dashed = 1,
}

/// A data type representing a 2 dimensional transformation that can be applied to an element.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct TransformationMatrix {
    /// 2x2 matrix containing rotation and scale,
    /// stored row-major
    pub rotation_scale: [[f32; 2]; 2],
    /// translation vector
    pub translation: [f32; 2],
}

impl Eq for TransformationMatrix {}

impl TransformationMatrix {
    /// The unit matrix, has no effect.
    pub fn unit() -> Self {
        Self {
            rotation_scale: [[1.0, 0.0], [0.0, 1.0]],
            translation: [0.0, 0.0],
        }
    }

    /// Move the origin by a given point
    pub fn translate(self, point: Point<ScaledPixels>) -> Self {
        self.compose(Self {
            rotation_scale: [[1.0, 0.0], [0.0, 1.0]],
            translation: [point.x.0, point.y.0],
        })
    }

    /// Clockwise rotation in radians around the origin
    pub fn rotate(self, angle: Radians) -> Self {
        self.compose(Self {
            rotation_scale: [
                [angle.0.cos(), -angle.0.sin()],
                [angle.0.sin(), angle.0.cos()],
            ],
            translation: [0.0, 0.0],
        })
    }

    /// Scale around the origin
    pub fn scale(self, size: Size<f32>) -> Self {
        self.compose(Self {
            rotation_scale: [[size.width, 0.0], [0.0, size.height]],
            translation: [0.0, 0.0],
        })
    }

    /// Perform matrix multiplication with another transformation
    /// to produce a new transformation that is the result of
    /// applying both transformations: first, `other`, then `self`.
    #[inline]
    pub fn compose(self, other: TransformationMatrix) -> TransformationMatrix {
        if other == Self::unit() {
            return self;
        }
        // Perform matrix multiplication
        TransformationMatrix {
            rotation_scale: [
                [
                    self.rotation_scale[0][0] * other.rotation_scale[0][0]
                        + self.rotation_scale[0][1] * other.rotation_scale[1][0],
                    self.rotation_scale[0][0] * other.rotation_scale[0][1]
                        + self.rotation_scale[0][1] * other.rotation_scale[1][1],
                ],
                [
                    self.rotation_scale[1][0] * other.rotation_scale[0][0]
                        + self.rotation_scale[1][1] * other.rotation_scale[1][0],
                    self.rotation_scale[1][0] * other.rotation_scale[0][1]
                        + self.rotation_scale[1][1] * other.rotation_scale[1][1],
                ],
            ],
            translation: [
                self.translation[0]
                    + self.rotation_scale[0][0] * other.translation[0]
                    + self.rotation_scale[0][1] * other.translation[1],
                self.translation[1]
                    + self.rotation_scale[1][0] * other.translation[0]
                    + self.rotation_scale[1][1] * other.translation[1],
            ],
        }
    }

    /// The smallest bounds that contain `bounds` once it is transformed.
    pub(crate) fn transform_bounds(&self, bounds: Bounds<ScaledPixels>) -> Bounds<ScaledPixels> {
        if *self == Self::unit() {
            return bounds;
        }
        let [[xx, xy], [yx, yy]] = self.rotation_scale;
        let [translate_x, translate_y] = self.translation;
        let corners = [
            bounds.origin,
            bounds.top_right(),
            bounds.bottom_left(),
            bounds.bottom_right(),
        ]
        .map(|corner| {
            Point::new(
                ScaledPixels(xx * corner.x.0 + xy * corner.y.0 + translate_x),
                ScaledPixels(yx * corner.x.0 + yy * corner.y.0 + translate_y),
            )
        });
        let [first, rest @ ..] = corners;
        let top_left = rest.iter().fold(first, |min, corner| min.min(corner));
        let bottom_right = rest.iter().fold(first, |max, corner| max.max(corner));
        Bounds::from_corners(top_left, bottom_right)
    }

    /// Apply transformation to a point, mainly useful for debugging
    pub fn apply(&self, point: Point<Pixels>) -> Point<Pixels> {
        let input = [point.x.0, point.y.0];
        let mut output = self.translation;
        for (i, output_cell) in output.iter_mut().enumerate() {
            for (k, input_cell) in input.iter().enumerate() {
                *output_cell += self.rotation_scale[i][k] * *input_cell;
            }
        }
        Point::new(output[0].into(), output[1].into())
    }
}

impl Default for TransformationMatrix {
    fn default() -> Self {
        Self::unit()
    }
}

#[derive(Copy, Clone, Debug)]
#[repr(C)]
#[expect(missing_docs)]
pub struct MonochromeSprite {
    pub order: DrawOrder,
    pub animation: SpriteAnimationId,
    pub tile: AtlasTile,
    pub bounds: Bounds<ScaledPixels>,
    pub content_mask: ContentMask<ScaledPixels>,
    pub color: Hsla,
    pub transformation: TransformationMatrix,
}

impl MonochromeSprite {
    /// QUILL (glass stage 5): after [`Scene::finish`], the index in
    /// [`Scene::inks`] of the ink this label takes from the glass under it.
    pub fn ink_entry(&self) -> Option<usize> {
        self.animation.ink_entry_index()
    }
}

/// Refers to an animation of the scene a sprite is inserted into.
///
/// A single `u32` so it fits the sprite's existing padding and leaves the
/// instance layout unchanged on every backend; renderers ignore it.
///
/// QUILL (glass stage 5): the same slot carries a glass label's ink, which
/// Quill's sprite shaders read. Until [`Scene::finish`] it holds an
/// animation (its index + 1) or an ink request (`INK_REQUEST` | its index);
/// after it, 0 or the label's ink entry + 1, so an animated sprite reads as
/// carrying no ink. Zero is none, for both.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct SpriteAnimationId(u32);

impl SpriteAnimationId {
    /// No animation (and, QUILL, no ink).
    pub const NONE: Self = Self(0);
    /// QUILL: marks an ink request, before `Scene::finish`.
    const INK_REQUEST: u32 = 1 << 31;

    /// The animation at `index` in the scene's list.
    fn animation(index: usize) -> Self {
        u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(1))
            .filter(|id| *id < Self::INK_REQUEST)
            .map_or(Self::NONE, Self)
    }

    fn animation_index(self) -> Option<usize> {
        (self.0 != 0 && self.0 < Self::INK_REQUEST).then(|| (self.0 - 1) as usize)
    }

    fn ink_request(request: usize) -> Self {
        u32::try_from(request)
            .ok()
            .filter(|request| *request < Self::INK_REQUEST)
            .map_or(Self::NONE, |request| Self(request | Self::INK_REQUEST))
    }

    fn ink_request_index(self) -> Option<usize> {
        (self.0 & Self::INK_REQUEST != 0).then(|| (self.0 & !Self::INK_REQUEST) as usize)
    }

    /// The `count`-th ink entry (counting from 1), after `Scene::finish`.
    fn ink_entry(count: usize) -> Self {
        u32::try_from(count)
            .ok()
            .filter(|count| *count < Self::INK_REQUEST)
            .map_or(Self::NONE, Self)
    }

    fn ink_entry_index(self) -> Option<usize> {
        self.0.checked_sub(1).map(|index| index as usize)
    }
}

impl Default for SpriteAnimationId {
    fn default() -> Self {
        Self::NONE
    }
}

/// A [`TimedTransformation`] the window applies to a sprite right before
/// every present, so the scene can be presented again without being redrawn.
#[derive(Copy, Clone, Debug)]
pub(crate) struct SpriteAnimation {
    transformation: TimedTransformation,
    center: Point<Pixels>,
    scale_factor: f32,
    base_color: Hsla,
    sprite_index: usize,
}

impl SpriteAnimation {
    pub(crate) fn new(
        transformation: TimedTransformation,
        center: Point<Pixels>,
        scale_factor: f32,
    ) -> Self {
        Self {
            transformation,
            center,
            scale_factor,
            base_color: Hsla::default(),
            sprite_index: usize::MAX,
        }
    }
}

impl From<MonochromeSprite> for Primitive {
    fn from(sprite: MonochromeSprite) -> Self {
        Primitive::MonochromeSprite(sprite)
    }
}

#[derive(Copy, Clone, Debug)]
#[repr(C)]
#[expect(missing_docs)]
pub struct SubpixelSprite {
    pub order: DrawOrder,
    pub pad: u32, // align to 8 bytes
    pub tile: AtlasTile,
    pub bounds: Bounds<ScaledPixels>,
    pub content_mask: ContentMask<ScaledPixels>,
    pub color: Hsla,
    pub transformation: TransformationMatrix,
}

impl From<SubpixelSprite> for Primitive {
    fn from(sprite: SubpixelSprite) -> Self {
        Primitive::SubpixelSprite(sprite)
    }
}

#[derive(Copy, Clone, Debug)]
#[repr(C)]
#[expect(missing_docs)]
pub struct PolychromeSprite {
    pub order: DrawOrder,
    pub pad: u32,
    pub tile: AtlasTile,
    pub grayscale: PaddedBool32,
    pub opacity: f32,
    pub bounds: Bounds<ScaledPixels>,
    pub content_mask: ContentMask<ScaledPixels>,
    pub corner_radii: Corners<ScaledPixels>,
}

impl From<PolychromeSprite> for Primitive {
    fn from(sprite: PolychromeSprite) -> Self {
        Primitive::PolychromeSprite(sprite)
    }
}

#[derive(Clone, Debug)]
#[allow(missing_docs)]
pub struct PaintSurface {
    pub order: DrawOrder,
    pub bounds: Bounds<ScaledPixels>,
    pub content_mask: ContentMask<ScaledPixels>,
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    pub image_buffer: objc2_core_foundation::CFRetained<objc2_core_video::CVPixelBuffer>,
    /// QUILL: CPU pixels where CoreVideo doesn't exist.
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    pub frame: std::sync::Arc<SurfaceFrame>,
}

/// QUILL: the pixel layout of a `SurfaceFrame`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceFormat {
    /// Four little-endian IEEE half floats per pixel (R, G, B, A), linear
    /// extended sRGB, premultiplied alpha: an HDR still.
    Rgba16Float,
    /// 8-bit 4:2:0 YUV, bi-planar: `width * height` luma bytes, then
    /// `height / 2` rows of `width / 2` interleaved (Cb, Cr) pairs. Both
    /// sides are even. Opaque; the frame's [`YuvColor`] says how its
    /// samples map to RGB. A video picture as most decoders produce it.
    Nv12,
}

impl SurfaceFormat {
    /// Bytes a tightly packed `width` x `height` frame of this format
    /// holds (`w * h * 8` for RGBA16F, `w * h * 3 / 2` for NV12).
    pub fn byte_len(self, width: u32, height: u32) -> Result<usize, SurfaceFrameError> {
        if width == 0 || height == 0 {
            return Err(SurfaceFrameError::Empty);
        }
        let pixels = (width as usize)
            .checked_mul(height as usize)
            .ok_or(SurfaceFrameError::TooLarge)?;
        match self {
            SurfaceFormat::Rgba16Float => pixels.checked_mul(8).ok_or(SurfaceFrameError::TooLarge),
            SurfaceFormat::Nv12 => {
                if !width.is_multiple_of(2) || !height.is_multiple_of(2) {
                    return Err(SurfaceFrameError::OddSize { width, height });
                }
                // Luma, plus a quarter as many chroma pairs of two bytes.
                pixels
                    .checked_add(pixels / 2)
                    .ok_or(SurfaceFrameError::TooLarge)
            }
        }
    }
}

/// QUILL: why [`SurfaceFrame::new`] refused its pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceFrameError {
    /// A side is zero.
    Empty,
    /// NV12 needs even sides (its chroma is half the size on each axis).
    OddSize {
        /// The frame's width.
        width: u32,
        /// The frame's height.
        height: u32,
    },
    /// The byte count overflows.
    TooLarge,
    /// The buffer is not exactly as long as the format and size say.
    Length {
        /// What the format and size need.
        expected: usize,
        /// What the buffer holds.
        actual: usize,
    },
}

impl std::fmt::Display for SurfaceFrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SurfaceFrameError::Empty => f.write_str("empty surface frame"),
            SurfaceFrameError::OddSize { width, height } => {
                write!(f, "NV12 frame of {width}x{height} has an odd side")
            }
            SurfaceFrameError::TooLarge => f.write_str("surface frame too large"),
            SurfaceFrameError::Length { expected, actual } => {
                write!(f, "surface frame has {actual} bytes, expected {expected}")
            }
        }
    }
}

impl std::error::Error for SurfaceFrameError {}

/// QUILL: the matrix a YUV frame's samples were encoded with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum YuvMatrix {
    /// ITU-R BT.601 (standard definition).
    Bt601,
    /// ITU-R BT.709 (high definition).
    #[default]
    Bt709,
}

/// QUILL: how a YUV frame's samples map to RGB.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct YuvColor {
    /// The encoding matrix.
    pub matrix: YuvMatrix,
    /// Full range (luma 0-255, "JPEG"); otherwise limited (luma 16-235,
    /// chroma 16-240, "TV"), which is what video almost always is.
    pub full_range: bool,
}

impl YuvColor {
    /// The affine map from sampled (Y, Cb, Cr) in 0..1 (an 8-bit sample
    /// over 255, as a UNORM texture reads it) to gamma-encoded R'G'B' in
    /// 0..1 (clamp afterwards): `rgb[i] = rows[i][0] * y + rows[i][1] * cb
    /// + rows[i][2] * cr + rows[i][3]`. Renderers upload these rows, so
    /// the matrix and range are picked here, in one place.
    pub fn rgb_rows(self) -> [[f32; 4]; 3] {
        let (kr, kb) = match self.matrix {
            YuvMatrix::Bt601 => (0.299_f32, 0.114_f32),
            YuvMatrix::Bt709 => (0.2126_f32, 0.0722_f32),
        };
        let kg = 1.0 - kr - kb;
        // Y' = (y - y0) * ys and C' = (c - c0) * cs.
        let (y0, ys, cs) = if self.full_range {
            (0.0, 1.0, 1.0)
        } else {
            (16.0 / 255.0, 255.0 / 219.0, 255.0 / 224.0)
        };
        let c0 = 128.0 / 255.0;
        let r_cr = 2.0 * (1.0 - kr) * cs;
        let g_cb = -2.0 * kb * (1.0 - kb) / kg * cs;
        let g_cr = -2.0 * kr * (1.0 - kr) / kg * cs;
        let b_cb = 2.0 * (1.0 - kb) * cs;
        let offset = |cb: f32, cr: f32| -y0 * ys - c0 * (cb + cr);
        [
            [ys, 0.0, r_cr, offset(0.0, r_cr)],
            [ys, g_cb, g_cr, offset(g_cb, g_cr)],
            [ys, b_cb, 0.0, offset(b_cb, 0.0)],
        ]
    }
}

/// QUILL: pixels a renderer uploads to a texture and draws like a
/// CoreVideo surface. The renderer may cache the upload by `id`, so a
/// frame's pixels never change; its HDR headroom may.
///
/// Which platforms draw them: Windows (the vendored DirectX renderer)
/// draws both formats, Linux (the vendored gpui-pre-wgpu renderer) draws
/// NV12, and macOS draws CoreVideo buffers instead. The
/// renderer reports each frame's [`SurfaceDrawState`]; a producer replaces
/// a refused frame with an image.
#[derive(Debug)]
pub struct SurfaceFrame {
    id: u64,
    width: u32,
    height: u32,
    format: SurfaceFormat,
    bytes: std::sync::Arc<[u8]>,
    /// How YUV samples map to RGB (YUV formats only).
    yuv: YuvColor,
    /// f32 bits of the display headroom to tone-map to (1.0 = SDR).
    headroom: std::sync::atomic::AtomicU32,
    /// [`SurfaceDrawState`]: 0 pending, 1 drawn, 2 refused.
    draw_state: std::sync::atomic::AtomicU8,
}

impl SurfaceFrame {
    /// A frame of `width` x `height` tightly packed pixels. Refused when
    /// `bytes` is not exactly [`SurfaceFormat::byte_len`] long or the
    /// format can't have that size (a zero side, or an odd one for NV12).
    pub fn new(
        width: u32,
        height: u32,
        format: SurfaceFormat,
        bytes: std::sync::Arc<[u8]>,
    ) -> Result<Self, SurfaceFrameError> {
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let expected = format.byte_len(width, height)?;
        if bytes.len() != expected {
            return Err(SurfaceFrameError::Length {
                expected,
                actual: bytes.len(),
            });
        }
        Ok(Self {
            id: NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            width,
            height,
            format,
            bytes,
            yuv: YuvColor::default(),
            headroom: std::sync::atomic::AtomicU32::new(1.0_f32.to_bits()),
            draw_state: std::sync::atomic::AtomicU8::new(0),
        })
    }

    /// The same frame with `yuv` as its colour (YUV formats).
    pub fn with_yuv(mut self, yuv: YuvColor) -> Self {
        self.yuv = yuv;
        self
    }

    /// How YUV samples map to RGB.
    pub fn yuv(&self) -> YuvColor {
        self.yuv
    }

    /// NV12: the luma plane (`width * height` bytes) and the interleaved
    /// chroma plane (`width * height / 2` bytes). `None` for other formats.
    pub fn nv12_planes(&self) -> Option<(&[u8], &[u8])> {
        if self.format != SurfaceFormat::Nv12 {
            return None;
        }
        let luma = (self.width as usize).checked_mul(self.height as usize)?;
        let (y, uv) = self.bytes.split_at_checked(luma)?;
        (uv.len() == luma / 2).then_some((y, uv))
    }

    /// Unique per frame; a renderer's texture cache key.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Pixel layout.
    pub fn format(&self) -> SurfaceFormat {
        self.format
    }

    /// Tightly packed rows of pixels.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The display headroom to tone-map to (at least 1.0).
    pub fn headroom(&self) -> f32 {
        let headroom = f32::from_bits(self.headroom.load(std::sync::atomic::Ordering::Relaxed));
        if headroom.is_finite() {
            headroom.max(1.0)
        } else {
            1.0
        }
    }

    /// Set the display headroom to tone-map to.
    pub fn set_headroom(&self, headroom: f32) {
        self.headroom
            .store(headroom.to_bits(), std::sync::atomic::Ordering::Relaxed);
    }

    /// What the renderer last did with the frame (renderers that draw
    /// this format report it; see [`SurfaceDrawState`]).
    pub fn draw_state(&self) -> SurfaceDrawState {
        match self.draw_state.load(std::sync::atomic::Ordering::Relaxed) {
            1 => SurfaceDrawState::Drawn,
            2 => SurfaceDrawState::Refused,
            _ => SurfaceDrawState::Pending,
        }
    }

    /// Renderer: the frame is on screen.
    pub fn mark_drawn(&self) {
        // A refusal stays: the producer replaces the frame.
        let _ = self.draw_state.compare_exchange(
            0,
            1,
            std::sync::atomic::Ordering::Relaxed,
            std::sync::atomic::Ordering::Relaxed,
        );
    }

    /// Renderer: the frame could not be drawn (its textures failed, or the
    /// pipeline is missing); the producer should show it another way.
    pub fn mark_refused(&self) {
        self.draw_state
            .store(2, std::sync::atomic::Ordering::Relaxed);
    }
}

/// QUILL: what a renderer did with a [`SurfaceFrame`], so its producer can
/// replace a frame this window can't draw (with an image) without a
/// process-wide switch: each window's renderer answers for itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceDrawState {
    /// Not drawn yet.
    Pending,
    /// Drawn at least once.
    Drawn,
    /// The renderer could not draw it.
    Refused,
}

impl PartialEq for SurfaceFrame {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for SurfaceFrame {}

impl From<BackdropLens> for Primitive {
    fn from(lens: BackdropLens) -> Self {
        Primitive::Lens(Box::new(lens))
    }
}

impl From<PaintSurface> for Primitive {
    fn from(surface: PaintSurface) -> Self {
        Primitive::Surface(surface)
    }
}

/// Where a platform view shows through the scene.
///
/// The view sits beneath the window's rendered content, so the renderer
/// erases everything painted earlier within these bounds, and everything
/// painted later covers the view. The view is therefore treated as opaque.
#[derive(Clone, Copy, Debug)]
pub struct PaintNativeView {
    /// The view's position among the scene's other primitives.
    pub order: DrawOrder,
    /// The view these bounds belong to.
    pub id: NativeViewId,
    /// The hitbox that decides whether a mouse position reaches the view.
    pub hitbox_id: HitboxId,
    /// Where the view is laid out, before clipping.
    pub bounds: Bounds<ScaledPixels>,
    /// The clip applied to the view.
    pub content_mask: ContentMask<ScaledPixels>,
    /// The color to show where the view has not caught up with these bounds,
    /// or a transparent color for none.
    pub background: Hsla,
}

impl From<PaintNativeView> for Primitive {
    fn from(native_view: PaintNativeView) -> Self {
        Primitive::NativeView(native_view)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
#[expect(missing_docs)]
pub struct PathCurve {
    pub p0: Point<f32>,
    pub p1: Point<f32>,
    pub a: Point<f32>,
    pub b: Point<f32>,
    pub sign: f32,
    pub pad: u32, // align to 8 bytes
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
#[expect(missing_docs)]
pub struct PathPaint {
    /// The path's bounds, in device pixels.
    pub bounds: Bounds<ScaledPixels>,
    pub content_mask: ContentMask<ScaledPixels>,
    pub color: Background,
    pub even_odd: PaddedBool32,
    pub pad: u32, // align to 8 bytes
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
#[expect(missing_docs)]
pub struct PathTile {
    pub paint: u32,
    pub curve_start: u32,
    pub curve_count: u32,
    pub backdrop: i32,
    pub top_left: Point<f32>,
    pub bottom_right: Point<f32>,
    pub reference: Point<f32>,
    pub curves_top_left: Point<f32>,
    pub curves_bottom_right: Point<f32>,
}

#[derive(Clone, Debug)]
#[expect(missing_docs)]
pub struct PaintedPath {
    order: DrawOrder,
    bounds: Bounds<ScaledPixels>,
    tiles: Rc<PathTiles>,
    paint: PathPaint,
}

#[expect(missing_docs)]
impl PaintedPath {
    /// Bins `path` into tiles, to be filled with `color` within `content_mask`.
    pub fn new(
        path: Path,
        scale_factor: f32,
        content_mask: ContentMask<Pixels>,
        color: Background,
    ) -> Self {
        let content_mask = content_mask.scale(scale_factor);
        let mut tiles = path.bin(scale_factor);
        tiles.retain_visible(content_mask.bounds);
        let paint = PathPaint {
            bounds: tiles.bounds(),
            content_mask,
            color,
            even_odd: path.even_odd().into(),
            pad: 0,
        };
        Self {
            order: 0,
            bounds: tiles.painted_bounds(),
            tiles: Rc::new(tiles),
            paint,
        }
    }

    pub fn tile_count(&self) -> usize {
        self.tiles.tiles().len()
    }

    /// QUILL: where the path sits among the scene's primitives.
    pub fn order(&self) -> DrawOrder {
        self.order
    }

    /// QUILL: what the path is filled with.
    pub fn color(&self) -> Background {
        self.paint.color
    }

    pub fn append_to(
        &self,
        paints: &mut Vec<PathPaint>,
        curves: &mut Vec<PathCurve>,
        tiles: &mut Vec<PathTile>,
    ) {
        let paint_index = paints.len() as u32;
        let curve_start = curves.len() as u32;
        paints.push(self.paint);
        curves.extend_from_slice(self.tiles.curves());
        tiles.extend(self.tiles.tiles().iter().map(|tile| PathTile {
            paint: paint_index,
            curve_start: curve_start + tile.curve_start,
            ..*tile
        }));
    }
}

impl From<PaintedPath> for Primitive {
    fn from(path: PaintedPath) -> Self {
        Primitive::Path(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{point, size};

    fn square(x: f32) -> Bounds<ScaledPixels> {
        Bounds {
            origin: point(ScaledPixels(x), ScaledPixels(0.)),
            size: size(ScaledPixels(10.), ScaledPixels(10.)),
        }
    }

    fn quad(x: f32) -> Quad {
        Quad {
            bounds: square(x),
            content_mask: ContentMask::rect(square(x)),
            ..Default::default()
        }
    }

    fn underline(x: f32) -> Underline {
        Underline {
            order: 0,
            pad: 0,
            bounds: square(x),
            content_mask: ContentMask::rect(square(x)),
            color: Hsla::default(),
            thickness: ScaledPixels(1.),
            wavy: false.into(),
        }
    }

    fn batch_labels(scene: &mut Scene) -> Vec<String> {
        scene.finish();
        scene.batches().map(|batch| batch.label()).collect()
    }

    #[test]
    fn a_native_view_in_a_layer_is_covered_by_what_the_layer_paints_after_it() {
        let mut scene = Scene::default();
        scene.push_layer();
        scene.insert_primitive(quad(0.));
        scene.insert_primitive(PaintNativeView {
            order: 0,
            id: NativeViewId::for_test(1),
            hitbox_id: HitboxId::placeholder(),
            bounds: square(0.),
            content_mask: ContentMask::rect(square(0.)),
            background: Hsla::default(),
        });
        scene.insert_primitive(quad(0.));
        // A quad beside the view shares nothing with it, so it isn't held back.
        scene.insert_primitive(quad(20.));
        scene.pop_layer();
        assert_eq!(
            batch_labels(&mut scene),
            ["quads (1)", "native views (1)", "quads (2)"]
        );
    }

    #[test]
    fn a_native_view_painted_twice_is_kept_where_it_was_painted_first() {
        let native_view = |hitbox_id: HitboxId, x: f32| PaintNativeView {
            order: 0,
            id: NativeViewId::for_test(1),
            hitbox_id,
            bounds: square(x),
            content_mask: ContentMask::rect(square(x)),
            background: Hsla::default(),
        };
        let first_hitbox = HitboxId::placeholder();
        let second_hitbox = first_hitbox.next();
        let mut scene = Scene::default();
        // The quad can't be drawn with the underline it covers, so the first
        // occurrence, painted over both, is drawn after the second, which
        // overlaps nothing.
        scene.insert_primitive(underline(0.));
        scene.insert_primitive(quad(0.));
        scene.insert_primitive(native_view(first_hitbox, 0.));
        scene.insert_primitive(native_view(second_hitbox, 20.));
        scene.finish();

        let shown = scene
            .native_views
            .iter()
            .map(|native_view| native_view.bounds)
            .collect::<Vec<_>>();
        assert_eq!(shown, [square(0.)]);
    }

    #[test]
    fn layers_are_drawn_in_paint_order() {
        // A quad painted after a layer goes above it.
        let mut scene = Scene::default();
        scene.insert_primitive(quad(0.));
        scene.push_layer();
        scene.insert_primitive(underline(0.));
        scene.insert_primitive(underline(5.));
        scene.pop_layer();
        scene.insert_primitive(quad(5.));
        assert_eq!(
            batch_labels(&mut scene),
            ["quads (1)", "underlines (2)", "quads (1)"]
        );

        // A layer goes above the layer painted before it.
        let mut scene = Scene::default();
        scene.push_layer();
        scene.insert_primitive(underline(0.));
        scene.pop_layer();
        scene.push_layer();
        scene.insert_primitive(quad(5.));
        scene.pop_layer();
        assert_eq!(batch_labels(&mut scene), ["underlines (1)", "quads (1)"]);
    }

    #[test]
    fn nested_layers_are_drawn_above_their_parents() {
        // A layer has one draw order, so what it paints after a nested layer
        // still goes below that layer.
        let mut scene = Scene::default();
        scene.push_layer();
        scene.insert_primitive(underline(0.));
        scene.push_layer();
        scene.insert_primitive(quad(5.));
        scene.pop_layer();
        scene.insert_primitive(underline(5.));
        scene.pop_layer();
        assert_eq!(batch_labels(&mut scene), ["underlines (2)", "quads (1)"]);

        // The same holds when the nested layer shares its parent's order.
        let mut scene = Scene::default();
        scene.push_layer();
        scene.insert_primitive(quad(0.));
        scene.push_layer();
        scene.insert_primitive(quad(5.));
        scene.pop_layer();
        scene.insert_primitive(quad(8.));
        scene.pop_layer();
        assert_eq!(batch_labels(&mut scene), ["quads (3)"]);
        assert_eq!(
            scene
                .quads
                .iter()
                .map(|quad| quad.bounds)
                .collect::<Vec<_>>(),
            [square(0.), square(8.), square(5.)]
        );

        // A layer goes above the layers nested in the layer before it.
        let mut scene = Scene::default();
        scene.push_layer();
        scene.push_layer();
        scene.insert_primitive(underline(0.));
        scene.pop_layer();
        scene.pop_layer();
        scene.push_layer();
        scene.insert_primitive(quad(5.));
        scene.pop_layer();
        assert_eq!(batch_labels(&mut scene), ["underlines (1)", "quads (1)"]);
    }
}
