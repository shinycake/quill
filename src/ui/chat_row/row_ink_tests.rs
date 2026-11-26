use super::{BadgeInk, RowInk, open_row_hover_fill};
use crate::ui::app_theme::kit_colors::apply;
use crate::ui::chat_row::selected_secondary_fg;
use crate::ui::chat_theme::accent;
use gpui_kit::component::theme::ThemeColor;
use gpui_kit::{Hsla, Rgba, hsla};
use quill::app_theme::BuiltinTheme;
use quill::layout_style::SelectedRow;

fn lum(c: Hsla) -> f32 {
    let c = Rgba::from(c);
    let lin = |v: f32| {
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

/// `top` painted over the opaque `under`.
fn over(top: Hsla, under: Hsla) -> Hsla {
    let (t, u) = (Rgba::from(top), Rgba::from(under));
    let mix = |a: f32, b: f32| a * t.a + b * (1. - t.a);
    Rgba {
        r: mix(t.r, u.r),
        g: mix(t.g, u.g),
        b: mix(t.b, u.b),
        a: 1.,
    }
    .into()
}

fn ratio(fg: Hsla, bg: Hsla) -> f32 {
    quill::layout_style::contrast_ratio(lum(over(fg, bg)), lum(bg))
}

/// Every built-in theme in every one of its accents.
fn themes() -> Vec<(String, ThemeColor)> {
    let mut all = Vec::new();
    for theme in BuiltinTheme::ALL {
        let accents = std::iter::once(None).chain(theme.accent_choices().iter().map(|a| Some(*a)));
        for accent in accents {
            let mut colors = ThemeColor::default();
            apply(&mut colors, &theme.palette_with_accent(accent));
            all.push((format!("{theme:?} {accent:x?}"), colors));
        }
    }
    all
}

#[test]
fn the_tint_is_the_colours_rows_drew_before() {
    let colors = ThemeColor::light();
    let rest = hsla(0.1, 0.2, 0.3, 1.);
    let muted = hsla(0.4, 0.5, 0.6, 1.);
    let open = RowInk::of(true, SelectedRow::Tint, rest, muted, &colors);
    assert_eq!(open.fill, colors.primary.opacity(0.25));
    assert_eq!(
        open.open_hover,
        Some((colors.primary.opacity(0.3), colors.primary.opacity(0.4)))
    );
    assert_eq!(open.title, None);
    assert_eq!(open.secondary, selected_secondary_fg(colors.foreground));
    assert_eq!(open.sender, open.secondary);
    assert_eq!(open.activity, colors.primary);
    assert_eq!(open.ring, colors.sidebar);
    assert_eq!(open.badges, BadgeInk::Own);
    for style in [SelectedRow::Tint, SelectedRow::Solid] {
        let other = RowInk::of(false, style, rest, muted, &colors);
        assert_eq!(other.fill, rest);
        assert_eq!(other.open_hover, None);
        assert_eq!(other.title, None);
        assert_eq!(other.secondary, muted);
        assert_eq!(other.sender, Hsla::from(accent()));
        assert_eq!(other.activity, colors.primary);
        assert_eq!(other.ring, colors.sidebar);
        assert_eq!(other.badges, BadgeInk::Own);
    }
    assert_eq!(
        open_row_hover_fill(SelectedRow::Tint, &colors),
        colors.primary.opacity(0.3)
    );
}

#[test]
fn the_solid_row_is_readable_in_every_theme_and_accent() {
    for (name, colors) in themes() {
        let ink = RowInk::of(
            true,
            SelectedRow::Solid,
            colors.sidebar,
            colors.muted_foreground,
            &colors,
        );
        let fill = ink.fill;
        let Some(title) = ink.title else {
            panic!("{name}: the solid row sets its title colour");
        };
        assert!(
            ratio(title, fill) >= 4.5,
            "{name}: title {}",
            ratio(title, fill)
        );
        assert!(
            ratio(ink.secondary, fill) >= 4.5,
            "{name}: secondary {}",
            ratio(ink.secondary, fill)
        );
        let BadgeInk::OnFill {
            fill: pill_text,
            text: pill,
        } = ink.badges
        else {
            panic!("{name}: the badges invert on the solid row");
        };
        assert!(
            ratio(pill_text, pill) >= 4.5,
            "{name}: badge {}",
            ratio(pill_text, pill)
        );
        assert!(
            ratio(pill, fill) >= 3.,
            "{name}: badge on row {}",
            ratio(pill, fill)
        );
    }
}
