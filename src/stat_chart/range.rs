//! Visible-range math: which points a range shows, the y limits for
//! them, which point the pointer is over, and the range selector under
//! the chart (tdesktop `data_statistics_chart.cpp`, `abstract_chart_view.cpp`,
//! `ChartWidget::Footer`).

use super::{ChartKind, StatChart};

/// A closed interval.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    pub min: f64,
    pub max: f64,
}

impl Limits {
    pub const fn new(min: f64, max: f64) -> Self {
        Self { min, max }
    }

    pub fn span(&self) -> f64 {
        self.max - self.min
    }

    /// `value` mapped to 0..=1 inside the interval (0 for an empty one).
    pub fn ratio(&self, value: f64) -> f64 {
        let span = self.span();
        if span == 0.0 {
            0.0
        } else {
            (value - self.min) / span
        }
    }

    /// Linear blend towards `to` (`t` in 0..=1).
    pub fn lerp(&self, to: &Limits, t: f64) -> Limits {
        Limits::new(
            self.min + (to.min - self.min) * t,
            self.max + (to.max - self.max) * t,
        )
    }
}

impl StatChart {
    /// First index at or after the range start (`findStartIndex`).
    pub fn find_start_index(&self, v: f64) -> usize {
        let n = self.x_frac.len();
        if v <= 0.0 || n < 2 {
            return 0;
        }
        self.x_frac.partition_point(|x| *x < v).min(n - 1)
    }

    /// Last index at or before the range end, never before `left`
    /// (`findEndIndex`).
    pub fn find_end_index(&self, left: usize, v: f64) -> usize {
        let n = self.x_frac.len();
        if n == 0 {
            return 0;
        }
        if v >= 1.0 {
            return n - 1;
        }
        let after = self.x_frac.partition_point(|x| *x <= v);
        after.saturating_sub(1).max(left).min(n - 1)
    }

    /// The first and last index a range shows.
    pub fn visible_indices(&self, range: Limits) -> (usize, usize) {
        let start = self.find_start_index(range.min);
        (start, self.find_end_index(start, range.max))
    }
}

/// Per-line multipliers that put both lines of a double-linear chart on
/// one scale (`DoubleLineRatios`): the smaller line is stretched to the
/// larger one's maximum. Every other kind gets 1.
pub fn line_ratios(chart: &StatChart, kind: ChartKind) -> Vec<f64> {
    let mut ratios = vec![1.0; chart.lines.len()];
    if kind == ChartKind::DoubleLinear && chart.lines.len() == 2 {
        let (first, second) = (chart.lines[0].max as f64, chart.lines[1].max as f64);
        if first > second && second > 0.0 {
            ratios[1] = first / second;
        } else if second >= first && first > 0.0 {
            ratios[0] = second / first;
        }
    }
    ratios
}

/// Y limits of the whole chart and of the visible part.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeightLimits {
    pub full: Limits,
    pub ranged: Limits,
}

/// The y limits for `kind` with `enabled` lines over `indices`
/// (`heightLimits` of each chart view).
pub fn height_limits(
    chart: &StatChart,
    kind: ChartKind,
    enabled: &[bool],
    indices: (usize, usize),
) -> HeightLimits {
    let on = |i: usize| enabled.get(i).copied().unwrap_or(true);
    let (start, end) = (indices.0, indices.1.min(chart.x.len().saturating_sub(1)));
    match kind {
        ChartKind::StackLinear => HeightLimits {
            full: Limits::new(0.0, 100.0),
            ranged: Limits::new(0.0, 100.0),
        },
        ChartKind::StackBar => {
            let sum = |i: usize| -> i64 {
                (0..chart.lines.len())
                    .filter(|&k| on(k))
                    .map(|k| chart.lines[k].y[i])
                    .sum()
            };
            let full = (0..chart.x.len()).map(sum).max().unwrap_or(0);
            let ranged = (start..=end).map(sum).max().unwrap_or(0).max(1);
            HeightLimits {
                full: Limits::new(0.0, full as f64),
                ranged: Limits::new(0.0, ranged as f64),
            }
        }
        _ => {
            let ratios = line_ratios(chart, kind);
            let (mut min, mut max) = (f64::MAX, 0.0f64);
            let (mut full_min, mut full_max) = (f64::MAX, 0.0f64);
            for (k, line) in chart.lines.iter().enumerate() {
                if !on(k) {
                    continue;
                }
                let r = ratios[k];
                for &v in &line.y[start..=end] {
                    max = max.max((v as f64 * r).trunc());
                    min = min.min((v as f64 * r).trunc());
                }
                full_max = full_max.max((line.max as f64 * r).trunc());
                full_min = full_min.min((line.min as f64 * r).trunc());
            }
            if max == min || min == f64::MAX {
                min = chart.min as f64;
                max = chart.max as f64;
            }
            if full_min == f64::MAX {
                full_min = chart.min as f64;
                full_max = chart.max as f64;
            }
            HeightLimits {
                full: Limits::new(full_min, full_max),
                ranged: Limits::new(min, max),
            }
        }
    }
}

