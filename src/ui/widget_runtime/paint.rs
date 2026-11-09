//! Paints a TwidgetCore display list (`twlite::to_json(DisplayList)`, CSS
//! px) into a tiny-skia pixmap at the widget's device scale: boxes, text,
//! images and canvases, gradients, clips, SVG paths, groups with opacity
//! and transforms, box shadows, filters and backdrop filters.
use super::backend::Resources;
use super::effects::{self, ColorFilter};
use super::geometry::{self as g, at};
use serde_json::Value;
use tiny_skia::{
    BlendMode, Color, FillRule, FilterQuality, IntRect, Mask, Paint, Path, PathBuilder, Pattern,
    Pixmap, PixmapPaint, Rect, Shader, SpreadMode, Transform,
};

/// One group or filter that draws into a layer of its own until its pop.
enum Layer {
    Group { opacity: f32 },
    Filter { filters: Vec<Value> },
}

struct Painter<'a> {
    resources: &'a Resources,
    scale: f32,
    width: u32,
    height: u32,
    /// The pixmaps drawn into: the frame, then one per open layer.
    targets: Vec<Pixmap>,
    layers: Vec<Layer>,
    transform: Transform,
    /// GroupPush: the transform before it, whether it opened a layer, whether it hides.
    groups: Vec<(Transform, bool, bool)>,
    hidden: usize,
    /// FilterPush: whether it opened a layer.
    filters: Vec<bool>,
    clip: Option<Mask>,
    clips: Vec<Option<Mask>>,
}

/// Paints `list` into `frame` (`scale` device px per CSS px) inside `damage`
/// (CSS px; `None` paints everything), over `background`.
pub fn paint(
    resources: &Resources,
    list: &[Value],
    scale: f32,
    damage: Option<&[Rect]>,
    background: Color,
    frame: &mut Pixmap,
) {
    let base = Transform::from_scale(scale, scale);
    let clip = damage.and_then(|rects| {
        let mut mask = Mask::new(frame.width(), frame.height())?;
        let mut path = PathBuilder::new();
        for rect in rects {
            path.push_rect(*rect);
        }
        let path = path.finish()?;
        mask.fill_path(&path, FillRule::Winding, false, base);
        Some(mask)
    });
    if damage.is_some() && clip.is_none() {
        // Nothing moved.
        return;
    }
    let Some(placeholder) = Pixmap::new(1, 1) else {
        return;
    };
    let target = std::mem::replace(frame, placeholder);
    let mut painter = Painter {
        resources,
        scale,
        width: target.width(),
        height: target.height(),
        targets: vec![target],
        layers: Vec::new(),
        transform: base,
        groups: Vec::new(),
        hidden: 0,
        filters: Vec::new(),
        clip,
        clips: Vec::new(),
    };
    painter.clear(background);
    for command in list {
        painter.command(command);
    }
    painter.close_layers();
    if let Some(done) = painter.targets.pop() {
        *frame = done;
    }
}

