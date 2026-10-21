//! Omarchy's current theme as a Quill palette. Omarchy writes the theme
//! it shows to `~/.local/state/omarchy/current/theme/colors.toml` (flat
//! `key = "#rrggbb"` lines plus `mode`) and bumps `theme.generation` on
//! every switch. Older Omarchy kept the theme in
//! `~/.config/omarchy/current/theme` with only `alacritty.toml` (and a
//! `light.mode` file on light themes); that is read as a fallback.
#![deny(clippy::indexing_slicing)]

use super::{Palette, PaletteKey, ThemeBackground};
use crate::theme_colorize::{contrast, readable};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Overrides where to look (tests, screenshot demos): a directory laid out
/// like `~/.local/state/omarchy/current`.
pub const STATE_DIR_ENV: &str = "QUILL_OMARCHY_STATE";

/// Omarchy's current-theme directory: the state one when it has a theme,
/// else the legacy config one. `None` off Omarchy.
pub fn current_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os(STATE_DIR_ENV) {
        return Some(PathBuf::from(dir));
    }
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/state"))
        .join("omarchy/current");
    if state.join("theme/colors.toml").is_file() {
        return Some(state);
    }
    let legacy = home.join(".config/omarchy/current");
    legacy
        .join("theme/alacritty.toml")
        .is_file()
        .then_some(legacy)
}

/// Changes whenever Omarchy switches theme: `theme.generation` when it is
/// there, else the colour file's modification time.
pub fn stamp(dir: &Path) -> Option<String> {
    if let Ok(generation) = std::fs::read_to_string(dir.join("theme.generation")) {
        return Some(generation.trim().to_string());
    }
    let theme = dir.join("theme");
    ["colors.toml", "alacritty.toml"].iter().find_map(|file| {
        let modified = std::fs::metadata(theme.join(file)).ok()?.modified().ok()?;
        Some(format!("{modified:?}"))
    })
}

/// The colours of one Omarchy theme.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OmarchyTheme {
    /// From `theme.name`, empty when Omarchy did not write one.
    pub name: String,
    pub dark: bool,
    /// Name → 0xRRGGBB.
    colors: BTreeMap<String, u32>,
}

impl OmarchyTheme {
    /// The theme in `dir` (see [`current_dir`]).
    pub fn read(dir: &Path) -> Option<Self> {
        let name = std::fs::read_to_string(dir.join("theme.name"))
            .map(|n| n.trim().to_string())
            .unwrap_or_default();
        let theme = dir.join("theme");
        let mut parsed = if let Ok(text) = std::fs::read_to_string(theme.join("colors.toml")) {
            Self::parse_colors(&text)?
        } else {
            let text = std::fs::read_to_string(theme.join("alacritty.toml")).ok()?;
            Self::parse_alacritty(&text, theme.join("light.mode").exists())?
        };
        parsed.name = name;
        Some(parsed)
    }

    /// `colors.toml`: `mode = "dark" | "light"` and `name = "#rrggbb"`.
    pub fn parse_colors(text: &str) -> Option<Self> {
        let mut colors = BTreeMap::new();
        let mut mode = None;
        for (key, value) in toml_pairs(text) {
            if key == "mode" {
                mode = Some(value == "dark");
            } else if let Some(rgb) = parse_rgb(&value) {
                colors.insert(key, rgb);
            }
        }
        Self::from_colors(colors, mode)
    }

