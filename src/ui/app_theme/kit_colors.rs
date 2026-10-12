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

/// Write `palette` into the kit colours.
pub(crate) fn apply(colors: &mut ThemeColor, palette: &Palette) {
    use PaletteKey as K;
    let c = |key: K| hsla(palette.rgba(key));
    let dark = palette.is_dark();
    let bg = c(K::WindowBg);
    let fg = c(K::WindowFg);
    let over = c(K::WindowBgOver);
    let ripple = c(K::WindowBgRipple);
    let sub = c(K::WindowSubTextFg);
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
    colors.danger_hover = lighter(colors.danger, if dark { 0.05 } else { -0.04 });
    colors.danger_active = pressed(colors.danger);
    colors.button_danger = colors.danger;
    colors.button_danger_hover = colors.danger_hover;
    colors.button_danger_active = colors.danger_active;
    colors.button_danger_foreground = on_active;
}

#[cfg(test)]
mod tests {
    use super::{apply, hsla};
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
        assert_eq!(
            colors.muted_foreground,
            hsla(palette.rgba(PaletteKey::WindowSubTextFg))
        );
        // Alpha survives: the scroll bar thumb is translucent.
        assert!(colors.scrollbar_thumb.a < 0.5);
    }
}
