//! The composer's emoji, sticker and GIF panel in each layout: the kit's
//! popover corners and shadow, or Web K's emoji dropdown
//! (`.emoji-dropdown` in tweb's `scss/partials/_emojiDropdown.scss`:
//! `border-radius: 1.25rem`), with the menus' shadow
//! (docs/decisions/floating-layout.md, "Floating leftovers").

use super::menu_shape::menu_shadow;
use gpui_kit::component::theme::Theme;
use gpui_kit::*;
use quill::layout_style::{ChromeMetrics, Look};

/// Web K's emoji dropdown corner radius.
const DROPDOWN_RADIUS: f32 = 20.;

/// The panel's corners; over cards its content is clipped to them.
pub(in crate::ui) fn corners<E: Styled>(this: E, metrics: &ChromeMetrics) -> E {
    match metrics.look {
        Look::Lines => this.rounded_lg(),
        Look::Cards => this.rounded(px(DROPDOWN_RADIUS)).clip_rounded(),
    }
}

/// The panel's shadow where it is solid (glass draws its own).
pub(in crate::ui) fn shadow<E: Styled>(this: E, metrics: &ChromeMetrics, theme: &Theme) -> E {
    match metrics.look {
        Look::Lines => this.shadow_lg(),
        Look::Cards => this.shadow(menu_shadow(theme.is_dark())),
    }
}
