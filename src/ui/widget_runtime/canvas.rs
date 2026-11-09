//! `<canvas>` bitmaps: the renderer resolves the 2D context's calls into
//! batches of operations in canvas px (`twlite::to_json(vector<CanvasOp>)`),
//! run here on one tiny-skia pixmap per canvas.
use super::effects;
use super::fonts::Fonts;
use super::geometry::{self as g, at};
use super::images::Images;
use serde_json::Value;
use std::collections::HashMap;
use tiny_skia::{
    BlendMode, FillRule, FilterQuality, LinearGradient, Mask, Paint, Path, PathBuilder, Pattern, Pixmap,
    PixmapPaint, Point, RadialGradient, Rect, Shader, SpreadMode, SweepGradient, Transform,
};

/// The most pixels one canvas holds (4096 × 4096).
const MAX_PIXELS: u64 = 4096 * 4096;

#[derive(Default)]
pub struct Canvases {
    canvases: HashMap<i32, Pixmap>,
}

/// `globalCompositeOperation` in the spec's order.
fn blend(code: i64) -> BlendMode {
    use BlendMode::*;
    const MODES: [BlendMode; 26] = [
        SourceOver, SourceIn, SourceOut, SourceAtop, DestinationOver, DestinationIn, DestinationOut, DestinationAtop,
        Plus, Source, Xor, Multiply, Screen, Overlay, Darken, Lighten, ColorDodge, ColorBurn, HardLight, SoftLight,
        Difference, Exclusion, Hue, Saturation, Color, Luminosity,
    ];
    usize::try_from(code).ok().and_then(|i| MODES.get(i).copied()).unwrap_or(SourceOver)
}

/// The paint of an op: "pk" 0 solid "c", 1 linear, 2 radial, 3 conic ("pa", "s"), in user space.
fn shader(op: &Value) -> Option<Shader<'static>> {
    let a = g::floats(op, "pa");
    let stops = g::stops(op, "s");
    let identity = Transform::identity();
    match g::int(op, "pk") {
        1 => LinearGradient::new(
            Point::from_xy(at(&a, 0), at(&a, 1)),
            Point::from_xy(at(&a, 2), at(&a, 3)),
            stops,
            SpreadMode::Pad,
            identity,
        ),
        2 => RadialGradient::new(
            Point::from_xy(at(&a, 0), at(&a, 1)),
            at(&a, 2).max(0.0),
            Point::from_xy(at(&a, 3), at(&a, 4)),
            at(&a, 5).max(0.0),
            stops,
            SpreadMode::Pad,
            identity,
        ),
        3 => SweepGradient::new(
            Point::from_xy(at(&a, 1), at(&a, 2)),
            0.0,
            360.0,
            stops,
            SpreadMode::Pad,
            Transform::from_rotate_at(at(&a, 0).to_degrees(), at(&a, 1), at(&a, 2)),
        ),
        _ => Some(Shader::SolidColor(g::color(op, "c"))),
    }
}

/// The op's clips ("cl", canvas px) as one mask; `None` without clips.
fn clips(op: &Value, width: u32, height: u32) -> Option<Option<Mask>> {
    let Some(list) = op.get("cl").and_then(Value::as_array) else {
        return Some(None);
    };
    let mut mask: Option<Mask> = None;
    for clip in list {
        let Some(path) = g::path(clip) else {
            // A clip to nothing: draws nothing.
            return None;
        };
        match &mut mask {
            Some(mask) => mask.intersect_path(&path, g::fill_rule(clip), true, Transform::identity()),
            None => {
                let mut fresh = Mask::new(width, height)?;
                fresh.fill_path(&path, g::fill_rule(clip), true, Transform::identity());
                mask = Some(fresh);
            }
        }
    }
    Some(mask)
}

impl Canvases {
    pub fn get(&self, id: i32) -> Option<&Pixmap> {
        self.canvases.get(&id)
    }

    pub fn release(&mut self, id: i32) {
        self.canvases.remove(&id);
    }

    /// Runs one batch on canvas `id` (`width` × `height` px).
    pub fn run(&mut self, id: i32, width: i32, height: i32, ops: &str, fonts: &Fonts, images: &Images) {
        let (Ok(width), Ok(height)) = (u32::try_from(width), u32::try_from(height)) else {
            return;
        };
        if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_PIXELS {
            self.canvases.remove(&id);
            return;
        }
        let Ok(Value::Array(ops)) = serde_json::from_str::<Value>(ops) else {
            return;
        };
        // Another canvas drawn into this one is read from a copy.
        let sources: HashMap<i32, Pixmap> = ops
            .iter()
            .filter_map(|op| op.get("cv").and_then(Value::as_i64))
            .filter_map(|cv| i32::try_from(cv).ok())
            .filter_map(|cv| self.canvases.get(&cv).map(|pixmap| (cv, pixmap.clone())))
            .collect();
        let fits = self
            .canvases
            .get(&id)
            .is_some_and(|canvas| canvas.width() == width && canvas.height() == height);
        if !fits {
            let Some(fresh) = Pixmap::new(width, height) else { return };
            self.canvases.insert(id, fresh);
        }
        let Some(canvas) = self.canvases.get_mut(&id) else { return };
        for op in &ops {
            run_op(canvas, op, fonts, images, &sources);
        }
    }

