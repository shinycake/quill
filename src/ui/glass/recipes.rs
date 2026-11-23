//! Every glass look Quill draws, in one place. A surface names its role;
//! the optics (tint, frost, bend, rim, shadow) live here, so the whole app
//! is tuned in this file. The contrast test beside it holds the limits:
//! over a pure black and a pure white backdrop, primary text on each recipe
//! keeps 4.5:1 and secondary text and icons keep 3:1
//! (docs/decisions/glass-effects.md, "Recipes").
//!
//! The presets follow quill-tools/gpui-liquid-glass (a port of
//! bilalgursen/liquid-glass, MIT): the frosted card for menus, panels and
//! bars, the clear button for controls and media, the tinted button for the
//! one prominent action.
#![deny(clippy::indexing_slicing)]

use gpui_kit::component::theme::ThemeColor;
use gpui_kit::{
    Hsla, LENS_DESIGN_HEIGHT, LensBend, LensDetail, LensMaterial, LensStyle, LensTune, Rgba,
    point, px,
};

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
    /// The one accent-tinted control of a view (send).
    Prominent,
}

impl GlassRole {
    /// Whether the role carries text (4.5:1) or only icons (3:1).
    pub(crate) fn holds_text(self) -> bool {
        self != GlassRole::MediaIcon
    }

    pub(crate) const ALL: [GlassRole; 8] = [
        GlassRole::Menu,
        GlassRole::Panel,
        GlassRole::Control,
        GlassRole::MediaIcon,
        GlassRole::MediaText,
        GlassRole::Bar,
        GlassRole::Selection,
        GlassRole::Prominent,
    ];
}

/// The labels a recipe has to keep readable: primary text and the
/// secondary text and icons.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Labels {
    pub(crate) primary: Hsla,
    pub(crate) secondary: Hsla,
}

/// Frost (how much of the tint covers the backdrop) per recipe. Larger
/// surfaces carry more tint to stay legible.
const MENU_FROST: f32 = 0.86;
const PANEL_FROST: f32 = 0.90;
const CONTROL_FROST: f32 = 0.84;
const MEDIA_ICON_FROST: f32 = 0.45;
const MEDIA_TEXT_FROST: f32 = 0.55;
const BAR_FROST: f32 = 0.84;

/// The look of `role` in a theme.
pub(crate) fn recipe(role: GlassRole, colors: &ThemeColor) -> LensStyle {
    match role {
        GlassRole::Menu => slab(colors.popover, MENU_FROST, 18.0, 10.0, 0.22),
        GlassRole::Panel => slab(colors.popover, PANEL_FROST, 22.0, 14.0, 0.26),
        GlassRole::Bar => LensStyle {
            shadow: 0.0,
            ..slab(colors.sidebar, BAR_FROST, 18.0, 8.0, 0.0)
        },
        GlassRole::Control => button(colors.background, CONTROL_FROST, 4.0),
        GlassRole::MediaIcon => button(black(), MEDIA_ICON_FROST, 6.0),
        GlassRole::MediaText => LensStyle {
            shadow: 0.0,
            ..slab(black(), MEDIA_TEXT_FROST, 10.0, 8.0, 0.0)
        },
        GlassRole::Selection => button(colors.background, CONTROL_FROST, 4.0),
        GlassRole::Prominent => LensStyle {
            material: LensMaterial::Tinted { edge_clear: 0.2 },
            ..button(colors.primary, 0.0, 0.0)
        },
    }
}

