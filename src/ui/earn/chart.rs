//! The revenue pages' charts (tdesktop `info_channel_earn_list.cpp` and
//! `info_bot_earn_list.cpp`): ad impressions as a step line, TON ad
//! rewards and Stars revenue as columns, each over the period the server
//! sent. Drawn with gpui-kit's `LineChart` and `BarChart` in the theme's
//! chart color; a revenue chart reads in its currency on the value axis
//! and in its currency and US dollars in the hover tooltip.

use super::super::app::QuillApp;
use super::super::chat_theme::text_muted;
use gpui_kit::component::chart::{BarChart, LineChart};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::stat_chart::{
    ChartCurrency, StatChart, axis_caption, detailed_day, format_decimal, nice_axis_max, usd_label,
};
use quill::telegram::envelope::StatisticalGraph;
use std::collections::HashSet;
use std::rc::Rc;

/// Height of a chart's plot and axes (tdesktop `statisticsChartHeight`).
const CHART_HEIGHT: f32 = 200.;
/// Dates labeled under a chart; the 300 px panel fits four.
const DATE_TICKS: usize = 4;
/// Value-axis ticks, the baseline included.
const VALUE_TICKS: usize = 5;

/// Every how many columns a date is labeled. The kit labels every
/// `margin`-th column counting from the first, so the margin is picked
/// to give about three dates with the last one clear of the right edge,
/// where a centered date would hang out of the panel.
pub(super) fn column_tick_margin(len: usize) -> usize {
    let mut margin = len.div_ceil(DATE_TICKS - 1).max(1);
    while margin > 1 && len % margin < margin / 3 + 1 {
        margin -= 1;
    }
    margin
}

/// What a chart shows.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum EarnChartKind {
    /// A count over time as a step line (tdesktop `ChartViewType::Bar`).
    Steps,
    /// An amount per period as columns (tdesktop `ChartViewType::StackBar`).
    Columns,
}

/// One period of a chart.
#[derive(Clone)]
struct Period {
    /// The x-axis label; unique within the chart.
    label: SharedString,
    /// The tooltip's date.
    date: SharedString,
    /// The raw value (nanotons for TON).
    raw: i64,
    /// The value in the currency's units, what the axis scales.
    value: f64,
}

/// The periods of `chart`: every series summed per point, as a stacked
/// column reads (the revenue graphs have one series).
fn periods(chart: &StatChart) -> Vec<Period> {
    let offset = quill::local_time::utc_offset_at(quill::local_time::now_unix());
    let mut seen = HashSet::new();
    chart
        .x
        .iter()
        .enumerate()
        .map(|(i, &x)| {
            let raw = chart
                .lines
                .iter()
                .filter_map(|line| line.y.get(i))
                .fold(0i64, |sum, v| sum.saturating_add(*v));
            let seconds = (x / 1000.0) as i64;
            let date = detailed_day(seconds, offset);
            let mut label = chart.day_label(i, offset);
            // A band chart needs distinct labels; repeat dates fall back
            // to the full date.
            if !seen.insert(label.clone()) {
                label = date.clone();
                seen.insert(label.clone());
            }
            Period {
                label: label.into(),
                date: date.into(),
                raw,
                value: chart.currency_units(raw as f64),
            }
        })
        .collect()
}

/// The hover tooltip's body: the date, the amount in the currency, and
/// in dollars at `cents_per_unit` for a revenue chart.
fn tooltip_body(period: &Period, chart: &StatChart, cents_per_unit: f64, cx: &App) -> Div {
    let amount = match chart.currency {
        ChartCurrency::Ton => format!("{} TON", chart.currency_amount(period.raw)),
        ChartCurrency::Stars => format!("{} Stars", chart.currency_amount(period.raw)),
        ChartCurrency::None => format_decimal(period.raw),
    };
    let mut body = div()
        .flex()
        .flex_col()
        .gap_0p5()
        .text_xs()
        .child(
            div()
                .text_color(cx.theme().muted_foreground)
                .child(period.date.clone()),
        )
        .child(div().font_semibold().child(amount));
    if chart.currency != ChartCurrency::None && cents_per_unit > 0.0 {
        body = body.child(div().child(usd_label(period.value, cents_per_unit)));
    }
    body
}

