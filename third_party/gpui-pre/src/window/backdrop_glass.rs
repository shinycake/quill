// Added by the Quill project (2026). See third_party/gpui-pre/QUILL-CHANGES.md.
//! QUILL (backdrop glass, stage 5): the window's side of inked labels and re-rendered
//! backdrops.

use std::borrow::Cow;

use crate::scene::{GlyphSource, SlotSpace, UnderItem, primitives_under_ranked};
use crate::{
    App, Bounds, IsZero as _, LensInk, LensRegion, MonochromeSprite, PolychromeSprite,
    RenderGlyphParams, RenderSvgParams, SMOOTH_SVG_SCALE_FACTOR, SUBPIXEL_VARIANTS_X,
    SUBPIXEL_VARIANTS_Y, ScaledPixels, Scene, SharpBackdrop, Size, TransformationMatrix, point,
    util::round_half_toward_zero,
};

use super::Window;

/// Whether painted glyphs record how to rasterize them again.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum GlyphRecording {
    /// No frame needs it.
    #[default]
    Off,
    /// The last frame had a re-rendered backdrop.
    On,
}

/// What painting glass-aware content needs from the window.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct GlassPaint {
    ink: Option<LensInk>,
    glyphs: GlyphRecording,
}

impl GlassPaint {
    pub(crate) fn ink(&self) -> Option<LensInk> {
        self.ink
    }

    pub(crate) fn records_glyphs(&self) -> bool {
        self.glyphs == GlyphRecording::On
    }
}

impl Window {
    /// QUILL (glass stage 5): paints `f` with label colours that follow the glass
    /// under them. Text and icons painted inside take `ink.on_light` where
    /// the glass under each glyph is light and `ink.on_dark` where it is
    /// dark, read by the renderer from what the lens bends, in the same
    /// frame. They draw in grayscale antialiasing. Where no lens lies under
    /// a glyph, or the renderer can't ink, it keeps its own colour.
    pub fn with_lens_ink<R>(&mut self, ink: LensInk, f: impl FnOnce(&mut Self) -> R) -> R {
        let outer = self.glass.ink.replace(ink);
        let result = f(self);
        self.glass.ink = outer;
        result
    }

    /// QUILL (glass stage 5): draws what lies under each `LensDetail::Rendered`
    /// lens of the finished frame again, `density` times finer, into
    /// `Scene::sharp_backdrops`. Glyphs painted while recording was off
    /// (the first frame with such a lens) are enlarged instead, and the
    /// next frame records them.
    pub(super) fn build_sharp_backdrops(&mut self, cx: &App) {
        let wanted = self
            .next_frame
            .scene
            .lenses
            .iter()
            .any(|lens| lens.density > 1);
        let recorded = self.glass.records_glyphs();
        self.glass.glyphs = if wanted {
            GlyphRecording::On
        } else {
            GlyphRecording::Off
        };
        if !wanted {
            return;
        }
        if !recorded {
            self.on_next_frame(|window, _| window.refresh());
        }
        let scale_factor = self.scale_factor();
        let target = (
            (self.viewport_size.width.0 * scale_factor).ceil().max(0.0) as u32,
            (self.viewport_size.height.0 * scale_factor).ceil().max(0.0) as u32,
        );
        let scene = std::mem::take(&mut self.next_frame.scene);
        let mut sharp = Vec::new();
        for (index, lens) in scene.lenses.iter().enumerate() {
            if lens.density <= 1 {
                continue;
            }
            let region = LensRegion::clipped(lens.bounds, target);
            if region.size.0 == 0 || region.size.1 == 0 {
                continue;
            }
            let bounds = region.bounds();
            let Some(under) = primitives_under_ranked(&scene, bounds, lens.order) else {
                continue;
            };
            // Paths draw through a target-sized intermediate the slot's
            // finer scale can't share, so glass over a path copies.
            if under.iter().any(|item| matches!(item, UnderItem::Path(_))) {
                continue;
            }
            let space = SlotSpace {
                origin: bounds.origin,
                density: lens.density as f32,
                size: Size {
                    width: ScaledPixels((region.size.0 * lens.density) as f32),
                    height: ScaledPixels((region.size.1 * lens.density) as f32),
                },
            };
            let mut sub = Scene::default();
            for item in under {
                self.redraw_under(&scene, item, &space, scale_factor, &mut sub, cx);
            }
            sub.finish();
            sharp.push(SharpBackdrop {
                lens: index,
                region: bounds,
                density: lens.density,
                scene: sub,
            });
        }
        self.next_frame.scene = scene;
        self.next_frame.scene.sharp_backdrops = sharp;
    }

