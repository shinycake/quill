//! Horizontal rulers with their captions and the x-axis date labels
//! (tdesktop `chart_rulers_data.cpp`, `ChartWidget::updateBottomDates`).

use super::{ChartKind, Limits, StatChart, ruler_caption};

/// One horizontal ruler.
#[derive(Debug, Clone, PartialEq)]
pub struct Ruler {
    /// The y value the ruler sits at.
    pub value: f64,
    /// Caption on the left.
    pub caption: String,
    /// Caption on the right (double-linear charts: the other line's
    /// scale); empty when there is none.
    pub right_caption: String,
}

const MIN_LINES: i64 = 2;
const MAX_LINES: i64 = 6;
const STEP: f64 = 5.0;

/// The rulers for y limits `limits` (`ChartRulersData` with
/// `useMinHeight`). `right_ratio` > 0 adds the scaled captions of a
/// double-linear chart's second line.
pub fn rulers(limits: Limits, right_ratio: f64) -> Vec<Ruler> {
    let max = limits.max as i64;
    let mut min = limits.min as i64;
    let diff = max - min;
    let (n, step) = if diff == 0 {
        min -= 1;
        (MAX_LINES / 2, 1.0)
    } else if diff < MAX_LINES {
        ((diff + 1).max(MIN_LINES), 1.0)
    } else if diff / 2 < MAX_LINES {
        (diff / 2 + diff % 2 + 1, 2.0)
    } else {
        let step = (max - min) as f64 / STEP;
        if step <= 0.0 {
            ((max - min + 1).max(MIN_LINES), 1.0)
        } else {
            (6, step)
        }
    };
    let skip_floats = right_ratio > 0.0 && step / right_ratio < 1.0;
    (0..n)
        .map(|i| {
            let value = min + (i as f64 * step) as i64;
            let right_caption = if right_ratio > 0.0 {
                let v = (min as f64 + i as f64 * step) / right_ratio;
                if !skip_floats || v - v.trunc() < 0.01 {
                    ruler_caption(v as i64)
                } else {
                    String::new()
                }
            } else {
                String::new()
            };
            Ruler {
                value: value as f64,
                caption: ruler_caption(value),
                right_caption,
            }
        })
        .collect()
}

/// Rulers for a chart of `kind` with y limits `limits`: a double-linear
/// chart puts the scaled line's captions where tdesktop does (the
/// stretched line reads on its own side).
pub fn chart_rulers(chart: &StatChart, kind: ChartKind, limits: Limits) -> Vec<Ruler> {
    let double = kind == ChartKind::DoubleLinear && chart.lines.len() == 2;
    if !double {
        return rulers(limits, 0.0);
    }
    let (first, second) = (chart.lines[0].max as f64, chart.lines[1].max as f64);
    let left_scaled = first <= second;
    let ratio = if left_scaled {
        if first > 0.0 { second / first } else { 1.0 }
    } else if second > 0.0 {
        first / second
    } else {
        1.0
    };
    let mut lines = rulers(limits, ratio);
    if left_scaled {
        for ruler in &mut lines {
            std::mem::swap(&mut ruler.caption, &mut ruler.right_caption);
        }
    }
    lines
}

/// Every how many points an x label is drawn, for a plot `width` pixels
/// wide showing `range` (`updateBottomDates`: about six labels, a power
/// of two so labels stay put while the range moves).
pub fn label_step(chart: &StatChart, range: Limits, width: f64) -> usize {
    if range.span() <= 0.0 || width <= 0.0 {
        return 1;
    }
    let full_width = width / range.span();
    let per_point = full_width * chart.step_fraction();
    if per_point <= 0.0 {
        return 1;
    }
    let raw = ((width / per_point) / 6.0) as u32;
    let highest = if raw == 0 {
        0
    } else {
        1 << (31 - raw.leading_zeros())
    };
    ((highest as usize) << 1).max(1)
}

/// The points that get an x label for `range` on a plot `width` pixels
/// wide, with `label_width` pixels per label. Includes a point past each
/// edge so labels slide in.
pub fn label_indices(chart: &StatChart, range: Limits, width: f64, label_width: f64) -> Vec<usize> {
    label_indices_for_step(
        chart,
        range,
        width,
        label_width,
        label_step(chart, range, width),
    )
}

/// [`label_indices`] for a given step: the labels of a step that is
/// fading out keep their places while the new step fades in.
pub fn label_indices_for_step(
    chart: &StatChart,
    range: Limits,
    width: f64,
    label_width: f64,
    step: usize,
) -> Vec<usize> {
    let n = chart.x.len();
    if n < 2 {
        return Vec::new();
    }
    let step = step.max(1) as i64;
    let per_point = ((width / n as f64) as i64).max(1);
    let offset = label_width as i64 / per_point;
    let (start, end) = chart.visible_indices(range);
    let from = (start as i64 - step).max(0);
    let to = (end as i64 + step).min(n as i64 - 2);
    (from..=to)
        .filter(|i| (i - offset).rem_euclid(step) == 0)
        .map(|i| i as usize)
        .collect()
}
