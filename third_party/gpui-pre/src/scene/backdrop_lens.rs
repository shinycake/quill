//! QUILL: backdrop glass. A [`BackdropLens`] is a slab of glass drawn over
//! whatever the frame already holds under it: the renderer copies the
//! footprint out of the target, blurs it into a few mip levels and draws the
//! lens shader from that copy. See third_party/gpui-pre/QUILL-CHANGES.md
//! ("Backdrop glass") for the design.
//!
//! The optics follow gpui-liquid-glass's CPU reference (`optics.rs`), which
//! follows bilalgursen/liquid-glass (MIT): lengths of the shading are in
//! design units of a lens 120 units tall, and `unit` says how many logical
//! pixels one unit is.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    Background, Bounds, ContentMask, Corners, DrawOrder, Edges, Hsla, Pixels, Point, Quad,
    ScaledPixels, Size, point, px,
};

/// Lens orders sit on multiples of `1 << LENS_BAND_BITS`: a lens takes the
/// next multiple above the highest order it covers, so every lens with no
/// glass under it shares one order, one batch and one copy of the frame.
pub(crate) const LENS_BAND_BITS: u32 = 24;

/// The height the shading constants were designed for, in units.
pub const LENS_DESIGN_HEIGHT: f32 = 120.0;

/// How the rim of clear glass bends what is behind it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum LensBend {
    /// Along the rim, as deep as the bevel, with a magnified middle and a
    /// slight colour split: buttons and lenses. `refract` is in units,
    /// `blur_center` and `blur_edge` in mip levels.
    Edge {
        /// How far the rim pulls the backdrop in, units.
        refract: f32,
        /// How much the dome magnifies the middle (0 = flat).
        magnify: f32,
        /// How far red and blue split from green.
        dispersion: f32,
        /// Blur in the middle, mip levels.
        blur_center: f32,
        /// Blur at the rim, mip levels.
        blur_edge: f32,
    },
    /// Towards the centre by `amount` logical pixels at the rim: cards.
    Radial {
        /// The pull at the rim.
        amount: Pixels,
    },
}

/// Clear glass shows (and bends) the backdrop; tinted glass is its own colour.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum LensMaterial {
    /// Glass that shows what is behind it.
    Clear {
        /// How the rim bends the backdrop.
        bend: LensBend,
        /// 1 = the flat highlight colour, 0 = perfectly clear.
        frost: f32,
        /// Gaussian deviation of the frosted blur. 0 = sharp.
        blur: Pixels,
        /// How much of the frost covers the sharp bend.
        blur_mix: f32,
    },
    /// Glass of its own colour, thinning towards the rim.
    Tinted {
        /// How much the rim thins towards the surface colour.
        edge_clear: f32,
    },
}

/// A line just inside the rim.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LensBorder {
    /// The line's colour; its alpha is how strongly it shows.
    pub color: Hsla,
    /// Width of the line.
    pub width: Pixels,
}

/// Multipliers on the default look; 1.0 everywhere is the default, except
/// `bend_curve` (1.6). `frost_edge` multiplies the frost's blur at the rim
/// (the source shader's `uFrostEdge`): above 1 the frost thickens towards
/// the edge, below 1 it clears. `rim_frost` multiplies the frost itself
/// across the rim band of clear glass (1 = even frost, 0 = a clear rim
/// that shows the bent backdrop around a frosted middle).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[allow(missing_docs)]
pub struct LensTune {
    pub rim: f32,
    pub rim_width: f32,
    pub fresnel: f32,
    pub inner: f32,
    pub reflection: f32,
    pub caustic: f32,
    pub bend_curve: f32,
    pub bend_depth: f32,
    pub saturation: f32,
    pub frost_edge: f32,
    pub rim_frost: f32,
}

impl Default for LensTune {
    fn default() -> Self {
        Self {
            rim: 1.0,
            rim_width: 1.0,
            fresnel: 1.0,
            inner: 1.0,
            reflection: 1.0,
            caustic: 1.0,
            bend_curve: 1.6,
            bend_depth: 1.0,
            saturation: 1.0,
            frost_edge: 1.0,
            rim_frost: 1.0,
        }
    }
}

/// QUILL (glass stage 5): a round droplet melted onto the body, on its
/// horizontal axis (the source kit's `droplet`): the arrow of an action
/// button pulling away from it. Logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LensDroplet {
    /// The droplet's centre, from the body's centre along x.
    pub offset_x: Pixels,
    /// Its radius.
    pub radius: Pixels,
    /// How wide the gooey bridge to the body is (0 = two separate shapes).
    pub softness: Pixels,
}