/// The point under a pointer at `frac` (0..=1 across the plot) for line
/// charts: the nearest x inside the range (`LinearChartView::findXIndexByPosition`).
pub fn nearest_index(chart: &StatChart, range: Limits, frac: f64) -> Option<usize> {
    if chart.x_frac.is_empty() || !(0.0..=1.0).contains(&frac) {
        return None;
    }
    let raw = range.min + range.span() * frac;
    let after = chart.x_frac.partition_point(|x| *x < raw);
    let mut best = if after == 0 {
        0
    } else if after >= chart.x_frac.len() {
        chart.x_frac.len() - 1
    } else {
        let left = raw - chart.x_frac[after - 1];
        let right = chart.x_frac[after] - raw;
        if right > left { after - 1 } else { after }
    };
    if chart.x_frac[best] > range.max && best > 0 {
        best -= 1;
    } else if chart.x_frac[best] < range.min && best + 1 < chart.x_frac.len() {
        best += 1;
    }
    Some(best)
}

/// Column geometry of a bar chart: the left edge of the first painted
/// column (index `start`) and the column width, in plot pixels
/// (`ComputeLeftStartAndStep`).
pub fn bar_columns(chart: &StatChart, range: Limits, width: f64, start: usize) -> (f64, f64) {
    let full_width = if range.span() > 0.0 {
        width / range.span()
    } else {
        width
    };
    let first = chart.x_frac.get(1).copied().unwrap_or(1.0);
    let p = first * full_width;
    let step = (first * (full_width - p)).max(0.0);
    let left =
        chart.x_frac.get(start).copied().unwrap_or(0.0) * (full_width - p) - full_width * range.min;
    (left, step)
}

/// The bar under a pointer at `x` plot pixels.
pub fn bar_index_at(chart: &StatChart, range: Limits, width: f64, x: f64) -> Option<usize> {
    if chart.x.is_empty() || x < 0.0 || x > width {
        return None;
    }
    let (start, end) = chart.visible_indices(range);
    let start = start.saturating_sub(2);
    let end = (end + 2).min(chart.x.len() - 1);
    let (left, step) = bar_columns(chart, range, width, start);
    if step <= 0.0 {
        return None;
    }
    (start..=end).find(|&i| {
        let column = left + (i - start) as f64 * step;
        x >= column && x < column + step
    })
}

/// Switch line `index` on or off from the legend. The last shown line
/// stays on (tdesktop shakes its switch); `false` when refused.
pub fn toggle_line(enabled: &mut [bool], index: usize) -> bool {
    let Some(&on) = enabled.get(index) else {
        return false;
    };
    if on && enabled.iter().filter(|e| **e).count() <= 1 {
        return false;
    }
    enabled[index] = !on;
    true
}

/// Pixel size of the range selector's handles (`statisticsChartFooter*`).
pub const FOOTER_SIDE: f64 = 10.0;
/// The smallest gap between the two handles.
pub const FOOTER_MIN_GAP: f64 = 5.0;

/// The range selector under the chart: two handles over a mini chart.
/// Positions are pixels across the selector, as in tdesktop's `Footer`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RangeSelector {
    width: f64,
    /// Left edge of the left handle.
    left: f64,
    /// Left edge of the right handle.
    right: f64,
}