/// The labels `role` draws in a theme.
pub(crate) fn labels(role: GlassRole, colors: &ThemeColor) -> Labels {
    match role {
        GlassRole::Menu | GlassRole::Panel => Labels {
            primary: colors.popover_foreground,
            secondary: colors.muted_foreground,
        },
        GlassRole::Control => Labels {
            primary: colors.foreground,
            secondary: colors.muted_foreground,
        },
        GlassRole::Bar => Labels {
            primary: colors.sidebar_foreground,
            secondary: colors.muted_foreground,
        },
        GlassRole::MediaIcon => Labels {
            primary: gpui_kit::white(),
            secondary: gpui_kit::white(),
        },
        GlassRole::MediaText => Labels {
            primary: gpui_kit::white(),
            secondary: Hsla {
                a: 0.8,
                ..gpui_kit::white()
            },
        },
        GlassRole::Selection => Labels {
            primary: colors.foreground,
            secondary: colors.muted_foreground,
        },
        GlassRole::Prominent => Labels {
            primary: colors.primary_foreground,
            secondary: colors.primary_foreground,
        },
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

/// No top reflection and no caustic: both darken or tint the middle, where
/// the labels sit, and the contrast arithmetic counts only the frost.
fn quiet_tune() -> LensTune {
    LensTune {
        reflection: 0.0,
        caustic: 0.0,
        inner: 0.5,
        ..LensTune::default()
    }
}

/// The frosted card: a heavy frost of `tint` with a soft blur, bent only at
/// the rim (so text behind never ripples through rows), a rim light and a
/// drop shadow. Lengths are logical pixels.
fn slab(tint: Hsla, frost: f32, blur: f32, bevel: f32, shadow: f32) -> LensStyle {
    LensStyle {
        unit: 1.0,
        bevel,
        band: None,
        shadow,
        shadow_softness: 22.0,
        shadow_offset: 8.0,
        strength: 0.6,
        color: opaque(tint),
        surface: opaque(tint),
        material: LensMaterial::Clear {
            bend: LensBend::Radial { amount: px(3.0) },
            frost,
            blur: px(blur),
            blur_mix: 1.0,
        },
        border: None,
        tune: quiet_tune(),
        light: point(0.0, -1.0),
        light_mix: 0.0,
        droplet: None,
        blob: None,
        detail: LensDetail::Copied,
    }
}

/// The clear button: a dome that bends the backdrop at its rim, frosted
/// with `tint`. Sized by the button's height.
fn button(tint: Hsla, frost: f32, blur: f32) -> LensStyle {
    LensStyle {
        unit: 0.0,
        bevel: LENS_DESIGN_HEIGHT * 0.42,
        band: None,
        shadow: 0.10,
        shadow_softness: 24.0,
        shadow_offset: 12.0,
        strength: 0.8,
        color: opaque(tint),
        surface: opaque(tint),
        material: LensMaterial::Clear {
            bend: LensBend::Edge {
                refract: 30.0,
                magnify: 0.0,
                dispersion: 0.06,
                blur_center: 1.0,
                blur_edge: 2.2,
            },
            frost,
            blur: px(blur),
            blur_mix: 1.0,
        },
        border: None,
        tune: quiet_tune(),
        light: point(0.0, -1.0),
        light_mix: 0.0,
        droplet: None,
        blob: None,
        detail: LensDetail::Copied,
    }
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

/// The colour the middle of the glass settles to over a flat `backdrop`:
/// the frost mixes the tint over it in the sRGB-encoded space the optics
/// work in; tinted glass is its own colour.
pub(crate) fn settled_over(style: &LensStyle, backdrop: Rgba) -> Rgba {
    let tint = Rgba::from(style.color);
    match style.material {
        LensMaterial::Tinted { .. } => Rgba { a: 1.0, ..tint },
        LensMaterial::Clear { frost, .. } => {
            let mix = |b: f32, t: f32| b + (t - b) * frost.clamp(0.0, 1.0);
            Rgba {
                r: mix(backdrop.r, tint.r),
                g: mix(backdrop.g, tint.g),
                b: mix(backdrop.b, tint.b),
                a: 1.0,
            }
        }
    }
}

/// WCAG contrast of `fg` (blended over `bg` by its alpha) against `bg`.
pub(crate) fn contrast(fg: Hsla, bg: Rgba) -> f32 {
    let fg = Rgba::from(fg);
    let blend = |f: f32, b: f32| b + (f - b) * fg.a.clamp(0.0, 1.0);
    let shown = Rgba {
        r: blend(fg.r, bg.r),
        g: blend(fg.g, bg.g),
        b: blend(fg.b, bg.b),
        a: 1.0,
    };
    let (a, b) = (luminance(shown), luminance(bg));
    let (hi, lo) = if a > b { (a, b) } else { (b, a) };
    (hi + 0.05) / (lo + 0.05)
}

fn luminance(color: Rgba) -> f32 {
    let channel = |c: f32| {
        let c = c.clamp(0.0, 1.0);
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(color.r) + 0.7152 * channel(color.g) + 0.0722 * channel(color.b)
}

#[cfg(test)]
#[path = "recipes_tests.rs"]
mod tests;