    /// Straight RGBA of a region into `out`; outside the bitmap transparent.
    pub fn read(&self, id: i32, x: i32, y: i32, w: i32, h: i32, out: &mut [u8]) -> bool {
        let (Ok(w), Ok(h)) = (usize::try_from(w), usize::try_from(h)) else {
            return false;
        };
        if out.len() < w * h * 4 {
            return false;
        }
        out.fill(0);
        let Some(canvas) = self.canvases.get(&id) else {
            return true;
        };
        for row in 0..h {
            for col in 0..w {
                let (cx, cy) = (x + col as i32, y + row as i32);
                let (Ok(cx), Ok(cy)) = (u32::try_from(cx), u32::try_from(cy)) else { continue };
                let Some(px) = canvas.pixel(cx, cy) else { continue };
                let c = px.demultiply();
                let i = (row * w + col) * 4;
                if let Some(slot) = out.get_mut(i..i + 4) {
                    slot.copy_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
                }
            }
        }
        true
    }
}

fn run_op(canvas: &mut Pixmap, op: &Value, fonts: &Fonts, images: &Images, sources: &HashMap<i32, Pixmap>) {
    let (width, height) = (canvas.width(), canvas.height());
    match g::int(op, "k") {
        0 => canvas.fill(tiny_skia::Color::TRANSPARENT),
        6 => put_image(canvas, op),
        kind => {
            let Some(clip) = clips(op, width, height) else { return };
            let mode = if kind == 3 { BlendMode::Clear } else { blend(g::int(op, "cm")) };
            match clip {
                // tiny-skia changes pixels outside a mask for blends other
                // than over: draw unclipped on a copy, then take the copy's
                // pixels inside the clip.
                Some(mask) if mode != BlendMode::SourceOver => {
                    let mut drawn = canvas.clone();
                    draw_op(&mut drawn, op, kind, mode, fonts, images, sources, None);
                    effects::take_masked(canvas, &drawn, &mask);
                }
                clip => draw_op(canvas, op, kind, mode, fonts, images, sources, clip.as_ref()),
            }
        }
    }
}

/// One drawing operation of `kind` with `mode`, inside `clip`.
#[allow(clippy::too_many_arguments)]
fn draw_op(
    canvas: &mut Pixmap,
    op: &Value,
    kind: i64,
    mode: BlendMode,
    fonts: &Fonts,
    images: &Images,
    sources: &HashMap<i32, Pixmap>,
    clip: Option<&Mask>,
) {
    let matrix = g::matrix(op, "m");
    let mut paint = Paint {
        anti_alias: true,
        blend_mode: mode,
        ..Paint::default()
    };
    let alpha = op.get("a").and_then(Value::as_f64).map_or(1.0, |a| a as f32).clamp(0.0, 1.0);
    match kind {
        1 | 2 => {
            let Some(path) = g::path(op) else { return };
            let Some(mut shader) = shader(op) else { return };
            shader.apply_opacity(alpha);
            shader.transform(matrix);
            paint.shader = shader;
            if kind == 1 {
                shadow(canvas, op, &path, Transform::identity(), None, clip);
                canvas.fill_path(&path, &paint, g::fill_rule(op), Transform::identity(), clip);
            } else if let Some(user) = matrix.invert().and_then(|inverse| path.clone().transform(inverse)) {
                let stroke = g::stroke(op, g::num(op, "lw"), "lc", "lj", "ml", "d", "do");
                shadow(canvas, op, &user, matrix, Some(&stroke), clip);
                canvas.stroke_path(&user, &paint, &stroke, matrix, clip);
            }
        }
        3 => {
            let Some(path) = g::path(op) else { return };
            canvas.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), clip);
        }
        4 => text(canvas, op, fonts, matrix, alpha, paint, clip),
        5 => image(canvas, op, images, sources, matrix, alpha, paint.blend_mode, clip),
        _ => {}
    }
}

/// The op's shadow ("shc" color, "sh" x, y, blur) of `path`, under it.
fn shadow(canvas: &mut Pixmap, op: &Value, path: &Path, transform: Transform, stroke: Option<&tiny_skia::Stroke>, clip: Option<&Mask>) {
    let color = g::color(op, "shc");
    if color.alpha() <= 0.0 {
        return;
    }
    let s = g::floats(op, "sh");
    let Some(mut layer) = Pixmap::new(canvas.width(), canvas.height()) else { return };
    let mut paint = Paint::default();
    paint.set_color(color);
    paint.anti_alias = true;
    let moved = transform.post_translate(at(&s, 0), at(&s, 1));
    match stroke {
        Some(stroke) => layer.stroke_path(path, &paint, stroke, moved, None),
        None => layer.fill_path(path, &paint, g::fill_rule(op), moved, None),
    }
    effects::blur(&mut layer, at(&s, 2) / 2.0);
    canvas.draw_pixmap(0, 0, layer.as_ref(), &PixmapPaint::default(), Transform::identity(), clip);
}

