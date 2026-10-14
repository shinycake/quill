//! The points a chart draws for its visible range: each line's value at
//! every x, the running sums a stacked chart draws, and the value axis's
//! domain (tdesktop `heightLimits` of each chart view, rounded so every
//! ruler reads as a round number).

use gpui_kit::SharedString;
use quill::stat_chart::{ChartKind, StatChart, nice_domain};
use std::collections::HashSet;

/// Even steps on a chart's value axis (tdesktop draws six rulers).
pub(super) const VALUE_INTERVALS: u32 = 5;

/// One x of the visible range.
#[derive(Clone)]
pub(super) struct Point {
    /// Index into the chart's x column.
    pub index: usize,
    /// The x-axis label; unique within the range.
    pub label: SharedString,
    /// Each line's value, in line order; 0 for a line switched off.
    pub values: Vec<f64>,
    /// Running sums of `values` from the first line up, as a stacked
    /// chart draws them; percentages of the total on a percentage chart.
    pub stack: Vec<f64>,
}

impl Point {
    /// Line `line`'s value.
    pub fn value(&self, line: usize) -> f64 {
        self.values.get(line).copied().unwrap_or(0.0)
    }

    /// The top of line `line`'s band in a stacked chart.
    pub fn stack_top(&self, line: usize) -> f64 {
        self.stack.get(line).copied().unwrap_or(0.0)
    }

    /// The bottom of line `line`'s band in a stacked chart.
    pub fn stack_base(&self, line: usize) -> f64 {
        line.checked_sub(1)
            .map_or(0.0, |below| self.stack_top(below))
    }

    /// The stack's top, the total of every shown line.
    pub fn total(&self) -> f64 {
        self.stack.last().copied().unwrap_or(0.0)
    }
}

/// The points of `chart` from `first` to `last` (inclusive, clamped to
/// the data) with only the `enabled` lines counted.
pub(super) fn points(
    chart: &StatChart,
    kind: ChartKind,
    enabled: &[bool],
    first: usize,
    last: usize,
) -> Vec<Point> {
    let Some(end) = chart.x.len().checked_sub(1).map(|back| last.min(back)) else {
        return Vec::new();
    };
    let offset = quill::local_time::utc_offset_at(quill::local_time::now_unix());
    let on = |k: usize| enabled.get(k).copied().unwrap_or(true);
    let mut seen = HashSet::new();
    (first.min(end)..=end)
        .map(|index| {
            let values: Vec<f64> = chart
                .lines
                .iter()
                .enumerate()
                .map(|(k, line)| match line.y.get(index) {
                    Some(v) if on(k) => *v as f64,
                    _ => 0.0,
                })
                .collect();
            let mut stack: Vec<f64> = values
                .iter()
                .scan(0.0, |sum, v| {
                    *sum += v;
                    Some(*sum)
                })
                .collect();
            if kind == ChartKind::StackLinear {
                let total = stack.last().copied().unwrap_or(0.0);
                for top in &mut stack {
                    *top = if total > 0.0 {
                        *top / total * 100.0
                    } else {
                        0.0
                    };
                }
            }
            let mut label = chart.day_label(index, offset);
            // The x scale needs distinct labels; a repeated one falls back
            // to the full title.
            if !seen.insert(label.clone()) {
                label = chart.point_title(index, offset);
                seen.insert(label.clone());
            }
            Point {
                index,
                label: label.into(),
                values,
                stack,
            }
        })
        .collect()
}

/// The value axis's domain for `points`: 0–100 on a percentage chart,
/// from zero to the tallest stack on stacked bars, around the shown
/// values of `lines` otherwise.
pub(super) fn value_domain(
    points: &[Point],
    kind: ChartKind,
    enabled: &[bool],
    lines: impl Iterator<Item = usize> + Clone,
) -> (f64, f64) {
    match kind {
        ChartKind::StackLinear => (0.0, 100.0),
        ChartKind::StackBar => {
            let top = points.iter().map(Point::total).fold(0.0, f64::max);
            nice_domain(0.0, top, VALUE_INTERVALS)
        }
        ChartKind::Linear | ChartKind::Bar | ChartKind::DoubleLinear => {
            match value_extent(points, enabled, lines) {
                Some((min, max)) => nice_domain(min, max, VALUE_INTERVALS),
                None => (0.0, 1.0),
            }
        }
    }
}

/// The smallest and largest value of the shown `lines` over `points`.
pub(super) fn value_extent(
    points: &[Point],
    enabled: &[bool],
    lines: impl Iterator<Item = usize> + Clone,
) -> Option<(f64, f64)> {
    let on = |k: &usize| enabled.get(*k).copied().unwrap_or(true);
    let (min, max) = points
        .iter()
        .flat_map(|p| lines.clone().filter(on).map(|k| p.value(k)))
        .fold((f64::MAX, f64::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)));
    (min <= max).then_some((min, max))
}

/// Whether a click on point `p` can zoom: tdesktop needs a positive value.
pub(super) fn zoomable_point(p: &Point) -> bool {
    p.values.iter().any(|v| *v > 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use quill::stat_chart::parse_chart;

    /// Two lines over three days, 2024-10-01 onward.
    fn chart() -> StatChart {
        let json = r##"{"columns":[["x",1727740800000,1727827200000,1727913600000],
            ["y0",10,20,30],["y1",30,20,10]],
            "types":{"x":"x","y0":"line","y1":"line"},
            "names":{"y0":"A","y1":"B"},
            "colors":{"y0":"#3497ED","y1":"#F34C44"}}"##;
        parse_chart(json).expect("a valid chart")
    }

    #[test]
    fn stacks_add_up_and_a_switched_off_line_counts_as_zero() {
        let chart = chart();
        let all = points(&chart, ChartKind::StackBar, &[true, true], 0, 9);
        assert_eq!(all.len(), 3, "the range is clamped to the data");
        assert_eq!(all[0].stack, vec![10.0, 40.0]);
        assert_eq!(all[0].stack_base(1), 10.0);
        assert_eq!(all[0].total(), 40.0);
        let one = points(&chart, ChartKind::StackBar, &[false, true], 1, 1);
        assert_eq!(one[0].values, vec![0.0, 20.0]);
        let shares = points(&chart, ChartKind::StackLinear, &[true, true], 0, 0);
        assert_eq!(shares[0].stack, vec![25.0, 100.0]);
        let labels: HashSet<_> = all.iter().map(|p| p.label.clone()).collect();
        assert_eq!(labels.len(), 3, "labels are unique");
    }

    #[test]
    fn value_domains_round_out_per_kind() {
        let chart = chart();
        let all = points(&chart, ChartKind::Linear, &[true, true], 0, 2);
        assert_eq!(
            value_domain(&all, ChartKind::StackLinear, &[true, true], 0..2),
            (0.0, 100.0)
        );
        let (low, high) = value_domain(&all, ChartKind::StackBar, &[true, true], 0..2);
        assert!(low == 0.0 && high >= 40.0);
        let (low, high) = value_domain(&all, ChartKind::Linear, &[true, false], 0..2);
        assert!(low <= 10.0 && high >= 30.0);
        assert_eq!(value_extent(&all, &[false, false], 0..2), None);
    }
}
