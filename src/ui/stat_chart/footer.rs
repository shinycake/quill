//! The range selector under a chart (tdesktop `ChartWidget::Footer`): the
//! whole period as a small chart with the parts outside the selection
//! dimmed, and a kit range slider that picks the days the plot shows.

use super::series::{Point, value_domain, value_extent};
use gpui_kit::component::chart::AreaChart;
use gpui_kit::component::slider::{Slider, SliderState};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::stat_chart::ChartKind;

/// Height of the small chart over the slider.
const PREVIEW_HEIGHT: f32 = 32.;

/// The selector for a chart of `kind` over `points` (the whole period)
/// with `visible` (positions in `points`) selected.
#[allow(clippy::too_many_arguments)]
pub(super) fn footer(
    id: &str,
    kind: ChartKind,
    points: Vec<Point>,
    enabled: &[bool],
    shown: Vec<(usize, Hsla)>,
    visible: (usize, usize),
    slider: &Entity<SliderState>,
    cx: &App,
) -> Stateful<Div> {
    let len = points.len();
    let lines = 0..enabled.len();
    let (min, max) = match kind {
        // A preview of stacked columns reads as stacked areas.
        ChartKind::StackBar | ChartKind::StackLinear => value_domain(&points, kind, enabled, lines),
        // Each line of a two-scale chart fills the preview on its own scale.
        ChartKind::DoubleLinear => (0., 1.),
        _ => value_domain(&points, ChartKind::Linear, enabled, lines),
    };
    // Each line's own extent, for a two-scale preview.
    let extents: Vec<(f64, f64)> = (0..enabled.len())
        .map(|k| value_extent(&points, enabled, k..k + 1).unwrap_or((0., 1.)))
        .collect();
    let stacked = matches!(kind, ChartKind::StackBar | ChartKind::StackLinear);
    let mut preview = AreaChart::new(points)
        .id(ElementId::Name(format!("{id}-preview").into()))
        .x(|p: &Point| p.label.clone())
        .x_axis(false)
        .grid(false)
        .interactive(false)
        .appear(false)
        .y_domain(min, max)
        .y_padding(0., 0.);
    if stacked {
        for (k, color) in shown.into_iter().rev() {
            preview = preview
                .y(move |p: &Point| p.stack_top(k))
                .stroke(color)
                .fill(color.opacity(0.7))
                .linear();
        }
    } else if kind == ChartKind::DoubleLinear {
        for (k, color) in shown {
            let (low, high) = extents.get(k).copied().unwrap_or((0., 1.));
            let span = (high - low).max(f64::EPSILON);
            preview = preview
                .y(move |p: &Point| (p.value(k) - low) / span)
                .stroke(color)
                .fill(transparent_black())
                .linear();
        }
    } else {
        for (k, color) in shown {
            preview = preview
                .y(move |p: &Point| p.value(k))
                .stroke(color)
                .fill(transparent_black())
                .linear();
        }
    }
    // The days outside the selection, dimmed over the preview.
    let back = len.saturating_sub(1).max(1) as f32;
    let start = visible.0 as f32 / back;
    let end = visible.1 as f32 / back;
    let dim = cx.theme().background.opacity(0.65);
    div()
        .id(ElementId::Name(format!("{id}-range").into()))
        .debug_selector({
            let id = id.to_string();
            move || format!("{id}-range")
        })
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .relative()
                .w_full()
                .h(px(PREVIEW_HEIGHT))
                .rounded_sm()
                .overflow_hidden()
                .child(preview)
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left_0()
                        .w(relative(start))
                        .bg(dim),
                )
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .right_0()
                        .w(relative((1. - end).max(0.)))
                        .bg(dim),
                ),
        )
        .child(Slider::new(slider))
}
