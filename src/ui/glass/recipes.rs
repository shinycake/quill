//! Every glass look Quill draws, in one place. A surface names its role;
//! the optics (tint, frost, bend, rim, shadow) live here, so the whole app
//! is tuned in this file.
//!
//! The look is clear glass: one even, lightly blurred tint edge to edge, a
//! rounded bevel at the rim that shows what lies just past the edge with
//! its colours split (after the QuartzCore-style demo), the kit's rim light
//! and top sheen, a two-sided rim specular, and a fine line on dark glass.
//! The bend, blur, specular and frost come from the person's tuning dials
//! (`quill::glass_tuning`); the roles scale them. The contrast test beside
//! it holds the limits for the default dials
//! (docs/decisions/glass-effects.md, "Recipes" and "Tuning").
#![deny(clippy::indexing_slicing)]

use gpui_kit::component::theme::ThemeColor;
use gpui_kit::{
    Hsla, LENS_DESIGN_HEIGHT, LensBend, LensBorder, LensDetail, LensMaterial, LensStyle, LensTune,
    Rgba, point, px,
};
use quill::glass_tuning::{GlassDial, GlassTuning};

/// What a surface is, which decides its look. The list is closed: a new
/// glass surface needs a reason and a place in the budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GlassRole {
    /// Context menus, submenus and the kit's popovers.
    Menu,
    /// Larger floating panels: the main menu grid, the emoji panel, toasts.
    Panel,
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
        (GlassRole::Bar, false) => 0.85,
        (_, false) => 0.83,
        (_, true) => 0.72,
    }
}

/// Dark glass takes this share of the light theme's frost dial: its
/// labels are light on a dark tint, which holds contrast with less (the
/// shipped look had 0.56 light, 0.49 dark). Media glass keeps the whole
/// floor whatever the dial: a video frame can be flat white.
const DARK_FROST: f32 = 0.875;
/// Bevel of slabs (menus, panels, the bar): the shaded band inside the
/// rim. Kept narrow, or a tall slab's corners crease along the diagonals.
const BEVEL: f32 = 24.0;
/// The bar shows only its lower edge over rows of text right under its
/// own labels: half the bend, and a thicker frost and blur than the rest,
/// or the rows ghost through.
const BAR_BEND: f32 = 0.5;
const BAR_MIDDLE_LIGHT: f32 = 0.78;
const BAR_MIDDLE_DARK: f32 = 0.68;
const BAR_BLUR: f32 = 8.0;
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

/// The middle's frost for `role` in a theme, from the frost dial.
fn middle_frost(role: GlassRole, dark: bool, tuning: &GlassTuning) -> f32 {
    let dial = tuning.value(GlassDial::Frost) / 100.0;
    let share = match (is_media(role), dark) {
        (true, _) => 1.0,
        (false, false) => dial,
        (false, true) => dial * DARK_FROST,
    };
    floor(role, dark) * share
}

/// The bevel bend from the dials, `scale` of their pull and split.
fn bevel_bend(tuning: &GlassTuning, scale: f32) -> LensBend {
    LensBend::Bevel {
        refract: px(tuning.value(GlassDial::Refraction) * scale),
        height: px(tuning.value(GlassDial::Bevel)),
        aberration: px(tuning.value(GlassDial::Aberration) * scale),
    }
}

/// The look of `role` in a theme, with the person's dials.
pub(crate) fn recipe(role: GlassRole, colors: &ThemeColor, tuning: &GlassTuning) -> LensStyle {
    let dark = is_dark(colors);
    let frost = middle_frost(role, dark, tuning);
    let blur = px(tuning.value(GlassDial::Blur));
    let specular = tuning.value(GlassDial::Specular);
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
        saturation: SATURATION,
        specular,
        ..LensTune::default()
    };
    let slab_tune = LensTune {
        reflection: SHEEN,
        ..tune
    };
    let slab = |tint: Hsla, shadow: f32, bend: LensBend| LensStyle {
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
            bend,
            frost,
            blur,
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
    // its rim (the bevel shrinks to a small button's half height).
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
            bend: bevel_bend(tuning, 1.0),
            frost,
            blur,
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
        GlassRole::Menu | GlassRole::Panel => slab(colors.popover, 0.12, bevel_bend(tuning, 1.0)),
        // Only the bar's lower edge shows: a quiet rim, or its light
        // reads as a neon line across the list.
        GlassRole::Bar => LensStyle {
            border: None,
            tune: LensTune {
                rim: 0.35,
                fresnel: 0.35,
                specular: specular * 0.5,
                ..slab_tune
            },
            material: LensMaterial::Clear {
                bend: bevel_bend(tuning, BAR_BEND),
                frost: floor(role, dark)
                    * if dark {
                        BAR_MIDDLE_DARK
                    } else {
                        BAR_MIDDLE_LIGHT
                    },
                blur: px(BAR_BLUR.max(tuning.value(GlassDial::Blur))),
                blur_mix: 1.0,
            },
            ..slab(colors.sidebar, 0.0, bevel_bend(tuning, BAR_BEND))
        },
        GlassRole::MediaText => LensStyle {
            border: None,
            ..slab(black(), 0.0, bevel_bend(tuning, 1.0))
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
