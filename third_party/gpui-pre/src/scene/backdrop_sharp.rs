// Added by the Quill project (2026). See third_party/gpui-pre/QUILL-CHANGES.md.
//! QUILL (backdrop glass, stage 5): re-rendered backdrops for lenses with
//! `LensDetail::Rendered` (a sharp magnifier). See third_party/gpui-pre/QUILL-CHANGES.md.
//!
//! At the end of a frame the window takes what the scene drew under such a
//! lens, scales it to `density` texels per device pixel and keeps it as a
//! scene of its own: geometry (quads, shadows, underlines, paths, images)
//! scaled exactly, glyphs and icons rasterized again at the finer scale
//! from the [`GlyphSource`] recorded when they were painted. The renderer
//! draws that scene into a texture and fills the lens's slot from it
//! instead of from the frame, so a magnified label stays crisp.

use std::rc::Rc;

use super::{PaintedPath, PathPaint, SpriteAnimationId};
use crate::{
    AtlasTile, Bounds, ContentMask, Corners, Edges, FontId, GlyphId, MonochromeSprite, Pixels,
    Point, PolychromeSprite, Quad, ScaledPixels, Scene, Shadow, SharedString, Size,
    SubpixelSprite, TransformationMatrix, Underline, point,
};

/// What lay under one lens, drawn again `density` times finer: `scene` is
/// in the slot's texels, (0, 0) at `region`'s top left.
pub struct SharpBackdrop {
    /// The lens, an index into `Scene::lenses`.
    pub lens: usize,
    /// What the lens reads in the target, whole device pixels.
    pub region: Bounds<ScaledPixels>,
    /// Texels per device pixel.
    pub density: u32,
    /// What lies under the lens, in the slot's texels.
    pub scene: Scene,
}

/// How to rasterize a sprite again at another scale.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum GlyphSource {
    Glyph {
        origin: Point<Pixels>,
        font_id: FontId,
        glyph_id: GlyphId,
        font_size: Pixels,
        dilation: u8,
    },
    Emoji {
        origin: Point<Pixels>,
        font_id: FontId,
        glyph_id: GlyphId,
        font_size: Pixels,
    },
    Svg {
        /// The icon's snapped bounds, device pixels.
        bounds: Bounds<ScaledPixels>,
        path: SharedString,
    },
}

/// A sprite's identity: the same tile at the same place is the same glyph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct GlyphKey {
    texture: u32,
    kind: u32,
    tile: u32,
    x: u32,
    y: u32,
}

impl GlyphKey {
    pub(crate) fn new(tile: &AtlasTile, bounds: &Bounds<ScaledPixels>) -> Self {
        Self {
            texture: tile.texture_id.index,
            kind: tile.texture_id.kind as u32,
            tile: tile.tile_id.0,
            x: bounds.origin.x.0.to_bits(),
            y: bounds.origin.y.0.to_bits(),
        }
    }
}

/// Maps the target's device pixels onto a slot `density` times finer whose
/// top left is `origin`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SlotSpace {
    pub origin: Point<ScaledPixels>,
    pub density: f32,
    /// The slot, in its own texels.
    pub size: Size<ScaledPixels>,
}

impl SlotSpace {
    pub(crate) fn len(&self, v: ScaledPixels) -> ScaledPixels {
        ScaledPixels(v.0 * self.density)
    }

    pub(crate) fn point(&self, p: Point<ScaledPixels>) -> Point<ScaledPixels> {
        point(
            ScaledPixels((p.x.0 - self.origin.x.0) * self.density),
            ScaledPixels((p.y.0 - self.origin.y.0) * self.density),
        )
    }

    pub(crate) fn bounds(&self, b: Bounds<ScaledPixels>) -> Bounds<ScaledPixels> {
        Bounds {
            origin: self.point(b.origin),
            size: Size {
                width: self.len(b.size.width),
                height: self.len(b.size.height),
            },
        }
    }