    /// Legacy `alacritty.toml`: `[colors.primary]` background and
    /// foreground, `[colors.normal]` for the accent (blue) and red.
    pub fn parse_alacritty(text: &str, light: bool) -> Option<Self> {
        let mut colors = BTreeMap::new();
        let mut section = String::new();
        for line in text.lines() {
            let line = line.trim();
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = name.trim().to_string();
                continue;
            }
            let Some((key, value)) = pair(line) else {
                continue;
            };
            let Some(rgb) = parse_rgb(&value) else {
                continue;
            };
            let name = match (section.as_str(), key.as_str()) {
                ("colors.primary", "background") => "background",
                ("colors.primary", "foreground") => "foreground",
                ("colors.normal", "blue") => "accent",
                ("colors.normal", "red") => "red",
                ("colors.normal", "green") => "green",
                ("colors.bright", "black") => "muted",
                _ => continue,
            };
            colors.insert(name.to_string(), rgb);
        }
        Self::from_colors(colors, Some(!light))
    }

    fn from_colors(colors: BTreeMap<String, u32>, mode: Option<bool>) -> Option<Self> {
        let background = *colors.get("background")?;
        colors.get("foreground")?;
        let dark = mode.unwrap_or_else(|| luminance(background) < 0.5);
        Some(Self {
            name: String::new(),
            dark,
            colors,
        })
    }

    fn get(&self, key: &str) -> Option<u32> {
        self.colors.get(key).copied()
    }

    /// The first of `keys` the theme sets, else `fallback`.
    fn pick(&self, keys: &[&str], fallback: u32) -> u32 {
        keys.iter().find_map(|k| self.get(k)).unwrap_or(fallback)
    }

    /// Every Quill palette key from the theme's colours. Surfaces nobody
    /// named are mixed from the background and foreground, the way
    /// Omarchy's own templates derive them.
    pub fn palette(&self) -> Palette {
        let bg = self.pick(&["background"], 0);
        let fg = self.pick(&["foreground"], 0xffffff);
        let toward_fg = |amount: f32| mix(bg, fg, amount);
        let accent = self.pick(&["accent", "blue"], toward_fg(0.6));
        let lighter = self.pick(&["lighter_background"], toward_fg(0.06));
        let darker = self.pick(&["dark_background"], mix(bg, 0x000000, 0.15));
        let selection = self.pick(&["selection"], toward_fg(0.15));
        let muted = self.pick(&["muted"], toward_fg(0.35));
        let secondary = readable(self.pick(&["dark_foreground"], toward_fg(0.6)), bg, 3.0);
        let bold = self.pick(&["bright_foreground", "light_foreground"], fg);
        let red = self.pick(&["red"], 0xe5484d);
        // Text on the accent: whichever of the background and the
        // foreground reads better on it.
        let on_accent = if contrast(bg, accent) >= contrast(fg, accent) {
            bg
        } else {
            fg
        };
        let link = readable(accent, bg, 3.0);
        let out_bubble = mix(bg, accent, if self.dark { 0.28 } else { 0.18 });
        let mut colors = [0xff; PaletteKey::COUNT];
        let mut put = |key: PaletteKey, rgb: u32| {
            if let Some(slot) = colors.get_mut(key.index()) {
                *slot = (rgb << 8) | 0xff;
            }
        };
        use PaletteKey as K;
        put(K::WindowBg, bg);
        put(K::WindowFg, fg);
        put(K::WindowBoldFg, bold);
        put(K::WindowSubTextFg, secondary);
        put(K::WindowBgOver, lighter);
        put(K::WindowBgRipple, selection);
        put(K::WindowBgActive, accent);
        put(K::WindowFgActive, on_accent);
        put(K::WindowActiveTextFg, link);
        put(K::ActiveButtonBg, accent);
        put(K::ActiveButtonBgOver, mix(accent, fg, 0.12));
        put(K::ActiveButtonFg, on_accent);
        put(K::AttentionButtonFg, red);
        put(K::BoxTextFgError, red);
        put(K::MenuBg, lighter);
        put(K::MenuSeparatorFg, muted);
        put(K::InputBorderFg, muted);
        put(K::PlaceholderFg, secondary);
        put(K::FilterInputInactiveBg, darker);
        put(K::CheckboxFg, muted);
        put(K::ScrollBarBg, muted);
        put(K::TitleBgActive, darker);
        put(K::DialogsBg, bg);
        put(K::DialogsUnreadBgMuted, muted);
        put(K::MsgInBg, lighter);
        put(K::MsgOutBg, out_bubble);
        put(K::MsgOutServiceFg, readable(accent, out_bubble, 3.0));
        let wallpaper = self.pick(&["darker_background", "dark_background"], darker);
        Palette::from_colors(colors, Some(ThemeBackground::Solid(wallpaper)))
    }
}

/// `key = "value"` lines outside any table; comments and blanks skipped.
fn toml_pairs(text: &str) -> impl Iterator<Item = (String, String)> + '_ {
    let mut in_table = false;
    text.lines().filter_map(move |line| {
        let line = line.trim();
        if line.starts_with('[') {
            in_table = true;
            return None;
        }
        if in_table {
            return None;
        }
        pair(line)
    })
}

/// One `key = "value"` (or `key = 'value'`) line, comments dropped.
fn pair(line: &str) -> Option<(String, String)> {
    if line.starts_with('#') {
        return None;
    }
    let (key, value) = line.split_once('=')?;
    let value = value.trim();
    let quote = value.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let inner = value.get(1..)?;
    let end = inner.find(quote)?;
    Some((key.trim().to_string(), inner.get(..end)?.to_string()))
}

