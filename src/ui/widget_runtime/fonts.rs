//! Widget fonts on every OS: cosmic-text finds and shapes them (system
//! fonts through fontdb, fallback per script), swash gives the glyph
//! outlines the painter fills. One font database per process, made on the
//! first widget, shared by all.
use super::ffi::FontMetrics;
use cosmic_text::{
    Attrs, Buffer, CacheKey, CacheKeyFlags, Command, Family, FeatureTag, FontFeatures, FontSystem,
    Metrics, Shaping, Style, SwashCache, SwashContent, Weight, Wrap,
};
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use tiny_skia::{Path, PathBuilder};

struct System {
    fonts: FontSystem,
    glyphs: SwashCache,
    /// Named families looked up already: whether the system has them.
    /// Every installed family, lowercased: read once, so a widget naming
    /// new families never grows anything or scans the font list again.
    installed: HashSet<String>,
}

fn first_present(fonts: &FontSystem, candidates: &[&str]) -> Option<String> {
    candidates
        .iter()
        .find(|name| {
            fonts.db().faces().any(|face| {
                face.families
                    .iter()
                    .any(|(family, _)| family.eq_ignore_ascii_case(name))
            })
        })
        .map(|name| (*name).to_owned())
}

fn system() -> &'static Mutex<System> {
    static SYSTEM: OnceLock<Mutex<System>> = OnceLock::new();
    SYSTEM.get_or_init(|| {
        let mut fonts = FontSystem::new();
        // The generic families as each OS's own interface text, as browsers
        // pick them; fontdb's defaults (Arial, Times New Roman) otherwise.
        let sans = first_present(&fonts, quill::message_widget::SANS_FONTS);
        let serif = first_present(&fonts, quill::message_widget::SERIF_FONTS);
        let mono = first_present(&fonts, quill::message_widget::MONO_FONTS);
        let db = fonts.db_mut();
        if let Some(sans) = sans {
            db.set_sans_serif_family(sans);
        }
        if let Some(serif) = serif {
            db.set_serif_family(serif);
        }
        if let Some(mono) = mono {
            db.set_monospace_family(mono);
        }
        let installed = fonts
            .db()
            .faces()
            .flat_map(|face| {
                face.families
                    .iter()
                    .map(|(family, _)| family.to_lowercase())
            })
            .collect();
        Mutex::new(System {
            fonts,
            glyphs: SwashCache::new(),
            installed,
        })
    })
}

/// A CSS family, resolved.
#[derive(Debug, Clone, PartialEq)]
enum FamilyName {
    Named(String),
    Serif,
    SansSerif,
    Monospace,
    Cursive,
    Fantasy,
}

impl FamilyName {
    fn family(&self) -> Family<'_> {
        match self {
            Self::Named(name) => Family::Name(name),
            Self::Serif => Family::Serif,
            Self::SansSerif => Family::SansSerif,
            Self::Monospace => Family::Monospace,
            Self::Cursive => Family::Cursive,
            Self::Fantasy => Family::Fantasy,
        }
    }
}

fn generic(name: &str) -> Option<FamilyName> {
    Some(match name.to_ascii_lowercase().as_str() {
        "sans-serif" | "system-ui" | "-apple-system" | "blinkmacsystemfont" | "ui-sans-serif"
        | "ui-rounded" => FamilyName::SansSerif,
        "serif" | "ui-serif" => FamilyName::Serif,
        "monospace" | "ui-monospace" => FamilyName::Monospace,
        "cursive" => FamilyName::Cursive,
        "fantasy" => FamilyName::Fantasy,
        _ => return None,
    })
}

/// The first family of a CSS `font-family` list the system can draw.
fn resolve(system: &mut System, families: &str) -> FamilyName {
    for name in families.split(',') {
        let name = name.trim().trim_matches(|c| c == '"' || c == '\'').trim();
        if name.is_empty() {
            continue;
        }
        if let Some(generic) = generic(name) {
            return generic;
        }
        if system.installed.contains(&name.to_lowercase()) {
            return FamilyName::Named(name.to_owned());
        }
    }
    FamilyName::SansSerif
}

/// `tag=value` pairs joined by commas, as the renderer passes them.
fn features(spec: &str) -> FontFeatures {
    let mut features = FontFeatures::new();
    for pair in spec.split(',') {
        let Some((tag, value)) = pair.split_once('=') else {
            continue;
        };
        let Ok(tag) = <[u8; 4]>::try_from(tag.trim().as_bytes()) else {
            continue;
        };
        features.set(FeatureTag::new(&tag), value.trim().parse().unwrap_or(1));
    }
    features
}

#[derive(Debug, Clone)]
struct Face {
    family: FamilyName,
    size: f32,
    weight: u16,
    italic: bool,
    features: FontFeatures,
}

