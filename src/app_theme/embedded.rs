//! The built-in themes: Quill's own light and dark palettes (the
//! defaults), and Telegram Desktop's four embedded themes
//! (`window_themes_embedded.cpp` `EmbeddedThemes`, `DefaultAccentColors`)
//! with their palettes for the keys Quill uses (resolved from
//! `lib_ui/ui/colors.palette` and the bundled `day-blue`, `night` and
//! `night-green` `.tdesktop-theme` files) and the accent colorizer
//! (`ColorizerFrom`).
//!
//! Quill Light and Quill Dark are drawn by Quill's own token pairs and the
//! kit's default colours, exactly as before themes existed. Their palette
//! here only describes them: the preview tiles, and the starting point of
//! "Create new theme".

use super::keys::PaletteKey;
use super::palette::{Palette, ThemeBackground};
use crate::theme_colorize::Colorizer;

/// One of the built-in themes. After Quill's two, the names are tdesktop's:
/// "Classic" is its default palette, "Day" is `day-blue`, "Tinted" is
/// `night` and "Night" is `night-green`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BuiltinTheme {
    QuillLight,
    QuillDark,
    Classic,
    Day,
    Tinted,
    Night,
}

impl BuiltinTheme {
    /// Quill's two first, then tdesktop's settings order.
    pub const ALL: [BuiltinTheme; 6] = [
        BuiltinTheme::QuillLight,
        BuiltinTheme::QuillDark,
        BuiltinTheme::Classic,
        BuiltinTheme::Day,
        BuiltinTheme::Tinted,
        BuiltinTheme::Night,
    ];

