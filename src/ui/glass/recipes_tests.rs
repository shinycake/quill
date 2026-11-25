//! The limits on every recipe, in the light and dark themes:
//!
//! - over a mid grey (what the blur makes of mixed content behind
//!   the labels), primary text keeps 4.5:1 and secondary text and icons
//!   3:1;
//! - over a flat pure black or pure white backdrop, primary text still
//!   keeps 3:1 (WCAG's floor for large text and controls);
//! - media glass, which sits over video that can be a flat white frame,
//!   keeps the full rule over pure black and white. A role that holds
//!   only icons keeps 3:1.
//!
//! (High contrast never draws glass: it resolves to the solid look,
//! `quill::glass::resolve`.)

use super::*;

impl GlassRole {
    /// Whether the role carries text (4.5:1) or only icons (3:1).
    fn holds_text(self) -> bool {
        self != GlassRole::MediaIcon
    }

    fn is_media(self) -> bool {
        is_media(self)
    }

    const ALL: [GlassRole; 7] = [
        GlassRole::Menu,
        GlassRole::Panel,
        GlassRole::Control,
        GlassRole::MediaIcon,
        GlassRole::MediaText,
        GlassRole::Bar,
        GlassRole::Selection,
    ];
}

/// The labels a recipe has to keep readable: primary text and the
/// secondary text and icons.
#[derive(Clone, Copy, Debug)]
struct Labels {
    primary: Hsla,
    secondary: Hsla,
}

/// The labels `role` draws in a theme.
fn labels(role: GlassRole, colors: &ThemeColor) -> Labels {
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
    }
}

/// The colour the middle of the glass settles to over a flat `backdrop`:
/// the frost mixes the tint over it in the sRGB-encoded space the optics
/// work in; tinted glass is its own colour.
fn settled_over(style: &LensStyle, backdrop: Rgba) -> Rgba {
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
fn contrast(fg: Hsla, bg: Rgba) -> f32 {
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

fn grey(v: f32) -> Rgba {
    Rgba {
        r: v,
        g: v,
        b: v,
        a: 1.0,
    }
}

/// The backdrops a role is held over, each with the floors for its
/// primary and secondary labels.
fn backdrops(role: GlassRole) -> Vec<(&'static str, Rgba, f32, f32)> {
    let full = if role.holds_text() { 4.5 } else { 3.0 };
    let mut list = vec![("mid grey", grey(0.5), full, 3.0)];
    if role.is_media() {
        list.push(("black", grey(0.0), full, 3.0));
        list.push(("white", grey(1.0), full, 3.0));
    } else {
        list.push(("black", grey(0.0), 3.0, 0.0));
        list.push(("white", grey(1.0), 3.0, 0.0));
    }
    list
}

fn check(theme: &str, colors: &ThemeColor) {
    let mut failures = Vec::new();
    for role in GlassRole::ALL {
        let style = recipe(role, colors);
        let labels = labels(role, colors);
        for (name, backdrop, primary_floor, secondary_floor) in backdrops(role) {
            let glass = settled_over(&style, backdrop);
            let primary = contrast(labels.primary, glass);
            let secondary = contrast(labels.secondary, glass);
            if primary < primary_floor {
                failures.push(format!(
                    "{theme} {role:?} over {name}: primary {primary:.2}:1"
                ));
            }
            if secondary < secondary_floor {
                failures.push(format!(
                    "{theme} {role:?} over {name}: secondary {secondary:.2}:1"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn every_recipe_keeps_its_labels_readable_in_light() {
    check("light", &ThemeColor::light());
}

#[test]
fn every_recipe_keeps_its_labels_readable_in_dark() {
    check("dark", &ThemeColor::dark());
}

#[test]
fn the_contrast_arithmetic_matches_wcag() {
    let black = Hsla::from(Rgba {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    });
    let white = Rgba {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    assert!((contrast(black, white) - 21.0).abs() < 0.01);
}
