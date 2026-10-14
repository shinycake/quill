//! The plot of each chart kind on gpui-kit charts: lines and step lines
//! as an `AreaChart` with unfilled areas, a percentage chart as stacked
//! areas, a two-scale chart as two `LineChart`s one above the other
//! (one value axis each), stacked columns as [`StackedBars`].

use super::series::{Point, VALUE_INTERVALS, value_domain, value_extent, zoomable_point};
use super::stacked_bars::StackedBars;
use super::tooltip;
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::chart::{AreaChart, LineChart};
use gpui_kit::component::plot::axis_gutter;
use gpui_kit::component::plot::label::{TEXT_GAP, TEXT_SIZE};
use gpui_kit::*;
use quill::stat_chart::{ChartKind, StatChart, axis_caption, nice_domain};
use std::cell::Cell;
use std::rc::Rc;

/// Dates labeled under a plot.
const DATE_TICKS: usize = 5;
/// Gap between the two plots of a two-scale chart.
const DOUBLE_GAP: f32 = 16.;
/// Width of a two-scale chart's value labels, wide enough for "12.5K".
const LABEL_COLUMN: f32 = 44.;
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
        move |point: &Point, _: &mut Window, _: &mut App| {
            hovered.set(Some(point.index));
            let zoom = zoomable && zoomable_point(point);
            tooltip::body(&chart, point, &enabled, shares, zoom).into_any_element()
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
pub(super) fn plot(input: PlotInput, cx: &App) -> AnyElement {
    match input.kind {
        ChartKind::StackBar => stacked_bars(input),
        ChartKind::DoubleLinear if input.shown().len() == 2 => two_scales(input, cx),
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

/// A two-scale chart: each line on its own plot and axis. The kit sizes
/// a chart's value-label column from its own labels, so two plots on
/// different scales would start at different x and their days would not
/// line up; the labels sit in one fixed column here instead.
fn two_scales(input: PlotInput, cx: &App) -> AnyElement {
    let shown = input.shown();
    let height = (input.height - DOUBLE_GAP) / 2.;
    let last = shown.len().saturating_sub(1);
    let muted = cx.theme().muted_foreground;
    let mut column = div().flex().flex_col().gap(px(DOUBLE_GAP)).size_full();
    for (row, (k, color)) in shown.into_iter().enumerate() {
        let (min, max) = value_extent(&input.points, &input.enabled, k..k + 1)
            .map_or((0.0, 1.0), |(min, max)| {
                nice_domain(min, max, DOUBLE_INTERVALS)
            });
        let dates = row == last;
        let chart = LineChart::new(input.points.clone())
            .id(ElementId::Name(format!("{}-scale-{k}", input.id).into()))
            .x(|p: &Point| p.label.clone())
            .x_axis(dates)
            .x_tick_count(DATE_TICKS)
            .y(move |p: &Point| p.value(k))
            .stroke(color)
            .linear()
            .y_axis(false)
            .y_domain(min, max)
            .y_padding(0., 0.)
            .y_tick_count(DOUBLE_INTERVALS as usize + 1)
            .tooltip_content(input.tooltip());
        // The rulers run over the plot, above the dates on the lower one.
        let plot_height = if dates {
            (height - axis_gutter(px(TEXT_SIZE))).max(0.)
        } else {
            height
        };
        let mut labels = div().relative().flex_none().w(px(LABEL_COLUMN)).h_full();
        for i in 0..=DOUBLE_INTERVALS {
            let share = f64::from(i) / f64::from(DOUBLE_INTERVALS);
            let y = plot_height * share as f32;
            let top = (y - TEXT_SIZE / 2.).clamp(0., (plot_height - TEXT_SIZE).max(0.));
            labels = labels.child(
                div()
                    .absolute()
                    .top(px(top))
                    .right(px(TEXT_GAP * 2.))
                    .text_size(px(TEXT_SIZE))
                    .line_height(px(TEXT_SIZE))
                    .text_color(muted)
                    .child(axis_caption(max - (max - min) * share)),
            );
        }
        column = column.child(
            div()
                .flex()
                .w_full()
                .h(px(height))
                .child(labels)
                .child(div().flex_1().min_w_0().h_full().child(chart)),
        );
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
