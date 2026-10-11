//! Recolouring a palette from an accent, the way Telegram Desktop applies
//! a chat theme (lib_ui `ui/style/style_palette_colorizer.cpp`,
//! `window/themes/window_themes_embedded.cpp` `ColorizerFrom`,
//! `ui/chat/chat_theme.cpp` `ThemeAdjustedColor`, `ui/color_contrast.cpp`).
//!
//! Colours are 0xRRGGBB. HSV and HSL follow Qt's integer conventions (hue
//! 0-359 or -1 for greys, the other channels 0-255), because the
//! colorizer's thresholds are written in them.

/// Qt's integer HSV.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hsv {
    /// 0-359, or -1 for an achromatic colour.
    pub hue: i32,
    pub saturation: i32,
    pub value: i32,
}

fn channels(rgb: u32) -> (f64, f64, f64) {
    (
        f64::from((rgb >> 16) & 0xff) / 255.0,
        f64::from((rgb >> 8) & 0xff) / 255.0,
        f64::from(rgb & 0xff) / 255.0,
    )
}

fn pack(r: f64, g: f64, b: f64) -> u32 {
    let c = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    (c(r) << 16) | (c(g) << 8) | c(b)
}

/// The hue in degrees (fractional), or `None` for greys.
fn hue_degrees(r: f64, g: f64, b: f64) -> Option<f64> {
    let max = r.max(g).max(b);
    let delta = max - r.min(g).min(b);
    if delta <= f64::EPSILON {
        return None;
    }
    let h = if (max - r).abs() <= f64::EPSILON {
        (g - b) / delta
    } else if (max - g).abs() <= f64::EPSILON {
        2.0 + (b - r) / delta
    } else {
        4.0 + (r - g) / delta
    } * 60.0;
    Some(if h < 0.0 { h + 360.0 } else { h })
}

impl Hsv {
    /// `QColor::getHsv`.
    pub fn of(rgb: u32) -> Self {
        let (r, g, b) = channels(rgb);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let saturation = if max <= 0.0 { 0.0 } else { (max - min) / max };
        Self {
            hue: hue_degrees(r, g, b).map_or(-1, |h| (h as i32).rem_euclid(360)),
            saturation: (saturation * 255.0).round() as i32,
            value: (max * 255.0).round() as i32,
        }
    }

    /// `QColor::fromHsv`.
    pub fn to_rgb(self) -> u32 {
        let s = f64::from(self.saturation.clamp(0, 255)) / 255.0;
        let v = f64::from(self.value.clamp(0, 255)) / 255.0;
        if self.hue < 0 || s <= 0.0 {
            return pack(v, v, v);
        }
        let h = f64::from(self.hue.rem_euclid(360)) / 60.0;
        let i = h.floor();
        let f = h - i;
        let p = v * (1.0 - s);
        let q = v * (1.0 - s * f);
        let t = v * (1.0 - s * (1.0 - f));
        let (r, g, b) = match i as i32 {
            0 => (v, t, p),
            1 => (q, v, p),
            2 => (p, v, t),
            3 => (p, q, v),
            4 => (t, p, v),
            _ => (v, p, q),
        };
        pack(r, g, b)
    }
}

/// HSL with fractional channels (hue in degrees, -1 for greys).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsl {
    pub hue: f64,
    pub saturation: f64,
    pub lightness: f64,
}

impl Hsl {
    pub fn of(rgb: u32) -> Self {
        let (r, g, b) = channels(rgb);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let lightness = (max + min) / 2.0;
        let delta = max - min;
        let saturation = if delta <= f64::EPSILON {
            0.0
        } else if lightness < 0.5 {
            delta / (max + min)
        } else {
            delta / (2.0 - max - min)
        };
        Self {
            hue: hue_degrees(r, g, b).unwrap_or(-1.0),
            saturation,
            lightness,
        }
    }

