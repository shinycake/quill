//! Drawing a percentage chart's local zoom: the pie of the shares with
//! their percentages, and the zoomed days as stacked columns in the
//! range selector (tdesktop `StackLinearChartView::paintZoomed`,
//! `paintPieText`, `paintZoomedFooter`). The pie grows in and fades over
//! the areas instead of tdesktop's area-to-pie morph.

use super::state_zoom::LocalZoomView;
use gpui_kit::*;
use quill::stat_chart::{FOOTER_SIDE, StatChart};

/// Pie diameter as a share of the plot's width (`kCircleSizeRatio`).
const CIRCLE_RATIO: f32 = 0.42;
/// How far a hovered slice moves out (`statisticsPieChartPartOffset`).
const PART_OFFSET: f32 = 8.;
/// Percent labels (`statisticsPieChartFont`, 20 px) shrink with their
/// share down to this part of the largest size (`kMinTextScaleRatio`).
const TEXT_SIZE: f32 = 20.;
const MIN_TEXT_SCALE: f32 = 0.3;
/// Slices under this share get no label (`kMinPercentage`).
const MIN_LABELED: f64 = 0.039;

fn arc_point(center: Point<Pixels>, radius: f32, degrees: f32) -> Point<Pixels> {
    let radians = degrees.to_radians();
    point(
        center.x + px(radius * radians.cos()),
        center.y + px(radius * radians.sin()),
    )
}

fn slice_offset(local: &LocalZoomView, k: usize, mid: f32) -> Point<Pixels> {
    if local.single {
        return point(px(0.), px(0.));
    }
    let amount = PART_OFFSET * local.offsets.get(k).copied().unwrap_or(0.);
    arc_point(point(px(0.), px(0.)), amount, mid)
}

/// The pie over the plot, fading and growing in with the zoom.
pub(super) fn paint_pie(
    rect: Bounds<Pixels>,
    chart: &StatChart,
    local: &LocalZoomView,
    window: &mut Window,
    cx: &mut App,
) {
    let progress = local.progress;
    if progress <= 0. {
        return;
    }
    let center = rect.center();
    let side = f32::from(rect.size.width) / 2. * CIRCLE_RATIO;
    let radius = side * (0.6 + 0.4 * progress);
    // Clockwise from the top.
    let mut start = -90f32;
    let mut slices = Vec::with_capacity(chart.lines.len());
    for (k, line) in chart.lines.iter().enumerate() {
        let share = local.shares.get(k).copied().unwrap_or(0.);
        let sweep = (share * 360.) as f32;
        let mid = start + sweep / 2.;
        slices.push((k, share, mid, start));
        if sweep > 0. {
            let origin = center + slice_offset(local, k, mid);
            let steps = ((sweep / 3.).ceil() as usize).max(2);
            let mut points = vec![origin];
            points.extend(
                (0..=steps)
                    .map(|i| arc_point(origin, radius, start + sweep * i as f32 / steps as f32)),
            );
            let mut path = PathBuilder::fill();
            path.add_polygon(&points, true);
            if let Ok(path) = path.build() {
                let color: Hsla = rgb(line.color()).into();
                window.paint_path(path, color.opacity(progress));
            }
        }
        start += sweep;
    }
    // Labels appear over the last part of the zoom.
    let text_alpha = ((progress - 0.6) / 0.4).clamp(0., 1.);
    if text_alpha <= 0. {
        return;
    }
    let max_scale = side / (TEXT_SIZE * 2.);
    let min_scale = max_scale * MIN_TEXT_SCALE;
    for (k, share, mid, _) in slices {
        if share <= MIN_LABELED {
            continue;
        }
        let Some(text) = local.texts.get(k) else {
            continue;
        };
        let scale = min_scale + share as f32 * (max_scale - min_scale);
        let text = SharedString::from(text.clone());
        let run = TextRun {
            len: text.len(),
            font: window.text_style().font(),
            color: white().opacity(text_alpha),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        // The label's centre moves in by the half width it lost to its
        // scale, so small slices keep their label inside.
        let full = window.text_system().shape_line(
            text.clone(),
            px(TEXT_SIZE),
            std::slice::from_ref(&run),
            None,
        );
        let distance =
            side * (1. - share as f32).sqrt() - f32::from(full.width) / 2. * (1. - scale);
        let at = arc_point(center + slice_offset(local, k, mid), distance.max(0.), mid);
        let size = px((TEXT_SIZE * scale).max(6.));
        let line = window.text_system().shape_line(text, size, &[run], None);
        let _ = line.paint(
            point(at.x - line.width / 2., at.y - size * 0.6),
            size * 1.2,
            TextAlign::Left,
            None,
            window,
            cx,
        );
    }
}

/// The zoomed days as 100% columns in the range selector.
pub(super) fn paint_zoom_columns(
    rect: Bounds<Pixels>,
    chart: &StatChart,
    local: &LocalZoomView,
    alpha: &[f32],
    window: &mut Window,
) {
    let progress = local.progress;
    if progress <= 0. {
        return;
    }
    let (first, last) = local.limit;
    let count = last.saturating_sub(first) + 1;
    let inner_left = f32::from(rect.left()) + FOOTER_SIDE as f32;
    let width = (f32::from(rect.size.width) - 2. * FOOTER_SIDE as f32).max(1.);
    let column = width / count as f32;
    let height = f32::from(rect.size.height);
    for (j, i) in (first..=last).enumerate() {
        let sum: f32 = chart
            .lines
            .iter()
            .enumerate()
            .map(|(k, line)| line.y[i].max(0) as f32 * alpha.get(k).copied().unwrap_or(1.))
            .sum();
        if sum <= 0. {
            continue;
        }
        // First line lowest, as in the areas.
        let mut bottom = f32::from(rect.bottom());
        let left = inner_left + column * j as f32;
        for (k, line) in chart.lines.iter().enumerate() {
            let h = height * line.y[i].max(0) as f32 * alpha.get(k).copied().unwrap_or(1.) / sum;
            if h <= 0. {
                continue;
            }
            let color: Hsla = rgb(line.color()).into();
            window.paint_quad(fill(
                Bounds::new(point(px(left), px(bottom - h)), size(px(column), px(h))),
                color.opacity(progress),
            ));
            bottom -= h;
        }
    }
}
