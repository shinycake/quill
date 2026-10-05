//! Named color tokens for the Quill UI (Phase 0 of the kit-first UI cleanup).
//!
//! Phase 8: every token is a function of the current [`ThemeMode`] — the app
//! boots dark, and the View → Toggle Theme menu item flips both the kit
//! theme (`Theme::change`, so kit components re-theme) and this palette
//! together through [`set_theme_mode`], so the two can never disagree.
//! Dark values are the exact pre-migration GitHub-dark colors; light values
//! are their GitHub primer light counterparts. Deliberate palette moves
//! belong to the later delight-tuning phases, not to the mechanical wiring.
//!
//! [`text_on_fill`] is the one semantic exception: text painted on solid
//! colored fills (accent buttons, chips, badges, avatars, outgoing bubbles)
//! stays white in both modes, exactly like the official clients.
//!
//! stories-high-contrast: the orthogonal [`high_contrast()`] flag re-roots
//! the palette for maximum contrast — pure-black surfaces, white
//! text/borders, dark semantic fills — while semantic hues keep their
//! dark values (designed for black backgrounds). The kit [`ThemeMode`]
//! stays Light/Dark (the kit has no HC variant); HC pairs with the dark
//! kit theme so kit components stay coherent with the black surfaces.

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, Rgba, Window};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
const fn hex(hex: u32) -> Rgba {
    Rgba {
        r: ((hex >> 16) & 0xff) as f32 / 255.0,
        g: ((hex >> 8) & 0xff) as f32 / 255.0,
        b: (hex & 0xff) as f32 / 255.0,
        a: 1.0,
    }
}

const fn hex_a(hex: u32) -> Rgba {
    Rgba {
        r: ((hex >> 24) & 0xff) as f32 / 255.0,
        g: ((hex >> 16) & 0xff) as f32 / 255.0,
        b: ((hex >> 8) & 0xff) as f32 / 255.0,
        a: (hex & 0xff) as f32 / 255.0,
    }
}

/// 1 = dark (the boot mode), 0 = light. Written only by [`set_theme_mode`].
static MODE_DARK: AtomicU8 = AtomicU8::new(1);

/// stories-high-contrast: orthogonal to [`MODE_DARK`]. While set, every
/// token resolves to its high-contrast value (pure-black surfaces, white
/// text/borders, dark semantic fills); semantic hues keep their dark
/// values, which were designed for black backgrounds. Written only by
/// [`set_high_contrast`].
static HIGH_CONTRAST: AtomicBool = AtomicBool::new(false);

#[inline]
fn dark() -> bool {
    MODE_DARK.load(Ordering::Relaxed) == 1
}

/// stories-high-contrast: whether the high-contrast palette is active.
#[inline]
pub fn high_contrast() -> bool {
    HIGH_CONTRAST.load(Ordering::Relaxed)
}

/// stories-high-contrast: flip the high-contrast palette. Independent of
/// the kit [`ThemeMode`] (which stays Light/Dark — the kit has no HC
/// variant); callers pair it with the dark kit theme so kit components
/// stay coherent with the black Quill surfaces.
pub fn set_high_contrast(on: bool) {
    HIGH_CONTRAST.store(on, Ordering::Relaxed);
}

/// kit Phase 8: switch the whole app between light and dark. Drives the kit
/// theme (`Theme::change`, so kit components re-theme) and the Quill token
/// palette in one call — callers never touch one without the other.
pub fn set_theme_mode(mode: ThemeMode, window: Option<&mut Window>, cx: &mut App) {
    MODE_DARK.store(u8::from(mode.is_dark()), Ordering::Relaxed);
    Theme::change(mode, window, cx);
}

/// Pick the dark/light hex value for the current mode. `hc_hex`
/// (`Some`) overrides both while high contrast is active; `None` keeps
/// the dark value in high-contrast mode (semantic hues are designed for
/// black backgrounds, so only surfaces/text/borders need overrides).
#[inline]
fn pick(dark_hex: u32, light_hex: u32, hc_hex: Option<u32>) -> Rgba {
    if high_contrast() {
        return hex(hc_hex.unwrap_or(dark_hex));
    }
    hex(if dark() { dark_hex } else { light_hex })
}