impl Face {
    fn attrs(&self) -> Attrs<'_> {
        Attrs::new()
            .family(self.family.family())
            .weight(Weight(self.weight))
            .style(if self.italic {
                Style::Italic
            } else {
                Style::Normal
            })
            .font_features(self.features.clone())
    }
}

/// A glyph that has no outline (a color emoji): its image, drawn as it is.
pub struct ColorGlyph {
    /// Straight RGBA, `width` × `height`.
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// The image's top left from the text origin, device px.
    pub left: f32,
    pub top: f32,
}

/// One line of text shaped for drawing: its outlines from the origin at
/// the baseline, CSS px, and the glyphs drawn as images at `scale`.
pub struct Shaped {
    pub outline: Option<Path>,
    pub color: Vec<ColorGlyph>,
}

/// The fonts one widget made; each handle is its own.
#[derive(Default)]
pub struct Fonts {
    next: i32,
    faces: HashMap<i32, Face>,
    widths: HashMap<(i32, String), f32>,
}

/// Entries the width cache holds before it starts over.
const WIDTH_CACHE: usize = 4096;

fn shape(fonts: &mut FontSystem, face: &Face, text: &str) -> Buffer {
    let size = face.size.max(1.0);
    let mut buffer = Buffer::new(fonts, Metrics::new(size, size * 1.25));
    buffer.set_wrap(Wrap::None);
    buffer.set_size(None, None);
    buffer.set_text(text, &face.attrs(), Shaping::Advanced, None);
    buffer.shape_until_scroll(fonts, false);
    buffer
}

impl Fonts {
    /// A font for the renderer: its handle (> 0) and metrics.
    pub fn create(
        &mut self,
        families: &str,
        size: f32,
        weight: i32,
        italic: bool,
        spec: &str,
    ) -> (i32, FontMetrics) {
        let mut system = quill::locks::lock_or_recover(system());
        let face = Face {
            family: resolve(&mut system, families),
            size: if size.is_finite() {
                size.clamp(1.0, 512.0)
            } else {
                16.0
            },
            weight: u16::try_from(weight.clamp(1, 1000)).unwrap_or(400),
            italic,
            features: features(spec),
        };
        let metrics = metrics(&mut system.fonts, &face);
        self.next = self.next.saturating_add(1);
        self.faces.insert(self.next, face);
        (self.next, metrics)
    }

    pub fn delete(&mut self, font: i32) {
        self.faces.remove(&font);
        self.widths.retain(|(id, _), _| *id != font);
    }

    /// The advance of `text`, CSS px.
    pub fn width(&mut self, font: i32, text: &str) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        if let Some(width) = self.widths.get(&(font, text.to_owned())) {
            return *width;
        }
        let Some(face) = self.faces.get(&font) else {
            return 0.0;
        };
        let mut system = quill::locks::lock_or_recover(system());
        let buffer = shape(&mut system.fonts, face, text);
        let width = buffer
            .layout_runs()
            .map(|run| run.line_w)
            .fold(0.0, f32::max);
        if self.widths.len() >= WIDTH_CACHE {
            self.widths.clear();
        }
        self.widths.insert((font, text.to_owned()), width);
        width
    }

    /// `text` in `font`, ready to fill; `scale` is device px per CSS px
    /// for the glyphs drawn as images.
    pub fn shape(&self, font: i32, text: &str, scale: f32) -> Option<Shaped> {
        let face = self.faces.get(&font)?;
        let mut guard = quill::locks::lock_or_recover(system());
        let system = &mut *guard;
        let buffer = shape(&mut system.fonts, face, text);
        let mut path = PathBuilder::new();
        let mut color = Vec::new();
        for run in buffer.layout_runs() {
            for glyph in run.glyphs {
                let x = glyph.x + glyph.font_size * glyph.x_offset;
                let y = glyph.y - glyph.font_size * glyph.y_offset;
                let flags = glyph.cache_key_flags | CacheKeyFlags::DISABLE_HINTING;
                let (key, _, _) = CacheKey::new(
                    glyph.font_id,
                    glyph.glyph_id,
                    glyph.font_size,
                    (0.0, 0.0),
                    glyph.font_weight,
                    flags,
                );
                let commands = system.glyphs.get_outline_commands(&mut system.fonts, key);
                match commands {
                    Some(commands)
                        if !commands.is_empty()
                            && !is_color(&mut system.fonts, glyph.font_id, glyph.font_weight) =>
                    {
                        append(&mut path, commands, x, y);
                    }
                    _ => {
                        let (key, _, _) = CacheKey::new(
                            glyph.font_id,
                            glyph.glyph_id,
                            glyph.font_size * scale,
                            (0.0, 0.0),
                            glyph.font_weight,
                            flags,
                        );
                        if let Some(image) =
                            system.glyphs.get_image_uncached(&mut system.fonts, key)
                            && let Some(glyph_image) = color_glyph(image, x * scale, y * scale)
                        {
                            color.push(glyph_image);
                        }
                    }
                }
            }
        }
        Some(Shaped {
            outline: path.finish(),
            color,
        })
    }
}