#[allow(clippy::too_many_arguments)]
fn text(canvas: &mut Pixmap, op: &Value, fonts: &Fonts, matrix: Transform, alpha: f32, mut paint: Paint<'static>, clip: Option<&Mask>) {
    let text = op.get("t").and_then(Value::as_str).unwrap_or("");
    let tp = g::floats(op, "tp");
    let place = matrix
        .pre_translate(at(&tp, 0), at(&tp, 1))
        .pre_scale(if at(&tp, 2) > 0.0 { at(&tp, 2) } else { 1.0 }, 1.0)
        .pre_translate(at(&tp, 3), 0.0);
    let device = (place.sx * place.sx + place.ky * place.ky).sqrt().max(0.01);
    let Some(shaped) = fonts.shape(g::int(op, "f") as i32, text, device) else { return };
    let Some(mut shader) = shader(op) else { return };
    shader.apply_opacity(alpha);
    // The paint is in user space; the glyphs are placed after it.
    if let Some(inverse) = place.invert() {
        shader.transform(inverse.pre_concat(matrix));
    }
    paint.shader = shader;
    if let Some(outline) = &shaped.outline {
        if g::flag(op, "st") {
            let stroke = g::stroke(op, g::num(op, "lw"), "lc", "lj", "ml", "d", "do");
            canvas.stroke_path(outline, &paint, &stroke, place, clip);
        } else {
            canvas.fill_path(outline, &paint, FillRule::Winding, place, clip);
        }
    }
    let glyphs = place.pre_scale(1.0 / device, 1.0 / device);
    for glyph in &shaped.color {
        super::paint::draw_rgba(Some(canvas), &glyph.rgba, glyph.width, glyph.height, glyphs.pre_translate(glyph.left, glyph.top), clip);
    }
}

#[allow(clippy::too_many_arguments)]
fn image(
    canvas: &mut Pixmap,
    op: &Value,
    images: &Images,
    sources: &HashMap<i32, Pixmap>,
    matrix: Transform,
    alpha: f32,
    blend_mode: BlendMode,
    clip: Option<&Mask>,
) {
    let source = match op.get("cv").and_then(Value::as_i64) {
        Some(cv) => i32::try_from(cv).ok().and_then(|cv| sources.get(&cv)),
        None => images.get(g::int(op, "i") as i32),
    };
    let Some(source) = source else { return };
    let r = g::floats(op, "ir");
    let (mut sx, mut sy, mut sw, mut sh) = (at(&r, 0), at(&r, 1), at(&r, 2), at(&r, 3));
    if sw < 0.0 || sh < 0.0 {
        (sx, sy, sw, sh) = (0.0, 0.0, source.width() as f32, source.height() as f32);
    }
    let (dx, dy, dw, dh) = (at(&r, 4), at(&r, 5), at(&r, 6), at(&r, 7));
    if sw <= 0.0 || sh <= 0.0 {
        return;
    }
    let Some(dst) = Rect::from_xywh(dx, dy, dw, dh) else { return };
    let place = Transform::from_translate(dx, dy)
        .pre_scale(dw / sw, dh / sh)
        .pre_translate(-sx, -sy);
    let quality = if g::flag(op, "ns") { FilterQuality::Nearest } else { FilterQuality::Bilinear };
    let paint = Paint {
        shader: Pattern::new(source.as_ref(), SpreadMode::Pad, quality, alpha, place),
        blend_mode,
        anti_alias: true,
        ..Paint::default()
    };
    canvas.fill_path(&PathBuilder::from_rect(dst), &paint, FillRule::Winding, matrix, clip);
}

/// putImageData: straight RGBA ("px", base64) at ("x", "y"), replacing.
fn put_image(canvas: &mut Pixmap, op: &Value) {
    use base64::Engine;
    let Some(px) = op.get("px").and_then(Value::as_str) else { return };
    let Ok(rgba) = base64::engine::general_purpose::STANDARD.decode(px) else { return };
    let (Ok(w), Ok(h)) = (u32::try_from(g::int(op, "w")), u32::try_from(g::int(op, "h"))) else {
        return;
    };
    let (x, y) = (g::num(op, "x") as i64, g::num(op, "y") as i64);
    let width = i64::from(canvas.width());
    let height = i64::from(canvas.height());
    let data = canvas.data_mut();
    for row in 0..i64::from(h) {
        for col in 0..i64::from(w) {
            let (cx, cy) = (x + col, y + row);
            if cx < 0 || cy < 0 || cx >= width || cy >= height {
                continue;
            }
            let from = ((row * i64::from(w) + col) * 4) as usize;
            let to = ((cy * width + cx) * 4) as usize;
            let (Some(src), Some(dst)) = (rgba.get(from..from + 4), data.get_mut(to..to + 4)) else { continue };
            let mut px = [0u8; 4];
            px.copy_from_slice(src);
            super::images::premultiply(&mut px);
            dst.copy_from_slice(&px);
        }
    }
}
