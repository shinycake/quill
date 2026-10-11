//! A chart's interaction state: the visible range, which lines are on,
//! the hovered or pinned point, a range-selector drag, and the short
//! animations of the y scale and line opacity. Kept per chart with
//! `Window::use_keyed_state`; notifying it redraws the panel.

use super::paint::SharedCache;
use gpui_kit::*;
use quill::stat_chart::{
    ChartKind, Limits, RangeSelector, SelectorGrab, StatChart, bar_index_at, height_limits,
    nearest_index, toggle_line,
};
use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Length of a y-scale or line-opacity change (tdesktop's height
/// animation runs about this long at 60 fps).
const TWEEN: Duration = Duration::from_millis(240);
/// One frame of the chart's own clock while something animates.
const FRAME: Duration = Duration::from_millis(16);

/// A value easing towards a target; restarting from the sampled value
/// keeps an interrupted change smooth.
#[derive(Clone, Copy)]
struct Tween {
    from: f64,
    to: f64,
    start: Option<Instant>,
}

impl Tween {
    fn at(value: f64) -> Self {
        Self {
            from: value,
            to: value,
            start: None,
        }
    }

    fn value(&self, now: Instant) -> f64 {
        let Some(start) = self.start else {
            return self.to;
        };
        let t = (now.saturating_duration_since(start).as_secs_f64() / TWEEN.as_secs_f64()).min(1.0);
        let eased = 1.0 - (1.0 - t).powi(3);
        self.from + (self.to - self.from) * eased
    }

    fn running(&self, now: Instant) -> bool {
        self.start
            .is_some_and(|start| now.saturating_duration_since(start) < TWEEN)
    }

    fn retarget(&mut self, to: f64, now: Instant, animate: bool) {
        if (to - self.to).abs() < f64::EPSILON {
            return;
        }
        if animate {
            self.from = self.value(now);
            self.start = Some(now);
        } else {
            self.from = to;
            self.start = None;
        }
        self.to = to;
    }
}

#[derive(Clone, Copy)]
struct TweenLimits {
    min: Tween,
    max: Tween,
}

impl TweenLimits {
    fn at(limits: Limits) -> Self {
        Self {
            min: Tween::at(limits.min),
            max: Tween::at(limits.max),
        }
    }

    fn value(&self, now: Instant) -> Limits {
        Limits::new(self.min.value(now), self.max.value(now))
    }

    fn running(&self, now: Instant) -> bool {
        self.min.running(now) || self.max.running(now)
    }

    fn retarget(&mut self, to: Limits, now: Instant, animate: bool) {
        self.min.retarget(to.min, now, animate);
        self.max.retarget(to.max, now, animate);
    }
}

/// What one frame of the chart draws.
pub(super) struct Snapshot {
    pub range: Limits,
    pub enabled: Vec<bool>,
    /// Each line's opacity (fading when switched).
    pub alpha: Vec<f32>,
    /// Y limits of the plot and of the range selector's mini chart.
    pub y: Limits,
    pub footer_y: Limits,
    /// Y limits the plot is heading to (its rulers).
    pub y_target: Limits,
    pub hover: Option<usize>,
    pub plot_bounds: Rc<Cell<Bounds<Pixels>>>,
    pub footer_bounds: Rc<Cell<Bounds<Pixels>>>,
    pub focus: FocusHandle,
    pub plot_cache: SharedCache,
    pub footer_cache: SharedCache,
}

pub(super) struct ChartState {
    chart: Rc<StatChart>,
    kind: ChartKind,
    range: Limits,
    enabled: Vec<bool>,
    alpha: Vec<Tween>,
    y: TweenLimits,
    footer_y: TweenLimits,
    hover: Option<usize>,
    pinned: bool,
    drag: Option<(RangeSelector, SelectorGrab, f64)>,
    plot_bounds: Rc<Cell<Bounds<Pixels>>>,
    footer_bounds: Rc<Cell<Bounds<Pixels>>>,
    focus: FocusHandle,
    plot_cache: SharedCache,
    footer_cache: SharedCache,
    ticking: bool,
}

fn animations_off() -> bool {
    quill::power_saving::on(quill::power_saving::Flag::Animations)
}

impl ChartState {
    pub(super) fn new(chart: &Rc<StatChart>, kind: ChartKind, cx: &mut Context<Self>) -> Self {
        let enabled: Vec<bool> = chart.lines.iter().map(|l| !l.hidden_on_start).collect();
        let range = chart.default_range();
        let mut state = Self {
            chart: chart.clone(),
            kind,
            range,
            alpha: enabled
                .iter()
                .map(|on| Tween::at(if *on { 1.0 } else { 0.0 }))
                .collect(),
            enabled,
            y: TweenLimits::at(Limits::new(0.0, 1.0)),
            footer_y: TweenLimits::at(Limits::new(0.0, 1.0)),
            hover: None,
            pinned: false,
            drag: None,
            plot_bounds: Rc::new(Cell::new(Bounds::default())),
            footer_bounds: Rc::new(Cell::new(Bounds::default())),
            focus: cx.focus_handle(),
            plot_cache: SharedCache::default(),
            footer_cache: SharedCache::default(),
            ticking: false,
        };
        state.retarget(false);
        state
    }

    /// A refreshed graph replaces the old one and starts over.
    pub(super) fn sync(&mut self, chart: &Rc<StatChart>, kind: ChartKind, cx: &mut Context<Self>) {
        if !Rc::ptr_eq(&self.chart, chart) && *self.chart != **chart || self.kind != kind {
            let focus = self.focus.clone();
            *self = Self::new(chart, kind, cx);
            self.focus = focus;
        } else if !Rc::ptr_eq(&self.chart, chart) {
            self.chart = chart.clone();
        }
    }

    pub(super) fn snapshot(&self) -> Snapshot {
        let now = Instant::now();
        Snapshot {
            range: self.range,
            enabled: self.enabled.clone(),
            alpha: self.alpha.iter().map(|a| a.value(now) as f32).collect(),
            y: self.y.value(now),
            footer_y: self.footer_y.value(now),
            y_target: Limits::new(self.y.min.to, self.y.max.to),
            hover: self.hover,
            plot_bounds: self.plot_bounds.clone(),
            footer_bounds: self.footer_bounds.clone(),
            focus: self.focus.clone(),
            plot_cache: self.plot_cache.clone(),
            footer_cache: self.footer_cache.clone(),
        }
    }

    pub(super) fn focus_handle(&self) -> FocusHandle {
        self.focus.clone()
    }

    /// The point under a pointer `x` pixels into a plot `width` wide.
    pub(super) fn index_at(&self, chart: &StatChart, x: f64, width: f64) -> Option<usize> {
        if width <= 0.0 {
            return None;
        }
        match self.kind {
            ChartKind::Bar | ChartKind::StackBar => bar_index_at(chart, self.range, width, x),
            _ => nearest_index(chart, self.range, x / width),
        }
    }

    fn animating(&self, now: Instant) -> bool {
        self.y.running(now)
            || self.footer_y.running(now)
            || self.alpha.iter().any(|a| a.running(now))
    }

    /// Point the y scales at the visible range and the shown lines.
    fn retarget(&mut self, animate: bool) {
        let now = Instant::now();
        let animate = animate && !animations_off();
        let indices = self.chart.visible_indices(self.range);
        let limits = height_limits(&self.chart, self.kind, &self.enabled, indices);
        if limits.ranged.min != limits.ranged.max {
            self.y.retarget(limits.ranged, now, animate);
        }
        self.footer_y.retarget(limits.full, now, animate);
    }

    /// Redraw for the next few frames while a change animates (the
    /// chart's own clock: it notifies only this state, so only the panel
    /// that shows the chart redraws, and stops when the change ends).
    fn tick(&mut self, cx: &mut Context<Self>) {
        cx.notify();
        if self.ticking || !self.animating(Instant::now()) {
            return;
        }
        self.ticking = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(FRAME).await;
                let more = this
                    .update(cx, |state, cx| {
                        cx.notify();
                        let more = state.animating(Instant::now());
                        state.ticking = more;
                        more
                    })
                    .unwrap_or(false);
                if !more {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn toggle_line(&mut self, index: usize, cx: &mut Context<Self>) {
        if !toggle_line(&mut self.enabled, index) {
            return;
        }
        let now = Instant::now();
        let target = if self.enabled[index] { 1.0 } else { 0.0 };
        self.alpha[index].retarget(target, now, !animations_off());
        self.retarget(true);
        self.tick(cx);
    }

    /// The pointer moved over the plot onto `index`.
    pub(super) fn hover(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        if self.pinned || self.drag.is_some() || self.hover == index {
            return;
        }
        self.hover = index;
        cx.notify();
    }

    /// A press on the plot pins the point under it; pressing the pinned
    /// point again lets it go (tdesktop shows the details on press).
    pub(super) fn press(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        if self.pinned && self.hover == index {
            self.pinned = false;
        } else {
            self.hover = index;
            self.pinned = index.is_some();
        }
        cx.notify();
    }

    /// The pointer left the plot.
    pub(super) fn leave(&mut self, cx: &mut Context<Self>) {
        if !self.pinned && self.hover.take().is_some() {
            cx.notify();
        }
    }

    /// Arrow keys: move the pinned point inside the visible range.
    pub(super) fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        let (start, end) = self.chart.visible_indices(self.range);
        let next = match self.hover {
            Some(i) => (i as isize + delta).clamp(start as isize, end as isize) as usize,
            None if delta < 0 => end,
            None => start,
        };
        self.hover = Some(next);
        self.pinned = true;
        cx.notify();
    }

    pub(super) fn clear(&mut self, cx: &mut Context<Self>) {
        self.pinned = false;
        if self.hover.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn selected(&self) -> Option<usize> {
        self.hover
    }

    fn selector(&self) -> Option<RangeSelector> {
        let width = f32::from(self.footer_bounds.get().size.width) as f64;
        (width > 0.0).then(|| RangeSelector::new(width, self.range))
    }

    fn set_range(&mut self, range: Limits, cx: &mut Context<Self>) {
        if range == self.range {
            return;
        }
        self.range = range;
        // tdesktop hides the details when the range moves.
        self.hover = None;
        self.pinned = false;
        self.retarget(true);
        self.tick(cx);
    }

    /// A press on the range selector at `x` pixels into it.
    pub(super) fn grab(&mut self, x: f64, cx: &mut Context<Self>) {
        let Some(mut selector) = self.selector() else {
            return;
        };
        let grab = selector.grab_at(x);
        if grab == SelectorGrab::Outside {
            // tdesktop animates this move; the range jumps here and the
            // y scale animates.
            selector.center_on(x);
            self.set_range(selector.range(), cx);
            return;
        }
        let offset = selector.grab_offset(grab, x);
        self.drag = Some((selector, grab, offset));
    }

    pub(super) fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    pub(super) fn drag_to(&mut self, x: f64, cx: &mut Context<Self>) {
        let Some((mut selector, grab, offset)) = self.drag else {
            return;
        };
        selector.drag(grab, x, offset);
        self.drag = Some((selector, grab, offset));
        self.set_range(selector.range(), cx);
    }

    pub(super) fn release(&mut self) {
        self.drag = None;
    }
}
