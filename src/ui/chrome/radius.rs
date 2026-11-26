//! The card radius: the one place Floating's 24 comes from.
//!
//! The kit derives its radius steps from the theme's base radius
//! (`radius_2xl` is × 2.5, `radius_4xl` × 3.5 = 21 at the default 6). The
//! card step is Quill's own, × 4 (24 at the default), so a theme that
//! changes its base radius moves the cards with it. Pills use the same
//! token: a 48 px pill at 24 is a capsule.

use gpui_kit::component::theme::Theme;
use gpui_kit::*;
use quill::layout_style::ChromeMetrics;

/// Cards and pills in `metrics`: the theme radius times the metric's step
/// (0 in Classic, so square).
pub(in crate::ui) fn card_in(metrics: &ChromeMetrics, theme: &Theme) -> Pixels {
    theme.radius * metrics.radius
}