    /// One primitive of `scene` drawn into `sub` in `space`: glyphs and
    /// icons rasterized again when their source is known, everything else
    /// scaled.
    fn redraw_under(
        &self,
        scene: &Scene,
        item: UnderItem,
        space: &SlotSpace,
        scale_factor: f32,
        sub: &mut Scene,
        cx: &App,
    ) {
        match item {
            UnderItem::Shadow(i) => {
                if let Some(s) = scene.shadows.get(i) {
                    sub.insert_primitive(space.shadow(s));
                }
            }
            UnderItem::Quad(i) => {
                if let Some(q) = scene.quads.get(i) {
                    sub.insert_primitive(space.quad(q));
                }
            }
            UnderItem::Path(i) => {
                if let Some(p) = scene.paths.get(i) {
                    sub.insert_primitive(space.path(p));
                }
            }
            UnderItem::Underline(i) => {
                if let Some(u) = scene.underlines.get(i) {
                    sub.insert_primitive(space.underline(u));
                }
            }
            UnderItem::MonochromeSprite(i) => {
                let Some(sprite) = scene.monochrome_sprites.get(i) else {
                    return;
                };
                let source = scene.glyph_source(&sprite.tile, &sprite.bounds);
                let redrawn = source
                    .and_then(|source| self.raster_mono(source, sprite, space, scale_factor, cx));
                sub.insert_primitive(redrawn.unwrap_or_else(|| space.monochrome(sprite)));
            }
            UnderItem::SubpixelSprite(i) => {
                if let Some(s) = scene.subpixel_sprites.get(i) {
                    sub.insert_primitive(space.subpixel(s));
                }
            }
            UnderItem::PolychromeSprite(i) => {
                let Some(sprite) = scene.polychrome_sprites.get(i) else {
                    return;
                };
                let source = scene.glyph_source(&sprite.tile, &sprite.bounds);
                let redrawn = source
                    .and_then(|source| self.raster_emoji(source, sprite, space, scale_factor));
                sub.insert_primitive(redrawn.unwrap_or_else(|| space.polychrome(sprite)));
            }
            // Glass under glass is never drawn again (`primitives_under_ranked`
            // refuses it).
            UnderItem::Lens(_) => {}
        }
    }

