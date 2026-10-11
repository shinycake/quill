//! How a pattern's coverage changes the fill under it (tdesktop
//! `CacheBackground` / `GenerateBackgroundImage`).
//!
//! - Positive intensity: the pattern is painted with Qt's `SoftLight`
//!   composition at `intensity` opacity, black over light fills and white
//!   over dark ones (`IsPatternInverted`).
//! - `is_inverted` (Telegram's negative intensity, dark themes): the fill
//!   only shows through the pattern (`DestinationIn`), and black covers the
//!   rest at `1 - intensity`, so the result is `fill * coverage *
//!   intensity` over black.
//!
//! Both depend on the fill colour under each pixel, so the result is one
//! 256 x 256 table (coverage, channel) built per composition.

/// What a pattern does to the fill.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PatternMix {
    /// 0-100.
    pub intensity: u8,
    /// The fill shows only through the pattern; black elsewhere.
    pub inverted: bool,
    /// Draw the pattern white (dark fills), see [`pattern_is_light`].
    pub light: bool,
}

impl PatternMix {
    /// From a `backgroundTypePattern` over `fill_colors`. A negative
    /// `intensity` also means inverted (Telegram's own encoding).
    pub fn new(fill_colors: &[u32], intensity: i32, is_inverted: bool) -> Self {
        let inverted = is_inverted || intensity < 0;
        let intensity = intensity.unsigned_abs().min(100) as u8;
        Self {
            intensity,
            inverted,
            light: pattern_is_light(fill_colors, inverted),
        }
    }
}

/// tdesktop `IsPatternInverted`: a positive-opacity pattern over a fill
/// whose average colour has an HSV value of 0.3 or less is drawn white.
pub fn pattern_is_light(fill_colors: &[u32], inverted: bool) -> bool {
    if inverted || fill_colors.is_empty() {
        return false;
    }
    let n = fill_colors.len() as u32;
    let mean = |shift: u32| fill_colors.iter().map(|c| (c >> shift) & 0xff).sum::<u32>() / n;
    let value = mean(16).max(mean(8)).max(mean(0));
    f64::from(value) / 255.0 <= 0.3
}

/// Qt's `soft_light_op` for an opaque destination (`da` = 255): `dst` and
/// the premultiplied source `src` with alpha `sa`, all 0-255.
fn soft_light_op(dst: i32, src: i32, sa: i32) -> i32 {
    const FACTOR: i32 = 255 * 255;
    let src2 = src << 1;
    let temp = dst * (255 - sa) * 255;
    if src2 < sa {
        (dst * (sa * 255 + (src2 - sa) * (255 - dst)) + temp) / FACTOR
    } else if 4 * dst <= 255 {
        let curve = (((16 * dst - 12 * 255) * dst + 3 * FACTOR) * dst) / FACTOR;
        (dst * sa * 255 + 255 * (src2 - sa) * curve + temp) / FACTOR
    } else {
        let root = f64::from(dst * 255).sqrt() as i32;
        (dst * sa * 255 + 255 * (src2 - sa) * (root - dst) + temp) / FACTOR
    }
}

/// The finished channel value for every (coverage, fill channel) pair.
pub struct MixTable {
    table: Vec<u8>,
}

impl MixTable {
    pub fn new(mix: PatternMix) -> Self {
        let mut table = vec![0u8; 256 * 256];
        let opacity = (i32::from(mix.intensity) * 255 + 50) / 100;
        for coverage in 0..256i32 {
            for dst in 0..256i32 {
                let value = if mix.inverted {
                    (dst * coverage * i32::from(mix.intensity) + 255 * 50) / (255 * 100)
                } else {
                    let src = if mix.light { coverage } else { 0 };
                    let mixed = soft_light_op(dst, src, coverage);
                    // `setOpacity`: Qt interpolates the result with the
                    // destination by the constant alpha.
                    (mixed * opacity + dst * (255 - opacity) + 127) / 255
                };
                table[(coverage as usize) << 8 | dst as usize] = value.clamp(0, 255) as u8;
            }
        }
        Self { table }
    }

    /// The channel `dst` once a pattern of `coverage` is mixed in.
    #[inline]
    pub fn mix(&self, dst: u8, coverage: u8) -> u8 {
        self.table[(coverage as usize) << 8 | dst as usize]
    }
}