/// `#rrggbb` (or `0xrrggbb`, as alacritty allows) as 0xRRGGBB.
fn parse_rgb(value: &str) -> Option<u32> {
    let digits = value
        .strip_prefix('#')
        .or_else(|| value.strip_prefix("0x"))?;
    (digits.len() == 6)
        .then(|| u32::from_str_radix(digits, 16).ok())
        .flatten()
}

/// `a` moved `amount` of the way to `b`, per channel.
fn mix(a: u32, b: u32, amount: f32) -> u32 {
    let channel = |shift: u32| {
        let x = ((a >> shift) & 0xff) as f32;
        let y = ((b >> shift) & 0xff) as f32;
        ((x + (y - x) * amount).round().clamp(0., 255.) as u32) << shift
    };
    channel(16) | channel(8) | channel(0)
}

/// Rough perceived brightness, 0..1.
fn luminance(rgb: u32) -> f32 {
    let r = ((rgb >> 16) & 0xff) as f32;
    let g = ((rgb >> 8) & 0xff) as f32;
    let b = (rgb & 0xff) as f32;
    (0.299 * r + 0.587 * g + 0.114 * b) / 255.
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKYO: &str = r##"mode = "dark"

accent = "#7aa2f7"
selection = "#292e42"
muted = "#414868"

background = "#1a1b26"
dark_background = "#13141c"
darker_background = "#0e0e14"
lighter_background = "#24283b"

foreground = "#a9b1d6"
dark_foreground = "#565f89"
bright_foreground = "#c0caf5"

red = "#f7768e"
"##;

    const LATTE: &str = r##"mode = "light"
accent = "#1e66f5"
selection = "#ccd0da"
muted = "#acb0be"
background = "#eff1f5"
dark_background = "#e3e4e8"
lighter_background = "#dce0e8"
foreground = "#4c4f69"
"##;

    #[test]
    fn colors_toml_becomes_a_dark_or_light_palette() {
        let tokyo = OmarchyTheme::parse_colors(TOKYO).expect("tokyo");
        assert!(tokyo.dark);
        let palette = tokyo.palette();
        assert!(palette.is_dark());
        assert_eq!(palette.rgb(PaletteKey::WindowBg), 0x1a1b26);
        assert_eq!(palette.rgb(PaletteKey::WindowBgActive), 0x7aa2f7);
        assert_eq!(palette.rgb(PaletteKey::WindowBoldFg), 0xc0caf5);
        assert_eq!(palette.rgb(PaletteKey::AttentionButtonFg), 0xf7768e);
        // Dark text reads better on Tokyo Night's light blue accent.
        assert_eq!(palette.rgb(PaletteKey::ActiveButtonFg), 0x1a1b26);

        let latte = OmarchyTheme::parse_colors(LATTE).expect("latte");
        assert!(!latte.dark);
        let palette = latte.palette();
        assert!(!palette.is_dark());
        assert_eq!(palette.rgb(PaletteKey::ActiveButtonFg), 0xeff1f5);
    }

    #[test]
    fn a_file_without_background_or_foreground_is_not_a_theme() {
        assert_eq!(OmarchyTheme::parse_colors("accent = \"#ffffff\""), None);
        assert_eq!(OmarchyTheme::parse_colors(""), None);
    }

    #[test]
    fn legacy_alacritty_files_still_theme_quill() {
        let text = r##"
[colors.primary]
background = '#1a1b26'
foreground = '#a9b1d6'

[colors.normal]
black = '#32344a'
red = '#f7768e'
blue = '#7aa2f7'
"##;
        let theme = OmarchyTheme::parse_alacritty(text, false).expect("alacritty");
        assert!(theme.dark);
        assert_eq!(theme.palette().rgb(PaletteKey::WindowBgActive), 0x7aa2f7);
        let light = OmarchyTheme::parse_alacritty(text, true).expect("light");
        assert!(!light.dark);
    }

    #[test]
    fn reads_the_state_dir_and_its_generation() {
        let root = crate::scratch::process_dir("omarchy-theme");
        let theme = root.join("theme");
        std::fs::create_dir_all(&theme).expect("dir");
        std::fs::write(theme.join("colors.toml"), TOKYO).expect("colors");
        std::fs::write(root.join("theme.name"), "Tokyo Night\n").expect("name");
        std::fs::write(root.join("theme.generation"), "7\n").expect("generation");
        let read = OmarchyTheme::read(&root).expect("read");
        assert_eq!(read.name, "Tokyo Night");
        assert_eq!(stamp(&root).as_deref(), Some("7"));
    }
}