/// QUILL (glass stage 5): where a blob's bridge gathers into a neck. Logical
/// pixels.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LensNeck {
    /// The neck's x, from the body's centre.
    pub x: Pixels,
    /// How far from it the bridge's softness falls off.
    pub width: Pixels,
}

/// QUILL (glass stage 5): a second rounded box melted into the body with a
/// smooth union, in any direction (the source kit's `blob`): a list
/// swelling out of its button. Logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LensBlob {
    /// Its centre, from the body's centre.
    pub offset: Point<Pixels>,
    /// Its half size.
    #[schemars(with = "(f32, f32)")]
    pub half_size: Size<Pixels>,
    /// Its corner radius.
    pub corner_radius: Pixels,
    /// How wide the gooey bridge to the body is (0 = two separate shapes).
    pub softness: Pixels,
    /// Gathers the bridge into a neck around one point.
    pub neck: Option<LensNeck>,
}

/// QUILL (glass stage 5): where the glass gets the picture it bends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum LensDetail {
    /// The pixels the frame drew under the lens.
    #[default]
    Copied,
    /// What lies under the lens drawn again at `density` texels per device
    /// pixel (text rasterized at that scale), so a magnifying lens stays
    /// sharp. Renderers that can't draw it this way copy the frame instead.
    Rendered {
        /// Texels per device pixel, 2 to 4.
        density: u32,
    },
}

/// The look of one lens, everything but where it is.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LensStyle {
    /// Logical pixels per design unit; 0 takes the body's height over
    /// [`LENS_DESIGN_HEIGHT`].
    pub unit: f32,
    /// Bevel width, units.
    pub bevel: f32,
    /// Bend band when it must not follow the bevel (square corners), units.
    pub band: Option<f32>,
    /// Drop shadow strength.
    pub shadow: f32,
    /// Drop shadow softness, units.
    pub shadow_softness: f32,
    /// Drop shadow offset, units.
    pub shadow_offset: f32,
    /// 0 = flat liquid, 1 = full glass lighting.
    pub strength: f32,
    /// The highlight (frosted) colour, or the tint of tinted glass.
    pub color: Hsla,
    /// The colour the glass sits on.
    pub surface: Hsla,
    /// Clear or tinted.
    pub material: LensMaterial,
    /// A line inside the rim.
    pub border: Option<LensBorder>,
    /// Multipliers on the look.
    pub tune: LensTune,
    /// Direction from the lens towards the pointer.
    pub light: Point<f32>,
    /// How far the rim light leans to `light` (0..1).
    pub light_mix: f32,
    /// QUILL (glass stage 5): a droplet melted onto the body.
    pub droplet: Option<LensDroplet>,
    /// QUILL (glass stage 5): a box melted into the body.
    pub blob: Option<LensBlob>,
    /// QUILL (glass stage 5): copied or re-rendered backdrop.
    pub detail: LensDetail,
}