    /// A content mask in slot texels, kept inside the slot.
    pub(crate) fn mask(&self, m: ContentMask<ScaledPixels>) -> ContentMask<ScaledPixels> {
        let slot = Bounds {
            origin: Point::default(),
            size: self.size,
        };
        ContentMask {
            bounds: self.bounds(m.bounds).intersect(&slot),
        }
    }

    fn corners(&self, c: Corners<ScaledPixels>) -> Corners<ScaledPixels> {
        Corners {
            top_left: self.len(c.top_left),
            top_right: self.len(c.top_right),
            bottom_right: self.len(c.bottom_right),
            bottom_left: self.len(c.bottom_left),
        }
    }

    fn edges(&self, e: Edges<ScaledPixels>) -> Edges<ScaledPixels> {
        Edges {
            top: self.len(e.top),
            right: self.len(e.right),
            bottom: self.len(e.bottom),
            left: self.len(e.left),
        }
    }

    fn transformation(&self, t: TransformationMatrix) -> TransformationMatrix {
        TransformationMatrix {
            rotation_scale: t.rotation_scale,
            translation: [
                t.translation[0] * self.density,
                t.translation[1] * self.density,
            ],
        }
    }

    pub(crate) fn quad(&self, q: &Quad) -> Quad {
        Quad {
            order: 0,
            bounds: self.bounds(q.bounds),
            content_mask: self.mask(q.content_mask),
            corner_radii: self.corners(q.corner_radii),
            border_widths: self.edges(q.border_widths),
            ..*q
        }
    }

    pub(crate) fn shadow(&self, s: &Shadow) -> Shadow {
        Shadow {
            order: 0,
            blur_radius: self.len(s.blur_radius),
            bounds: self.bounds(s.bounds),
            corner_radii: self.corners(s.corner_radii),
            content_mask: self.mask(s.content_mask),
            shape_bounds: self.bounds(s.shape_bounds),
            shape_corner_radii: self.corners(s.shape_corner_radii),
            ..*s
        }
    }

    pub(crate) fn underline(&self, u: &Underline) -> Underline {
        Underline {
            order: 0,
            bounds: self.bounds(u.bounds),
            content_mask: self.mask(u.content_mask),
            thickness: self.len(u.thickness),
            ..*u
        }
    }

    /// A path's tiles moved and scaled with the slot: a positive scale and a
    /// move keep the binning valid, so nothing is binned again.
    pub(crate) fn path(&self, p: &PaintedPath) -> PaintedPath {
        PaintedPath {
            order: 0,
            bounds: self.bounds(p.bounds),
            tiles: Rc::new(
                p.tiles
                    .mapped(point(self.origin.x.0, self.origin.y.0), self.density),
            ),
            paint: PathPaint {
                bounds: self.bounds(p.paint.bounds),
                content_mask: self.mask(p.paint.content_mask),
                ..p.paint
            },
        }
    }

    /// A sprite whose tile is drawn larger (no source to rasterize it
    /// again from).
    pub(crate) fn monochrome(&self, s: &MonochromeSprite) -> MonochromeSprite {
        MonochromeSprite {
            order: 0,
            animation: SpriteAnimationId::NONE,
            bounds: self.bounds(s.bounds),
            content_mask: self.mask(s.content_mask),
            transformation: self.transformation(s.transformation),
            ..*s
        }
    }

    pub(crate) fn subpixel(&self, s: &SubpixelSprite) -> SubpixelSprite {
        SubpixelSprite {
            order: 0,
            bounds: self.bounds(s.bounds),
            content_mask: self.mask(s.content_mask),
            transformation: self.transformation(s.transformation),
            ..*s
        }
    }

    pub(crate) fn polychrome(&self, s: &PolychromeSprite) -> PolychromeSprite {
        PolychromeSprite {
            order: 0,
            bounds: self.bounds(s.bounds),
            content_mask: self.mask(s.content_mask),
            corner_radii: self.corners(s.corner_radii),
            ..*s
        }
    }
}
