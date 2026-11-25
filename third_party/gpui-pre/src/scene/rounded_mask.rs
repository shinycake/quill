// QUILL: content masks with rounded corners (docs/decisions/rounded-content-masks.md).
//
// A `ContentMask` is a rectangle, clipped exactly in the vertex stage, and at
// most one rounded rectangle, cut per fragment. This file holds the rounded
// shape, the rules for nesting two masks, the corner culling that keeps
// primitives away from the per-fragment path when they are nowhere near a
// corner, and the point test hit testing uses.

use std::fmt::Debug;

use crate::{Bounds, ContentMask, Corners, Pixels, Point, ScaledPixels};

/// The rounded shape of a [`ContentMask`]: one rounded rectangle. A mask
/// whose radii are all zero has none, and only its rectangle applies.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct RoundedMask<P: Clone + Debug + Default + PartialEq> {
    /// The rounded rectangle's bounds.
    pub bounds: Bounds<P>,
    /// Its corner radii, each at most half its shorter side.
    pub radii: Corners<P>,
}

/// A length a mask can be measured in: [`Pixels`] for the window's mask
/// stack and hitboxes, [`ScaledPixels`] for primitives.
pub trait MaskUnit:
    Copy
    + Clone
    + Debug
    + Default
    + PartialEq
    + PartialOrd
    + std::ops::Add<Output = Self>
    + std::ops::Sub<Output = Self>
{
    /// The length as a float.
    fn to_f32(self) -> f32;
    /// A length from a float.
    fn from_f32(value: f32) -> Self;
}

impl MaskUnit for Pixels {
    fn to_f32(self) -> f32 {
        self.0
    }

    fn from_f32(value: f32) -> Self {
        Pixels(value)
    }
}

impl MaskUnit for ScaledPixels {
    fn to_f32(self) -> f32 {
        self.0
    }

    fn from_f32(value: f32) -> Self {
        ScaledPixels(value)
    }
}