/// A lens in the scene, as the renderers read it. Lengths are device
/// pixels unless a field says units; the optics are [`LensStyle`]'s,
/// flattened.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
#[allow(missing_docs)]
pub struct BackdropLens {
    pub order: DrawOrder,
    /// 0 = clear, 1 = tinted.
    pub material: u32,
    /// The footprint: the body and the room its shadow takes. The renderer
    /// copies what is under it.
    pub bounds: Bounds<ScaledPixels>,
    pub content_mask: ContentMask<ScaledPixels>,
    /// The glass itself.
    pub body: Bounds<ScaledPixels>,
    pub corner_radius: ScaledPixels,
    /// Device pixels per logical pixel.
    pub scale_factor: f32,
    /// Logical pixels per design unit.
    pub unit: f32,
    pub bevel: f32,
    /// Units; 0 = the bend follows the bevel.
    pub band: f32,
    pub shadow: f32,
    pub shadow_softness: f32,
    pub shadow_offset: f32,
    pub strength: f32,
    pub opacity: f32,
    pub color: Hsla,
    pub surface: Hsla,
    /// 0 = edge, 1 = radial (clear glass).
    pub bend: u32,
    pub refract: f32,
    pub magnify: f32,
    pub dispersion: f32,
    pub blur_center: f32,
    pub blur_edge: f32,
    pub frost: f32,
    /// Logical pixels.
    pub blur: f32,
    pub blur_mix: f32,
    /// Logical pixels.
    pub radial_amount: f32,
    pub edge_clear: f32,
    /// The border's colour; alpha 0 = no border.
    pub border_color: Hsla,
    /// Logical pixels.
    pub border_width: f32,
    pub tune_rim: f32,
    pub tune_rim_width: f32,
    pub tune_fresnel: f32,
    pub tune_inner: f32,
    pub tune_reflection: f32,
    pub tune_caustic: f32,
    pub tune_bend_curve: f32,
    pub tune_bend_depth: f32,
    pub tune_saturation: f32,
    pub light_x: f32,
    pub light_y: f32,
    pub light_mix: f32,
    /// QUILL (glass stage 5): the order the lens was painted in this frame, for
    /// labels and re-rendered backdrops that refer to it.
    pub id: u32,
    /// 0 = copied backdrop, else the density of a re-rendered one.
    pub density: u32,
    /// The droplet: centre x from the body's centre, radius and bridge
    /// softness, logical pixels (radius 0 = none).
    pub drop_x: f32,
    pub drop_r: f32,
    pub drop_k: f32,
    /// The blob: centre from the body's centre, half size, corner radius
    /// and bridge softness, logical pixels (softness 0 = none).
    pub blob_x: f32,
    pub blob_y: f32,
    pub blob_hx: f32,
    pub blob_hy: f32,
    pub blob_r: f32,
    pub blob_k: f32,
    /// The neck: x from the body's centre and falloff width, logical
    /// pixels (width 0 = none).
    pub neck_x: f32,
    pub neck_w: f32,
    pub tune_frost_edge: f32,
}

/// Room around a body of `height` for the drop shadow below it and the
/// rim light around it, logical pixels: (sides, top, bottom). As
/// gpui-liquid-glass's element pads its canvas.
fn lens_pad(height: f32, casts_shadow: bool) -> (f32, f32, f32) {
    if !casts_shadow {
        return (16.0, 16.0, 16.0);
    }
    let top = (height * 0.3 + 6.0).round().max(16.0);
    let bottom = (height * 1.1 + 8.0)
        .round()
        .min(height * 0.6 + 40.0)
        .max(16.0);
    (16.0, top, bottom)
}

