//! Every glass look Quill draws, in one place. A surface names its role;
//! the optics (tint, frost, bend, rim, shadow) live here, so the whole app
//! is tuned in this file.
//!
//! The look is clear glass after the source kit's frosted card: one even,
//! lightly blurred tint edge to edge, a thin band at the rim that bends
//! the backdrop in, the kit's rim light and top sheen, and a fine line on
//! dark glass. The contrast test beside it holds the limits
//! (docs/decisions/glass-effects.md, "Recipes").
#![deny(clippy::indexing_slicing)]

use gpui_kit::component::theme::ThemeColor;
use gpui_kit::{
    Hsla, LENS_DESIGN_HEIGHT, LensBend, LensBorder, LensDetail, LensMaterial, LensStyle, LensTune,
    Rgba, point, px,
};

/// What a surface is, which decides its look. The list is closed: a new
/// glass surface needs a reason and a place in the budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GlassRole {
    /// Context menus, submenus and the kit's popovers.
    Menu,
    /// Larger floating panels: the main menu grid, the emoji panel, toasts.
    Panel,
    /// Floating's cards (the chat list, the side panels, the header and
    /// composer pills): large and always on screen, so a quiet rim, a pull
    /// gathered at the edge, and the bar's frost under their rows.
    Card,
    /// Floating buttons over the history: jump buttons, the date pill.
    Control,
    /// Buttons over photos and video (viewer, stories, calls): always the
    /// dark look with white labels.
    MediaIcon,
    /// Plates that hold text over photos and video (captions).
    MediaText,
    /// Bars over the chat list (the sidebar header, folder strip).
    Bar,
    /// The selected tab's capsule (segmented tabs): frosted at rest,
    /// clearing while it slides ([`sliding`]).
    Selection,
}

/// The frost each role would need in the middle for its labels to keep
/// 4.5:1 (secondary text and icons 3:1) over a flat pure black and pure
/// white backdrop, rounded up from what the recipe test finds.
fn floor(role: GlassRole, dark: bool) -> f32 {
    match (role, dark) {
        (GlassRole::MediaIcon, _) => 0.44,
        (GlassRole::MediaText, _) => 0.56,
        (GlassRole::Bar | GlassRole::Card, false) => 0.85,
        (_, false) => 0.83,
        (_, true) => 0.72,
    }
}

/// The middle's frost as a share of the floor, in the light and dark
/// themes. Well below 1: the glass is as clear as liquid (Idan picked the
/// clearest of three steps, 2026-11-25), and the recipe test holds the
/// rule over mixed content (a mid grey) only. Light glass keeps a little
/// more for its grey secondary text. Media glass keeps the whole floor: a
/// video frame can be flat white.
const MIDDLE_LIGHT: f32 = 0.56;
const MIDDLE_DARK: f32 = 0.49;
/// The frost's blur on slabs; domes show the backdrop sharp.
const BLUR: f32 = 2.0;
const DOME_BLUR: f32 = 0.0;
/// Bevel of slabs (menus, panels, the bar): the shaded band inside the
/// rim. Kept narrow, or a tall slab's corners crease along the diagonals.
const BEVEL: f32 = 24.0;
/// How far the rim pulls the backdrop in, and how deep into the slab the
/// pull reaches: a deep lens that curves what is behind it like water.
const BEND: f32 = 80.0;
const BEND_CURVE: f32 = 4.0;
const BEND_DEPTH: f32 = 1.7;
/// The bar shows only its lower edge: a shallow pull gathered at the rim.
const BAR_BEND: f32 = 30.0;
const BAR_BEND_CURVE: f32 = 9.75;
const BAR_BEND_DEPTH: f32 = 1.35;
/// The bar sits over rows of text right under its own labels, so it keeps
/// a thicker frost and blur than the rest, or the rows ghost through.
const BAR_MIDDLE_LIGHT: f32 = 0.78;
const BAR_MIDDLE_DARK: f32 = 0.68;
const BAR_BLUR: f32 = 8.0;
/// Floating's cards run the window's height with rows of text on them:
/// the liquid look with a pull gathered at the rim (the bar's), so a card
/// curves what is behind its edges without warping the middle, a quiet rim
/// light (no glowing frame), and the bar's frost, so rows stay readable
/// over a busy wallpaper.
const CARD_BEND: f32 = 30.0;
const CARD_RIM: f32 = 0.35;
/// Vibrancy of the bent backdrop, and the top sheen of slabs.
const SATURATION: f32 = 1.3;
const SHEEN: f32 = 1.8;
/// Alpha and width of the light line inside the edge of dark glass.
const HAIRLINE: f32 = 0.24;
const HAIRLINE_WIDTH: f32 = 1.3;

fn is_dark(colors: &ThemeColor) -> bool {
    colors.background.l < 0.5
}

fn is_media(role: GlassRole) -> bool {
    matches!(role, GlassRole::MediaIcon | GlassRole::MediaText)
}

/// The middle's frost for `role` in a theme.
fn middle_frost(role: GlassRole, dark: bool) -> f32 {
    let share = match (is_media(role), dark) {
        (true, _) => 1.0,
        (false, false) => MIDDLE_LIGHT,
        (false, true) => MIDDLE_DARK,
    };
    floor(role, dark) * share
}