/// What a press on the selector grabbed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectorGrab {
    Left,
    Right,
    Middle,
    /// Outside the window: a click there moves the window to it.
    Outside,
}

impl RangeSelector {
    /// A selector `width` pixels wide showing `range`.
    pub fn new(width: f64, range: Limits) -> Self {
        let mut selector = Self {
            width,
            left: 0.0,
            right: width - FOOTER_SIDE,
        };
        selector.set_range(range);
        selector
    }

    fn between(&self) -> f64 {
        (self.width - FOOTER_SIDE * 2.0).max(0.0)
    }

    /// The selected range as fractions of the span.
    pub fn range(&self) -> Limits {
        let between = self.between();
        if between <= 0.0 {
            return Limits::new(0.0, 1.0);
        }
        Limits::new(
            (self.left / between).clamp(0.0, 1.0),
            ((self.right - FOOTER_SIDE) / between).clamp(0.0, 1.0),
        )
    }

    pub fn set_range(&mut self, range: Limits) {
        let between = self.between();
        // Open the window fully first so neither handle is clamped by the
        // other's old position.
        self.left = 0.0;
        self.right = self.width - FOOTER_SIDE;
        self.move_side(true, range.min * between);
        self.move_side(false, range.max * between + FOOTER_SIDE);
    }

    /// Left edges of the two handles.
    pub fn handles(&self) -> (f64, f64) {
        (self.left, self.right)
    }

    pub fn width(&self) -> f64 {
        self.width
    }

    /// Resize, keeping the selected range.
    pub fn resize(&mut self, width: f64) {
        if (width - self.width).abs() > f64::EPSILON {
            let range = self.range();
            self.width = width;
            self.set_range(range);
        }
    }

    /// What a press at `x` grabs.
    pub fn grab_at(&self, x: f64) -> SelectorGrab {
        if (self.left..=self.left + FOOTER_SIDE).contains(&x) {
            SelectorGrab::Left
        } else if (self.right..=self.right + FOOTER_SIDE).contains(&x) {
            SelectorGrab::Right
        } else if x < self.left || x > self.right + FOOTER_SIDE {
            SelectorGrab::Outside
        } else {
            SelectorGrab::Middle
        }
    }

    /// Offset of a press from the grabbed handle's left edge
    /// (`_diffBetweenStartAndSide`).
    pub fn grab_offset(&self, grab: SelectorGrab, x: f64) -> f64 {
        match grab {
            SelectorGrab::Right => x - self.right,
            _ => x - self.left,
        }
    }

    /// Drag to `x` with the grab's offset.
    pub fn drag(&mut self, grab: SelectorGrab, x: f64, offset: f64) {
        let to = x - offset;
        match grab {
            SelectorGrab::Left => self.move_side(true, to),
            SelectorGrab::Right => self.move_side(false, to),
            SelectorGrab::Middle => self.move_center(to <= self.left, x, offset),
            SelectorGrab::Outside => {}
        }
    }

    /// Center the window on `x` (a click outside it; tdesktop animates
    /// this move).
    pub fn center_on(&mut self, x: f64) {
        let start = self.left + (self.right + FOOTER_SIDE - self.left) / 2.0;
        let offset = start - self.left;
        self.move_center(x <= start, x, offset);
    }

    fn move_center(&mut self, to_left: bool, x: f64, offset: f64) {
        let to = x - offset;
        let gap = (self.right - self.left).max(FOOTER_MIN_GAP);
        if to_left {
            self.move_side(true, to);
            self.move_side(false, self.left + gap);
        } else {
            self.move_side(false, to + gap);
            self.move_side(true, self.right - gap);
        }
    }

    fn move_side(&mut self, left: bool, x: f64) {
        let usable = self.width - FOOTER_SIDE;
        if usable < 2.0 * FOOTER_SIDE + FOOTER_MIN_GAP {
            return;
        }
        if left {
            let limit = self.right - FOOTER_SIDE - FOOTER_MIN_GAP;
            let max = if limit <= 0.0 { self.between() } else { limit };
            self.left = x.clamp(0.0, max.max(0.0));
        } else {
            let min = self.left + FOOTER_SIDE + FOOTER_MIN_GAP;
            self.right = x.clamp(min.min(usable), usable);
        }
    }
}
