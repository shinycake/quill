//! What the box looks like: the backdrop the user picked, the colors
//! derived from it (tdesktop `ComputePeerQrColors`) and the color list
//! (`QrThemeSwatchesValue`).

use super::color::{Rgb, Rgba, contrast, from_lch, mix_lab, to_lab};

const SOLVE_ITERATIONS: usize = 24;
const INK_CONTRAST: f64 = 4.4;
const FOREGROUND_CONTRAST: f64 = 4.5;
const INK_CHROMA_SCALE: f64 = 1.25;
const INK_CHROMA_ADD: f64 = 0.02;
const INK_CHROMA_MAX: f64 = 0.20;
const DARK_INK_GLASS_ALPHA: f32 = 0.35;
const WHITE_GLYPH_GLASS_ALPHA: f32 = 0.12;
const WHITE_GLYPH_CONTRAST: f64 = 3.;
const GLASS_ALPHA: f32 = 0.25;
const GRADIENT_MIDDLE_MIX: f64 = 0.5;
const DARK_EDGE_FROM: f64 = 7.;
const DARK_EDGE_TILL: f64 = 11.;
const DARK_INK_CONTRAST: f64 = 2.3;
/// The background gradient's steps, mixed in OKLab (`kGradientSteps`).
pub const GRADIENT_STEPS: usize = 8;
/// Gradient radius over the card width (`kGradientRadiusToCard`).
pub const GRADIENT_RADIUS_TO_CARD: f32 = 1.6;

/// tdesktop `PeerQrDefaultAppearance`: the dark backdrop shown until a
/// color is picked.
pub const DEFAULT_EDGE: Rgb = Rgb::hex(0x0E_0F0F);
pub const DEFAULT_CENTER: Rgb = Rgb::hex(0x36_3738);

/// One backdrop from `getProfileShareThemeBackdrops`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareThemeBackdrop {
    pub id: i32,
    pub name: String,
    pub center: Rgb,
    pub edge: Rgb,
}

/// Which circle of the colors panel is chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SwatchKey {
    /// The default dark backdrop (no `background_backdrop_id`).
    #[default]
    Default,
    Backdrop(i32),
}

/// One circle of the colors panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Swatch {
    pub key: SwatchKey,
    pub center: Rgb,
    pub edge: Rgb,
}

impl Swatch {
    pub const DEFAULT: Swatch = Swatch {
        key: SwatchKey::Default,
        center: DEFAULT_CENTER,
        edge: DEFAULT_EDGE,
    };
}

fn is_default(backdrop: &ShareThemeBackdrop) -> bool {
    backdrop.center == DEFAULT_CENTER && backdrop.edge == DEFAULT_EDGE
}

/// The colors panel's circles in server order. A backdrop with the default
/// colors is the default circle; without one, the default circle comes
/// first (tdesktop `QrThemeSwatchesValue`).
pub fn swatches(backdrops: &[ShareThemeBackdrop]) -> Vec<Swatch> {
    let mut out = Vec::with_capacity(backdrops.len() + 1);
    if !backdrops.iter().any(is_default) {
        out.push(Swatch::DEFAULT);
    }
    out.extend(backdrops.iter().map(|backdrop| {
        if is_default(backdrop) {
            Swatch::DEFAULT
        } else {
            Swatch {
                key: SwatchKey::Backdrop(backdrop.id),
                center: backdrop.center,
                edge: backdrop.edge,
            }
        }
    }));
    out
}

/// The backdrop and pattern the box shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Appearance {
    pub center: Rgb,
    pub edge: Rgb,
    pub emoji: Vec<super::theme::ThemeEmoji>,
    /// The upgraded gift whose model and symbol make the pattern, by id.
    pub gift: Option<i64>,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            center: DEFAULT_CENTER,
            edge: DEFAULT_EDGE,
            emoji: Vec::new(),
            gift: None,
        }
    }
}

