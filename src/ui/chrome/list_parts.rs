//! The chat list's own parts in each layout: the search field's shape and
//! the folder tabs' indicator (docs/decisions/floating-layout.md).

use gpui_kit::Styled;
use gpui_kit::component::tab::TabBar;
use quill::layout_style::{ChromeMetrics, FolderTabs, SearchShape};

/// The search field: Classic's field, or a pill.
pub(in crate::ui) fn search_field<E: Styled>(this: E, metrics: &ChromeMetrics) -> E {
    match metrics.search {
        SearchShape::Field => this,
        SearchShape::Pill => this.rounded_full(),
    }
}

/// The folder tabs' indicator: Classic's underline, or a pill that slides
/// between tabs (the kit's Pill variant).
pub(in crate::ui) fn folder_tabs(bar: TabBar, metrics: &ChromeMetrics) -> TabBar {
    match metrics.folder_tabs {
        FolderTabs::Underline => bar.underline(),
        FolderTabs::Pill => bar.pill(),
    }
}
