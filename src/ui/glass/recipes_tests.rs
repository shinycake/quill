//! The hard limit on every recipe: over a pure black and a pure white
//! backdrop, primary text keeps 4.5:1 and secondary text and icons 3:1, in
//! the light and dark themes. A role that holds only icons keeps 3:1.
//! (High contrast never draws glass: it resolves to the solid look,
//! `quill::glass::resolve`.)

use super::*;

impl GlassRole {
    /// Whether the role carries text (4.5:1) or only icons (3:1).
    fn holds_text(self) -> bool {
        self != GlassRole::MediaIcon
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

fn backdrops() -> [(&'static str, Rgba); 2] {
    [
        (
            "black",
            Rgba {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
        ),
        (
            "white",
            Rgba {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            },
        ),
    ]
}

fn check(theme: &str, colors: &ThemeColor) {
    let mut failures = Vec::new();
    for role in GlassRole::ALL {
        let style = recipe(role, colors);
        let labels = labels(role, colors);
        for (name, backdrop) in backdrops() {
            let glass = settled_over(&style, backdrop);
            let primary = contrast(labels.primary, glass);
            let secondary = contrast(labels.secondary, glass);
            let floor = if role.holds_text() { 4.5 } else { 3.0 };
            if primary < floor {
                failures.push(format!(
                    "{theme} {role:?} over {name}: primary {primary:.2}:1"
                ));
            }
            if secondary < 3.0 {
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
fn larger_surfaces_carry_more_tint_than_buttons() {
    let frost = |role| match recipe(role, &ThemeColor::light()).material {
        LensMaterial::Clear { frost, .. } => frost,
        LensMaterial::Tinted { .. } => 1.0,
    };
    assert!(frost(GlassRole::Panel) > frost(GlassRole::Menu));
    assert!(frost(GlassRole::Menu) > frost(GlassRole::Control));
    assert!(frost(GlassRole::MediaText) > frost(GlassRole::MediaIcon));
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
