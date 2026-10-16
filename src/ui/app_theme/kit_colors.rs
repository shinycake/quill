//! The kit's colours (`gpui_kit` `ThemeColor`) from an app theme's
//! palette, so kit components follow the theme along with Quill's own
//! tokens. Semantic colours the palette has no key for (success,
//! warning, info, charts, the base hues) keep the kit's light or dark
//! values.

use gpui_kit::component::theme::ThemeColor;
use gpui_kit::{Hsla, Rgba};
use quill::app_theme::{Palette, PaletteKey};

/// 0xRRGGBBAA as a kit colour.
pub(crate) fn hsla(rgba: u32) -> Hsla {
    Rgba {
        r: ((rgba >> 24) & 0xff) as f32 / 255.,
        g: ((rgba >> 16) & 0xff) as f32 / 255.,
        b: ((rgba >> 8) & 0xff) as f32 / 255.,
        a: (rgba & 0xff) as f32 / 255.,
    }
    .into()
}

fn lighter(color: Hsla, by: f32) -> Hsla {
    Hsla {
        l: (color.l + by).clamp(0., 1.),
        ..color
    }
}

fn rgb_of(color: Hsla) -> u32 {
    let rgba = Rgba::from(color);
    let byte = |v: f32| (v.clamp(0., 1.) * 255.).round() as u32;
    (byte(rgba.r) << 16) | (byte(rgba.g) << 8) | byte(rgba.b)
}

/// `color` moved in lightness away from `against` until it reaches WCAG AA
/// (4.5:1), alpha kept.
fn readable_hsla(color: Hsla, against: Hsla) -> Hsla {
    let rgb = quill::theme_colorize::readable(
        rgb_of(color),
        rgb_of(against),
        quill::theme_colorize::WCAG_AA,
    );
    Hsla {
        a: color.a,
        ..Hsla::from(Rgba {
            r: ((rgb >> 16) & 0xff) as f32 / 255.,
            g: ((rgb >> 8) & 0xff) as f32 / 255.,
            b: (rgb & 0xff) as f32 / 255.,
            a: 1.,
        })
    }
}

/// Destructive colours keep WCAG AA whatever the source red is (the kit's
/// own red is 3.8:1 on white, Day's attention red 3.8:1 too): the text
/// colour against the background, the button fill against its label.
/// Hover and pressed states follow the corrected colours.
pub(crate) fn ensure_danger_aa(colors: &mut ThemeColor, dark: bool) {
    let step = if dark { 0.05 } else { -0.04 };
    // Outline danger buttons draw this text on a wash of the same red
    // (0.1 idle, 0.2 hovered): correct against the hovered wash, the worst case.
    let wash = |danger: Hsla| {
        let (d, b) = (Rgba::from(danger), Rgba::from(colors.background));
        let mix = |x: f32, y: f32| x * 0.2 + y * 0.8;
        Hsla::from(Rgba {
            r: mix(d.r, b.r),
            g: mix(d.g, b.g),
            b: mix(d.b, b.b),
            a: 1.,
        })
    };
    let on_background = readable_hsla(colors.danger, colors.background);
    colors.danger = readable_hsla(on_background, wash(on_background));
    colors.danger_hover = lighter(colors.danger, step);
    colors.danger_active = lighter(colors.danger, if dark { 0.06 } else { -0.06 });
    colors.button_danger = readable_hsla(colors.button_danger, colors.button_danger_foreground);
    colors.button_danger_hover = lighter(colors.button_danger, step);
    colors.button_danger_active = lighter(colors.button_danger, if dark { 0.06 } else { -0.06 });
}

