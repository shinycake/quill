//! The layout picker's previews: a small drawing of a window in each
//! layout, laid out from the same [`ChromeMetrics`] the real window reads,
//! so a change to a metric changes its preview too. About twenty quads, no
//! image (docs/decisions/floating-layout.md).

use gpui_kit::component::theme::Theme;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use quill::layout_style::{Backdrop, ChromeMetrics, Placement, SelectedRow};

/// The real window the preview stands for, in interface pixels.
const MODEL_WIDTH: f32 = 1200.;
/// The list column of the model window.
const MODEL_LIST: f32 = 340.;
/// The header and the composer of the model window.
const MODEL_BAR: f32 = 56.;

/// A preview of a window laid out with `metrics`, `size` large.
pub(in crate::ui) fn miniature(metrics: &ChromeMetrics, size: Size<Pixels>, theme: &Theme) -> Div {
    let scale = f32::from(size.width) / MODEL_WIDTH;
    let s = |v: f32| px((v * scale).max(0.));
    // A thin line stays visible however small the preview.
    let line = |v: f32| px(if v > 0. { 1. } else { 0. });
    let gutter = s(metrics.gutter);
    let gap = s(metrics.bar_gap.max(8.) * 0.5);
    let radius = super::radius::card_in(metrics, theme) * scale;
    let floating = metrics.placement == Placement::Overlaid;
    let wallpaper = wallpaper(theme);
    // Dark cards keep a faint light edge over the wallpaper, as the real
    // ones do (`frames::surface`).
    let edge = (floating && theme.is_dark()).then(|| hsla(0., 0., 1., 0.14));
    let card = theme.sidebar;
    let pane = theme.background;
    let bar = theme.background;
    let border = theme.border;
    let divider = line(metrics.divider);

    let row = |selected: bool| {
        let fill = match (selected, metrics.selected_row) {
            (false, _) => None,
            (true, SelectedRow::Tint) => Some(theme.primary.opacity(0.16)),
            (true, SelectedRow::Solid) => Some(theme.primary),
        };
        let ink = match (selected, metrics.selected_row) {
            (true, SelectedRow::Solid) => theme.primary_foreground,
            _ => theme.muted_foreground,
        };
        div()
            .flex()
            .items_center()
            .gap(s(16.))
            .h(s(64.))
            .px(s(12.))
            .rounded(radius * 0.66)
            .when_some(fill, |row, fill| row.bg(fill))
            .child(div().size(s(40.)).rounded_full().bg(ink.opacity(0.5)))
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
        .bg(card)
        .rounded(radius)
        .when_some(edge, |list, edge| list.border_1().border_color(edge))
        .when(!floating, |list| {
            list.border_r(divider).border_color(border)
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
    let pill = |el: Div| {
        if floating {
            el.rounded(radius)
                .bg(bar)
                .when_some(edge, |el, edge| el.border_1().border_color(edge))
        } else {
            el.bg(bar)
        }
    };
    let header = pill(div().h(s(MODEL_BAR)).flex_none())
        .when(!floating, |h| h.border_b(divider).border_color(border));
    let composer = pill(div().h(s(MODEL_BAR)).flex_none())
        .when(!floating, |c| c.border_t(divider).border_color(border));
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
    let chat_backdrop = match metrics.backdrop {
        Backdrop::History => Some(wallpaper),
        Backdrop::Window => None,
    };
    let chat = div()
        .flex()
        .flex_col()
        .flex_1()
        .h_full()
        .gap(if floating { gap } else { px(0.) })
        .when_some(chat_backdrop, |chat, fill| chat.bg(fill))
        .child(header)
        .child(history)
        .child(composer);
    div()
        .flex()
        .w(size.width)
        .h(size.height)
        .flex_none()
        .overflow_hidden()
        .gap(gutter)
        .p(gutter)
        .bg(match metrics.backdrop {
            Backdrop::History => Background::from(pane),
            Backdrop::Window => wallpaper,
        })
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