impl Painter<'_> {
    fn target(&mut self) -> Option<&mut Pixmap> {
        self.targets.last_mut()
    }

    fn clear(&mut self, background: Color) {
        let full = Rect::from_xywh(0.0, 0.0, self.width as f32, self.height as f32);
        let clip = self.clip.clone();
        if let (Some(target), Some(full)) = (self.targets.last_mut(), full) {
            let mut paint = Paint::default();
            // Opaque, so drawing over is replacing (tiny-skia's Source
            // blend under a mask also clears whole rows outside it).
            paint.set_color(background);
            target.fill_rect(full, &paint, Transform::identity(), clip.as_ref());
        }
    }

    /// The clip with the command's own clip rect ("l") on it.
    fn clip_for(&self, command: &Value) -> Option<Mask> {
        let Some(rect) = g::rect(command, "l") else {
            return self.clip.clone();
        };
        self.with_clip(&PathBuilder::from_rect(rect), FillRule::Winding)
    }

    /// The current clip intersected with `path` (CSS px, under the transform).
    fn with_clip(&self, path: &Path, rule: FillRule) -> Option<Mask> {
        match &self.clip {
            Some(clip) => {
                let mut mask = clip.clone();
                mask.intersect_path(path, rule, true, self.transform);
                Some(mask)
            }
            None => {
                let mut mask = Mask::new(self.width, self.height)?;
                mask.fill_path(path, rule, true, self.transform);
                Some(mask)
            }
        }
    }

    fn fill(&mut self, path: &Path, shader: Shader<'_>, rule: FillRule, clip: Option<&Mask>) {
        let transform = self.transform;
        if let Some(target) = self.target() {
            let paint = Paint {
                shader,
                anti_alias: true,
                ..Paint::default()
            };
            target.fill_path(path, &paint, rule, transform, clip);
        }
    }

    fn stroke(
        &mut self,
        path: &Path,
        shader: Shader<'_>,
        stroke: &tiny_skia::Stroke,
        clip: Option<&Mask>,
    ) {
        let transform = self.transform;
        if let Some(target) = self.target() {
            let paint = Paint {
                shader,
                anti_alias: true,
                ..Paint::default()
            };
            target.stroke_path(path, &paint, stroke, transform, clip);
        }
    }

    fn command(&mut self, c: &Value) {
        let kind = g::int(c, "k");
        // Pushes and pops keep their pairs even while hidden.
        match kind {
            9 => return self.push_clip(c),
            10 => return self.pop_clip(),
            12 => return self.push_group(c),
            13 => return self.pop_group(),
            15 => return self.push_filter(c),
            16 => return self.pop_filter(),
            _ => {}
        }
        if self.hidden > 0 {
            return;
        }
        let radii = g::floats(c, "q");
        let clip = self.clip_for(c);
        let clip = clip.as_ref();
        match kind {
            0 | 1 => {
                let Some(path) = g::rect(c, "r").and_then(|r| g::rounded(r, &radii)) else {
                    return;
                };
                let shader = Shader::SolidColor(g::color(c, "c"));
                if kind == 0 {
                    self.fill(&path, shader, FillRule::Winding, clip);
                } else {
                    let stroke = tiny_skia::Stroke {
                        width: g::num(c, "w"),
                        ..Default::default()
                    };
                    self.stroke(&path, shader, &stroke, clip);
                }
            }
            2 | 3 => {
                let Some(path) = g::rect(c, "r").and_then(PathBuilder::from_oval) else {
                    return;
                };
                let shader = Shader::SolidColor(g::color(c, "c"));
                if kind == 2 {
                    self.fill(&path, shader, FillRule::Winding, clip);
                } else {
                    let stroke = tiny_skia::Stroke {
                        width: g::num(c, "w"),
                        ..Default::default()
                    };
                    self.stroke(&path, shader, &stroke, clip);
                }
            }
            4 => self.text(c, clip),
            5 => self.image(c, &radii, clip),
            6..=8 => {
                let Some(path) = g::rect(c, "r").and_then(|r| g::rounded(r, &radii)) else {
                    return;
                };
                if let Some(shader) = g::box_gradient(kind, &g::floats(c, "a"), g::stops(c, "s")) {
                    self.fill(&path, shader, FillRule::Winding, clip);
                }
            }
            11 => self.svg_path(c, clip),
            14 => self.shadow(c, &radii, clip),
            17 => self.backdrop(c, &radii, clip),
            _ => {}
        }
    }

    fn push_clip(&mut self, c: &Value) {
        self.clips.push(self.clip.clone());
        let shape = match c.get("p") {
            Some(p) => g::path(p).map(|path| (path, g::fill_rule(p))),
            None => g::rect(c, "r")
                .and_then(|r| g::rounded(r, &g::floats(c, "q")))
                .map(|path| (path, FillRule::Winding)),
        };
        self.clip = match shape {
            Some((path, rule)) => self.with_clip(&path, rule),
            // A clip of nothing: an empty mask.
            None => Mask::new(self.width, self.height),
        };
    }

    fn pop_clip(&mut self) {
        if let Some(clip) = self.clips.pop() {
            self.clip = clip;
        }
    }

    fn push_layer(&mut self, layer: Layer) -> bool {
        let Some(pixmap) = Pixmap::new(self.width, self.height) else {
            return false;
        };
        self.targets.push(pixmap);
        self.layers.push(layer);
        true
    }

    fn pop_layer(&mut self) {
        let (Some(layer), Some(mut pixmap)) = (self.layers.pop(), self.targets.pop()) else {
            return;
        };
        let opacity = match layer {
            Layer::Group { opacity } => opacity,
            Layer::Filter { filters } => {
                self.run_filters(&mut pixmap, &filters);
                1.0
            }
        };
        if let Some(target) = self.target() {
            let paint = PixmapPaint {
                opacity,
                ..PixmapPaint::default()
            };
            target.draw_pixmap(0, 0, pixmap.as_ref(), &paint, Transform::identity(), None);
        }
    }

    fn push_group(&mut self, c: &Value) {
        let opacity = c
            .get("o")
            .and_then(Value::as_f64)
            .map_or(1.0, |o| o as f32)
            .clamp(0.0, 1.0);
        let hides = g::flag(c, "h");
        let layered = self.hidden == 0
            && !hides
            && opacity < 1.0
            && self.push_layer(Layer::Group { opacity });
        self.groups.push((self.transform, layered, hides));
        self.transform = self.transform.pre_concat(g::matrix(c, "m"));
        if hides {
            self.hidden += 1;
        }
    }

    fn pop_group(&mut self) {
        let Some((transform, layered, hides)) = self.groups.pop() else {
            return;
        };
        self.transform = transform;
        if hides {
            self.hidden = self.hidden.saturating_sub(1);
        }
        if layered {
            self.pop_layer();
        }
    }

    fn push_filter(&mut self, c: &Value) {
        let filters = c
            .get("fl")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let layered =
            self.hidden == 0 && !filters.is_empty() && self.push_layer(Layer::Filter { filters });
        self.filters.push(layered);
    }

    fn pop_filter(&mut self) {
        if self.filters.pop() == Some(true) {
            self.pop_layer();
        }
    }

    fn close_layers(&mut self) {
        while !self.layers.is_empty() {
            self.pop_layer();
        }
    }

    /// The CSS filter functions ([kind, amount, x, y, ARGB]) over `pixmap`, in order.
    fn run_filters(&self, pixmap: &mut Pixmap, filters: &[Value]) {
        let device = {
            let (sx, sy) = self.transform.get_scale();
            sx.max(sy)
        }
        .max(self.scale.abs());
        for filter in filters {
            let f: Vec<f64> = filter
                .as_array()
                .map(|items| items.iter().map(|v| v.as_f64().unwrap_or(0.0)).collect())
                .unwrap_or_default();
            let item = |n: usize| f.get(n).copied().unwrap_or(0.0) as f32;
            let amount = item(1);
            let color = match ColorFilterKind::from(item(0) as i64) {
                ColorFilterKind::Blur => {
                    effects::blur(pixmap, amount * device);
                    continue;
                }
                ColorFilterKind::DropShadow => {
                    let color = g::argb(f.get(4).copied().unwrap_or(0.0) as u64);
                    drop_shadow(
                        pixmap,
                        item(2) * device,
                        item(3) * device,
                        amount * device,
                        color,
                    );
                    continue;
                }
                ColorFilterKind::Color(filter) => filter(amount),
                ColorFilterKind::Unknown => continue,
            };
            effects::apply(pixmap, color);
        }
    }

    fn text(&mut self, c: &Value, clip: Option<&Mask>) {
        let text = c.get("t").and_then(Value::as_str).unwrap_or("");
        if text.is_empty() {
            return;
        }
        let t = self.transform;
        let device = (t.sx * t.sx + t.ky * t.ky).sqrt().max(0.01);
        let Some(shaped) = self
            .resources
            .fonts
            .shape(g::int(c, "f") as i32, text, device)
        else {
            return;
        };
        let origin = self.transform.pre_translate(g::num(c, "x"), g::num(c, "y"));
        let saved = std::mem::replace(&mut self.transform, origin);
        if let Some(outline) = &shaped.outline {
            self.fill(
                outline,
                Shader::SolidColor(g::color(c, "c")),
                FillRule::Winding,
                clip,
            );
        }
        let glyphs = origin.pre_scale(1.0 / device, 1.0 / device);
        for glyph in &shaped.color {
            draw_rgba(
                self.target(),
                &glyph.rgba,
                glyph.width,
                glyph.height,
                glyphs.pre_translate(glyph.left, glyph.top),
                clip,
            );
        }
        self.transform = saved;
    }

    fn image(&mut self, c: &Value, radii: &[f32], clip: Option<&Mask>) {
        let Some(rect) = g::rect(c, "r") else { return };
        let resources = self.resources;
        let pixmap = match c.get("cv").and_then(Value::as_i64) {
            Some(canvas) => resources.canvases.get(canvas as i32),
            None => resources.images.get(g::int(c, "i") as i32),
        };
        let Some(pixmap) = pixmap else { return };
        let Some(path) = g::rounded(rect, radii) else {
            return;
        };
        let (iw, ih) = (pixmap.width() as f32, pixmap.height() as f32);
        let place = Transform::from_row(
            rect.width() / iw,
            0.0,
            0.0,
            rect.height() / ih,
            rect.x(),
            rect.y(),
        );
        let shader = Pattern::new(
            pixmap.as_ref(),
            SpreadMode::Pad,
            FilterQuality::Bilinear,
            1.0,
            place,
        );
        self.fill(&path, shader, FillRule::Winding, clip);
    }

    fn svg_path(&mut self, c: &Value, clip: Option<&Mask>) {
        let Some(p) = c.get("p") else { return };
        let Some(path) = g::path(p) else { return };
        let gradient = g::int(p, "g");
        let fill = if gradient != 0 {
            g::path_gradient(gradient, &g::floats(p, "ga"), g::stops(p, "s"))
        } else if p.get("f").is_some() {
            Some(Shader::SolidColor(g::color(p, "f")))
        } else {
            None
        };
        if let Some(shader) = fill {
            self.fill(&path, shader, g::fill_rule(p), clip);
        }
        if p.get("sc").is_some() || p.get("sg").is_some() {
            let stroke = g::stroke(p, g::num(p, "sw"), "lc", "lj", "ml", "d", "do");
            let shader = match g::int(p, "sg") {
                0 => Some(Shader::SolidColor(g::color(p, "sc"))),
                kind => g::path_gradient(kind, &g::floats(p, "sga"), g::stops(p, "ss")),
            };
            if let Some(shader) = shader {
                self.stroke(&path, shader, &stroke, clip);
            }
        }
    }

    /// A box shadow: "a" offset x, y, blur, spread; "n" inset.
    fn shadow(&mut self, c: &Value, radii: &[f32], clip: Option<&Mask>) {
        let Some(rect) = g::rect(c, "r") else { return };
        let a = g::floats(c, "a");
        let (dx, dy, blur, spread) = (at(&a, 0), at(&a, 1), at(&a, 2), at(&a, 3));
        let inset = g::flag(c, "n");
        let color = g::color(c, "c");
        let Some(box_path) = g::rounded(rect, radii) else {
            return;
        };
        let sigma = blur / 2.0 * {
            let (sx, sy) = self.transform.get_scale();
            sx.max(sy)
        }
        .max(1e-3);
        let Some(mut layer) = Pixmap::new(self.width, self.height) else {
            return;
        };
        let mut paint = Paint::default();
        paint.set_color(color);
        paint.anti_alias = true;
        if inset {
            // Everywhere inside the box but the shrunk, moved hole.
            layer.fill(color);
            if let Some((hole, hole_radii)) = g::inflate(rect, radii, -spread)
                && let Some(hole) = g::rounded(hole, &hole_radii)
            {
                paint.blend_mode = BlendMode::Clear;
                layer.fill_path(
                    &hole,
                    &paint,
                    FillRule::Winding,
                    self.transform.pre_translate(dx, dy),
                    None,
                );
            }
        } else if let Some((grown, grown_radii)) = g::inflate(rect, radii, spread)
            && let Some(shape) = g::rounded(grown, &grown_radii)
        {
            layer.fill_path(
                &shape,
                &paint,
                FillRule::Winding,
                self.transform.pre_translate(dx, dy),
                None,
            );
        }
        effects::blur(&mut layer, sigma);
        // An outer shadow never shows under its box; an inset one only does.
        let mut mask = match clip {
            Some(clip) => clip.clone(),
            None => {
                let Some(mut full) = Mask::new(self.width, self.height) else {
                    return;
                };
                full.invert();
                full
            }
        };
        let Some(mut shape) = Mask::new(self.width, self.height) else {
            return;
        };
        shape.fill_path(&box_path, FillRule::Winding, true, self.transform);
        if !inset {
            shape.invert();
        }
        multiply_masks(&mut mask, &shape);
        if let Some(target) = self.target() {
            target.draw_pixmap(
                0,
                0,
                layer.as_ref(),
                &PixmapPaint::default(),
                Transform::identity(),
                Some(&mask),
            );
        }
    }

    /// `backdrop-filter`: what is under the box so far, filtered, inside it.
    fn backdrop(&mut self, c: &Value, radii: &[f32], clip: Option<&Mask>) {
        let Some(rect) = g::rect(c, "r") else { return };
        let filters = c
            .get("fl")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let Some(bounds) = g::device_bounds(rect, self.transform) else {
            return;
        };
        let Some(area) =
            bounds.intersect(&IntRect::from_xywh(0, 0, self.width, self.height).unwrap_or(bounds))
        else {
            return;
        };
        let Some(mut under) = self.targets.last().and_then(|t| t.clone_rect(area)) else {
            return;
        };
        self.run_filters(&mut under, &filters);
        let Some(path) = g::rounded(rect, radii) else {
            return;
        };
        let mut mask = match clip {
            Some(clip) => clip.clone(),
            None => {
                let Some(mut full) = Mask::new(self.width, self.height) else {
                    return;
                };
                full.invert();
                full
            }
        };
        mask.intersect_path(&path, FillRule::Winding, true, self.transform);
        if let Some(target) = self.target() {
            // Over, not Source: see `clear`.
            target.draw_pixmap(
                area.x(),
                area.y(),
                under.as_ref(),
                &PixmapPaint::default(),
                Transform::identity(),
                Some(&mask),
            );
        }
    }
}

