//! The chat list's own parts in each layout: the search field's shape and
//! the folder tabs' indicator (docs/decisions/floating-layout.md).

use gpui_kit::Styled;
use gpui_kit::component::tab::TabBar;
use quill::layout_style::{ChromeMetrics, Look};

/// The search field: Classic's field, or a pill.
pub(in crate::ui) fn search_field<E: Styled>(this: E, metrics: &ChromeMetrics) -> E {
    match metrics.look {
        Look::Lines => this,
        Look::Cards => this.rounded_full(),
    }
}

/// The folder tabs' indicator: Classic's underline, or a pill that slides
/// between tabs (the kit's Pill variant).
pub(in crate::ui) fn folder_tabs(bar: TabBar, metrics: &ChromeMetrics) -> TabBar {
    match metrics.look {
        Look::Lines => bar.underline(),
        Look::Cards => bar.pill(),
    }
}