impl BackdropLens {
    /// A lens whose glass fills `body` (device pixels) and whose shadow
    /// reaches `bounds`, drawn inside `content_mask`.
    pub fn new(
        body: Bounds<ScaledPixels>,
        corner_radius: ScaledPixels,
        bounds: Bounds<ScaledPixels>,
        content_mask: ContentMask<ScaledPixels>,
        scale_factor: f32,
        style: &LensStyle,
    ) -> Self {
        let height = body.size.height.0 / scale_factor.max(1e-3);
        let unit = if style.unit > 0.0 {
            style.unit
        } else {
            (height / LENS_DESIGN_HEIGHT).max(1e-3)
        };
        let mut lens = Self {
            order: 0,
            material: 0,
            bounds,
            content_mask,
            body,
            corner_radius,
            scale_factor,
            unit,
            bevel: style.bevel,
            band: style.band.unwrap_or(0.0),
            shadow: style.shadow,
            shadow_softness: style.shadow_softness,
            shadow_offset: style.shadow_offset,
            strength: style.strength,
            opacity: 1.0,
            color: style.color,
            surface: style.surface,
            bend: 0,
            refract: 0.0,
            magnify: 0.0,
            dispersion: 0.0,
            blur_center: 0.0,
            blur_edge: 0.0,
            frost: 0.0,
            blur: 0.0,
            blur_mix: 0.0,
            radial_amount: 0.0,
            edge_clear: 0.0,
            border_color: style.border.map_or(Hsla::default(), |border| border.color),
            border_width: style.border.map_or(0.0, |border| border.width.0),
            tune_rim: style.tune.rim,
            tune_rim_width: style.tune.rim_width,
            tune_fresnel: style.tune.fresnel,
            tune_inner: style.tune.inner,
            tune_reflection: style.tune.reflection,
            tune_caustic: style.tune.caustic,
            tune_bend_curve: style.tune.bend_curve,
            tune_bend_depth: style.tune.bend_depth,
            tune_saturation: style.tune.saturation,
            light_x: style.light.x,
            light_y: style.light.y,
            light_mix: style.light_mix.clamp(0.0, 1.0),
            id: 0,
            density: match style.detail {
                LensDetail::Copied => 0,
                LensDetail::Rendered { density } => density.clamp(2, 4),
            },
            drop_x: 0.0,
            drop_r: 0.0,
            drop_k: 0.0,
            blob_x: 0.0,
            blob_y: 0.0,
            blob_hx: 0.0,
            blob_hy: 0.0,
            blob_r: 0.0,
            blob_k: 0.0,
            neck_x: 0.0,
            neck_w: 0.0,
            tune_frost_edge: style.tune.frost_edge.max(0.0),
        };
        if let Some(droplet) = style.droplet.filter(|d| d.radius.0 > 0.0) {
            lens.drop_x = droplet.offset_x.0;
            lens.drop_r = droplet.radius.0;
            lens.drop_k = droplet.softness.0.max(0.0);
        }
        if let Some(blob) = style.blob.filter(|b| b.softness.0 > 0.0) {
            lens.blob_x = blob.offset.x.0;
            lens.blob_y = blob.offset.y.0;
            lens.blob_hx = blob.half_size.width.0.max(0.0);
            lens.blob_hy = blob.half_size.height.0.max(0.0);
            lens.blob_r = blob
                .corner_radius
                .0
                .clamp(0.0, lens.blob_hx.min(lens.blob_hy));
            lens.blob_k = blob.softness.0;
            if let Some(neck) = blob.neck.filter(|n| n.width.0 > 0.0) {
                lens.neck_x = neck.x.0;
                lens.neck_w = neck.width.0;
            }
        }
        match style.material {
            LensMaterial::Clear {
                bend,
                frost,
                blur,
                blur_mix,
            } => {
                lens.frost = frost;
                lens.edge_clear = (1.0 - style.tune.rim_frost).clamp(0.0, 1.0);
                lens.blur = blur.0.max(0.0);
                lens.blur_mix = blur_mix;
                match bend {
                    LensBend::Edge {
                        refract,
                        magnify,
                        dispersion,
                        blur_center,
                        blur_edge,
                    } => {
                        lens.refract = refract;
                        lens.magnify = magnify;
                        lens.dispersion = dispersion;
                        lens.blur_center = blur_center;
                        lens.blur_edge = blur_edge;
                    }
                    LensBend::Radial { amount } => {
                        lens.bend = 1;
                        lens.radial_amount = amount.0;
                    }
                }
            }
            LensMaterial::Tinted { edge_clear } => {
                lens.material = 1;
                lens.band = 0.0;
                lens.edge_clear = edge_clear;
            }
        }
        lens
    }

    /// The look where the glass can't be drawn (a lens inside a paint
    /// layer, a renderer without the glass shader, a frame it can't read):
    /// the body as a rounded quad of the glass's colour.
    pub fn fallback_quad(&self) -> Quad {
        let half = self.body.size.width.0.min(self.body.size.height.0) / 2.0;
        let radius = ScaledPixels(self.corner_radius.0.clamp(0.0, half.max(0.0)));
        let fill = if self.material == 1 {
            self.color
        } else {
            let alpha = self.color.a * (0.55 + 0.45 * self.frost.clamp(0.0, 1.0));
            Hsla {
                a: alpha,
                ..self.color
            }
        };
        let border = ScaledPixels(self.border_width * self.scale_factor);
        let has_border = border.0 > 0.0 && self.border_color.a > 0.0;
        Quad {
            order: self.order,
            border_style: Default::default(),
            bounds: self.body,
            content_mask: self.content_mask,
            background: Background::from(fill.opacity(self.opacity)),
            border_color: self.border_color.opacity(self.opacity),
            corner_radii: Corners::all(radius),
            border_widths: Edges::all(if has_border {
                border
            } else {
                ScaledPixels(0.0)
            }),
        }
    }
}

/// Mip levels of a lens atlas: level k is blurred over about 2^k pixels.
pub const LENS_ATLAS_LEVELS: u32 = 5;
/// Atlas slots start on multiples of this, so each level's slot starts on
/// a whole texel.
pub const LENS_ATLAS_ALIGN: u32 = 1 << (LENS_ATLAS_LEVELS - 1);
/// Room between atlas slots; no tap crosses it (the shaders clamp to the
/// slot).
pub const LENS_ATLAS_GUTTER: u32 = 16;

/// One lens footprint a renderer copies: from the target at `from` to the
/// atlas at `to`, `size` device pixels. A re-rendered backdrop
/// (`density` > 1) fills `size` x `density` texels of the atlas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LensCopy {
    /// Where the footprint starts in the target.
    pub from: (u32, u32),
    /// Where its slot starts in the atlas.
    pub to: (u32, u32),
    /// The footprint, clipped to the target.
    pub size: (u32, u32),
    /// QUILL (glass stage 5): atlas texels per device pixel (1 for a copy).
    pub density: u32,
}