    pub fn to_rgb(self) -> u32 {
        let (h, s, l) = (self.hue, self.saturation, self.lightness);
        if h < 0.0 || s <= 0.0 {
            return pack(l, l, l);
        }
        let q = if l < 0.5 {
            l * (1.0 + s)
        } else {
            l + s - l * s
        };
        let p = 2.0 * l - q;
        let hk = h / 360.0;
        let channel = |t: f64| {
            let t = t.rem_euclid(1.0);
            if t < 1.0 / 6.0 {
                p + (q - p) * 6.0 * t
            } else if t < 0.5 {
                q
            } else if t < 2.0 / 3.0 {
                p + (q - p) * (2.0 / 3.0 - t) * 6.0
            } else {
                p
            }
        };
        pack(
            channel(hk + 1.0 / 3.0),
            channel(hk),
            channel(hk - 1.0 / 3.0),
        )
    }
}

/// tdesktop `ThemeAdjustedColor`: `original`'s lightness with `by`'s hue
/// and saturation.
pub fn adjusted(original: u32, by: u32) -> u32 {
    let base = Hsl::of(original);
    let tone = Hsl::of(by);
    Hsl {
        hue: tone.hue,
        saturation: tone.saturation,
        lightness: base.lightness,
    }
    .to_rgb()
}

