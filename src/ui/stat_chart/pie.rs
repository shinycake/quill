//! A percentage chart zoomed into a few days: a kit pie of each line's
//! share over the selected days, the days picked with a kit range slider
//! (tdesktop `ChartWidget::processLocalZoom`, `StackLinearChartView`).

use gpui_kit::component::chart::PieChart;
use gpui_kit::*;
use quill::stat_chart::{StatChart, format_decimal, pie_parts};

/// One slice: a shown line's total over the days.
struct Slice {
    name: SharedString,
    color: Hsla,
    total: i64,
    share: f32,
    text: SharedString,
}

/// The slices of the `enabled` lines over `days` (inclusive); lines with
/// nothing in them are left out.
fn slices(chart: &StatChart, enabled: &[bool], days: (usize, usize)) -> Vec<Slice> {
    let totals: Vec<i64> = chart
        .lines
        .iter()
        .enumerate()
        .map(|(k, line)| {
            if !enabled.get(k).copied().unwrap_or(true) {
                return 0;
            }
            line.y
                .get(days.0..=days.1)
                .unwrap_or_default()
                .iter()
                .fold(0i64, |sum, v| sum.saturating_add(*v))
        })
        .collect();
    let sums: Vec<f64> = totals.iter().map(|t| *t as f64).collect();
    pie_parts(&sums, true)
        .into_iter()
        .zip(chart.lines.iter().zip(totals))
        .filter(|(_, (_, total))| *total > 0)
        .map(|(part, (line, total))| Slice {
            name: line.name.clone().into(),
            color: rgb(line.color()).into(),
            total,
            share: part.share as f32,
            text: part.text.into(),
        })
        .collect()
}

/// The pie of `chart` over `days`.
pub(super) fn pie(
    id: &str,
    chart: &StatChart,
    enabled: &[bool],
    days: (usize, usize),
) -> AnyElement {
    PieChart::new(slices(chart, enabled, days))
        .id(ElementId::Name(format!("{id}-pie").into()))
        .value(|s: &Slice| s.share)
        .color(|s: &Slice| s.color)
        .label(|s: &Slice| s.text.clone())
        .pad_angle(0.01)
        .tooltip_name(|s: &Slice| s.name.clone())
        .tooltip_value(|s: &Slice, _, _| format_decimal(s.total).into())
        .into_any_element()
}