impl LensCopy {
    /// The slot's size in atlas texels.
    pub fn texels(&self) -> (u32, u32) {
        let density = self.density.max(1);
        (
            self.size.0.saturating_mul(density),
            self.size.1.saturating_mul(density),
        )
    }
}

/// QUILL (glass stage 5): a part of the target a lens reads, and how densely its
/// slot holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LensRegion {
    /// Where it starts in the target, device pixels.
    pub from: (u32, u32),
    /// Its size, device pixels.
    pub size: (u32, u32),
    /// Atlas texels per device pixel.
    pub density: u32,
}

impl LensRegion {
    /// `bounds` rounded out to whole pixels and clipped to a `target` of
    /// that size, one texel per pixel.
    pub fn clipped(bounds: Bounds<ScaledPixels>, target: (u32, u32)) -> Self {
        let clamp_x = |v: f32| v.clamp(0.0, target.0 as f32);
        let clamp_y = |v: f32| v.clamp(0.0, target.1 as f32);
        let x0 = clamp_x(bounds.origin.x.0.floor());
        let y0 = clamp_y(bounds.origin.y.0.floor());
        let x1 = clamp_x((bounds.origin.x.0 + bounds.size.width.0).ceil());
        let y1 = clamp_y((bounds.origin.y.0 + bounds.size.height.0).ceil());
        Self {
            from: (x0 as u32, y0 as u32),
            size: ((x1 - x0).max(0.0) as u32, (y1 - y0).max(0.0) as u32),
            density: 1,
        }
    }

    /// Whether `other` lies inside this region.
    pub fn contains(&self, other: &LensRegion) -> bool {
        let (x0, y0) = (u64::from(self.from.0), u64::from(self.from.1));
        let (x1, y1) = (x0 + u64::from(self.size.0), y0 + u64::from(self.size.1));
        let (ox0, oy0) = (u64::from(other.from.0), u64::from(other.from.1));
        let (ox1, oy1) = (ox0 + u64::from(other.size.0), oy0 + u64::from(other.size.1));
        ox0 >= x0 && oy0 >= y0 && ox1 <= x1 && oy1 <= y1
    }

    /// The region as device-pixel bounds.
    pub fn bounds(&self) -> Bounds<ScaledPixels> {
        Bounds {
            origin: point(
                ScaledPixels(self.from.0 as f32),
                ScaledPixels(self.from.1 as f32),
            ),
            size: Size {
                width: ScaledPixels(self.size.0 as f32),
                height: ScaledPixels(self.size.1 as f32),
            },
        }
    }
}

/// Where a lens batch's footprints go in the atlas, and the atlas size they
/// need. Every backend packs the same way.
#[derive(Debug, PartialEq, Eq)]
pub struct LensPacking {
    /// One per lens, in the batch's order.
    pub copies: Vec<LensCopy>,
    /// The atlas width the slots need.
    pub width: u32,
    /// The atlas height the slots need.
    pub height: u32,
}

fn align_up(value: u32) -> u32 {
    value
        .div_ceil(LENS_ATLAS_ALIGN)
        .saturating_mul(LENS_ATLAS_ALIGN)
}

impl LensPacking {
    /// Shelves the footprints of `lenses`, clipped to a `target` of that
    /// size, into an atlas; `None` when they don't fit in `max_side`.
    pub fn pack(lenses: &[BackdropLens], target: (u32, u32), max_side: u32) -> Option<Self> {
        let regions: Vec<LensRegion> = lenses
            .iter()
            .map(|lens| LensRegion::clipped(lens.bounds, target))
            .collect();
        Self::pack_regions(&regions, target, max_side)
    }

