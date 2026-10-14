//! The plot of each chart kind on gpui-kit charts: lines and step lines
//! as an `AreaChart` with unfilled areas, a percentage chart as stacked
//! areas, a two-scale chart as two `LineChart`s one above the other
//! (one value axis each), stacked columns as [`StackedBars`].

use super::series::{Point, VALUE_INTERVALS, value_domain, value_extent, zoomable_point};
use super::stacked_bars::StackedBars;
use super::tooltip;
use gpui_kit::component::chart::{AreaChart, LineChart};
use gpui_kit::*;
use quill::stat_chart::{ChartKind, StatChart, axis_caption, nice_domain};
use std::cell::Cell;
use std::rc::Rc;

/// Dates labeled under a plot.
const DATE_TICKS: usize = 5;
/// Gap between the two plots of a two-scale chart.
const DOUBLE_GAP: f32 = 16.;
/// Rulers of each plot of a two-scale chart (half the height, half the
/// rulers).
const DOUBLE_INTERVALS: u32 = 3;

/// What a plot is drawn from.
pub(super) struct PlotInput {
    pub id: SharedString,
    pub chart: Rc<StatChart>,
    pub kind: ChartKind,
    pub points: Vec<Point>,
    pub enabled: Vec<bool>,
    /// Set to the point the tooltip shows, for a click to zoom into.
    pub hovered: Rc<Cell<Option<usize>>>,
    /// A click on a point zooms in.
    pub zoomable: bool,
    pub height: f32,
}

impl PlotInput {
    /// The shown lines with their colors, in line order.
    fn shown(&self) -> Vec<(usize, Hsla)> {
        self.chart
            .lines
            .iter()
            .enumerate()
            .filter(|(k, _)| self.enabled.get(*k).copied().unwrap_or(true))
            .map(|(k, line)| (k, rgb(line.color()).into()))
            .collect()
    }

    /// Each line's share shows in the tooltip.
    fn shares(&self) -> bool {
        self.chart.has_percentages || self.kind == ChartKind::StackLinear
    }

    /// The tooltip body, which also records the hovered point.
    fn tooltip(&self) -> impl Fn(&Point, &mut Window, &mut App) -> AnyElement + 'static {
        let chart = self.chart.clone();
        let enabled = self.enabled.clone();
        let hovered = self.hovered.clone();
        let (shares, zoomable) = (self.shares(), self.zoomable);
        move |point: &Point, _: &mut Window, cx: &mut App| {
            hovered.set(Some(point.index));
            let zoom = zoomable && zoomable_point(point);
            tooltip::body(&chart, point, &enabled, shares, zoom, cx).into_any_element()
        }
    }
}

/// A value-axis caption: whole percents on a percentage chart.
fn caption(kind: ChartKind) -> impl Fn(f64) -> String + 'static {
    move |value| match kind {
        ChartKind::StackLinear => format!("{}%", value.round()),
        _ => axis_caption(value),
    }
}

/// The plot of `input`.
pub(super) fn plot(input: PlotInput) -> AnyElement {
    match input.kind {
        ChartKind::StackBar => stacked_bars(input),
        ChartKind::DoubleLinear if input.shown().len() == 2 => two_scales(input),
        _ => areas(input),
    }
}

/// Lines, step lines and stacked percentage areas.
fn areas(input: PlotInput) -> AnyElement {
    let shown = input.shown();
    let tooltip = input.tooltip();
    let (min, max) = value_domain(
        &input.points,
        input.kind,
        &input.enabled,
        0..input.chart.lines.len(),
    );
    let mut chart = AreaChart::new(input.points)
        .id(ElementId::Name(format!("{}-area", input.id).into()))
        .x(|p: &Point| p.label.clone())
        .x_tick_count(DATE_TICKS)
        .y_axis(true)
        .y_domain(min, max)
        .y_padding(0., 0.)
        .y_tick_count(VALUE_INTERVALS as usize + 1)
        .y_tick_format(caption(input.kind))
        .tooltip_content(tooltip);
    if input.kind == ChartKind::StackLinear {
        // The tallest stack first, so each lower band paints over it.
        for (k, color) in shown.into_iter().rev() {
            chart = chart
                .y(move |p: &Point| p.stack_top(k))
                .stroke(color)
                .fill(color)
                .linear();
        }
    } else {
        for (k, color) in shown {
            chart = chart
                .y(move |p: &Point| p.value(k))
                .stroke(color)
                .fill(transparent_black());
            chart = if input.kind == ChartKind::Bar {
                chart.step_after()
            } else {
                chart.linear()
            };
        }
    }
    chart.into_any_element()
}

/// A two-scale chart: each line on its own plot and axis.
fn two_scales(input: PlotInput) -> AnyElement {
    let shown = input.shown();
    let height = (input.height - DOUBLE_GAP) / 2.;
    let last = shown.len().saturating_sub(1);
    let mut column = div().flex().flex_col().gap(px(DOUBLE_GAP)).size_full();
    for (row, (k, color)) in shown.into_iter().enumerate() {
        let (min, max) = value_extent(&input.points, &input.enabled, k..k + 1)
            .map_or((0.0, 1.0), |(min, max)| {
                nice_domain(min, max, DOUBLE_INTERVALS)
            });
        let chart = LineChart::new(input.points.clone())
            .id(ElementId::Name(format!("{}-line-{k}", input.id).into()))
            .x(|p: &Point| p.label.clone())
            .x_axis(row == last)
            .x_tick_count(DATE_TICKS)
            .y(move |p: &Point| p.value(k))
            .stroke(color)
            .linear()
            .y_axis(true)
            .y_domain(min, max)
            .y_padding(0., 0.)
            .y_tick_count(DOUBLE_INTERVALS as usize + 1)
            .y_tick_format(axis_caption)
            .tooltip_content(input.tooltip());
        column = column.child(div().w_full().h(px(height)).child(chart));
    }
    column.into_any_element()
}

/// Stacked columns.
fn stacked_bars(input: PlotInput) -> AnyElement {
    let shown = input.shown();
    let tooltip = input.tooltip();
    let (min, max) = value_domain(
        &input.points,
        input.kind,
        &input.enabled,
        0..input.chart.lines.len(),
    );
    let every = input.points.len().div_ceil(DATE_TICKS).max(1);
    StackedBars::new(
        ElementId::Name(format!("{}-bars", input.id).into()),
        input.points,
    )
    .lines(shown)
    .domain(min, max, VALUE_INTERVALS as usize + 1)
    .tick_format(caption(input.kind))
    .label_every(every)
    .tooltip(tooltip)
    .into_any_element()
}
