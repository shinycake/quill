//! A chart's interaction state: the visible range, which lines are on,
//! the point whose details are shown, a range-selector drag, the fades
//! of rulers, dates and details, and the short animations of the y scale
//! and line opacity. Kept per chart with `Window::use_keyed_state`;
//! notifying it redraws the panel. The pie zoom of percentage charts
//! lives in `state_zoom.rs`.

use super::paint::SharedCache;
use super::state_zoom::{LocalZoomState, LocalZoomView};
use super::tween::{Ease, Fades, Tween, TweenLimits};
use gpui_kit::*;
use quill::stat_chart::{
    ChartKind, Limits, RangeSelector, SelectorGrab, StatChart, bar_index_at, height_limits,
    label_step, nearest_index, toggle_line,
};
use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Length of a y-scale or line-opacity change (tdesktop's height
/// animation runs about this long at 60 fps).
const Y_TWEEN: Duration = Duration::from_millis(240);
/// The details box fades in and out (`_details.animation`, 200 ms).
const DETAILS_FADE: Duration = Duration::from_millis(200);
/// Date labels cross-fade when their step changes (200 ms).
const LABEL_FADE: Duration = Duration::from_millis(200);
/// tdesktop adds a new ruler set at most every 320 ms while dragging.
const RULER_MERGE: Duration = Duration::from_millis(320);
/// A click outside the selector's window moves it there
/// (`slideWrapDuration`, sine in-out).
const RANGE_MOVE: Duration = Duration::from_millis(150);
/// One frame of the chart's own clock while something animates.
const FRAME: Duration = Duration::from_millis(16);

/// What one frame of the chart draws.
pub(super) struct Snapshot {
    pub range: Limits,
    pub enabled: Vec<bool>,
    /// Each line's opacity (fading when switched).
    pub alpha: Vec<f32>,
    /// Y limits of the plot and of the range selector's mini chart.
    pub y: Limits,
    pub footer_y: Limits,
    /// Ruler sets (by the y limits they were made for) and their opacity.
    pub rulers: Vec<(Limits, f32)>,
    /// Date label steps and their opacity.
    pub labels: Vec<(usize, f32)>,
    /// The point whose details show, and the box's opacity.
    pub details: Option<usize>,
    pub details_alpha: f32,
    /// Where the selector's window is drawn (fractions of the selector).
    pub selector: Limits,
    /// The pie of a percentage chart zoomed into a few days.
    pub local: Option<LocalZoomView>,
    pub plot_bounds: Rc<Cell<Bounds<Pixels>>>,
    pub footer_bounds: Rc<Cell<Bounds<Pixels>>>,
    pub focus: FocusHandle,
    pub plot_cache: SharedCache,
    pub footer_cache: SharedCache,
}

pub(super) struct ChartState {
    pub(super) chart: Rc<StatChart>,
    pub(super) kind: ChartKind,
    range: TweenLimits,
    pub(super) enabled: Vec<bool>,
    pub(super) alpha: Vec<Tween>,
    y: TweenLimits,
    footer_y: TweenLimits,
    rulers: Fades<Limits>,
    labels: Fades<usize>,
    details: Option<usize>,
    details_alpha: Tween,
    pressing: bool,
    drag: Option<(RangeSelector, SelectorGrab, f64)>,
    pub(super) local: Option<LocalZoomState>,
    plot_bounds: Rc<Cell<Bounds<Pixels>>>,
    pub(super) footer_bounds: Rc<Cell<Bounds<Pixels>>>,
    focus: FocusHandle,
    plot_cache: SharedCache,
    footer_cache: SharedCache,
    ticking: bool,
}

impl ChartState {
    pub(super) fn new(chart: &Rc<StatChart>, kind: ChartKind, cx: &mut Context<Self>) -> Self {
        let enabled: Vec<bool> = chart.lines.iter().map(|l| !l.hidden_on_start).collect();
        let mut range = TweenLimits::at(chart.default_range(), RANGE_MOVE);
        range.min = Tween::at(range.min.to, RANGE_MOVE, Ease::InOutSine);
        range.max = Tween::at(range.max.to, RANGE_MOVE, Ease::InOutSine);
        let mut state = Self {
            chart: chart.clone(),
            kind,
            range,
            alpha: enabled
                .iter()
                .map(|on| Tween::at(if *on { 1.0 } else { 0.0 }, Y_TWEEN, Ease::OutCubic))
                .collect(),
            enabled,
            y: TweenLimits::at(Limits::new(0.0, 1.0), Y_TWEEN),
            footer_y: TweenLimits::at(Limits::new(0.0, 1.0), Y_TWEEN),
            rulers: Fades::new(Y_TWEEN),
            labels: Fades::new(LABEL_FADE),
            details: None,
            details_alpha: Tween::at(0.0, DETAILS_FADE, Ease::Linear),
            pressing: false,
            drag: None,
            local: None,
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

    /// Called on every render: a refreshed graph starts over; finished
    /// fades are dropped; the date step follows the plot's width.
    pub(super) fn sync(&mut self, chart: &Rc<StatChart>, kind: ChartKind, cx: &mut Context<Self>) {
        if !Rc::ptr_eq(&self.chart, chart) && *self.chart != **chart || self.kind != kind {
            let focus = self.focus.clone();
            *self = Self::new(chart, kind, cx);
            self.focus = focus;
        } else if !Rc::ptr_eq(&self.chart, chart) {
            self.chart = chart.clone();
        }
        self.settle(Instant::now());
        self.update_labels(true);
    }

    fn settle(&mut self, now: Instant) {
        self.rulers.settle(now);
        self.labels.settle(now);
        if self.details_alpha.to == 0.0 && !self.details_alpha.running(now) {
            self.details = None;
        }
        self.settle_zoom(now);
    }

    /// The range the plot shows right now.
    pub(super) fn range_now(&self) -> Limits {
        self.range.value(Instant::now())
    }

    pub(super) fn plot_width(&self) -> f64 {
        f32::from(self.plot_bounds.get().size.width) as f64
    }

    fn update_labels(&mut self, animate: bool) {
        let width = self.plot_width();
        if width <= 0.0 {
            return;
        }
        let step = label_step(&self.chart, self.range.target(), width);
        self.labels
            .show(step, Instant::now(), animate, Duration::ZERO);
    }

    pub(super) fn snapshot(&self) -> Snapshot {
        let now = Instant::now();
        let range = self.range.value(now);
        let local = self.local_view(now);
        let selector = match &local {
            Some(view) => view.selector,
            None => range,
        };
        Snapshot {
            range,
            enabled: self.enabled.clone(),
            alpha: self.alpha.iter().map(|a| a.value(now) as f32).collect(),
            y: self.y.value(now),
            footer_y: self.footer_y.value(now),
            rulers: self.rulers.visible(now),
            labels: self.labels.visible(now),
            details: self.details,
            details_alpha: self.details_alpha.value(now) as f32,
            selector,
            local,
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
    pub(super) fn index_at(&self, x: f64, width: f64) -> Option<usize> {
        if width <= 0.0 {
            return None;
        }
        let range = self.range_now();
        match self.kind {
            ChartKind::Bar | ChartKind::StackBar => bar_index_at(&self.chart, range, width, x),
            _ => nearest_index(&self.chart, range, x / width),
        }
    }

    fn animating(&self, now: Instant) -> bool {
        self.y.running(now)
            || self.footer_y.running(now)
            || self.range.running(now)
            || self.alpha.iter().any(|a| a.running(now))
            || self.details_alpha.running(now)
            || self.rulers.running(now)
            || self.labels.running(now)
            || self.zoom_running(now)
    }

    /// Point the y scales at the visible range and the shown lines; a
    /// new ruler set fades in over the old.
    pub(super) fn retarget(&mut self, animate: bool) {
        let now = Instant::now();
        let indices = self.chart.visible_indices(self.range.target());
        let limits = height_limits(&self.chart, self.kind, &self.enabled, indices);
        if limits.ranged.min != limits.ranged.max {
            self.y.retarget(limits.ranged, now, animate);
            self.rulers.show(limits.ranged, now, animate, RULER_MERGE);
        }
        self.footer_y.retarget(limits.full, now, animate);
    }

    /// Redraw for the next few frames while a change animates (the
    /// chart's own clock: it notifies only this state, so only the panel
    /// that shows the chart redraws, and stops when the change ends).
    pub(super) fn tick(&mut self, cx: &mut Context<Self>) {
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
                        let now = Instant::now();
                        let more = state.animating(now);
                        if !more {
                            state.settle(now);
                        }
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
        self.alpha[index].retarget(target, now, true);
        self.retarget(true);
        self.retarget_shares(now);
        self.tick(cx);
    }

    fn show_details(&mut self, index: usize, now: Instant) {
        self.details = Some(index);
        self.details_alpha.retarget(1.0, now, true);
    }

    pub(super) fn hide_details(&mut self, cx: &mut Context<Self>) {
        if self.details.is_some() {
            self.details_alpha.retarget(0.0, Instant::now(), true);
            self.settle(Instant::now());
            self.tick(cx);
        }
    }

    fn details_shown(&self) -> bool {
        self.details.is_some() && self.details_alpha.to > 0.0
    }

    /// A press on the plot shows the details of the point under it;
    /// pressing the shown point again hides them (tdesktop
    /// `setupDetails`).
    pub(super) fn press(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        if self.range.running(Instant::now()) {
            return;
        }
        self.pressing = true;
        match index {
            None => self.hide_details(cx),
            Some(i) if self.details_shown() && self.details == Some(i) => self.hide_details(cx),
            Some(i) => {
                self.show_details(i, Instant::now());
                self.tick(cx);
            }
        }
    }

    /// The pointer moved with the button held since a press on the plot:
    /// the details follow it.
    pub(super) fn drag_point(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        if !self.pressing || self.drag.is_some() {
            return;
        }
        match index {
            Some(i) if self.details != Some(i) || !self.details_shown() => {
                self.show_details(i, Instant::now());
                self.tick(cx);
            }
            None => self.hide_details(cx),
            _ => {}
        }
    }

    pub(super) fn release_press(&mut self) {
        self.pressing = false;
    }

    /// Arrow keys: move the shown point inside the visible range.
    pub(super) fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        let (start, end) = self.chart.visible_indices(self.range_now());
        let next = match self.details.filter(|_| self.details_shown()) {
            Some(i) => (i as isize + delta).clamp(start as isize, end as isize) as usize,
            None if delta < 0 => end,
            None => start,
        };
        self.show_details(next, Instant::now());
        self.tick(cx);
    }

    pub(super) fn selected(&self) -> Option<usize> {
        self.details.filter(|_| self.details_shown())
    }

    fn selector_width(&self) -> f64 {
        f32::from(self.footer_bounds.get().size.width) as f64
    }

    fn set_range(&mut self, range: Limits, animate: bool, cx: &mut Context<Self>) {
        if range == self.range.target() {
            return;
        }
        let now = Instant::now();
        self.range.retarget(range, now, animate);
        // tdesktop hides the details when the range moves.
        if self.details.is_some() {
            self.details_alpha.retarget(0.0, now, true);
        }
        self.retarget(true);
        self.update_labels(true);
        self.tick(cx);
    }

    /// A press on the range selector at `x` pixels into it.
    pub(super) fn grab(&mut self, x: f64, cx: &mut Context<Self>) {
        let width = self.selector_width();
        if width <= 0.0 {
            return;
        }
        let current = match &self.local {
            Some(zoom) if !zoom.leaving => zoom.range,
            Some(_) => return,
            None => self.range.target(),
        };
        let mut selector = RangeSelector::new(width, current);
        let grab = selector.grab_at(x);
        if grab == SelectorGrab::Outside {
            // tdesktop slides the window to the click (sine, 150 ms).
            selector.center_on(x);
            self.apply_selector(selector.range(), true, cx);
            return;
        }
        let offset = selector.grab_offset(grab, x);
        self.drag = Some((selector, grab, offset));
    }

    fn apply_selector(&mut self, range: Limits, animate: bool, cx: &mut Context<Self>) {
        if self.local.is_some() {
            self.set_zoom_range(range, cx);
        } else {
            self.set_range(range, animate, cx);
        }
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
        self.apply_selector(selector.range(), false, cx);
    }

    pub(super) fn release(&mut self) {
        self.drag = None;
    }

    /// Leave the range as it is now (before a local zoom).
    pub(super) fn range_target(&self) -> Limits {
        self.range.target()
    }
}
