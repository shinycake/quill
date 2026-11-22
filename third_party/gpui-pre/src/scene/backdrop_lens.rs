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

use crate::{
    Background, Bounds, ContentMask, Corners, DrawOrder, Edges, Hsla, Pixels, Point, Quad,
    ScaledPixels, point, px,
};

/// Lens orders sit on multiples of `1 << LENS_BAND_BITS`: a lens takes the
/// next multiple above the highest order it covers, so every lens with no
/// glass under it shares one order, one batch and one copy of the frame.
pub(crate) const LENS_BAND_BITS: u32 = 24;

/// The height the shading constants were designed for, in units.
pub const LENS_DESIGN_HEIGHT: f32 = 120.0;

/// How the rim of clear glass bends what is behind it.
#[derive(Clone, Copy, Debug, PartialEq)]
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
#[derive(Clone, Copy, Debug, PartialEq)]
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LensBorder {
    /// The line's colour; its alpha is how strongly it shows.
    pub color: Hsla,
    /// Width of the line.
    pub width: Pixels,
}

/// Multipliers on the default look; 1.0 everywhere is the default, except
/// `bend_curve` (1.6).
#[derive(Clone, Copy, Debug, PartialEq)]
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
        }
    }
}

/// The look of one lens, everything but where it is.
#[derive(Clone, Copy, Debug, PartialEq)]
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
        };
        match style.material {
            LensMaterial::Clear {
                bend,
                frost,
                blur,
                blur_mix,
            } => {
                lens.frost = frost;
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

impl LensStyle {
    /// Where a lens whose body fills `body` draws: the body and the room
    /// its shadow and rim light take around it.
    pub fn footprint(&self, body: Bounds<Pixels>) -> Bounds<Pixels> {
        let casts_shadow = self.shadow * self.strength > 0.0;
        let (sides, top, bottom) = lens_pad(body.size.height.0, casts_shadow);
        Bounds::from_corners(
            point(body.left() - px(sides), body.top() - px(top)),
            point(body.right() + px(sides), body.bottom() + px(bottom)),
        )
    }
}
