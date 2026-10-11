//! The emoji avatar's background: Telegram Desktop's gradient drawing
//! (`Images::GenerateGradient`) and the colour maths of its gradient
//! editor (up to four colours, hex entry, hue and lightness).

/// An opaque square picture: `side` x `side` RGBA bytes, row by row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Square {
    pub side: u32,
    pub rgba: Vec<u8>,
}

impl Square {
    fn from_fn(side: u32, mut pixel: impl FnMut(u32, u32) -> [u8; 3]) -> Self {
        let mut rgba = Vec::with_capacity((side * side * 4) as usize);
        for y in 0..side {
            for x in 0..side {
                let [r, g, b] = pixel(x, y);
                rgba.extend_from_slice(&[r, g, b, 255]);
            }
        }
        Self { side, rgba }
    }

    /// The pixel at `x`, `y` as `[r, g, b, a]`.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * self.side + x) * 4) as usize;
        [
            self.rgba[at],
            self.rgba[at + 1],
            self.rgba[at + 2],
            self.rgba[at + 3],
        ]
    }

    /// Bilinear scale to `side` (what `Qt::SmoothTransformation` gives).
    fn scaled(&self, side: u32) -> Self {
        let last = (self.side - 1) as f32;
        let ratio = if side > 1 {
            last / (side - 1) as f32
        } else {
            0.0
        };
        Self::from_fn(side, |x, y| {
            let (fx, fy) = (x as f32 * ratio, y as f32 * ratio);
            let (x0, y0) = (fx.floor() as u32, fy.floor() as u32);
            let (x1, y1) = ((x0 + 1).min(self.side - 1), (y0 + 1).min(self.side - 1));
            let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
            let mix = |c: usize| {
                let p = |px: u32, py: u32| self.pixel(px, py)[c] as f32;
                let top = p(x0, y0) + (p(x1, y0) - p(x0, y0)) * tx;
                let bottom = p(x0, y1) + (p(x1, y1) - p(x0, y1)) * tx;
                (top + (bottom - top) * ty).round() as u8
            };
            [mix(0), mix(1), mix(2)]
        })
    }
}

/// Most colours a background has (`kMaxColors` in the gradient editor).
pub const MAX_COLORS: usize = 4;

/// Where the four colour points sit for the first rotation phase the
/// builder uses (rotation 45 is phase 1), as `positionsForPhase` of
/// `GenerateSmallComplexGradient` yields them (y already flipped).
fn points(phase: usize) -> [(f32, f32); 4] {
    const POSITIONS: [(f32, f32); 8] = [
        (0.80, 0.10),
        (0.60, 0.20),
        (0.35, 0.25),
        (0.25, 0.60),
        (0.20, 0.90),
        (0.40, 0.80),
        (0.65, 0.75),
        (0.75, 0.40),
    ];
    std::array::from_fn(|i| {
        let (x, y) = POSITIONS[(phase + i * 2) % 8];
        (x, 1.0 - y)
    })
}

fn channels(color: u32) -> [f32; 3] {
    [
        ((color >> 16) & 0xFF) as f32,
        ((color >> 8) & 0xFF) as f32,
        (color & 0xFF) as f32,
    ]
}

/// The two-colour (or one-colour) gradient at 45 degrees, corner to corner
/// like Qt's `QLinearGradient` from the top right to the bottom left.
fn linear(colors: &[u32], side: u32) -> Square {
    let first = channels(colors.first().copied().unwrap_or(0));
    let last = channels(
        colors
            .get(1)
            .copied()
            .unwrap_or(colors.first().copied().unwrap_or(0)),
    );
    let span = side.max(1) as f32;
    Square::from_fn(side, |x, y| {
        // Rotation 45 runs from (width, 0) to (0, height).
        let t = (((span - x as f32) + y as f32) / (2.0 * span)).clamp(0.0, 1.0);
        let mix = |i: usize| (first[i] + (last[i] - first[i]) * t).round() as u8;
        [mix(0), mix(1), mix(2)]
    })
}

/// Three or four colours blended around moving points: the 64 x 64
/// picture Telegram Desktop makes, then scaled to `side`.
fn complex(colors: &[u32], side: u32) -> Square {
    const SMALL: u32 = 64;
    // Phase 1 is rotation 45; the colours sit at the phase's points.
    let current = points(1);
    let count = colors.len().min(4);
    let rgb: Vec<[f32; 3]> = colors.iter().take(count).map(|c| channels(*c)).collect();
    let small = Square::from_fn(SMALL, |x, y| {
        let direct_x = x as f32 / SMALL as f32;
        let direct_y = y as f32 / SMALL as f32;
        let cx = direct_x - 0.5;
        let cy = direct_y - 0.5;
        let center = (cx * cx + cy * cy).sqrt();
        let swirl = 0.35 * center;
        let theta = swirl * swirl * 0.8 * 8.0;
        let (sin, cos) = theta.sin_cos();
        let px = (0.5 + cx * cos - cy * sin).clamp(0.0, 1.0);
        let py = (0.5 + cx * sin + cy * cos).clamp(0.0, 1.0);
        let (mut sum, mut r, mut g, mut b) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for (i, color) in rgb.iter().enumerate() {
            let (qx, qy) = current[i];
            let distance = (0.9 - ((px - qx).powi(2) + (py - qy).powi(2)).sqrt()).max(0.0);
            let weight = distance.powi(4);
            sum += weight;
            r += weight * color[0];
            g += weight * color[1];
            b += weight * color[2];
        }
        if sum <= 0.0 {
            return [rgb[0][0] as u8, rgb[0][1] as u8, rgb[0][2] as u8];
        }
        [(r / sum) as u8, (g / sum) as u8, (b / sum) as u8]
    });
    if side == SMALL {
        small
    } else {
        small.scaled(side)
    }
}