/// Write `palette` into the kit colours.
pub(crate) fn apply(colors: &mut ThemeColor, palette: &Palette) {
    use PaletteKey as K;
    let c = |key: K| hsla(palette.rgba(key));
    let dark = palette.is_dark();
    let bg = c(K::WindowBg);
    let fg = c(K::WindowFg);
    let over = c(K::WindowBgOver);
    let ripple = c(K::WindowBgRipple);
    // Secondary text keeps WCAG AA on the window background even where a
    // palette's own gray is lighter (Day's #999999 on white).
    let sub = {
        let raw = palette.rgba(K::WindowSubTextFg);
        let fg = quill::theme_colorize::readable(
            raw >> 8,
            palette.rgba(K::WindowBg) >> 8,
            quill::theme_colorize::WCAG_AA,
        );
        hsla((fg << 8) | (raw & 0xff))
    };
    let active = c(K::WindowBgActive);
    let on_active = c(K::WindowFgActive);
    let link = c(K::WindowActiveTextFg);
    let border = c(K::InputBorderFg);
    let separator = c(K::MenuSeparatorFg);
    let panel = c(K::FilterInputInactiveBg);
    // Pressing goes darker on day themes and lighter at night.
    let pressed = |color: Hsla| lighter(color, if dark { 0.06 } else { -0.06 });

    colors.background = bg;
    colors.foreground = fg;
    colors.caret = fg;
    colors.muted = over;
    colors.muted_foreground = sub;
    colors.border = border;
    colors.input = border;
    colors.ring = active;
    colors.selection = active.opacity(0.35);
    colors.drag_border = active;
    colors.drop_target = active.opacity(0.25);

    colors.accent = over;
    colors.accent_foreground = fg;
    colors.secondary = over;
    colors.secondary_hover = ripple;
    colors.secondary_active = ripple;
    colors.secondary_foreground = fg;

    colors.primary = active;
    colors.primary_hover = lighter(active, if dark { 0.05 } else { -0.04 });
    colors.primary_active = pressed(active);
    colors.primary_foreground = on_active;
    let button = c(K::ActiveButtonBg);
    colors.button_primary = button;
    colors.button_primary_hover = c(K::ActiveButtonBgOver);
    colors.button_primary_active = pressed(button);
    colors.button_primary_foreground = c(K::ActiveButtonFg);
    colors.button = bg;
    colors.button_hover = over;
    colors.button_active = ripple;
    colors.button_foreground = fg;
    colors.button_secondary = over;
    colors.button_secondary_hover = ripple;
    colors.button_secondary_active = pressed(ripple);
    colors.button_secondary_foreground = fg;

    colors.link = link;
    colors.link_hover = link;
    colors.link_active = link;

    colors.popover = c(K::MenuBg);
    colors.popover_foreground = fg;
    colors.accordion = bg;
    colors.group_box = panel;
    colors.group_box_foreground = fg;
    colors.description_list_label = panel;
    colors.description_list_label_foreground = sub;
    colors.skeleton = over;

    colors.list = bg;
    colors.list_even = bg;
    colors.list_head = bg;
    colors.list_hover = over;
    colors.list_active = active.opacity(0.15);
    colors.list_active_border = active;
    colors.table = bg;
    colors.table_even = bg;
    colors.table_head = panel;
    colors.table_head_foreground = sub;
    colors.table_foot = panel;
    colors.table_foot_foreground = sub;
    colors.table_hover = over;
    colors.table_active = active.opacity(0.15);
    colors.table_active_border = active;
    colors.table_row_border = separator;

    colors.sidebar = c(K::DialogsBg);
    colors.sidebar_foreground = fg;
    colors.sidebar_border = separator;
    colors.sidebar_accent = over;
    colors.sidebar_accent_foreground = fg;
    colors.sidebar_primary = active;
    colors.sidebar_primary_foreground = on_active;

    colors.tab = bg;
    colors.tab_bar = bg;
    colors.tab_bar_segmented = panel;
    colors.tab_active = over;
    colors.tab_foreground = sub;
    colors.tab_active_foreground = fg;

    colors.title_bar = c(K::TitleBgActive);
    colors.title_bar_border = separator;
    colors.status_bar = c(K::TitleBgActive);
    colors.status_bar_border = separator;
    colors.window_border = border;

    colors.scrollbar = bg.opacity(0.);
    let thumb = c(K::ScrollBarBg);
    colors.scrollbar_thumb = thumb;
    colors.scrollbar_thumb_hover = Hsla {
        a: (thumb.a * 1.5).min(1.),
        ..thumb
    };
    colors.switch = c(K::CheckboxFg);
    colors.switch_thumb = on_active;
    colors.slider_bar = active;
    colors.slider_thumb = on_active;
    colors.progress_bar = active;
    colors.danger = c(K::AttentionButtonFg);
    colors.button_danger = colors.danger;
    colors.button_danger_foreground = on_active;
    ensure_danger_aa(colors, dark);
}

#[cfg(test)]
mod tests {
    use super::{apply, hsla};

    use super::rgb_of as hsla_to_rgb;
    use gpui_kit::component::theme::ThemeColor;
    use quill::app_theme::{BuiltinTheme, PaletteKey};

    #[test]
    fn kit_colours_follow_the_palette() {
        let palette = BuiltinTheme::Tinted.palette_with_accent(Some(0xaa6084));
        let mut colors = ThemeColor::default();
        apply(&mut colors, &palette);
        assert_eq!(colors.background, hsla(palette.rgba(PaletteKey::WindowBg)));
        assert_eq!(
            colors.primary,
            hsla(palette.rgba(PaletteKey::WindowBgActive))
        );
        assert_eq!(colors.sidebar, hsla(palette.rgba(PaletteKey::DialogsBg)));
        // Secondary text keeps WCAG AA on the window background: this
        // palette's own gray is lighter than that, so it is darkened.
        let sub = palette.rgba(PaletteKey::WindowSubTextFg);
        let readable = quill::theme_colorize::readable(
            sub >> 8,
            palette.rgba(PaletteKey::WindowBg) >> 8,
            quill::theme_colorize::WCAG_AA,
        );
        assert_eq!(
            colors.muted_foreground,
            hsla((readable << 8) | (sub & 0xff))
        );
        // Destructive text and button fills keep WCAG AA.
        let bg = palette.rgba(PaletteKey::WindowBg) >> 8;
        let danger = hsla_to_rgb(colors.danger);
        assert!(quill::theme_colorize::contrast(danger, bg) >= 4.5);
        let button = hsla_to_rgb(colors.button_danger);
        let label = palette.rgba(PaletteKey::WindowFgActive) >> 8;
        assert!(quill::theme_colorize::contrast(button, label) >= 4.5);
        // Alpha survives: the scroll bar thumb is translucent.
        assert!(colors.scrollbar_thumb.a < 0.5);
    }

    /// The kit's own reds (Quill Light / Dark draw from them) reach AA once
    /// corrected, on a light and on a dark surface.
    #[test]
    fn danger_colours_reach_aa() {
        for (dark, bg) in [(false, 0xffffffu32), (true, 0x0d1117)] {
            let mut colors = ThemeColor {
                background: hsla((bg << 8) | 0xff),
                danger: hsla(0xef4444ff),
                button_danger: hsla(0xef4444ff),
                button_danger_foreground: hsla(0xffffffff),
                ..ThemeColor::default()
            };
            super::ensure_danger_aa(&mut colors, dark);
            let text = hsla_to_rgb(colors.danger);
            assert!(
                quill::theme_colorize::contrast(text, bg) >= 4.5,
                "dark={dark}"
            );
            let fill = hsla_to_rgb(colors.button_danger);
            assert!(
                quill::theme_colorize::contrast(fill, 0xffffff) >= 4.5,
                "dark={dark}"
            );
        }
    }
}