impl QuillApp {
    /// A titled chart of `graph` for the revenue pages: the title, the
    /// period it covers, and the chart; `None` while an async graph loads
    /// or for a graph that has no data (tdesktop leaves those out).
    /// `cents_per_unit` is the dollar rate of a revenue chart.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn earn_chart(
        &self,
        chat_id: i64,
        id: &'static str,
        title: &'static str,
        graph: &StatisticalGraph,
        kind: EarnChartKind,
        cents_per_unit: f64,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let json = self.stat_graph_json(chat_id, graph, cx)?;
        let chart = super::super::stat_chart::parsed_chart(json)?;
        let data = periods(&chart);
        if data.is_empty() {
            return None;
        }
        let len = data.len();
        let last = len.saturating_sub(1);
        let range = chart.range_title(0, last);
        let color = cx.theme().chart_2;
        let currency = chart.currency;
        // The line's axis tops out at a round number so every tick reads
        // as one; the columns' axis leaves its baseline unlabeled so it
        // does not run into the first date.
        let top = data.iter().map(|p| p.value).fold(0.0, f64::max);
        let line_top = nice_axis_max(top, (VALUE_TICKS - 1) as u32);
        let tooltip_chart = Rc::clone(&chart);
        let tooltip = move |period: &Period, _: &mut Window, cx: &mut App| {
            tooltip_body(period, &tooltip_chart, cents_per_unit, cx)
        };
        let plot = match kind {
            EarnChartKind::Steps => LineChart::new(data)
                .id(id)
                .x(|p: &Period| p.label.clone())
                .y(|p: &Period| p.value)
                .step_after()
                .stroke(color)
                .y_axis(true)
                .y_domain(0.0, line_top)
                .y_padding(0., 0.)
                .y_tick_count(VALUE_TICKS)
                .y_tick_format(axis_caption)
                .x_tick_count(DATE_TICKS)
                .tooltip_content(tooltip)
                .into_any_element(),
            EarnChartKind::Columns => BarChart::new(data)
                .id(id)
                .band(|p: &Period| p.label.clone())
                .value(|p: &Period| p.value)
                .fill(move |_: &Period, _, _, _| color)
                .value_axis(true)
                .value_tick_count(VALUE_TICKS)
                .value_tick_format(|value| {
                    if value.abs() <= 1e-9 {
                        String::new()
                    } else {
                        axis_caption(value)
                    }
                })
                .tick_margin(column_tick_margin(len))
                .padding_inner(0.2)
                .padding_outer(0.1)
                .corner_radii(Corners {
                    top_left: px(2.),
                    top_right: px(2.),
                    bottom_left: px(0.),
                    bottom_right: px(0.),
                })
                .tooltip_content(tooltip)
                .into_any_element(),
        };
        let unit = match currency {
            ChartCurrency::Ton => Some("Amounts in TON"),
            ChartCurrency::Stars => Some("Amounts in Stars"),
            ChartCurrency::None => None,
        };
        let subtitle = match (range.is_empty(), unit) {
            (false, Some(unit)) => Some(format!("{range} \u{00B7} {unit}")),
            (false, None) => Some(range),
            (true, Some(unit)) => Some(unit.to_string()),
            (true, None) => None,
        };
        Some(
            div()
                .id(id)
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_sm().font_semibold().child(title))
                .children(subtitle.map(|text| div().text_xs().text_color(text_muted()).child(text)))
                .child(
                    div()
                        .debug_selector(move || id.to_string())
                        .mt_2()
                        .h(px(CHART_HEIGHT))
                        .w_full()
                        .child(plot),
                )
                .into_any_element(),
        )
    }
}
