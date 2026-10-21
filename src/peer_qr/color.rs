//! Color math for the QR card: sRGB, OKLab/OKLCh and WCAG contrast, ported
//! from tdesktop `ui/color_contrast.cpp` so the derived ink and glyph
//! colors match it for every backdrop.

const GAMUT_ITERATIONS: usize = 30;
const GAMUT_EPSILON: f64 = 1e-4;

/// An opaque sRGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const WHITE: Rgb = Rgb::hex(0xFF_FFFF);
    pub const BLACK: Rgb = Rgb::hex(0);

    /// `0xRRGGBB`, the form TDLib sends colors in.
    pub const fn hex(value: u32) -> Self {
        Self {
            r: ((value >> 16) & 0xFF) as u8,
            g: ((value >> 8) & 0xFF) as u8,
            b: (value & 0xFF) as u8,
        }
    }

    pub const fn to_hex(self) -> u32 {
        ((self.r as u32) << 16) | ((self.g as u32) << 8) | self.b as u32
    }

    /// Straight sRGB interpolation (Qt's `anim::color`).
    pub fn mix(self, other: Rgb, ratio: f64) -> Rgb {
        let channel = |a: u8, b: u8| {
            let value = f64::from(a) + (f64::from(b) - f64::from(a)) * ratio.clamp(0., 1.);
            value.round().clamp(0., 255.) as u8
        };
        Rgb {
            r: channel(self.r, other.r),
            g: channel(self.g, other.g),
            b: channel(self.b, other.b),
        }
    }
}

/// A color with straight alpha in `0..=1`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub rgb: Rgb,
    pub alpha: f32,
}

impl Rgba {
    pub const fn new(rgb: Rgb, alpha: f32) -> Self {
        Self { rgb, alpha }
    }

    /// `0xRRGGBBAA`, the form GPUI's `rgba()` takes.
    pub fn to_rgba_hex(self) -> u32 {
        let alpha = (f64::from(self.alpha.clamp(0., 1.)) * 255.).round() as u32;
        (self.rgb.to_hex() << 8) | alpha
    }
}

/// OKLab coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lab {
    pub lightness: f64,
    pub a: f64,
    pub b: f64,
}

impl Lab {
    pub fn chroma(self) -> f64 {
        self.a.hypot(self.b)
    }

    pub fn hue(self) -> f64 {
        let degrees = self.b.atan2(self.a).to_degrees() % 360.;
        if degrees < 0. {
            degrees + 360.
        } else {
            degrees
        }
    }
}

fn to_linear(channel: u8) -> f64 {
    let value = f64::from(channel) / 255.;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn from_linear(value: f64) -> f64 {
    let value = value.clamp(0., 1.);
    if value <= 0.003_130_8 {
        12.92 * value
    } else {
        1.055 * value.powf(1. / 2.4) - 0.055
    }
}

pub fn to_lab(color: Rgb) -> Lab {
    let (r, g, b) = (to_linear(color.r), to_linear(color.g), to_linear(color.b));
    let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
    let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
    let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
    Lab {
        lightness: 0.210_454_255_3 * l + 0.793_617_785 * m - 0.004_072_046_8 * s,
        a: 1.977_998_495_1 * l - 2.428_592_205 * m + 0.450_593_709_9 * s,
        b: 0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766 * s,
    }
}

fn linear_from_lch(lightness: f64, chroma: f64, hue: f64) -> [f64; 3] {
    let radians = hue.to_radians();
    let (a, b) = (chroma * radians.cos(), chroma * radians.sin());
    let l = lightness + 0.396_337_777_4 * a + 0.215_803_757_3 * b;
    let m = lightness - 0.105_561_345_8 * a - 0.063_854_172_8 * b;
    let s = lightness - 0.089_484_177_5 * a - 1.291_485_548 * b;
    let (l3, m3, s3) = (l * l * l, m * m * m, s * s * s);
    [
        4.076_741_662_1 * l3 - 3.307_711_591_3 * m3 + 0.230_969_929_2 * s3,
        -1.268_438_004_6 * l3 + 2.609_757_401_1 * m3 - 0.341_319_396_5 * s3,
        -0.004_196_086_3 * l3 - 0.703_418_614_7 * m3 + 1.707_614_701 * s3,
    ]
}

fn in_gamut(lightness: f64, chroma: f64, hue: f64) -> bool {
    linear_from_lch(lightness, chroma, hue)
        .iter()
        .all(|value| (-GAMUT_EPSILON..=1. + GAMUT_EPSILON).contains(value))
}

/// The sRGB color at OKLCh, with chroma reduced until it is in gamut.
pub fn from_lch(lightness: f64, mut chroma: f64, hue: f64) -> Rgb {
    if !in_gamut(lightness, chroma, hue) {
        let (mut low, mut high) = (0., chroma);
        for _ in 0..GAMUT_ITERATIONS {
            let middle = (low + high) / 2.;
            if in_gamut(lightness, middle, hue) {
                low = middle;
            } else {
                high = middle;
            }
        }
        chroma = low;
    }
    let [r, g, b] = linear_from_lch(lightness, chroma, hue);
    let channel = |value: f64| (from_linear(value) * 255.).round().clamp(0., 255.) as u8;
    Rgb {
        r: channel(r),
        g: channel(g),
        b: channel(b),
    }
}

/// WCAG contrast ratio between two colors (tdesktop `CountContrast`).
pub fn contrast(a: Rgb, b: Rgb) -> f64 {
    let luminance = |c: Rgb| {
        let map = |channel: u8| {
            let value = f64::from(channel) / 255.;
            if value <= 0.039_28 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        map(c.r) * 0.2126 + map(c.g) * 0.7152 + map(c.b) * 0.0722
    };
    let (first, second) = (luminance(a), luminance(b));
    (first.max(second) + 0.05) / (first.min(second) + 0.05)
}

/// Interpolation in OKLab (tdesktop `MixLab`).
pub fn mix_lab(a: Rgb, b: Rgb, ratio: f64) -> Rgb {
    let (from, till) = (to_lab(a), to_lab(b));
    let mix = |x: f64, y: f64| x + (y - x) * ratio;
    let lab = Lab {
        lightness: mix(from.lightness, till.lightness),
        a: mix(from.a, till.a),
        b: mix(from.b, till.b),
    };
    from_lch(lab.lightness, lab.chroma(), lab.hue())
}