/// [`pick`] for `0xRRGGBBAA` values.
#[inline]
fn pick_a(dark_hex: u32, light_hex: u32, hc_hex: Option<u32>) -> Rgba {
    if high_contrast() {
        return hex_a(hc_hex.unwrap_or(dark_hex));
    }
    hex_a(if dark() { dark_hex } else { light_hex })
}

// --- accent ---
#[inline]
pub fn accent() -> Rgba {
    pick(0x58a6ff, 0x0969da, Some(0xffd60a))
}
#[inline]
pub fn accent_strong() -> Rgba {
    pick(0x1f6feb, 0x0969da, Some(0xb3d91))
}
#[inline]
pub fn accent_light() -> Rgba {
    pick(0x2f81f7, 0x2f81f7, Some(0xffd60a))
}
#[inline]
pub fn accent_info() -> Rgba {
    pick(0x9ecbff, 0x0969da, Some(0xffd60a))
}

// --- text ---
/// Text on solid colored fills: white in both modes (official-client behavior).
#[inline]
pub fn text_on_fill() -> Rgba {
    hex(0xffffff)
}
#[inline]
pub fn text_bright() -> Rgba {
    pick(0xffffff, 0x1f2328, Some(0xffffff))
}
#[inline]
pub fn text_primary() -> Rgba {
    pick(0xc9d1d9, 0x24292f, Some(0xffffff))
}
#[inline]
pub fn text_muted() -> Rgba {
    pick(0x8b949e, 0x59636e, Some(0xd9d9d9))
}
#[inline]
pub fn text_faint() -> Rgba {
    pick(0x9a9a9a, 0x6e7781, Some(0xd9d9d9))
}
#[inline]
pub fn text_menu() -> Rgba {
    pick(0xe6edf3, 0x1f2328, Some(0xffffff))
}

// --- danger ---
#[inline]
pub fn danger() -> Rgba {
    pick(0xf85149, 0xd1242c, None)
}
#[inline]
pub fn danger_soft() -> Rgba {
    pick(0xe17076, 0xd1242c, None)
}
#[inline]
pub fn danger_bright() -> Rgba {
    pick(0xff7b72, 0xcf222e, None)
}
#[inline]
pub fn danger_pale() -> Rgba {
    pick(0xff8a8a, 0xa40e26, None)
}
#[inline]
pub fn danger_vivid() -> Rgba {
    pick(0xff6b6b, 0xd1242c, None)
}
#[inline]
pub fn danger_dark() -> Rgba {
    pick(0xd44a3a, 0x8c1d18, None)
}
#[inline]
pub fn danger_bg() -> Rgba {
    pick(0x3d1f1f, 0xffebe9, Some(0x2a0d0d))
}
#[inline]
pub fn danger_bg_deep() -> Rgba {
    pick(0x3a1414, 0xffebe9, Some(0x2a0d0d))
}

// --- success ---
#[inline]
pub fn success() -> Rgba {
    pick(0x3fb950, 0x1a7f37, None)
}
#[inline]
pub fn success_bg() -> Rgba {
    pick(0x238636, 0x1f883d, Some(0xd2b16))
}
#[inline]
pub fn success_bg_subtle() -> Rgba {
    pick(0x1b3324, 0xdafbe1, Some(0xd2b16))
}

