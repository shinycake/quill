// Modified by the Quill project (2026) from gpui-pre 0.3.9 (Apache-2.0):
// `SurfaceFrame` (RGBA16F and NV12, `YuvColor`) and the non-Apple `PaintSurface::frame`;
// primitives a window's paint filter refuses never enter the scene; a replayed
// range enters the bounds tree once.
// See third_party/gpui-pre/QUILL-CHANGES.md.
// todo("windows"): remove
#![cfg_attr(windows, allow(dead_code))]

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    AtlasTextureId, AtlasTile, Background, Bounds, ContentMask, Corners, Edges, Hsla, Pixels,
    Point, Radians, ScaledPixels, Size, bounds_tree::BoundsTree, point,
};
use std::{
    fmt::Debug,
    iter::Peekable,
    ops::{Add, Range, Sub},
    slice,
};

#[allow(non_camel_case_types, unused)]
#[expect(missing_docs)]
pub type PathVertex_ScaledPixels = PathVertex<ScaledPixels>;

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

#[derive(Default)]
#[expect(missing_docs)]
pub struct Scene {
    pub(crate) paint_operations: Vec<PaintOperation>,
    primitive_bounds: BoundsTree<ScaledPixels>,
    layer_stack: Vec<DrawOrder>,
    pub shadows: Vec<Shadow>,
    pub quads: Vec<Quad>,
    pub paths: Vec<Path<ScaledPixels>>,
    pub underlines: Vec<Underline>,
    pub monochrome_sprites: Vec<MonochromeSprite>,
    pub subpixel_sprites: Vec<SubpixelSprite>,
    pub polychrome_sprites: Vec<PolychromeSprite>,
    pub surfaces: Vec<PaintSurface>,
    /// Quill: the window's paint filter refuses the tags being painted
    /// now, so primitives inserted meanwhile are dropped
    /// (`Window::set_paint_filter`).
    pub(crate) suppressed: bool,
    /// Quill: this scene is drawn under a paint filter, so cached
    /// primitives are not replayed (they were never filtered).
    pub(crate) filtered: bool,
}

#[expect(missing_docs)]
impl Scene {
    pub fn clear(&mut self) {
        self.paint_operations.clear();
        self.primitive_bounds.clear();
        self.layer_stack.clear();
        self.paths.clear();
        self.shadows.clear();
        self.quads.clear();
        self.underlines.clear();
        self.monochrome_sprites.clear();
        self.subpixel_sprites.clear();
        self.polychrome_sprites.clear();
        self.surfaces.clear();
        self.suppressed = false;
        self.filtered = false;
    }

    pub fn len(&self) -> usize {
        self.paint_operations.len()
    }

    pub fn push_layer(&mut self, bounds: Bounds<ScaledPixels>) {
        let order = self.primitive_bounds.insert(bounds);
        self.layer_stack.push(order);
        self.paint_operations
            .push(PaintOperation::StartLayer(bounds));
    }

    pub fn pop_layer(&mut self) {
        self.layer_stack.pop();
        self.paint_operations.push(PaintOperation::EndLayer);
    }

    pub fn insert_primitive(&mut self, primitive: impl Into<Primitive>) {
        if self.suppressed {
            return;
        }
        let primitive = primitive.into();
        let clipped_bounds = primitive.clipped_bounds();

        if clipped_bounds.is_empty() {
            return;
        }

        let order = self
            .layer_stack
            .last()
            .copied()
            .unwrap_or_else(|| self.primitive_bounds.insert(clipped_bounds));
        self.push_primitive(primitive, order);
    }