/// The background picture for `colors` at `side` x `side`: solid for one
/// colour, a diagonal gradient for two, the blended freeform gradient for
/// three or four (what `chatPhotoSticker`'s fill looks like on desktop).
pub fn render_background(colors: &[u32], side: u32) -> Square {
    let side = side.max(1);
    match colors.len() {
        0 => Square::from_fn(side, |_, _| [0, 0, 0]),
        1 | 2 => linear(colors, side),
        _ => complex(colors, side),
    }
}

/// `#RRGGBB` or `RRGGBB` (also three digits) to `0xRRGGBB`.
pub fn parse_hex(text: &str) -> Option<u32> {
    let text = text.trim().trim_start_matches('#');
    match text.len() {
        6 => u32::from_str_radix(text, 16).ok(),
        3 => {
            let v = u32::from_str_radix(text, 16).ok()?;
            let (r, g, b) = ((v >> 8) & 0xF, (v >> 4) & 0xF, v & 0xF);
            Some(((r * 17) << 16) | ((g * 17) << 8) | (b * 17))
        }
        _ => None,
    }
}

/// `0xRRGGBB` as `#RRGGBB`.
pub fn format_hex(color: u32) -> String {
    format!("#{:06X}", color & 0x00FF_FFFF)
}

/// Hue (degrees), saturation and lightness (both 0 to 1) of a colour.
pub fn to_hsl(color: u32) -> (f32, f32, f32) {
    let [r, g, b] = channels(color).map(|c| c / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let delta = max - min;
    if delta <= f32::EPSILON {
        return (0.0, 0.0, l);
    }
    let s = delta / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    (h * 60.0, s.clamp(0.0, 1.0), l)
}

/// The colour for a hue (degrees), saturation and lightness.
pub fn from_hsl(h: f32, s: f32, l: f32) -> u32 {
    let h = h.rem_euclid(360.0) / 60.0;
    let s = s.clamp(0.0, 1.0);
    let l = l.clamp(0.0, 1.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let byte = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u32;
    (byte(r) << 16) | (byte(g) << 8) | byte(b)
}

/// A colour for a newly added slot: the last one, turned 50 degrees round
/// the hue wheel (desktop picks a random colour near it).
pub fn next_color(last: u32) -> u32 {
    let (h, s, l) = to_hsl(last);
    from_hsl(h + 50.0, s.max(0.6), l.clamp(0.35, 0.65))
}

#[cfg(test)]
mod tests {
    use super::{format_hex, from_hsl, next_color, parse_hex, render_background, to_hsl};

    #[test]
    fn hex_round_trips_and_rejects_noise() {
        assert_eq!(parse_hex("#FF8000"), Some(0xFF8000));
        assert_eq!(parse_hex(" ff8000 "), Some(0xFF8000));
        assert_eq!(parse_hex("#f80"), Some(0xFF8800));
        assert_eq!(parse_hex("#12345"), None);
        assert_eq!(parse_hex("zzzzzz"), None);
        assert_eq!(format_hex(0x0A0B0C), "#0A0B0C");
        assert_eq!(format_hex(0xFF0A0B0C), "#0A0B0C");
    }

    #[test]
    fn hsl_conversion_keeps_primary_colours() {
        for color in [0xFF0000, 0x00FF00, 0x0000FF, 0x808080, 0xFFFFFF, 0x000000] {
            let (h, s, l) = to_hsl(color);
            assert_eq!(from_hsl(h, s, l), color, "{color:06X}");
        }
        assert_eq!(to_hsl(0x00FF00).0, 120.0);
        assert_eq!(from_hsl(480.0, 1.0, 0.5), 0x00FF00);
    }

    #[test]
    fn new_slot_colour_differs_from_the_last() {
        assert_ne!(next_color(0x20E2CD), 0x20E2CD);
        assert_ne!(next_color(0x000000), 0x000000);
    }

    #[test]
    fn solid_and_linear_backgrounds() {
        let solid = render_background(&[0x102030], 8);
        assert!(solid.rgba.chunks(4).all(|p| p == [0x10, 0x20, 0x30, 255]));
        let diagonal = render_background(&[0x000000, 0xFFFFFF], 32);
        // Rotation 45 starts at the top right and ends at the bottom left.
        assert!(diagonal.pixel(31, 0)[0] < 10);
        assert!(diagonal.pixel(0, 31)[0] > 245);
    }

    #[test]
    fn freeform_background_uses_every_colour_and_stays_opaque() {
        let image = render_background(&[0xFF0000, 0x00FF00, 0x0000FF, 0xFFFF00], 64);
        assert_eq!(image.side, 64);
        assert!(image.rgba.chunks(4).all(|p| p[3] == 255));
        let has = |test: fn(&[u8]) -> bool| image.rgba.chunks(4).any(test);
        assert!(has(|p| p[0] > 150 && p[1] < 110 && p[2] < 110));
        assert!(has(|p| p[1] > 150 && p[0] < 110 && p[2] < 110));
        assert!(has(|p| p[2] > 150 && p[0] < 110 && p[1] < 110));
        assert!(has(|p| p[0] > 150 && p[1] > 150 && p[2] < 110));
        // Scaled output has the requested size.
        let big = render_background(&[1, 2, 3], 100);
        assert_eq!((big.side, big.rgba.len()), (100, 100 * 100 * 4));
    }
}