/// WCAG contrast ratio, 1-21 (tdesktop `CountContrast`).
pub fn contrast(a: u32, b: u32) -> f64 {
    let luminance = |rgb: u32| {
        let (r, g, b) = channels(rgb);
        let map = |v: f64| {
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        map(r) * 0.2126 + map(g) * 0.7152 + map(b) * 0.0722
    };
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// tdesktop `kMinAcceptableContrast` for text on outgoing bubbles.
pub const MIN_ACCEPTABLE_CONTRAST: f64 = 1.14;

/// The mean of several colours.
pub fn average(colors: &[u32]) -> u32 {
    if colors.is_empty() {
        return 0;
    }
    let n = colors.len() as u32;
    let mean = |shift: u32| colors.iter().map(|c| (c >> shift) & 0xff).sum::<u32>() / n;
    (mean(16) << 16) | (mean(8) << 8) | mean(0)
}

/// `style::colorizer`: shifts every colour whose hue is within
/// `hue_threshold` of `was` towards `now`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Colorizer {
    pub was: Hsv,
    pub now: Hsv,
    pub hue_threshold: i32,
}

impl Colorizer {
    /// `ColorizerFrom`: the palette built on `base_accent` recoloured to
    /// `accent`, with tdesktop's lightness limits (at most 160 on day
    /// themes, at least 64 on night ones).
    pub fn from_accent(base_accent: u32, accent: u32, dark: bool) -> Self {
        let hsl = Hsl::of(accent);
        let lightness = (hsl.lightness * 255.0).round() as i32;
        let limited = if dark {
            lightness.max(64)
        } else {
            lightness.min(160)
        };
        let now = if limited == lightness {
            Hsv::of(accent)
        } else {
            Hsv::of(
                Hsl {
                    lightness: f64::from(limited) / 255.0,
                    ..hsl
                }
                .to_rgb(),
            )
        };
        Self {
            was: Hsv::of(base_accent),
            now,
            hue_threshold: 15,
        }
    }

    /// `ChatTheme::bubblesAccentColorizer`: every chromatic colour moves
    /// with the outgoing accent.
    pub fn for_bubbles(base_accent: u32, accent: u32) -> Self {
        Self {
            was: Hsv::of(base_accent),
            now: Hsv::of(accent),
            hue_threshold: 255,
        }
    }

    fn shift(&self, color: Hsv) -> Option<Hsv> {
        let (was, now) = (self.was, self.now);
        if (color.hue - was.hue).abs() >= self.hue_threshold {
            return None;
        }
        let hue = color.hue + (now.hue - was.hue);
        let saturation = if color.saturation > was.saturation && now.saturation > was.saturation {
            (now.saturation * (255 - was.saturation)
                + (color.saturation - was.saturation) * (255 - now.saturation))
                / (255 - was.saturation).max(1)
        } else if color.saturation != was.saturation && was.saturation != 0 {
            color.saturation * now.saturation / was.saturation
        } else {
            now.saturation
        };
        let value = if color.value > was.value {
            (now.value * (255 - was.value) + (color.value - was.value) * (255 - now.value))
                / (255 - was.value).max(1)
        } else if color.value < was.value {
            color.value * now.value / was.value.max(1)
        } else {
            now.value
        };
        Some(Hsv {
            hue: (hue + 360).rem_euclid(360),
            saturation: saturation.clamp(0, 255),
            value: value.clamp(0, 255),
        })
    }

    /// `rgb` recoloured, or unchanged when its hue is too far from the
    /// base accent (greys always are).
    pub fn apply(&self, rgb: u32) -> u32 {
        let hsv = Hsv::of(rgb);
        if hsv.hue < 0 {
            return rgb;
        }
        self.shift(hsv).map_or(rgb, Hsv::to_rgb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsv_and_hsl_match_qt() {
        assert_eq!(
            Hsv::of(0xff0000),
            Hsv {
                hue: 0,
                saturation: 255,
                value: 255
            }
        );
        assert_eq!(Hsv::of(0x808080).hue, -1);
        let blue = Hsv::of(0x1f6feb);
        assert_eq!(blue.hue, 216);
        for rgb in [0x1f6feb, 0x40a7e3, 0xf778ba, 0x000000, 0xffffff, 0x3fb950] {
            let back = Hsv::of(rgb).to_rgb();
            let close = |s: u32| ((rgb >> s) & 0xff).abs_diff((back >> s) & 0xff) <= 3;
            assert!(close(16) && close(8) && close(0), "{rgb:06x} -> {back:06x}");
            let hsl = Hsl::of(rgb).to_rgb();
            let close = |s: u32| ((rgb >> s) & 0xff).abs_diff((hsl >> s) & 0xff) <= 1;
            assert!(close(16) && close(8) && close(0), "{rgb:06x} -> {hsl:06x}");
        }
    }

    #[test]
    fn colorizing_moves_accent_hues_and_leaves_greys() {
        // Blue base, pink theme accent (light mode).
        let c = Colorizer::from_accent(0x0969da, 0xe0569b, false);
        let link = c.apply(0x0969da);
        let hsv = Hsv::of(link);
        assert!((hsv.hue - Hsv::of(0xe0569b).hue).abs() <= 2, "{link:06x}");
        // Greys and far hues (green, red) keep their colour.
        assert_eq!(c.apply(0x8b8b8b), 0x8b8b8b);
        assert_eq!(c.apply(0x1a7f37), 0x1a7f37);
        assert_eq!(c.apply(0xcf222e), 0xcf222e);
        // A lighter blue stays lighter than the accent after the shift.
        assert!(Hsv::of(c.apply(0x9ecbff)).value >= Hsv::of(link).value);
    }

    #[test]
    fn accent_lightness_is_limited_like_tdesktop() {
        // A very light accent is darkened to lightness 160 on day themes.
        let day = Colorizer::from_accent(0x0969da, 0xf0f8ff, false);
        let l = Hsl::of(day.now.to_rgb()).lightness * 255.0;
        assert!(l <= 162.0, "{l}");
        // A very dark accent is lifted to 64 on night themes.
        let night = Colorizer::from_accent(0x1f6feb, 0x050510, true);
        let l = Hsl::of(night.now.to_rgb()).lightness * 255.0;
        assert!(l >= 62.0, "{l}");
    }

    #[test]
    fn bubble_colorizer_moves_every_chromatic_colour() {
        let c = Colorizer::for_bubbles(0x1f6feb, 0x3fb950);
        assert_ne!(c.apply(0xcf222e), 0xcf222e);
        assert_eq!(c.apply(0xffffff), 0xffffff);
    }

    #[test]
    fn adjusted_keeps_lightness_and_takes_the_tone() {
        let out = adjusted(0x404040, 0xff0000);
        let hsl = Hsl::of(out);
        assert!((hsl.lightness - Hsl::of(0x404040).lightness).abs() < 0.01);
        assert!(hsl.hue.abs() < 1.0 && hsl.saturation > 0.9);
    }

    #[test]
    fn contrast_is_wcag() {
        assert!((contrast(0x000000, 0xffffff) - 21.0).abs() < 0.01);
        assert!((contrast(0x777777, 0x777777) - 1.0).abs() < 1e-9);
        assert!(contrast(0xffffff, 0xf2f2f2) < MIN_ACCEPTABLE_CONTRAST);
        assert_eq!(average(&[0x000000, 0xfefefe]), 0x7f7f7f);
        assert_eq!(average(&[]), 0);
    }
}