    fn push_primitive(&mut self, mut primitive: Primitive, order: DrawOrder) {
        match &mut primitive {
            Primitive::Shadow(shadow) => {
                shadow.order = order;
                self.shadows.push(*shadow);
            }
            Primitive::Quad(quad) => {
                quad.order = order;
                self.quads.push(*quad);
            }
            Primitive::Path(path) => {
                path.order = order;
                path.id = PathId(self.paths.len());
                self.paths.push(path.clone());
            }
            Primitive::Underline(underline) => {
                underline.order = order;
                self.underlines.push(*underline);
            }
            Primitive::MonochromeSprite(sprite) => {
                sprite.order = order;
                self.monochrome_sprites.push(*sprite);
            }
            Primitive::SubpixelSprite(sprite) => {
                sprite.order = order;
                self.subpixel_sprites.push(*sprite);
            }
            Primitive::PolychromeSprite(sprite) => {
                sprite.order = order;
                self.polychrome_sprites.push(*sprite);
            }
            Primitive::Surface(surface) => {
                surface.order = order;
                self.surfaces.push(surface.clone());
            }
        }
        self.paint_operations
            .push(PaintOperation::Primitive(primitive));
    }

    pub fn replay(&mut self, range: Range<usize>, prev_scene: &Scene) {
        if self.filtered {
            return;
        }
        let operations = &prev_scene.paint_operations[range];
        if self.suppressed || !self.layer_stack.is_empty() {
            self.repaint_operations(operations);
        } else {
            self.replay_as_one(operations);
        }
    }

    fn repaint_operations(&mut self, operations: &[PaintOperation]) {
        for operation in operations {
            match operation {
                PaintOperation::Primitive(primitive) => self.insert_primitive(primitive.clone()),
                PaintOperation::StartLayer(bounds) => self.push_layer(*bounds),
                PaintOperation::EndLayer => self.pop_layer(),
            }
        }
    }