/// The colors drawn over a backdrop (tdesktop `PeerQrColors`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PeerQrColors {
    /// QR modules, the center disc and the name on the white card.
    pub ink: Rgb,
    /// Icons and labels on the backdrop.
    pub foreground: Rgb,
    /// The round buttons' fill.
    pub control: Rgba,
    /// The panels' glass.
    pub glass: Rgba,
}

/// The lightest color of `hue`/`chroma` that keeps `contrast` against
/// `against` (tdesktop `SolveLightness`).
fn solve_lightness(hue: f64, chroma: f64, against: Rgb, target: f64) -> Rgb {
    let (mut from, mut till) = (0., to_lab(against).lightness);
    for _ in 0..SOLVE_ITERATIONS {
        let middle = (from + till) / 2.;
        if contrast(from_lch(middle, chroma, hue), against) >= target {
            from = middle;
        } else {
            till = middle;
        }
    }
    from_lch(from, chroma, hue)
}

fn ink_chroma(chroma: f64) -> f64 {
    (chroma * INK_CHROMA_SCALE + INK_CHROMA_ADD).min(INK_CHROMA_MAX)
}

fn backdrop_ink(center: Rgb, edge: Rgb) -> Rgb {
    let lab = to_lab(edge);
    let hue = lab.hue();
    let shipped = solve_lightness(hue, ink_chroma(lab.chroma()), Rgb::WHITE, INK_CONTRAST);
    let darkness = ((contrast(edge, Rgb::WHITE) - DARK_EDGE_FROM)
        / (DARK_EDGE_TILL - DARK_EDGE_FROM))
        .clamp(0., 1.);
    if darkness <= 0. {
        return shipped;
    }
    // Dark neutrals: the ink is that much darker than their center.
    let shade = solve_lightness(hue, lab.chroma(), center, DARK_INK_CONTRAST);
    if contrast(shade, Rgb::WHITE) <= contrast(shipped, Rgb::WHITE) {
        return shipped;
    }
    if darkness >= 1. {
        shade
    } else {
        mix_lab(shipped, shade, darkness)
    }
}

impl PeerQrColors {
    /// tdesktop `ComputePeerQrColors`.
    pub fn compute(center: Rgb, edge: Rgb) -> Self {
        let lab = to_lab(edge);
        let hue = lab.hue();
        let chroma = ink_chroma(lab.chroma());
        let reference = mix_lab(center, edge, GRADIENT_MIDDLE_MIX);
        let darkest = [center, reference, edge]
            .into_iter()
            .max_by(|a, b| contrast(*a, Rgb::WHITE).total_cmp(&contrast(*b, Rgb::WHITE)))
            .unwrap_or(edge);
        let dark = solve_lightness(hue, chroma, darkest, FOREGROUND_CONTRAST);
        let glass = |alpha: f32| reference.mix(Rgb::WHITE, f64::from(alpha));
        let white_contrast = contrast(Rgb::WHITE, glass(WHITE_GLYPH_GLASS_ALPHA));
        let white_glyphs = white_contrast >= WHITE_GLYPH_CONTRAST
            || white_contrast >= contrast(dark, glass(DARK_INK_GLASS_ALPHA));
        Self {
            ink: backdrop_ink(center, edge),
            foreground: if white_glyphs { Rgb::WHITE } else { dark },
            control: Rgba::new(
                Rgb::WHITE,
                if white_glyphs {
                    WHITE_GLYPH_GLASS_ALPHA
                } else {
                    DARK_INK_GLASS_ALPHA
                },
            ),
            glass: Rgba::new(Rgb::WHITE, GLASS_ALPHA),
        }
    }
}

/// The background gradient's stops from the center outwards, mixed in
/// OKLab (tdesktop `PaintAppearanceGradient`).
pub fn gradient_stops(center: Rgb, edge: Rgb) -> [Rgb; GRADIENT_STEPS + 1] {
    std::array::from_fn(|i| mix_lab(center, edge, i as f64 / GRADIENT_STEPS as f64))
}