// --- warning ---
#[inline]
pub fn warning() -> Rgba {
    pick(0xd29922, 0x9a6700, None)
}
#[inline]
pub fn warning_bright() -> Rgba {
    pick(0xf0b429, 0x9a6700, None)
}
#[inline]
pub fn warning_text() -> Rgba {
    pick(0xffd479, 0x9a6700, None)
}
#[inline]
pub fn warning_soft() -> Rgba {
    pick(0xffc861, 0x9a6700, None)
}
#[inline]
pub fn warning_orange() -> Rgba {
    // Former `ORANGE`: poll-restriction labels. Dark value is the exact
    // pre-migration 0xf0883e; light reuses the primer dark-amber text value.
    pick(0xf0883e, 0x9a6700, None)
}
#[inline]
pub fn warning_bg() -> Rgba {
    pick(0x2a2318, 0xfff8c5, Some(0x2e230a))
}
#[inline]
pub fn warning_bg_deep() -> Rgba {
    pick(0x3a2a10, 0xfff8c5, Some(0x2e230a))
}
// --- surfaces ---
#[inline]
pub fn bg_deep() -> Rgba {
    pick(0x0d1117, 0xffffff, Some(0x0))
}
#[inline]
pub fn bg_canvas() -> Rgba {
    pick(0x161b22, 0xf6f8fa, Some(0x0))
}
#[inline]
pub fn bg_subtle() -> Rgba {
    pick(0x21262d, 0xeaeef2, Some(0x0))
}
#[inline]
pub fn bg_tile() -> Rgba {
    pick(0x010409, 0xffffff, Some(0x0))
}
#[inline]
pub fn bg_video() -> Rgba {
    // Video surfaces stay dark in both modes (official-client behavior).
    hex(0x161616)
}
#[inline]
pub fn bg_black() -> Rgba {
    // Deliberate black chips stay black in both modes.
    hex(0x000000)
}
#[inline]
pub fn bg_code() -> Rgba {
    pick(0x22262d, 0xf6f8fa, Some(0x0))
}
#[inline]
pub fn bg_bubble_incoming() -> Rgba {
    pick(0x2d333b, 0xeaeef2, Some(0x1a1a1a))
}
#[inline]
pub fn bg_badge_muted() -> Rgba {
    // Mid-gray badge fill with white text works in both modes.
    hex(0x6e7681)
}
// 0x444c56 doubles as a muted fill and (once) a spoiler-mask text color.
#[inline]
pub fn fill_muted() -> Rgba {
    pick(0x444c56, 0xd0d7de, Some(0x444c56))
}

// --- borders ---
#[inline]
pub fn border() -> Rgba {
    pick(0x30363d, 0xd0d7de, Some(0xffffff))
}
#[inline]
pub fn border_video() -> Rgba {
    // Video chrome stays dark in both modes.
    hex(0x333333)
}

// --- scrims (modal dims) ---
#[inline]
pub fn scrim() -> Rgba {
    pick_a(0x000000e6, 0x000000e6, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// stories-high-contrast: the palette contract — surfaces go pure
    /// black, text/borders go white, semantic hues keep their dark
    /// values (designed for black backgrounds), and flipping the flag
    /// back restores the dark/light tokens. `MODE_DARK` boots dark and
    /// nothing in the test binary flips it, so the restore check is
    /// deterministic.
    #[test]
    fn high_contrast_roots_palette() {
        set_high_contrast(true);
        assert_eq!(bg_canvas(), hex(0x000000));
        assert_eq!(bg_deep(), hex(0x000000));
        assert_eq!(bg_subtle(), hex(0x000000));
        assert_eq!(text_primary(), hex(0xffffff));
        assert_eq!(text_bright(), hex(0xffffff));
        assert_eq!(border(), hex(0xffffff));
        // Links go yellow for maximum pop on black…
        assert_eq!(accent(), hex(0xffd60a));
        // …while fills keep dark values white text stays legible on.
        assert_eq!(accent_strong(), hex(0x0b3d91));
        assert_eq!(danger(), hex(0xf85149));
        assert_eq!(success(), hex(0x3fb950));
        set_high_contrast(false);
        assert_eq!(bg_canvas(), hex(0x161b22));
        assert_eq!(text_primary(), hex(0xc9d1d9));
        assert_eq!(accent(), hex(0x58a6ff));
    }
}