    pub fn name(self) -> &'static str {
        match self {
            BuiltinTheme::QuillLight => "Quill Light",
            BuiltinTheme::QuillDark => "Quill Dark",
            BuiltinTheme::Classic => "Classic",
            BuiltinTheme::Day => "Day",
            BuiltinTheme::Tinted => "Tinted",
            BuiltinTheme::Night => "Night",
        }
    }

    /// The id stored in the appearance prefs.
    pub fn id(self) -> &'static str {
        match self {
            BuiltinTheme::QuillLight => "quill_light",
            BuiltinTheme::QuillDark => "quill_dark",
            BuiltinTheme::Classic => "classic",
            BuiltinTheme::Day => "day",
            BuiltinTheme::Tinted => "tinted",
            BuiltinTheme::Night => "night",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.id() == id)
    }

    pub fn is_dark(self) -> bool {
        matches!(
            self,
            BuiltinTheme::QuillDark | BuiltinTheme::Tinted | BuiltinTheme::Night
        )
    }

    /// Quill's own palettes: drawn by its token pairs, not by a palette.
    pub fn is_quill(self) -> bool {
        matches!(self, BuiltinTheme::QuillLight | BuiltinTheme::QuillDark)
    }

    /// The colour the palette is drawn around (`EmbeddedScheme::accentColor`).
    pub fn accent(self) -> u32 {
        match self {
            BuiltinTheme::QuillLight => 0x0969da,
            BuiltinTheme::QuillDark => 0x1f6feb,
            BuiltinTheme::Classic | BuiltinTheme::Day => 0x40a7e3,
            BuiltinTheme::Tinted => 0x5288c1,
            BuiltinTheme::Night => 0x3fc1b0,
        }
    }

    /// The accent circles after the theme's own (`DefaultAccentColors`;
    /// Quill's own presets for its two themes).
    pub fn accent_choices(self) -> &'static [u32] {
        match self {
            BuiltinTheme::QuillLight | BuiltinTheme::QuillDark => &[
                0x2f81f7, 0x3fb950, 0x8957e5, 0xf778ba, 0xd29922, 0x39c5cf, 0xf85149,
            ],
            BuiltinTheme::Classic | BuiltinTheme::Day => &[
                0x45bce7, 0x52b440, 0xd46c99, 0xdf8a49, 0x9978c8, 0xc55245, 0x687b98, 0xdea922,
            ],
            BuiltinTheme::Tinted => &[
                0x58bfe8, 0x466f42, 0xaa6084, 0xa46d3c, 0x917bbd, 0xab5149, 0x697b97, 0x9b834b,
            ],
            BuiltinTheme::Night => &[
                0x60a8e7, 0x4e9c57, 0xca7896, 0xcc925c, 0xa58ed2, 0xd27570, 0x7b8799, 0xcbac67,
            ],
        }
    }

    /// The colours of the theme's preview tile (`EmbeddedScheme`):
    /// background, outgoing bubble, incoming bubble.
    pub fn preview(self) -> [u32; 3] {
        match self {
            BuiltinTheme::QuillLight => [0xf6f8fa, 0x0969da, 0xeaeef2],
            BuiltinTheme::QuillDark => [0x0d1117, 0x1f6feb, 0x2d333b],
            BuiltinTheme::Classic => [0x9bd494, 0xeaffdc, 0xffffff],
            BuiltinTheme::Day => [0x7ec4ea, 0xd7f0ff, 0xffffff],
            BuiltinTheme::Tinted => [0x485761, 0x5ca7d4, 0x6b808d],
            BuiltinTheme::Night => [0x485761, 0x6b808d, 0x6b808d],
        }
    }

    /// The theme's chat background: none for Quill's (the window shows
    /// through, as before), the default Telegram wallpaper's colours for
    /// Classic (`WallPaper::ConstructDefault`, without its pattern), and
    /// the one-pixel `background.png` of the others.
    pub fn background(self) -> Option<ThemeBackground> {
        Some(match self {
            BuiltinTheme::QuillLight | BuiltinTheme::QuillDark => return None,
            BuiltinTheme::Classic => {
                ThemeBackground::Gradient(vec![0xdbddbb, 0x6ba587, 0xd5d88d, 0x88b884])
            }
            BuiltinTheme::Day => ThemeBackground::Solid(0x74b4e0),
            BuiltinTheme::Tinted => ThemeBackground::Solid(0x0e1621),
            BuiltinTheme::Night => ThemeBackground::Solid(0x18191d),
        })
    }

    /// The theme's own palette, before any accent.
    pub fn palette(self) -> Palette {
        let colors = match self {
            BuiltinTheme::QuillLight => QUILL_LIGHT,
            BuiltinTheme::QuillDark => QUILL_DARK,
            BuiltinTheme::Classic => CLASSIC,
            BuiltinTheme::Day => DAY,
            BuiltinTheme::Tinted => TINTED,
            BuiltinTheme::Night => NIGHT,
        };
        Palette::from_colors(colors, self.background())
    }

    /// The palette recoloured to `accent` (0xRRGGBB) the way tdesktop
    /// colorizes an embedded theme; `None` or the theme's own accent keep
    /// it as it is. Quill's accent only re-points the primary fills (the
    /// kit's primary colours), as it always has.
    pub fn palette_with_accent(self, accent: Option<u32>) -> Palette {
        let mut palette = self.palette();
        match accent {
            Some(accent) if self.is_quill() => {
                for key in [PaletteKey::WindowBgActive, PaletteKey::ActiveButtonBg] {
                    palette.set(key, (accent << 8) | 0xff);
                }
                palette
            }
            Some(accent) if accent != self.accent() => palette.colorized(&Colorizer::from_accent(
                self.accent(),
                accent,
                self.is_dark(),
            )),
            _ => palette,
        }
    }
}

// In `PaletteKey::ALL` order (0xRRGGBBAA). Quill's two: its token pairs
// (`ui/chat_theme.rs`) for the token each key stands for.
const QUILL_LIGHT: [u32; PaletteKey::COUNT] = [
    0xffffffff, 0x24292fff, 0x1f2328ff, 0x59636eff, 0xeaeef2ff, 0xd0d7deff, 0x0969daff, 0xffffffff,
    0x0969daff, 0x0969daff, 0x0860caff, 0xffffffff, 0xd1242cff, 0xcf222eff, 0xffffffff, 0xd0d7deff,
    0xd0d7deff, 0x6e7781ff, 0xf6f8faff, 0xd0d7deff, 0x00000053, 0xf6f8faff, 0xf6f8faff, 0x6e7681ff,
    0xeaeef2ff, 0x0969daff, 0xffffffff,
];

