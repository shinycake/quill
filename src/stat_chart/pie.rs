//! A percentage chart zoomed into a few days becomes a pie of the shares
//! over the selected days (tdesktop `StackLinearChartView::maybeLocalZoom`,
//! `PiePartsPercentage`, `FindStackXIndicesFromRawXPercentages`).

use super::{Limits, StatChart};

/// One slice of the pie.
#[derive(Debug, Clone, PartialEq)]
pub struct PiePart {
    /// Share of the whole, 0..=1.
    pub share: f64,
    /// `42%`.
    pub text: String,
}

/// Days around the zoomed point that the range selector then covers.
const LIMIT_LENGTH: usize = 8;

/// Shares of `sums` (`PiePartsPercentage`). With `round`, each share is
/// rounded to a whole percent and the rounding error goes to the part
/// rounded the most, so the labels add up to 100%.
pub fn pie_parts(sums: &[f64], round: bool) -> Vec<PiePart> {
    let total: f64 = sums.iter().sum();
    let mut shares = Vec::with_capacity(sums.len());
    let mut diff_sum = 0.0;
    let (mut max_diff, mut max_index) = (0.0, None);
    for (k, sum) in sums.iter().enumerate() {
        let raw = if total > 0.0 { sum / total } else { 0.0 };
        let share = if round {
            (raw * 100.0).round() * 0.01
        } else {
            raw
        };
        diff_sum += raw - share;
        if (raw - share).abs() > max_diff {
            max_diff = (raw - share).abs();
            max_index = Some(k);
        }
        shares.push(share);
    }
    // tdesktop's min and max trackers both keep the largest error, so
    // the correction always lands on that part.
    if round && let Some(index) = max_index {
        shares[index] += diff_sum;
    }
    shares
        .into_iter()
        .map(|share| PiePart {
            share,
            text: format!("{}%", (share * 100.0 + 1e-9) as i64),
        })
        .collect()
}

/// Each line's total over points `start..=end`, scaled by its opacity
/// (`PiePartsPercentageByIndices`).
pub fn pie_sums(chart: &StatChart, alpha: &[f64], indices: (usize, usize)) -> Vec<f64> {
    let end = indices.1.min(chart.x.len().saturating_sub(1));
    chart
        .lines
        .iter()
        .enumerate()
        .map(|(k, line)| {
            let sum: i64 = (indices.0..=end).map(|i| line.y[i]).sum();
            sum as f64 * alpha.get(k).copied().unwrap_or(1.0)
        })
        .collect()
}

/// Start and end of each slice in degrees, clockwise from the top.
pub fn pie_angles(parts: &[PiePart]) -> Vec<(f64, f64)> {
    let mut start = 0.0;
    parts
        .iter()
        .map(|part| {
            let end = start + part.share * 360.0;
            let slice = (start, end);
            start = end;
            slice
        })
        .collect()
}

/// The slice at offset (`dx`, `dy`) from the centre (y down), by angle
/// alone, as tdesktop picks it.
pub fn pie_part_at(parts: &[PiePart], dx: f64, dy: f64) -> Option<usize> {
    if dx == 0.0 && dy == 0.0 {
        return None;
    }
    // Clockwise from the top.
    let angle = (dx.atan2(-dy).to_degrees() + 360.0) % 360.0;
    pie_angles(parts)
        .iter()
        .position(|(start, end)| angle > *start && angle <= *end)
        .or_else(|| (angle == 0.0).then_some(0))
}

/// A local zoom into a percentage chart: the points the selector covers
/// and the part of them selected first.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocalZoom {
    /// First and last point the selector spans.
    pub limit: (usize, usize),
    /// The selected part, as fractions of the selector.
    pub range: Limits,
}

/// Zoom into the day at `x_index`: eight days around it, that day
/// selected.
pub fn local_zoom(chart: &StatChart, x_index: usize) -> LocalZoom {
    let n = chart.x_frac.len();
    if n < 2 {
        return LocalZoom {
            limit: (0, n.saturating_sub(1)),
            range: Limits::new(0.0, 1.0),
        };
    }
    let back = n - 1;
    let x_index = x_index.min(back);
    let (left_side, right_side) = (LIMIT_LENGTH / 2, LIMIT_LENGTH / 2 + 1);
    let limit = if x_index < left_side {
        (0, (left_side + right_side).min(back))
    } else if x_index + right_side > back {
        (back.saturating_sub(LIMIT_LENGTH), back)
    } else {
        (x_index - left_side, x_index + right_side)
    };
    let neighbour = if x_index == back {
        x_index - 1
    } else {
        x_index + 1
    };
    let one_day = (chart.x_frac[x_index] - chart.x_frac[neighbour]).abs();
    let offset = if limit.1 == back { -one_day } else { 0.0 };
    let window = Limits::new(chart.x_frac[limit.0], chart.x_frac[limit.1]);
    let at = chart.x_frac[x_index];
    LocalZoom {
        limit,
        range: Limits::new(
            window.ratio(at + one_day * 0.25 + offset),
            window.ratio(at + one_day * 0.75 + offset),
        ),
    }
}

/// The points a selector `range` covers inside a local zoom
/// (`FindStackXIndicesFromRawXPercentages`): the right edge is shifted one
/// point left so the last point can still be shown.
pub fn local_zoom_indices(
    chart: &StatChart,
    range: Limits,
    limit: (usize, usize),
) -> (usize, usize) {
    let n = chart.x_frac.len();
    if n == 0 {
        return (0, 0);
    }
    let window = Limits::new(chart.x_frac[limit.0], chart.x_frac[limit.1.min(n - 1)]);
    let offset: i64 = if window.max == 1.0 { 0 } else { -1 };
    let right_shrink = if range.max == 1.0 && window.max != 1.0 {
        1
    } else {
        0
    };
    let zoomed = Limits::new(
        window.min + window.span() * range.min,
        window.min + window.span() * range.max,
    );
    let mut min_it: i64 = -1;
    let mut max_it: i64 = n as i64;
    for (i, x) in chart.x_frac.iter().enumerate() {
        if min_it < 0 && *x > zoomed.min {
            min_it = i as i64;
        }
        if max_it >= n as i64 && *x > zoomed.max {
            max_it = i as i64;
        }
    }
    let low = limit.0 as i64;
    let high = (limit.1 as i64 - right_shrink).max(low);
    (
        (min_it + offset).clamp(low, high) as usize,
        (max_it + offset).clamp(low, high) as usize,
    )
}