/// The look of `role` in a theme.
pub(crate) fn recipe(role: GlassRole, colors: &ThemeColor) -> LensStyle {
    let dark = is_dark(colors);
    let frost = middle_frost(role, dark);
    // Dark glass and glass over media take a light hairline (the lit
    // edge); light glass takes the theme's border, or it melts into a
    // light window.
    let hairline = LensBorder {
        color: if dark || is_media(role) {
            Hsla {
                a: HAIRLINE,
                ..gpui_kit::white()
            }
        } else {
            colors.border
        },
        width: px(if dark || is_media(role) {
            HAIRLINE_WIDTH
        } else {
            1.0
        }),
    };
    // The source kit's lighting (rim light, fresnel, top sheen, caustic)
    // with a faint inner shade (stronger, it greys light glass and creases
    // a tall slab's corners), and an even frost edge to edge: a clear
    // ring around a frosted middle reads as a thick border.
    let tune = LensTune {
        inner: 0.25,
        ..LensTune::default()
    };
    let slab_tune = LensTune {
        bend_curve: BEND_CURVE,
        bend_depth: BEND_DEPTH,
        saturation: SATURATION,
        reflection: SHEEN,
        ..tune
    };
    let slab = |tint: Hsla, shadow: f32, bend: f32| LensStyle {
        unit: 1.0,
        bevel: BEVEL,
        band: None,
        shadow,
        shadow_softness: 10.0,
        shadow_offset: 4.0,
        strength: 1.0,
        color: opaque(tint),
        surface: opaque(tint),
        material: LensMaterial::Clear {
            bend: LensBend::Radial { amount: px(bend) },
            frost,
            blur: px(BLUR),
            blur_mix: 1.0,
        },
        border: Some(hairline),
        tune: slab_tune,
        light: point(0.0, -1.0),
        light_mix: 0.0,
        droplet: None,
        blob: None,
        detail: LensDetail::Copied,
    };
    // A clear dome sized by the button's height, bending the backdrop at
    // its rim.
    let dome = |tint: Hsla| LensStyle {
        unit: 0.0,
        bevel: LENS_DESIGN_HEIGHT * 0.42,
        band: None,
        shadow: 0.10,
        shadow_softness: 14.0,
        shadow_offset: 5.0,
        strength: 1.0,
        color: opaque(tint),
        surface: opaque(tint),
        material: LensMaterial::Clear {
            bend: LensBend::Edge {
                refract: 64.0,
                magnify: 0.18,
                dispersion: 0.25,
                blur_center: 0.0,
                blur_edge: 1.0,
            },
            frost,
            blur: px(DOME_BLUR),
            blur_mix: 1.0,
        },
        border: None,
        tune,
        light: point(0.0, -1.0),
        light_mix: 0.0,
        droplet: None,
        blob: None,
        detail: LensDetail::Copied,
    };
    match role {
        GlassRole::Menu | GlassRole::Panel => slab(colors.popover, 0.12, BEND),
        GlassRole::Card => LensStyle {
            tune: LensTune {
                rim: CARD_RIM,
                fresnel: CARD_RIM,
                bend_curve: BAR_BEND_CURVE,
                bend_depth: BAR_BEND_DEPTH,
                ..slab_tune
            },
            material: LensMaterial::Clear {
                bend: LensBend::Radial {
                    amount: px(CARD_BEND),
                },
                frost: floor(role, dark)
                    * if dark {
                        BAR_MIDDLE_DARK
                    } else {
                        BAR_MIDDLE_LIGHT
                    },
                blur: px(BAR_BLUR),
                blur_mix: 1.0,
            },
            ..slab(colors.sidebar, 0.08, CARD_BEND)
        },
        // Only the bar's lower edge shows: a quiet rim, or its light
        // reads as a neon line across the list.
        GlassRole::Bar => LensStyle {
            border: None,
            tune: LensTune {
                rim: 0.35,
                fresnel: 0.35,
                bend_curve: BAR_BEND_CURVE,
                bend_depth: BAR_BEND_DEPTH,
                ..slab_tune
            },
            material: LensMaterial::Clear {
                bend: LensBend::Radial {
                    amount: px(BAR_BEND),
                },
                frost: floor(role, dark)
                    * if dark {
                        BAR_MIDDLE_DARK
                    } else {
                        BAR_MIDDLE_LIGHT
                    },
                blur: px(BAR_BLUR),
                blur_mix: 1.0,
            },
            ..slab(colors.sidebar, 0.0, BAR_BEND)
        },
        GlassRole::MediaText => LensStyle {
            border: None,
            ..slab(black(), 0.0, BEND)
        },
        GlassRole::Control | GlassRole::Selection => dome(colors.background),
        GlassRole::MediaIcon => dome(black()),
    }
}

fn black() -> Hsla {
    Rgba {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    }
    .into()
}

fn opaque(color: Hsla) -> Hsla {
    Hsla { a: 1.0, ..color }
}

/// The selection while it slides: `travel` (0 at rest, 1 a whole tab
/// away) takes the frost away and deepens the bend, so the backdrop shows
/// through the moving lens; at rest it is the recipe as is.
pub(crate) fn sliding(style: LensStyle, travel: f32) -> LensStyle {
    let travel = travel.clamp(0.0, 1.0);
    let material = match style.material {
        LensMaterial::Clear {
            bend,
            frost,
            blur,
            blur_mix,
        } => LensMaterial::Clear {
            bend,
            frost: frost * (1.0 - travel),
            blur,
            blur_mix: blur_mix * (1.0 - travel),
        },
        tinted => tinted,
    };
    LensStyle { material, ..style }
}

#[cfg(test)]
#[path = "recipes_tests.rs"]
mod tests;
