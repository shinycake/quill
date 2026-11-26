//! Marks between history rows in each layout
//! (docs/decisions/floating-layout.md, "Marks in the history").

use gpui_kit::component::StyledExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::layout_style::{ChromeMetrics, Look};

/// The ink of a mark: Classic's bar fill, or the service pill's fill, and
/// the text colour both draw with.
pub(in crate::ui) struct MarkInk {
    pub bar: Hsla,
    pub pill: Hsla,
    pub text: Hsla,
}

/// The "Unread messages" mark around its label: Classic's full-width bar
/// (`row` carries the bar's fill), or, over cards, a service pill centred
/// in the column, the date pills' shape (Web K's `.service-msg`), so no
/// square band crosses the wallpaper. `wide` centres Classic's label in
/// the wide column; the column of cards is never wide.
pub(in crate::ui) fn unread_mark<E: ParentElement + Styled + FluentBuilder>(
    row: E,
    label: SharedString,
    wide: impl FnOnce(Div) -> AnyElement,
    ink: MarkInk,
    metrics: &ChromeMetrics,
) -> E {
    let row = row.w_full().text_xs().font_medium().text_color(ink.text);
    match metrics.look {
        Look::Lines => row
            .my_2()
            .py_1()
            .bg(ink.bar)
            .child(wide(div().w_full().flex().justify_center().child(label))),
        Look::Cards => row.my_2().flex().justify_center().child(
            div()
                .px_3()
                .py_0p5()
                .rounded_full()
                .bg(ink.pill)
                .child(label),
        ),
    }
}