/// Whether nesting two masks kept both shapes exactly (rules 1 to 4 of
/// `docs/decisions/rounded-content-masks.md`) or lost the outer shape's
/// rounding where the two cross (rule 5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nesting {
    /// The nested mask is exactly the intersection of the two.
    Exact,
    /// The inner shape won and the outer one's rounding is lost where they
    /// cross.
    Inexact,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Rect {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

impl Rect {
    fn of<P: MaskUnit>(bounds: &Bounds<P>) -> Self {
        let x0 = bounds.origin.x.to_f32();
        let y0 = bounds.origin.y.to_f32();
        Rect {
            x0,
            y0,
            x1: x0 + bounds.size.width.to_f32(),
            y1: y0 + bounds.size.height.to_f32(),
        }
    }

    /// Whether the two share an area (touching edges do not count).
    fn overlaps(&self, other: &Rect) -> bool {
        self.x0.max(other.x0) < self.x1.min(other.x1)
            && self.y0.max(other.y0) < self.y1.min(other.y1)
    }

    fn intersect(&self, other: &Rect) -> Rect {
        Rect {
            x0: self.x0.max(other.x0),
            y0: self.y0.max(other.y0),
            x1: self.x1.min(other.x1),
            y1: self.y1.min(other.y1),
        }
    }
}

/// One corner of a rounded shape: its radius, the radius × radius square at
/// the corner, the circle's centre, and the shape's corner point.
#[derive(Clone, Copy, Debug)]
struct Corner {
    radius: f32,
    square: Rect,
    centre: (f32, f32),
}

impl<P: MaskUnit> RoundedMask<P> {
    /// A rounded rectangle, with radii clamped to half its shorter side and
    /// at least zero.
    pub fn new(bounds: Bounds<P>, radii: Corners<P>) -> Self {
        let rect = Rect::of(&bounds);
        let limit = ((rect.x1 - rect.x0).min(rect.y1 - rect.y0) * 0.5).max(0.0);
        let clamp = |r: P| P::from_f32(r.to_f32().clamp(0.0, limit));
        RoundedMask {
            bounds,
            radii: Corners {
                top_left: clamp(radii.top_left),
                top_right: clamp(radii.top_right),
                bottom_right: clamp(radii.bottom_right),
                bottom_left: clamp(radii.bottom_left),
            },
        }
    }

    /// Whether there is no rounded shape (every radius is zero).
    pub fn is_none(&self) -> bool {
        self.radii_f32().iter().all(|r| *r <= 0.0)
    }

    fn radii_f32(&self) -> [f32; 4] {
        [
            self.radii.top_left.to_f32(),
            self.radii.top_right.to_f32(),
            self.radii.bottom_right.to_f32(),
            self.radii.bottom_left.to_f32(),
        ]
    }

    /// The corners with a radius, in the order top left, top right, bottom
    /// right, bottom left (a corner without one is `None`).
    fn corners(&self) -> [Option<Corner>; 4] {
        let rect = Rect::of(&self.bounds);
        let [tl, tr, br, bl] = self.radii_f32();
        let corner = |radius: f32, x0: f32, y0: f32, centre: (f32, f32)| {
            (radius > 0.0).then_some(Corner {
                radius,
                square: Rect {
                    x0,
                    y0,
                    x1: x0 + radius,
                    y1: y0 + radius,
                },
                centre,
            })
        };
        [
            corner(tl, rect.x0, rect.y0, (rect.x0 + tl, rect.y0 + tl)),
            corner(tr, rect.x1 - tr, rect.y0, (rect.x1 - tr, rect.y0 + tr)),
            corner(br, rect.x1 - br, rect.y1 - br, (rect.x1 - br, rect.y1 - br)),
            corner(bl, rect.x0, rect.y1 - bl, (rect.x0 + bl, rect.y1 - bl)),
        ]
    }

    /// Whether any corner square with a radius overlaps `area`: only then
    /// can the shape cut anything inside it.
    fn has_active_corner(&self, area: &Rect) -> bool {
        self.corners()
            .iter()
            .flatten()
            .any(|corner| corner.square.overlaps(area))
    }

    /// The signed distance from `p` to the shape's edge: negative inside.
    /// The same function as the shaders' `quad_sdf`.
    fn signed_distance(&self, p: (f32, f32)) -> f32 {
        let rect = Rect::of(&self.bounds);
        let half = ((rect.x1 - rect.x0) * 0.5, (rect.y1 - rect.y0) * 0.5);
        let centre_to_point = (p.0 - (rect.x0 + half.0), p.1 - (rect.y0 + half.1));
        let [tl, tr, br, bl] = self.radii_f32();
        let radius = match (centre_to_point.0 < 0.0, centre_to_point.1 < 0.0) {
            (true, true) => tl,
            (false, true) => tr,
            (false, false) => br,
            (true, false) => bl,
        };
        let corner_to_point = (
            centre_to_point.0.abs() - half.0,
            centre_to_point.1.abs() - half.1,
        );
        let c = (corner_to_point.0 + radius, corner_to_point.1 + radius);
        if radius == 0.0 {
            c.0.max(c.1)
        } else {
            let outside = (c.0.max(0.0).powi(2) + c.1.max(0.0).powi(2)).sqrt();
            let inside = c.0.max(c.1).min(0.0);
            outside + inside - radius
        }
    }

    /// Whether `p` lies inside the shape. A mask with no shape contains
    /// every point.
    pub fn contains(&self, p: &Point<P>) -> bool {
        self.is_none() || self.signed_distance((p.x.to_f32(), p.y.to_f32())) <= 0.0
    }

    /// Whether the inner shape lies inside this one at every corner of this
    /// one that overlaps `area` (rule 4).
    fn holds(&self, inner: &Self, area: &Rect) -> bool {
        let inner_rect = Rect::of(&inner.bounds).intersect(area);
        let inner_corners = inner.corners();
        let inner_rect_full = Rect::of(&inner.bounds);
        let inner_points = [
            (inner_rect_full.x0, inner_rect_full.y0),
            (inner_rect_full.x1, inner_rect_full.y0),
            (inner_rect_full.x1, inner_rect_full.y1),
            (inner_rect_full.x0, inner_rect_full.y1),
        ];
        self.corners()
            .iter()
            .zip(inner_corners.iter().zip(inner_points.iter()))
            .all(|(outer, (inner_corner, &(px, py)))| {
                let Some(outer) = outer else {
                    return true;
                };
                if !outer.square.overlaps(area) || !outer.square.overlaps(&inner_rect) {
                    return true;
                }
                let (radius, centre) = match inner_corner {
                    Some(corner) => (corner.radius, corner.centre),
                    None => (0.0, (px, py)),
                };
                // Sufficient, not exact: a disc inside the outer corner's
                // disc keeps the inner shape inside the outer one there.
                let distance =
                    ((centre.0 - outer.centre.0).powi(2) + (centre.1 - outer.centre.1).powi(2))
                        .sqrt();
                distance + radius <= outer.radius + 1e-4
            })
    }

    /// This shape for a primitive that covers `area`: kept when `area`
    /// reaches one of its corner squares, cleared otherwise. Inside the
    /// mask's rectangle and away from the corner squares the shape cuts
    /// nothing, so the primitive keeps the fast path.
    pub fn culled_for(&self, area: &Bounds<P>) -> Self {
        if self.is_none() || self.has_active_corner(&Rect::of(area)) {
            *self
        } else {
            Self::default()
        }
    }
}

impl<P: MaskUnit> ContentMask<P> {
    /// A rectangular mask.
    pub fn rect(bounds: Bounds<P>) -> Self {
        ContentMask {
            bounds,
            rounded: RoundedMask::default(),
        }
    }

    /// A mask with rounded corners: `bounds` cut to `radii` at its corners.
    /// Radii are clamped to half the shorter side.
    pub fn rounded(bounds: Bounds<P>, radii: Corners<P>) -> Self {
        let rounded = RoundedMask::new(bounds, radii);
        ContentMask {
            bounds,
            rounded: if rounded.is_none() {
                RoundedMask::default()
            } else {
                rounded
            },
        }
    }

    /// Whether `point` is inside the mask: inside its rectangle and inside
    /// its rounded shape.
    pub fn contains(&self, point: &Point<P>) -> bool {
        self.bounds.contains(point) && self.rounded.contains(point)
    }

    /// The mask that holds what both masks let through, with `self` the
    /// inner (newer) mask and `outer` the one it is nested in, and whether
    /// that is exact. The rectangles intersect exactly. The rounded shapes
    /// follow the nesting rules of `docs/decisions/rounded-content-masks.md`:
    ///
    /// 1. one side has no shape: the other's;
    /// 2. the outer shape has no corner in the new rectangle: the inner's;
    /// 3. the inner shape has none: the outer's;
    /// 4. the inner shape lies inside the outer at every corner that
    ///    matters: the inner's;
    /// 5. otherwise the inner's, and the result is [`Nesting::Inexact`].
    pub fn nest_in(&self, outer: &Self) -> (Self, Nesting) {
        let bounds = self.bounds.intersect(&outer.bounds);
        let area = Rect::of(&bounds);
        let inner_shape = &self.rounded;
        let outer_shape = &outer.rounded;
        let (rounded, nesting) = if inner_shape.is_none() {
            (*outer_shape, Nesting::Exact)
        } else if outer_shape.is_none() || !outer_shape.has_active_corner(&area) {
            (*inner_shape, Nesting::Exact)
        } else if !inner_shape.has_active_corner(&area) || outer_shape.holds(inner_shape, &area) {
            if inner_shape.has_active_corner(&area) {
                (*inner_shape, Nesting::Exact)
            } else {
                (*outer_shape, Nesting::Exact)
            }
        } else {
            (*inner_shape, Nesting::Inexact)
        };
        // A shape with no corner inside the rectangle cuts nothing there.
        let rounded = if rounded.has_active_corner(&area) {
            rounded
        } else {
            RoundedMask::default()
        };
        (ContentMask { bounds, rounded }, nesting)
    }

    /// This mask for a primitive that covers `area` (its bounds, transformed
    /// when it is): the rounded shape is cleared when `area`, inside the
    /// rectangle, reaches none of its corners.
    pub fn culled_for(&self, area: &Bounds<P>) -> Self {
        if self.rounded.is_none() {
            return *self;
        }
        let area = area.intersect(&self.bounds);
        ContentMask {
            bounds: self.bounds,
            rounded: self.rounded.culled_for(&area),
        }
    }
}

impl ContentMask<Pixels> {
    /// Scale the content mask's pixel units by the given scaling factor.
    pub fn scale(&self, factor: f32) -> ContentMask<ScaledPixels> {
        ContentMask {
            bounds: self.bounds.scale(factor),
            rounded: RoundedMask {
                bounds: self.rounded.bounds.scale(factor),
                radii: self.rounded.radii.scale(factor),
            },
        }
    }

    /// Intersect the content mask with the given content mask, `self` being
    /// the inner one (see [`ContentMask::nest_in`]).
    pub fn intersect(&self, other: &Self) -> Self {
        self.nest_in(other).0
    }
}
