//! A chart's interaction state, kept per chart with
//! `Window::use_keyed_state` (notifying it redraws the panel): which
//! lines are on, the range the kit slider under the chart selects, the
//! point the pointer or the keyboard is on, and the pie a percentage
//! chart zooms into (tdesktop `ChartWidget`, `processLocalZoom`).

use gpui_kit::component::slider::{SliderEvent, SliderState, SliderValue};
use gpui_kit::*;
use quill::stat_chart::{ChartKind, StatChart, local_zoom_window, toggle_line};
use std::cell::Cell;
use std::rc::Rc;

/// A percentage chart zoomed into a pie of a few days.
pub(super) struct PieZoom {
    /// The days the pie's slider spans.
    pub window: (usize, usize),
    /// The selected days, as x indices.
    pub slider: Entity<SliderState>,
}

pub(super) struct ChartState {
    chart: Rc<StatChart>,
    kind: ChartKind,
    pub(super) enabled: Vec<bool>,
    /// The visible range as x indices; `None` when the chart has no
    /// range selector.
    pub(super) range: Option<Entity<SliderState>>,
    /// The point the kit tooltip shows, set while the tooltip draws;
    /// a click on the plot zooms into it.
    pub(super) hovered: Rc<Cell<Option<usize>>>,
    /// The point picked with the arrow keys.
    pub(super) key_point: Option<usize>,
    pub(super) pie: Option<PieZoom>,
    pub(super) focus: FocusHandle,
    _range_changes: Option<Subscription>,
    _pie_changes: Option<Subscription>,
}

/// A range slider over x indices `min..=max` starting at `value`.
fn index_slider(
    min: usize,
    max: usize,
    value: (usize, usize),
    cx: &mut App,
) -> Entity<SliderState> {
    cx.new(|_| {
        SliderState::new()
            .min(min as f32)
            .max(max as f32)
            .step(1.0)
            .default_value((value.0 as f32, value.1 as f32))
    })
}

/// The indices a range slider holds, clamped to `min..=max` and ordered.
fn slider_indices(slider: &SliderState, min: usize, max: usize) -> (usize, usize) {
    let (a, b) = match slider.value() {
        SliderValue::Range(a, b) => (a, b),
        SliderValue::Single(v) => (v, v),
    };
    let index = |v: f32| {
        let v = if v.is_finite() {
            v.round().max(0.0)
        } else {
            0.0
        };
        (v as usize).clamp(min, max)
    };
    let (a, b) = (index(a), index(b));
    (a.min(b), a.max(b))
}

impl ChartState {
    pub(super) fn new(chart: &Rc<StatChart>, kind: ChartKind, cx: &mut Context<Self>) -> Self {
        let mut state = Self {
            chart: chart.clone(),
            kind,
            enabled: chart.lines.iter().map(|l| !l.hidden_on_start).collect(),
            range: None,
            hovered: Rc::new(Cell::new(None)),
            key_point: None,
            pie: None,
            focus: cx.focus_handle(),
            _range_changes: None,
            _pie_changes: None,
        };
        state.reset_range(cx);
        state
    }

    fn reset_range(&mut self, cx: &mut Context<Self>) {
        let len = self.chart.x.len();
        if self.chart.footer_hidden || len < 2 {
            self.range = None;
            self._range_changes = None;
            return;
        }
        let back = len - 1;
        let (first, last) = self.chart.default_zoom;
        let (first, last) = (first.min(back), last.min(back));
        let slider = index_slider(0, back, (first.min(last), first.max(last)), cx);
        self._range_changes = Some(cx.subscribe(&slider, |state, _, _: &SliderEvent, cx| {
            state.key_point = None;
            cx.notify();
        }));
        self.range = Some(slider);
    }

    /// Called on every render: a refreshed graph of different data
    /// starts over.
    pub(super) fn sync(&mut self, chart: &Rc<StatChart>, kind: ChartKind, cx: &mut Context<Self>) {
        if Rc::ptr_eq(&self.chart, chart) && self.kind == kind {
            return;
        }
        let same = *self.chart == **chart && self.kind == kind;
        self.chart = chart.clone();
        if !same {
            self.kind = kind;
            self.enabled = chart.lines.iter().map(|l| !l.hidden_on_start).collect();
            self.key_point = None;
            self.pie = None;
            self._pie_changes = None;
            self.reset_range(cx);
        }
    }

    /// The first and last point shown.
    pub(super) fn visible(&self, cx: &App) -> (usize, usize) {
        let back = self.chart.x.len().saturating_sub(1);
        let Some(slider) = self.range.as_ref() else {
            return (0, back);
        };
        let (first, last) = slider_indices(slider.read(cx), 0, back);
        // One point is no range: keep at least two in view.
        if first == last && back > 0 {
            if last < back {
                (first, last + 1)
            } else {
                (first.saturating_sub(1), last)
            }
        } else {
            (first, last)
        }
    }

    /// Switch line `index` on or off; the last line shown stays on.
    pub(super) fn toggle_line(&mut self, index: usize, cx: &mut Context<Self>) {
        if toggle_line(&mut self.enabled, index) {
            cx.notify();
        }
    }

    /// Move the keyboard's point by `delta` within the visible range.
    pub(super) fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        let (first, last) = self.visible(cx);
        let next = match self.key_point {
            Some(at) => at.saturating_add_signed(delta).clamp(first, last),
            None if delta < 0 => last,
            None => first,
        };
        self.key_point = Some(next);
        cx.notify();
    }

    pub(super) fn clear_key_point(&mut self, cx: &mut Context<Self>) {
        if self.key_point.take().is_some() {
            cx.notify();
        }
    }

    /// Whether a click zooms into a pie here instead of asking the
    /// server (tdesktop: percentage charts with a range selector).
    pub(super) fn zooms_locally(&self) -> bool {
        self.kind == ChartKind::StackLinear && self.range.is_some() && self.chart.x.len() > 2
    }

    /// Turn the areas into a pie of the day at `index` and the days
    /// around it.
    pub(super) fn enter_pie(&mut self, index: usize, cx: &mut Context<Self>) {
        let window = local_zoom_window(self.chart.x.len(), index);
        let at = index.clamp(window.0, window.1);
        let slider = index_slider(window.0, window.1, (at, at), cx);
        self._pie_changes = Some(cx.subscribe(&slider, |_, _, _: &SliderEvent, cx| cx.notify()));
        self.pie = Some(PieZoom { window, slider });
        self.key_point = None;
        cx.notify();
    }

    pub(super) fn exit_pie(&mut self, cx: &mut Context<Self>) {
        if self.pie.take().is_some() {
            self._pie_changes = None;
            cx.notify();
        }
    }

    /// The days the pie sums.
    pub(super) fn pie_days(&self, cx: &App) -> Option<(usize, usize)> {
        let pie = self.pie.as_ref()?;
        Some(slider_indices(
            pie.slider.read(cx),
            pie.window.0,
            pie.window.1,
        ))
    }
}