enum ColorFilterKind {
    Blur,
    DropShadow,
    Color(fn(f32) -> ColorFilter),
    Unknown,
}

impl From<i64> for ColorFilterKind {
    /// `twlite::Filter::Kind`'s order.
    fn from(kind: i64) -> Self {
        match kind {
            0 => Self::Blur,
            1 => Self::Color(ColorFilter::Brightness),
            2 => Self::Color(ColorFilter::Contrast),
            3 => Self::Color(ColorFilter::Grayscale),
            4 => Self::Color(ColorFilter::HueRotate),
            5 => Self::Color(ColorFilter::Invert),
            6 => Self::Color(ColorFilter::Opacity),
            7 => Self::Color(ColorFilter::Saturate),
            8 => Self::Color(ColorFilter::Sepia),
            9 => Self::DropShadow,
            _ => Self::Unknown,
        }
    }
}

/// `drop-shadow()`: the layer's own shape, tinted, blurred and moved, under it.
fn drop_shadow(pixmap: &mut Pixmap, dx: f32, dy: f32, sigma: f32, color: Color) {
    let Some(mut shadow) = Pixmap::new(pixmap.width(), pixmap.height()) else {
        return;
    };
    let tint = color.premultiply().to_color_u8();
    for ([r, g, b, a], [.., alpha]) in shadow
        .data_mut()
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(pixmap.data().as_chunks::<4>().0)
    {
        let alpha = u16::from(*alpha);
        let scale = |c: u8| ((u16::from(c) * alpha + 127) / 255) as u8;
        (*r, *g, *b, *a) = (
            scale(tint.red()),
            scale(tint.green()),
            scale(tint.blue()),
            scale(tint.alpha()),
        );
    }
    effects::blur(&mut shadow, sigma);
    let Some(mut out) = Pixmap::new(pixmap.width(), pixmap.height()) else {
        return;
    };
    out.draw_pixmap(
        0,
        0,
        shadow.as_ref(),
        &PixmapPaint::default(),
        Transform::from_translate(dx, dy),
        None,
    );
    out.draw_pixmap(
        0,
        0,
        pixmap.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
    *pixmap = out;
}

/// `mask` × `other`, pixel by pixel.
fn multiply_masks(mask: &mut Mask, other: &Mask) {
    for (a, b) in mask.data_mut().iter_mut().zip(other.data()) {
        *a = ((u16::from(*a) * u16::from(*b) + 127) / 255) as u8;
    }
}

/// Straight RGBA drawn under `transform` (device px of the image).
pub fn draw_rgba(
    target: Option<&mut Pixmap>,
    rgba: &[u8],
    width: u32,
    height: u32,
    transform: Transform,
    clip: Option<&Mask>,
) {
    let Some(target) = target else { return };
    let mut data = rgba.to_vec();
    super::images::premultiply(&mut data);
    let Some(size) = tiny_skia::IntSize::from_wh(width, height) else {
        return;
    };
    let Some(image) = Pixmap::from_vec(data, size) else {
        return;
    };
    let paint = PixmapPaint {
        quality: FilterQuality::Bilinear,
        ..PixmapPaint::default()
    };
    target.draw_pixmap(0, 0, image.as_ref(), &paint, transform, clip);
}

#[cfg(test)]
#[path = "paint_tests.rs"]
mod tests;
