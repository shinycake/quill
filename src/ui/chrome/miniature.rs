//! The layout picker's previews: a small drawing of a window in each
//! layout, laid out from the same [`ChromeMetrics`] the real window reads,
//! so a change to a metric changes its preview too. About twenty quads, no
//! image (docs/decisions/floating-layout.md).

use super::bars::BarLine;
use super::frames;
use gpui_kit::component::theme::Theme;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use quill::density::Density;
use quill::layout_style::{Chrome, ChromeMetrics, Edge, Elevation, Look, SelectedRow};

/// The real window the preview stands for, in interface pixels.
const MODEL_WIDTH: f32 = 1200.;
/// The list column of the model window.
const MODEL_LIST: f32 = 340.;
/// The header and the composer of the model window.
const MODEL_BAR: f32 = 56.;

/// A preview of a window laid out with `metrics`, `size` large. The cards
/// are the real ones (`frames::surface`, `frames::pill`) with their radius
/// scaled down, and the rows take the density table's Standard sizes.
pub(in crate::ui) fn miniature(
    metrics: &'static ChromeMetrics,
    size: Size<Pixels>,
    theme: &Theme,
) -> Div {
    let scale = f32::from(size.width) / MODEL_WIDTH;
    let s = |v: f32| px((v * scale).max(0.));
    let rows = Density::Standard.metrics_in(metrics.rows);
    let radius = super::radius::card_in(metrics, theme) * scale;
    let chrome = Chrome {
        metrics,
        edge: Edge::None,
        elevation: Elevation::Shadowed,
    };
    let pane = theme.background;

    let row = |selected: bool| {
        let fill = match (selected, metrics.look.selected_row()) {
            (false, _) => None,
            (true, SelectedRow::Tint) => Some(theme.primary.opacity(0.16)),
            (true, SelectedRow::Solid) => Some(theme.primary),
        };
        let ink = match (selected, metrics.look.selected_row()) {
            (true, SelectedRow::Solid) => theme.primary_foreground,
            _ => theme.muted_foreground,
        };
        div()
            .flex()
            .items_center()
            .gap(s(rows.row_gap))
            .h(s(rows.row_base))
            .px(s(rows.row_inset))
            .rounded(radius * 0.66)
            .when_some(fill, |row, fill| row.bg(fill))
            .child(
                div()
                    .size(s(rows.avatar))
                    .rounded_full()
                    .bg(ink.opacity(0.5)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(s(10.))
                    .child(
                        div()
                            .w(s(140.))
                            .h(s(10.))
                            .rounded_full()
                            .bg(ink.opacity(0.7)),
                    )
                    .child(div().w(s(90.)).h(s(8.)).rounded_full().bg(ink.opacity(0.4))),
            )
    };
    let list = div()
        .flex()
        .flex_col()
        .gap(s(4.))
        .w(s(MODEL_LIST))
        .h_full()
        .p(s(8.))
        .map(|list| frames::list_frame(list, &chrome, theme, None))
        .rounded(match metrics.look {
            Look::Lines => px(0.),
            Look::Cards => radius,
        })
        .child(
            div()
                .h(s(36.))
                .mb(s(8.))
                .rounded(radius.max(px(2.)))
                .bg(pane),
        )
        .child(row(false))
        .child(row(true))
        .child(row(false));
    let bubble = |outgoing: bool, width: f32| {
        div().flex().when(outgoing, |row| row.justify_end()).child(
            div().w(s(width)).h(s(44.)).rounded(s(15.)).bg(if outgoing {
                theme.primary.opacity(0.55)
            } else {
                theme.background
            }),
        )
    };
    // The header over the history and the composer under it: lined bars,
    // or the real pills.
    let bar = |line: BarLine| {
        let bar = div().h(s(MODEL_BAR)).flex_none();
        match (metrics.look, line) {
            (Look::Lines, BarLine::Under) => bar.bg(pane).border_b_1().border_color(theme.border),
            (Look::Lines, BarLine::Over) => bar.bg(pane).border_t_1().border_color(theme.border),
            (Look::Cards, _) => frames::pill(bar, &chrome, theme, None).rounded(radius),
        }
    };
    let history = div()
        .flex()
        .flex_col()
        .flex_1()
        .justify_end()
        .gap(s(10.))
        .px(s(24.))
        .py(s(12.))
        .child(bubble(false, 260.))
        .child(bubble(true, 200.));
    let (window_fill, chat_fill, stack_gap) = match metrics.look {
        Look::Lines => (Background::from(pane), Some(wallpaper(theme)), px(0.)),
        Look::Cards => (wallpaper(theme), None, s(metrics.bar_gap.max(8.) * 0.5)),
    };
    let chat = div()
        .flex()
        .flex_col()
        .flex_1()
        .h_full()
        .gap(stack_gap)
        .when_some(chat_fill, |chat, fill| chat.bg(fill))
        .child(bar(BarLine::Under))
        .child(history)
        .child(bar(BarLine::Over));
    let gutter = s(metrics.gutter);
    div()
        .flex()
        .w(size.width)
        .h(size.height)
        .flex_none()
        .overflow_hidden()
        .gap(gutter)
        .p(gutter)
        .bg(window_fill)
        .child(list)
        .child(chat)
}

/// The preview's wallpaper: a soft two-colour gradient in the theme's
/// lightness, so the cards over it read as cards.
fn wallpaper(theme: &Theme) -> Background {
    let (from, to) = if theme.is_dark() {
        (hsla(0.48, 0.38, 0.30, 1.), hsla(0.72, 0.34, 0.32, 1.))
    } else {
        (hsla(0.48, 0.45, 0.74, 1.), hsla(0.72, 0.42, 0.80, 1.))
    };
    linear_gradient(135., linear_color_stop(from, 0.), linear_color_stop(to, 1.))
}
