//! The local zoom of a percentage chart: clicking a day's details turns
//! the areas into a pie of the shares over the selected days, with the
//! range selector spanning eight days around it (tdesktop
//! `ChartWidget::processLocalZoom`, `StackLinearChartView`).

use super::state::ChartState;
use super::tween::{Ease, Tween};
use gpui_kit::*;
use quill::stat_chart::{
    ChartKind, Limits, local_zoom, local_zoom_indices, pie_part_at, pie_parts, pie_sums,
};
use std::time::{Duration, Instant};

/// The zoom in and out (`kFooterZoomDuration`, ease-out circ).
const ZOOM: Duration = Duration::from_millis(400);
/// Shares slide when the selection changes; a hovered slice moves out
/// (`slideWrapDuration`).
const PIE_PART: Duration = Duration::from_millis(150);

pub(super) struct LocalZoomState {
    /// Points the selector spans.
    pub limit: (usize, usize),
    /// The selection, as fractions of the selector.
    pub range: Limits,
    /// The chart's range before the zoom, restored on the way out.
    pub wide: Limits,
    pub leaving: bool,
    progress: Tween,
    shares: Vec<Tween>,
    texts: Vec<String>,
    single: bool,
    hover: Option<usize>,
    offsets: Vec<Tween>,
}

/// What a frame draws of a local zoom.
#[derive(Clone)]
pub(super) struct LocalZoomView {
    pub limit: (usize, usize),
    pub indices: (usize, usize),
    /// On the way back to the areas.
    pub leaving: bool,
    /// 0 at the areas, 1 at the pie.
    pub progress: f32,
    /// Where the selector's window is drawn.
    pub selector: Limits,
    /// Each line's share, and its label.
    pub shares: Vec<f64>,
    pub texts: Vec<String>,
    /// One line holds everything: slices do not move out.
    pub single: bool,
    pub hover: Option<usize>,
    /// How far each slice has moved out (0..=1).
    pub offsets: Vec<f32>,
}

impl ChartState {
    /// Whether this chart zooms into a pie instead of asking the server.
    pub(super) fn zooms_locally(&self) -> bool {
        self.kind == ChartKind::StackLinear && !self.chart.footer_hidden && self.chart.x.len() > 2
    }

    pub(super) fn local_zoomed(&self) -> bool {
        self.local.as_ref().is_some_and(|zoom| !zoom.leaving)
    }

    fn target_alpha(&self) -> Vec<f64> {
        self.alpha.iter().map(|a| a.to).collect()
    }

    /// Zoom into the day at `index`.
    pub(super) fn start_local_zoom(&mut self, index: usize, cx: &mut Context<Self>) {
        if !self.zooms_locally() || self.local_zoomed() {
            return;
        }
        let now = Instant::now();
        let zoom = local_zoom(&self.chart, index);
        let lines = self.chart.lines.len();
        let mut progress = Tween::at(0.0, ZOOM, Ease::OutCirc);
        progress.restart(0.0, 1.0, now);
        self.local = Some(LocalZoomState {
            limit: zoom.limit,
            range: zoom.range,
            wide: self.range_target(),
            leaving: false,
            progress,
            shares: vec![Tween::at(0.0, PIE_PART, Ease::OutCubic); lines],
            texts: vec![String::new(); lines],
            single: false,
            hover: None,
            offsets: vec![Tween::at(0.0, PIE_PART, Ease::Linear); lines],
        });
        self.retarget_shares(now);
        if let Some(local) = self.local.as_mut() {
            // The pie appears with the shares in place.
            for share in &mut local.shares {
                share.restart(share.to, share.to, now);
            }
        }
        self.hide_details(cx);
        self.tick(cx);
    }

    /// "Zoom out": back to the areas and the range from before.
    pub(super) fn exit_local_zoom(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        if let Some(local) = self.local.as_mut() {
            local.leaving = true;
            local.hover = None;
            local.progress.retarget(0.0, now, true);
        }
        self.settle_zoom(now);
        self.tick(cx);
    }

    pub(super) fn set_zoom_range(&mut self, range: Limits, cx: &mut Context<Self>) {
        let now = Instant::now();
        match self.local.as_mut() {
            Some(local) if !local.leaving && local.range != range => local.range = range,
            _ => return,
        }
        self.retarget_shares(now);
        self.tick(cx);
    }

    /// Point the shares at the selected days and the shown lines.
    pub(super) fn retarget_shares(&mut self, now: Instant) {
        let alpha = self.target_alpha();
        let Some(local) = self.local.as_mut() else {
            return;
        };
        let indices = local_zoom_indices(&self.chart, local.range, local.limit);
        let parts = pie_parts(&pie_sums(&self.chart, &alpha, indices), true);
        local.single = parts.iter().any(|p| p.share >= 1.0);
        for (k, part) in parts.iter().enumerate() {
            local.shares[k].retarget(part.share, now, true);
        }
        local.texts = parts.into_iter().map(|p| p.text).collect();
    }

    /// The pointer over the pie at (`dx`, `dy`) from its centre, or off
    /// it: the slice under it moves out and shows its total.
    pub(super) fn pie_hover(&mut self, offset: Option<(f64, f64)>, cx: &mut Context<Self>) {
        let now = Instant::now();
        let Some(local) = self.local.as_mut().filter(|l| !l.leaving) else {
            return;
        };
        if local.progress.value(now) < 1.0 {
            return;
        }
        let parts: Vec<quill::stat_chart::PiePart> = local
            .shares
            .iter()
            .map(|s| quill::stat_chart::PiePart {
                share: s.value(now),
                text: String::new(),
            })
            .collect();
        let hover = offset.and_then(|(dx, dy)| pie_part_at(&parts, dx, dy));
        if hover == local.hover {
            return;
        }
        local.hover = hover;
        for (k, offset) in local.offsets.iter_mut().enumerate() {
            offset.retarget(if hover == Some(k) { 1.0 } else { 0.0 }, now, true);
        }
        self.tick(cx);
    }

    pub(super) fn zoom_running(&self, now: Instant) -> bool {
        self.local.as_ref().is_some_and(|local| {
            local.progress.running(now)
                || local.shares.iter().any(|s| s.running(now))
                || local.offsets.iter().any(|o| o.running(now))
        })
    }

    /// Drop a zoom that finished leaving.
    pub(super) fn settle_zoom(&mut self, now: Instant) {
        if self
            .local
            .as_ref()
            .is_some_and(|l| l.leaving && !l.progress.running(now))
        {
            self.local = None;
        }
    }

    pub(super) fn local_view(&self, now: Instant) -> Option<LocalZoomView> {
        let local = self.local.as_ref()?;
        let progress = local.progress.value(now);
        let shares: Vec<f64> = local.shares.iter().map(|s| s.value(now)).collect();
        let animating = local.shares.iter().any(|s| s.running(now));
        let texts = if animating {
            shares
                .iter()
                .map(|s| format!("{}%", (s * 100.0).round() as i64))
                .collect()
        } else {
            local.texts.clone()
        };
        Some(LocalZoomView {
            limit: local.limit,
            indices: local_zoom_indices(&self.chart, local.range, local.limit),
            leaving: local.leaving,
            progress: progress as f32,
            selector: local.wide.lerp(&local.range, progress),
            shares,
            texts,
            single: local.single,
            hover: local.hover,
            offsets: local.offsets.iter().map(|o| o.value(now) as f32).collect(),
        })
    }
}