    /// A glyph or an icon rasterized again at the slot's scale.
    fn raster_mono(
        &self,
        source: &GlyphSource,
        sprite: &MonochromeSprite,
        space: &SlotSpace,
        scale_factor: f32,
        cx: &App,
    ) -> Option<MonochromeSprite> {
        match source {
            GlyphSource::Glyph {
                origin,
                font_id,
                glyph_id,
                font_size,
                dilation,
            } => {
                let at = space.point(point(
                    ScaledPixels(origin.x.0 * scale_factor),
                    ScaledPixels(origin.y.0 * scale_factor),
                ));
                let quantized = point(
                    round_half_toward_zero(at.x.0 * SUBPIXEL_VARIANTS_X as f32)
                        / SUBPIXEL_VARIANTS_X as f32,
                    round_half_toward_zero(at.y.0 * SUBPIXEL_VARIANTS_Y as f32)
                        / SUBPIXEL_VARIANTS_Y as f32,
                );
                let params = RenderGlyphParams {
                    font_id: *font_id,
                    glyph_id: *glyph_id,
                    font_size: *font_size,
                    subpixel_variant: point(
                        (quantized.x.fract() * SUBPIXEL_VARIANTS_X as f32) as u8,
                        (quantized.y.fract() * SUBPIXEL_VARIANTS_Y as f32) as u8,
                    ),
                    scale_factor: scale_factor * space.density,
                    is_emoji: false,
                    subpixel_rendering: false,
                    dilation: *dilation,
                };
                let raster = self.text_system().raster_bounds(&params).ok()?;
                if raster.is_zero() {
                    return None;
                }
                let tile = self
                    .sprite_atlas
                    .get_or_insert_with(params.clone().into(), &mut || {
                        let (size, bytes) = self.text_system().rasterize_glyph(&params)?;
                        Ok(Some((size, Cow::Owned(bytes))))
                    })
                    .ok()??;
                let integer = quantized.map(|c| ScaledPixels(c.trunc()));
                Some(MonochromeSprite {
                    order: 0,
                    animation: crate::SpriteAnimationId::NONE,
                    bounds: Bounds {
                        origin: integer + raster.origin.map(Into::into),
                        size: tile.bounds.size.map(Into::into),
                    },
                    content_mask: space.mask(sprite.content_mask),
                    color: sprite.color,
                    tile,
                    transformation: TransformationMatrix::unit(),
                })
            }
            GlyphSource::Svg { bounds, path } => {
                let bounds = space.bounds(*bounds);
                let params = RenderSvgParams {
                    path: path.clone(),
                    size:
                        bounds.size.map(|pixels| {
                            crate::DevicePixels::from(
                                (pixels.0 * SMOOTH_SVG_SCALE_FACTOR).ceil() as i32
                            )
                        }),
                };
                let tile = self
                    .sprite_atlas
                    .get_or_insert_with(params.clone().into(), &mut || {
                        let Some((size, bytes)) =
                            cx.svg_renderer.render_alpha_mask(&params, None)?
                        else {
                            return Ok(None);
                        };
                        Ok(Some((size, Cow::Owned(bytes))))
                    })
                    .ok()??;
                let half = |v: i32| ScaledPixels(v as f32 / SMOOTH_SVG_SCALE_FACTOR / 2.0);
                let size = tile
                    .bounds
                    .size
                    .map(|v| ScaledPixels(v.0 as f32 / SMOOTH_SVG_SCALE_FACTOR));
                let origin = bounds.center()
                    - point(
                        half(tile.bounds.size.width.0),
                        half(tile.bounds.size.height.0),
                    );
                Some(MonochromeSprite {
                    order: 0,
                    animation: crate::SpriteAnimationId::NONE,
                    bounds: Bounds {
                        origin: origin.map(|v| ScaledPixels(round_half_toward_zero(v.0))),
                        size: size.map(|v| v.ceil()),
                    },
                    content_mask: space.mask(sprite.content_mask),
                    color: sprite.color,
                    tile,
                    transformation: TransformationMatrix {
                        rotation_scale: sprite.transformation.rotation_scale,
                        translation: [
                            sprite.transformation.translation[0] * space.density,
                            sprite.transformation.translation[1] * space.density,
                        ],
                    },
                })
            }
            GlyphSource::Emoji { .. } => None,
        }
    }

    /// An emoji rasterized again at the slot's scale.
    fn raster_emoji(
        &self,
        source: &GlyphSource,
        sprite: &PolychromeSprite,
        space: &SlotSpace,
        scale_factor: f32,
    ) -> Option<PolychromeSprite> {
        let GlyphSource::Emoji {
            origin,
            font_id,
            glyph_id,
            font_size,
        } = source
        else {
            return None;
        };
        let at = space.point(point(
            ScaledPixels(origin.x.0 * scale_factor),
            ScaledPixels(origin.y.0 * scale_factor),
        ));
        let params = RenderGlyphParams {
            font_id: *font_id,
            glyph_id: *glyph_id,
            font_size: *font_size,
            subpixel_variant: Default::default(),
            scale_factor: scale_factor * space.density,
            is_emoji: true,
            subpixel_rendering: false,
            dilation: 0,
        };
        let raster = self.text_system().raster_bounds(&params).ok()?;
        if raster.is_zero() {
            return None;
        }
        let tile = self
            .sprite_atlas
            .get_or_insert_with(params.clone().into(), &mut || {
                let (size, bytes) = self.text_system().rasterize_glyph(&params)?;
                Ok(Some((size, Cow::Owned(bytes))))
            })
            .ok()??;
        let integer = at.map(|c| ScaledPixels(round_half_toward_zero(c.0)));
        Some(PolychromeSprite {
            order: 0,
            bounds: Bounds {
                origin: integer + raster.origin.map(Into::into),
                size: tile.bounds.size.map(Into::into),
            },
            content_mask: space.mask(sprite.content_mask),
            tile,
            ..*sprite
        })
    }
}
