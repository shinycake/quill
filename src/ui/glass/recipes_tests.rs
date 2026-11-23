//! The hard limit on every recipe: over a pure black and a pure white
//! backdrop, primary text keeps 4.5:1 and secondary text and icons 3:1, in
//! the light and dark themes. A role that holds only icons keeps 3:1.
//! (High contrast never draws glass: it resolves to the solid look,
//! `quill::glass::resolve`.)

use super::*;

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
                failures.push(format!("{theme} {role:?} over {name}: primary {primary:.2}:1"));
            }
            if secondary < 3.0 {
                failures.push(format!("{theme} {role:?} over {name}: secondary {secondary:.2}:1"));
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