    /// QUILL (glass stage 5): shelves `regions` into an atlas, in order; `None`
    /// when they don't fit in `max_side`. The same regions always land in
    /// the same slots.
    pub fn pack_regions(
        regions: &[LensRegion],
        target: (u32, u32),
        max_side: u32,
    ) -> Option<Self> {
        let texels = |region: &LensRegion| {
            let density = region.density.max(1);
            (
                region.size.0.saturating_mul(density),
                region.size.1.saturating_mul(density),
            )
        };
        let widest = regions
            .iter()
            .map(|region| align_up(texels(region).0))
            .max()
            .unwrap_or(0);
        let one_row: u32 = regions
            .iter()
            .map(|region| align_up(texels(region).0).saturating_add(LENS_ATLAS_GUTTER))
            .fold(0, u32::saturating_add);
        let width = one_row
            .min(align_up(target.0).saturating_add(LENS_ATLAS_GUTTER))
            .max(widest)
            .max(LENS_ATLAS_ALIGN);
        if width > max_side {
            return None;
        }
        let (mut x, mut y, mut row) = (0u32, 0u32, 0u32);
        let mut copies = Vec::with_capacity(regions.len());
        for region in regions {
            let (w, h) = texels(region);
            let slot_w = align_up(w);
            if x > 0 && x.saturating_add(slot_w) > width {
                x = 0;
                y = y.saturating_add(row).saturating_add(LENS_ATLAS_GUTTER);
                row = 0;
            }
            copies.push(LensCopy {
                from: region.from,
                to: (x, y),
                size: region.size,
                density: region.density.max(1),
            });
            x = x.saturating_add(slot_w).saturating_add(LENS_ATLAS_GUTTER);
            row = row.max(align_up(h));
        }
        let height = y.saturating_add(row).max(LENS_ATLAS_ALIGN);
        (height <= max_side).then_some(Self {
            copies,
            width,
            height,
        })
    }
}

impl LensStyle {
    /// QUILL (glass stage 5): the glass's extent around a body that fills `body`:
    /// the body, its droplet and its blob, and the bulge of the gooey
    /// bridges between them (a smooth union reaches a quarter of its
    /// softness past the shapes it joins).
    pub fn glass_bounds(&self, body: Bounds<Pixels>) -> Bounds<Pixels> {
        let center = body.center();
        let mut glass = body;
        let mut bulge = 0.0f32;
        if let Some(droplet) = self.droplet.filter(|d| d.radius.0 > 0.0) {
            let at = point(center.x + droplet.offset_x, center.y);
            let r = droplet.radius;
            glass = glass.union(&Bounds::from_corners(
                point(at.x - r, at.y - r),
                point(at.x + r, at.y + r),
            ));
            bulge = bulge.max(droplet.softness.0);
        }
        if let Some(blob) = self.blob.filter(|b| b.softness.0 > 0.0) {
            let at = center + blob.offset;
            let half = blob.half_size;
            glass = glass.union(&Bounds::from_corners(
                point(at.x - half.width, at.y - half.height),
                point(at.x + half.width, at.y + half.height),
            ));
            bulge = bulge.max(blob.softness.0);
        }
        glass.dilate(px(bulge.max(0.0) / 4.0))
    }

    /// Where a lens whose body fills `body` draws: the glass and the room
    /// its shadow and rim light take around it.
    pub fn footprint(&self, body: Bounds<Pixels>) -> Bounds<Pixels> {
        let casts_shadow = self.shadow * self.strength > 0.0;
        let (sides, top, bottom) = lens_pad(body.size.height.0, casts_shadow);
        let glass = self.glass_bounds(body);
        Bounds::from_corners(
            point(glass.left() - px(sides), glass.top() - px(top)),
            point(glass.right() + px(sides), glass.bottom() + px(bottom)),
        )
    }
}

impl BackdropLens {
    /// QUILL (glass stage 5): the glass's extent in device pixels (the body, its
    /// droplet and its blob), without the shadow's room.
    pub fn glass_bounds(&self) -> Bounds<ScaledPixels> {
        let s = self.scale_factor;
        let center = self.body.center();
        let mut glass = self.body;
        let mut bulge = 0.0f32;
        if self.drop_r > 0.0 {
            let (x, r) = (center.x.0 + self.drop_x * s, self.drop_r * s);
            glass = glass.union(&Bounds::from_corners(
                point(ScaledPixels(x - r), ScaledPixels(center.y.0 - r)),
                point(ScaledPixels(x + r), ScaledPixels(center.y.0 + r)),
            ));
            bulge = bulge.max(self.drop_k * s);
        }
        if self.blob_k > 0.0 {
            let (x, y) = (center.x.0 + self.blob_x * s, center.y.0 + self.blob_y * s);
            let (hx, hy) = (self.blob_hx * s, self.blob_hy * s);
            glass = glass.union(&Bounds::from_corners(
                point(ScaledPixels(x - hx), ScaledPixels(y - hy)),
                point(ScaledPixels(x + hx), ScaledPixels(y + hy)),
            ));
            bulge = bulge.max(self.blob_k * s);
        }
        glass.dilate(ScaledPixels(bulge / 4.0))
    }
}
