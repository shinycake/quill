//! Secondary text colour on the selected chat row.

use gpui_kit::Hsla;

/// Opacity of the foreground used for secondary text (time, preview) on
/// the selected chat row.
const SELECTED_SECONDARY_ALPHA: f32 = 0.78;

/// Secondary text colour on the selected row's tinted fill: the theme
/// foreground, softened but kept above WCAG AA (4.5:1) on that fill in
/// both themes (the grey `muted_foreground` is only 3.2:1 there).
pub(crate) fn selected_secondary_fg(foreground: Hsla) -> Hsla {
    foreground.opacity(SELECTED_SECONDARY_ALPHA)
}

#[cfg(test)]
mod selected_row_tests {
    use gpui_kit::{Hsla, Rgba, rgb};

    fn blend(top: Rgba, a: f32, under: Rgba) -> Rgba {
        Rgba {
            r: top.r * a + under.r * (1. - a),
            g: top.g * a + under.g * (1. - a),
            b: top.b * a + under.b * (1. - a),
            a: 1.,
        }
    }
    fn lum(c: Rgba) -> f32 {
        let f = |v: f32| {
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b)
    }
    fn ratio(a: Rgba, b: Rgba) -> f32 {
        let (x, y) = (lum(a), lum(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    /// Selected-row secondary text on the 25% primary tint, both themes.
    #[test]
    fn selected_secondary_text_meets_aa() {
        // (foreground, sidebar, primary) of the default light and dark themes.
        for (fg, sidebar, primary) in [
            (0x1f2328, 0xfafafa, 0x0969da),
            (0xe6edf3, 0x22272e, 0x2f81f7),
        ] {
            let fgh: Hsla = rgb(fg).into();
            let text: Rgba = super::selected_secondary_fg(fgh).into();
            let tint = blend(rgb(primary), 0.25, rgb(sidebar));
            let shown = blend(text, text.a, tint);
            assert!(
                ratio(shown, tint) >= 4.5,
                "fg {fg:x}: {}",
                ratio(shown, tint)
            );
        }
    }
}