    fn replay_as_one(&mut self, operations: &[PaintOperation]) {
        let extent = operations
            .iter()
            .filter_map(|operation| match operation {
                PaintOperation::Primitive(primitive) => {
                    Some((primitive.clipped_bounds(), primitive.order(), primitive.order()))
                }
                PaintOperation::StartLayer(_) | PaintOperation::EndLayer => None,
            })
            .reduce(|(bounds, lowest, highest), (next, low, high)| {
                (bounds.union(&next), lowest.min(low), highest.max(high))
            });
        let shift = extent.map_or(0, |(bounds, lowest, highest)| {
            self.primitive_bounds
                .insert_span(bounds, highest - lowest)
                .wrapping_sub(lowest)
        });
        for operation in operations {
            match operation {
                PaintOperation::Primitive(primitive) => {
                    self.push_primitive(primitive.clone(), primitive.order().wrapping_add(shift));
                }
                PaintOperation::StartLayer(bounds) => {
                    self.paint_operations
                        .push(PaintOperation::StartLayer(*bounds));
                }
                PaintOperation::EndLayer => self.paint_operations.push(PaintOperation::EndLayer),
            }
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn refuse_paint_for_test(&mut self, filtered: bool, suppressed: bool) {
        self.filtered = filtered;
        self.suppressed = suppressed;
    }

    pub fn finish(&mut self) {
        self.shadows.sort_by_key(|shadow| shadow.order);
        self.quads.sort_by_key(|quad| quad.order);
        self.paths.sort_by_key(|path| path.order);
        self.underlines.sort_by_key(|underline| underline.order);
        self.monochrome_sprites
            .sort_by_key(|sprite| (sprite.order, sprite.tile.texture_id.index));
        self.subpixel_sprites
            .sort_by_key(|sprite| (sprite.order, sprite.tile.texture_id.index));
        self.polychrome_sprites
            .sort_by_key(|sprite| (sprite.order, sprite.tile.texture_id.index));
        self.surfaces.sort_by_key(|surface| surface.order);
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
        }
    }
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
}

pub(crate) enum PaintOperation {
    Primitive(Primitive),
    StartLayer(Bounds<ScaledPixels>),
    EndLayer,
}

#[derive(Clone)]
#[expect(missing_docs)]
pub enum Primitive {
    Shadow(Shadow),
    Quad(Quad),
    Path(Path<ScaledPixels>),
    Underline(Underline),
    MonochromeSprite(MonochromeSprite),
    SubpixelSprite(SubpixelSprite),
    PolychromeSprite(PolychromeSprite),
    Surface(PaintSurface),
}

#[expect(missing_docs)]
impl Primitive {
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
        }
    }

    pub fn content_mask(&self) -> &ContentMask<ScaledPixels> {
        match self {
            Primitive::Shadow(shadow) => &shadow.content_mask,
            Primitive::Quad(quad) => &quad.content_mask,
            Primitive::Path(path) => &path.content_mask,
            Primitive::Underline(underline) => &underline.content_mask,
            Primitive::MonochromeSprite(sprite) => &sprite.content_mask,
            Primitive::SubpixelSprite(sprite) => &sprite.content_mask,
            Primitive::PolychromeSprite(sprite) => &sprite.content_mask,
            Primitive::Surface(surface) => &surface.content_mask,
        }
    }

    /// QUILL: the box the primitive draws in, which for a drop shadow reaches
    /// as far as its blur (the shaders' margin). Draw order goes by this box,
    /// so a tail sorts the same painted or replayed.
    pub fn painted_bounds(&self) -> Bounds<ScaledPixels> {
        match self {
            Primitive::Shadow(shadow) if shadow.inset != 0 => shadow.element_bounds,
            Primitive::Shadow(shadow) => shadow.bounds.dilate(shadow.blur_radius * 3.),
            primitive => *primitive.bounds(),
        }
    }

    fn clipped_bounds(&self) -> Bounds<ScaledPixels> {
        self.painted_bounds()
            .intersect(&self.content_mask().bounds)
    }

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
    paths_iter: Peekable<slice::Iter<'a, Path<ScaledPixels>>>,
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
            (self.paths_iter.peek().map(|q| q.order), PrimitiveKind::Path),
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
    pub element_bounds: Bounds<ScaledPixels>,
    pub element_corner_radii: Corners<ScaledPixels>,
    /// 0 = drop shadow (rendered outside the element), 1 = inset shadow (rendered inside).
    pub inset: u32,
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
    pub fn translate(mut self, point: Point<ScaledPixels>) -> Self {
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
    pub pad: u32,
    pub tile: AtlasTile,
    pub bounds: Bounds<ScaledPixels>,
    pub content_mask: ContentMask<ScaledPixels>,
    pub color: Hsla,
    pub transformation: TransformationMatrix,
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
/// draws both formats. Linux (the registry gpui-pre-wgpu renderer) draws
/// no surfaces, and macOS draws CoreVideo buffers instead, so a producer
/// hands these over only on Windows and keeps to images elsewhere. The
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
        if headroom.is_finite() { headroom.max(1.0) } else { 1.0 }
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

impl From<PaintSurface> for Primitive {
    fn from(surface: PaintSurface) -> Self {
        Primitive::Surface(surface)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[expect(missing_docs)]
pub struct PathId(pub usize);

/// A line made up of a series of vertices and control points.
#[derive(Clone, Debug)]
#[expect(missing_docs)]
pub struct Path<P: Clone + Debug + Default + PartialEq> {
    pub id: PathId,
    pub order: DrawOrder,
    pub bounds: Bounds<P>,
    pub content_mask: ContentMask<P>,
    pub vertices: Vec<PathVertex<P>>,
    pub color: Background,
    start: Point<P>,
    current: Point<P>,
    contour_count: usize,
}

impl Path<Pixels> {
    /// Create a new path with the given starting point.
    pub fn new(start: Point<Pixels>) -> Self {
        Self {
            id: PathId(0),
            order: DrawOrder::default(),
            vertices: Vec::new(),
            start,
            current: start,
            bounds: Bounds {
                origin: start,
                size: Default::default(),
            },
            content_mask: Default::default(),
            color: Default::default(),
            contour_count: 0,
        }
    }

    /// Scale this path by the given factor.
    pub fn scale(&self, factor: f32) -> Path<ScaledPixels> {
        Path {
            id: self.id,
            order: self.order,
            bounds: self.bounds.scale(factor),
            content_mask: self.content_mask.scale(factor),
            vertices: self
                .vertices
                .iter()
                .map(|vertex| vertex.scale(factor))
                .collect(),
            start: self.start.map(|start| start.scale(factor)),
            current: self.current.scale(factor),
            contour_count: self.contour_count,
            color: self.color,
        }
    }

    /// Move the start, current point to the given point.
    pub fn move_to(&mut self, to: Point<Pixels>) {
        self.contour_count += 1;
        self.start = to;
        self.current = to;
    }

    /// Draw a straight line from the current point to the given point.
    pub fn line_to(&mut self, to: Point<Pixels>) {
        self.contour_count += 1;
        if self.contour_count > 1 {
            self.push_triangle(
                (self.start, self.current, to),
                (point(0., 1.), point(0., 1.), point(0., 1.)),
            );
        }
        self.current = to;
    }

    /// Draw a curve from the current point to the given point, using the given control point.
    pub fn curve_to(&mut self, to: Point<Pixels>, ctrl: Point<Pixels>) {
        self.contour_count += 1;
        if self.contour_count > 1 {
            self.push_triangle(
                (self.start, self.current, to),
                (point(0., 1.), point(0., 1.), point(0., 1.)),
            );
        }

        self.push_triangle(
            (self.current, ctrl, to),
            (point(0., 0.), point(0.5, 0.), point(1., 1.)),
        );
        self.current = to;
    }

    /// Push a triangle to the Path.
    pub fn push_triangle(
        &mut self,
        xy: (Point<Pixels>, Point<Pixels>, Point<Pixels>),
        st: (Point<f32>, Point<f32>, Point<f32>),
    ) {
        self.bounds = self
            .bounds
            .union(&Bounds {
                origin: xy.0,
                size: Default::default(),
            })
            .union(&Bounds {
                origin: xy.1,
                size: Default::default(),
            })
            .union(&Bounds {
                origin: xy.2,
                size: Default::default(),
            });

        self.vertices.push(PathVertex {
            xy_position: xy.0,
            st_position: st.0,
            content_mask: Default::default(),
        });
        self.vertices.push(PathVertex {
            xy_position: xy.1,
            st_position: st.1,
            content_mask: Default::default(),
        });
        self.vertices.push(PathVertex {
            xy_position: xy.2,
            st_position: st.2,
            content_mask: Default::default(),
        });
    }
}

impl<T> Path<T>
where
    T: Clone + Debug + Default + PartialEq + PartialOrd + Add<T, Output = T> + Sub<Output = T>,
{
    #[allow(unused)]
    #[expect(missing_docs)]
    pub fn clipped_bounds(&self) -> Bounds<T> {
        self.bounds.intersect(&self.content_mask.bounds)
    }
}

impl From<Path<ScaledPixels>> for Primitive {
    fn from(path: Path<ScaledPixels>) -> Self {
        Primitive::Path(path)
    }
}

#[derive(Clone, Debug)]
#[repr(C)]
#[expect(missing_docs)]
pub struct PathVertex<P: Clone + Debug + Default + PartialEq> {
    pub xy_position: Point<P>,
    pub st_position: Point<f32>,
    pub content_mask: ContentMask<P>,
}

#[expect(missing_docs)]
impl PathVertex<Pixels> {
    pub fn scale(&self, factor: f32) -> PathVertex<ScaledPixels> {
        PathVertex {
            xy_position: self.xy_position.scale(factor),
            st_position: self.st_position,
            content_mask: self.content_mask.scale(factor),
        }
    }
}