const QUILL_DARK: [u32; PaletteKey::COUNT] = [
    0x0d1117ff, 0xc9d1d9ff, 0xffffffff, 0x8b949eff, 0x21262dff, 0x444c56ff, 0x1f6febff, 0xffffffff,
    0x58a6ffff, 0x1f6febff, 0x388bfdff, 0xffffffff, 0xf85149ff, 0xff7b72ff, 0x161b22ff, 0x30363dff,
    0x30363dff, 0x9a9a9aff, 0x161b22ff, 0x444c56ff, 0xffffff53, 0x161b22ff, 0x161b22ff, 0x6e7681ff,
    0x2d333bff, 0x1f6febff, 0xffffffff,
];

// Generated from tdesktop's palettes.
const CLASSIC: [u32; PaletteKey::COUNT] = [
    0xffffffff, 0x000000ff, 0x222222ff, 0x999999ff, 0xf1f1f1ff, 0xe5e5e5ff, 0x40a7e3ff, 0xffffffff,
    0x168acdff, 0x40a7e3ff, 0x39a5dbff, 0xffffffff, 0xd14e4eff, 0xd84d4dff, 0xffffffff, 0xf1f1f1ff,
    0xe0e0e0ff, 0x999999ff, 0xf1f1f1ff, 0xb3b3b3ff, 0x00000053, 0xf1f1f1ff, 0xffffffff, 0xbbbbbbff,
    0xffffffff, 0xeffddeff, 0x45a32dff,
];

const DAY: [u32; PaletteKey::COUNT] = [
    0xffffffff, 0x000000ff, 0x222222ff, 0x999999ff, 0xf1f1f1ff, 0xe5e5e5ff, 0x40a7e3ff, 0xffffffff,
    0x168acdff, 0x40a7e3ff, 0x39a5dbff, 0xffffffff, 0xd14e4eff, 0xd84d4dff, 0xffffffff, 0xf1f1f1ff,
    0xe0e0e0ff, 0x999999ff, 0xf1f1f1ff, 0xb3b3b3ff, 0x00000053, 0xf1f1f1ff, 0xffffffff, 0xbbbbbbff,
    0xffffffff, 0xdef1fdff, 0x168acdff,
];

const TINTED: [u32; PaletteKey::COUNT] = [
    0x17212bff, 0xf5f5f5ff, 0xe9e8e8ff, 0x708499ff, 0x232e3cff, 0x24303dff, 0x5288c1ff, 0xffffffff,
    0x6ab3f3ff, 0x2f6ea5ff, 0x3476abff, 0xffffffff, 0xec3942ff, 0xdc3d3dff, 0x17212bff, 0x232f39ff,
    0x313c49ff, 0x6d7883ff, 0x242f3dff, 0x4f6276ff, 0xffffff53, 0x242f3dff, 0x17212bff, 0x3e546aff,
    0x182533ff, 0x2b5278ff, 0x90caffff,
];

const NIGHT: [u32; PaletteKey::COUNT] = [
    0x282e33ff, 0xf5f5f5ff, 0xe9e8e8ff, 0x82868aff, 0x313b43ff, 0x3f4850ff, 0x3fc1b0ff, 0xffffffff,
    0x4be1c3ff, 0x2da192ff, 0x32a898ff, 0xffffffff, 0xf57474ff, 0xd84d4dff, 0x282e33ff, 0x42484dff,
    0x6f6f6fff, 0x818991ff, 0x3d444bff, 0x6c6c6cff, 0xffffff53, 0x3a4047ff, 0x282e33ff, 0x495159ff,
    0x33393fff, 0x2a2f33ff, 0x60e5cbff,
];
