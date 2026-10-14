//! A resolved theme: one colour per [`PaletteKey`] plus the chat
//! background the theme brings.

use super::keys::PaletteKey;
use crate::theme_colorize::{Colorizer, Hsv};

/// The chat background a theme brings (tdesktop's theme wallpaper).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ThemeBackground {
    /// One colour, 0xRRGGBB.
    Solid(u32),
    /// A freeform gradient of three or four 0xRRGGBB colours (the default
    /// Telegram wallpaper, without its pattern).
    Gradient(Vec<u32>),
}

impl ThemeBackground {
    /// One colour standing for the whole background.
    pub fn average(&self) -> u32 {
        match self {
            ThemeBackground::Solid(c) => *c,
            ThemeBackground::Gradient(colors) => crate::theme_colorize::average(colors),
        }
    }

    fn colorized(&self, colorizer: &Colorizer) -> Self {
        match self {
            ThemeBackground::Solid(c) => ThemeBackground::Solid(colorizer.apply(*c)),
            ThemeBackground::Gradient(colors) => {
                ThemeBackground::Gradient(colors.iter().map(|c| colorizer.apply(*c)).collect())
            }
        }
    }
}

/// Every key's colour as 0xRRGGBBAA, the background and whether the
/// theme is dark.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Palette {
    colors: [u32; PaletteKey::COUNT],
    pub background: Option<ThemeBackground>,
}

impl Palette {
    /// A palette from colours in [`PaletteKey::ALL`] order.
    pub fn from_colors(
        colors: [u32; PaletteKey::COUNT],
        background: Option<ThemeBackground>,
    ) -> Self {
        Self { colors, background }
    }

    /// The key's colour as 0xRRGGBBAA.
    #[expect(
        clippy::indexing_slicing,
        reason = "`colors` has one slot per `PaletteKey` and `index()` is the discriminant"
    )]
    pub fn rgba(&self, key: PaletteKey) -> u32 {
        self.colors[key.index()]
    }

    /// The key's colour as 0xRRGGBB, alpha dropped.
    pub fn rgb(&self, key: PaletteKey) -> u32 {
        self.rgba(key) >> 8
    }

    #[expect(
        clippy::indexing_slicing,
        reason = "`colors` has one slot per `PaletteKey` and `index()` is the discriminant"
    )]
    pub fn set(&mut self, key: PaletteKey, rgba: u32) {
        self.colors[key.index()] = rgba;
    }

    pub fn colors(&self) -> &[u32; PaletteKey::COUNT] {
        &self.colors
    }

    /// tdesktop's night test (`IsThemeDarkValue`): the chat list
    /// background's HSV value is under one half.
    pub fn is_dark(&self) -> bool {
        Hsv::of(self.rgb(PaletteKey::DialogsBg)).value < 128
    }

    /// The palette recoloured by `colorizer`, skipping the keys tdesktop
    /// never colorizes. Alpha is kept.
    pub fn colorized(&self, colorizer: &Colorizer) -> Self {
        let mut out = self.clone();
        for &key in PaletteKey::ALL {
            if key.colorize_ignored() {
                continue;
            }
            let rgba = self.rgba(key);
            let rgb = colorizer.apply(rgba >> 8);
            out.set(key, (rgb << 8) | (rgba & 0xff));
        }
        out.background = self.background.as_ref().map(|b| b.colorized(colorizer));
        out
    }

    /// A stable fingerprint, to notice that the applied palette changed.
    pub fn fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

/// `#rrggbb` or `#rrggbbaa` for 0xRRGGBBAA (alpha written only when it is
/// not opaque).
pub fn hex_string(rgba: u32) -> String {
    if rgba & 0xff == 0xff {
        format!("#{:06x}", rgba >> 8)
    } else {
        format!("#{rgba:08x}")
    }
}

/// 0xRRGGBBAA from `#rrggbb` / `#rrggbbaa` (the `#` is required, as in
/// tdesktop's palette files).
pub fn parse_hex(value: &str) -> Option<u32> {
    let digits = value.strip_prefix('#')?;
    if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let parsed = u32::from_str_radix(digits, 16).ok()?;
    match digits.len() {
        6 => Some((parsed << 8) | 0xff),
        8 => Some(parsed),
        _ => None,
    }
}
