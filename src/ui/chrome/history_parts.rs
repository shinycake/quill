//! Marks between history rows in each layout
//! (docs/decisions/floating-layout.md, "Floating leftovers").

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

/// Web K's empty-chat service bubble (`.empty-bubble-placeholder` with a
/// description): `.75rem 1rem .875rem` padding, `1.5rem` corners.
const NOTE_RADIUS: f32 = 24.;

/// A note in place of the history ("Choose a topic", "No messages in this
/// topic yet"): Classic's plain text at the pane's top (`classic`), or,
/// over cards, Web K's service bubble centred on the wallpaper, so the
/// text never sits bare on the pattern.
pub(in crate::ui) fn pane_note(
    classic: impl FnOnce() -> AnyElement,
    title: SharedString,
    body: SharedString,
    ink: &MarkInk,
    metrics: &ChromeMetrics,
) -> AnyElement {
    match metrics.look {
        Look::Lines => classic(),
        Look::Cards => div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .p_6()
            .child(
                div()
                    .max_w(px(320.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .px_4()
                    .pt_3()
                    .pb(px(14.))
                    .rounded(px(NOTE_RADIUS))
                    .bg(ink.pill)
                    .text_color(ink.text)
                    .text_center()
                    .child(div().font_semibold().child(title))
                    .child(div().text_sm().opacity(0.8).child(body)),
            )
            .into_any_element(),
    }
}

/// A sponsored message's card in the history: the kit's corners, or a
/// bubble's (Web K draws the ad as a bubble, `$bubble-border-radius-big`).
pub(in crate::ui) fn sponsored_corners<E: Styled>(this: E, metrics: &ChromeMetrics) -> E {
    match metrics.look {
        Look::Lines => this.rounded_lg(),
        Look::Cards => this.rounded(px(SPONSORED_RADIUS)),
    }
}

/// Web K's `$bubble-border-radius-big`.
const SPONSORED_RADIUS: f32 = 15.;