/// A font whose glyphs are pictures (an emoji font): drawn from images so
/// they keep their colors.
fn is_color(fonts: &mut FontSystem, id: cosmic_text::fontdb::ID, weight: Weight) -> bool {
    fonts.get_font(id, weight).is_some_and(|font| {
        let font = font.as_swash();
        font.color_palettes().next().is_some() || font.color_strikes().next().is_some()
    })
}

fn color_glyph(image: cosmic_text::SwashImage, x: f32, y: f32) -> Option<ColorGlyph> {
    let (width, height) = (image.placement.width, image.placement.height);
    if width == 0 || height == 0 {
        return None;
    }
    let rgba = match image.content {
        SwashContent::Color => image.data,
        // An outline drawn as a mask: a glyph of a color font without
        // color; black, as text with no color of its own.
        SwashContent::Mask => image
            .data
            .iter()
            .flat_map(|alpha| [0, 0, 0, *alpha])
            .collect(),
        SwashContent::SubpixelMask => return None,
    };
    Some(ColorGlyph {
        rgba,
        width,
        height,
        left: x + image.placement.left as f32,
        top: y - image.placement.top as f32,
    })
}

/// A glyph's outline (font units scaled, y up) at `(x, y)` in y-down space.
fn append(path: &mut PathBuilder, commands: &[Command], x: f32, y: f32) {
    for command in commands {
        match *command {
            Command::MoveTo(p) => path.move_to(x + p.x, y - p.y),
            Command::LineTo(p) => path.line_to(x + p.x, y - p.y),
            Command::QuadTo(c, p) => path.quad_to(x + c.x, y - c.y, x + p.x, y - p.y),
            Command::CurveTo(a, b, p) => {
                path.cubic_to(x + a.x, y - a.y, x + b.x, y - b.y, x + p.x, y - p.y)
            }
            Command::Close => path.close(),
        }
    }
}

fn metrics(fonts: &mut FontSystem, face: &Face) -> FontMetrics {
    let buffer = shape(fonts, face, "0");
    let zero = buffer
        .layout_runs()
        .next()
        .and_then(|run| run.glyphs.first().cloned());
    let zero_width = zero.as_ref().map_or(face.size * 0.5, |glyph| glyph.w);
    let font = zero.and_then(|glyph| fonts.get_font(glyph.font_id, glyph.font_weight));
    let Some(font) = font else {
        return FontMetrics {
            ascent: face.size * 0.8,
            descent: face.size * 0.2,
            height: face.size * 1.2,
            x_height: face.size * 0.5,
            zero_width,
            em_ascent: 0.0,
            em_descent: 0.0,
        };
    };
    let m = font.as_swash().metrics(&[]).scale(face.size);
    let (ascent, descent) = (m.ascent.abs(), m.descent.abs());
    let total = ascent + descent;
    let em_ascent = if total > 0.0 {
        (ascent * face.size / total * 64.0).round() / 64.0
    } else {
        0.0
    };
    FontMetrics {
        ascent,
        descent,
        height: ascent + descent + m.leading.max(0.0),
        x_height: if m.x_height > 0.0 {
            m.x_height
        } else {
            face.size * 0.5
        },
        zero_width,
        em_ascent,
        em_descent: if total > 0.0 {
            face.size - em_ascent
        } else {
            0.0
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn css_family_lists_resolve_generics_and_skip_unknown_names() {
        let mut system = quill::locks::lock_or_recover(system());
        assert_eq!(
            resolve(&mut system, "\"No Such Font 42\", serif"),
            FamilyName::Serif
        );
        assert_eq!(
            resolve(&mut system, "-apple-system, sans-serif"),
            FamilyName::SansSerif
        );
        assert_eq!(resolve(&mut system, ""), FamilyName::SansSerif);
    }

    #[test]
    fn a_font_measures_text_and_wider_text_is_wider() {
        let mut fonts = Fonts::default();
        let (font, metrics) = fonts.create("sans-serif", 16.0, 400, false, "tnum=1");
        assert!(font > 0);
        assert!(metrics.ascent > 0.0 && metrics.descent > 0.0 && metrics.zero_width > 0.0);
        let one = fonts.width(font, "Quill");
        assert!(one > 0.0);
        assert!(fonts.width(font, "Quill Quill") > one);
        assert!(
            fonts
                .shape(font, "Quill", 2.0)
                .and_then(|shaped| shaped.outline)
                .is_some()
        );
        fonts.delete(font);
        assert_eq!(fonts.width(font, "Quill"), 0.0);
    }
}
