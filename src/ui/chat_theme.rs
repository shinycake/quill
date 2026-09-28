//! Named color tokens for the Quill UI (Phase 0 of the kit-first UI cleanup).
//!
//! Every hardcoded `rgb(0x…)` / `rgba(0x…)` in `src/ui` now resolves to one of
//! these constants. Values are the exact pre-migration colors, so this is a
//! pure rename with zero visual change.
//!
//! Why plain constants instead of `cx.theme()` tokens: the kit's default dark
//! theme is zinc-based while Quill's palette is GitHub-dark — mapping onto
//! kit `ThemeColor`s here would silently shift hues (e.g. links blue → white).
//! Deliberate palette moves belong to the later delight-tuning phases, not to
//! the mechanical migration. Kit components adopted in later phases read
//! `cx.theme()` internally, which is a separate, reviewable change.
//!
//! Phase 8 (light theme) will make these mode-aware; until then the app is
//! dark-only.

use gpui_kit::Rgba;

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

// --- accent ---
pub const ACCENT: Rgba = hex(0x58a6ff);
pub const ACCENT_STRONG: Rgba = hex(0x1f6feb);
pub const ACCENT_LIGHT: Rgba = hex(0x2f81f7);
pub const ACCENT_INFO: Rgba = hex(0x9ecbff);

// --- text ---
pub const TEXT_BRIGHT: Rgba = hex(0xffffff);
pub const TEXT_PRIMARY: Rgba = hex(0xc9d1d9);
pub const TEXT_MUTED: Rgba = hex(0x8b949e);
pub const TEXT_FAINT: Rgba = hex(0x9a9a9a);
pub const TEXT_MENU: Rgba = hex(0xe6edf3);

// --- danger ---
pub const DANGER: Rgba = hex(0xf85149);
pub const DANGER_SOFT: Rgba = hex(0xe17076);
pub const DANGER_BRIGHT: Rgba = hex(0xff7b72);
pub const DANGER_PALE: Rgba = hex(0xff8a8a);
pub const DANGER_VIVID: Rgba = hex(0xff6b6b);
pub const DANGER_DARK: Rgba = hex(0xd44a3a);
pub const DANGER_BG: Rgba = hex(0x3d1f1f);
pub const DANGER_BG_DEEP: Rgba = hex(0x3a1414);

// --- success ---
pub const SUCCESS: Rgba = hex(0x3fb950);
pub const SUCCESS_BG: Rgba = hex(0x238636);
pub const SUCCESS_BG_SUBTLE: Rgba = hex(0x1b3324);

// --- warning ---
pub const WARNING: Rgba = hex(0xd29922);
pub const WARNING_BRIGHT: Rgba = hex(0xf0b429);
pub const WARNING_TEXT: Rgba = hex(0xffd479);
pub const WARNING_SOFT: Rgba = hex(0xffc861);
pub const WARNING_BG: Rgba = hex(0x2a2318);
pub const WARNING_BG_DEEP: Rgba = hex(0x3a2a10);
pub const ORANGE: Rgba = hex(0xf0883e);

// --- surfaces ---
pub const BG_DEEP: Rgba = hex(0x0d1117);
pub const BG_CANVAS: Rgba = hex(0x161b22);
pub const BG_SUBTLE: Rgba = hex(0x21262d);
pub const BG_TILE: Rgba = hex(0x010409);
pub const BG_VIDEO: Rgba = hex(0x161616);
pub const BG_BLACK: Rgba = hex(0x000000);
pub const BG_CODE: Rgba = hex(0x22262d);
pub const BG_BUBBLE_INCOMING: Rgba = hex(0x2d333b);
pub const BG_BADGE_MUTED: Rgba = hex(0x6e7681);
pub const BG_PREMIUM: Rgba = hex(0x8250df);
// 0x444c56 doubles as a muted fill and (once) a spoiler-mask text color.
pub const FILL_MUTED: Rgba = hex(0x444c56);

// --- borders ---
pub const BORDER: Rgba = hex(0x30363d);
pub const BORDER_VIDEO: Rgba = hex(0x333333);

// --- scrims (modal dims) ---
pub const SCRIM: Rgba = hex_a(0x000000e6);
pub const SCRIM_LIGHT: Rgba = hex_a(0x00000099);
