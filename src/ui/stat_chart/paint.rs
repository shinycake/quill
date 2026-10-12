//! Drawing a chart: the series of each kind, rulers with captions, the
//! x-axis dates, the hovered point, and the range selector with its mini
//! chart (tdesktop `view/*_chart_view.cpp`, `chart_rulers_view.cpp`,
//! `ChartWidget::Footer`). Built series are cached per range, scale and
//! size, so a hover redraw replays them instead of re-tessellating.

use super::state_zoom::LocalZoomView;
use gpui_kit::component::ActiveTheme;
use gpui_kit::*;
use quill::stat_chart::{
    ChartKind, FOOTER_SIDE, Limits, RangeSelector, StatChart, bar_columns, chart_rulers,
    label_indices_for_step, label_step, line_ratios,
};
use std::cell::RefCell;
use std::rc::Rc;

/// Room above the plot for the top ruler's caption.
const TOP_INSET: f32 = 16.;
/// Room under the plot for the dates (`statisticsChartBottomCaption*`).
const BOTTOM_INSET: f32 = 22.;
const CAPTION_SIZE: f32 = 11.;
const CAPTION_LINE: f32 = 14.;
const LINE_WIDTH: f32 = 2.;
const DOT_RADIUS: f32 = 5.;
/// An x label's width budget (`dayStringMaxWidth`).
const LABEL_WIDTH: f64 = 40.;

/// Where the drawing area starts below the plot element's top.
pub(super) fn plot_rect_top() -> f32 {
    TOP_INSET
}

/// The plot's drawing area inside its element.
pub(super) fn plot_rect(bounds: Bounds<Pixels>) -> Bounds<Pixels> {
    Bounds::new(
        point(bounds.left(), bounds.top() + px(TOP_INSET)),
        size(
            bounds.size.width,
            (bounds.size.height - px(TOP_INSET + BOTTOM_INSET)).max(px(1.)),
        ),
    )
}

#[derive(Clone)]
enum Prim {
    Path(Path<Pixels>, Hsla),
    Quad(PaintQuad),
}

#[derive(PartialEq)]
struct GeomKey {
    rect: Bounds<Pixels>,
    range: Limits,
    y: Limits,
    alpha: Vec<f32>,
    selected: Option<usize>,
}

/// Built series of one drawing, reused while nothing it depends on
/// changes.
#[derive(Default)]
pub(super) struct SeriesCache {
    key: Option<GeomKey>,
    prims: Vec<Prim>,
}

pub(super) type SharedCache = Rc<RefCell<SeriesCache>>;

struct Scale {
    rect: Bounds<Pixels>,
    range: Limits,
    y: Limits,
}

impl Scale {
    fn x(&self, chart: &StatChart, i: usize) -> f32 {
        let frac = self.range.ratio(chart.x_frac[i]);
        f32::from(self.rect.left()) + f32::from(self.rect.size.width) * frac as f32
    }

    fn height(&self) -> f32 {
        f32::from(self.rect.size.height)
    }

    /// Fraction of the height `value` reaches above the bottom.
    fn level(&self, value: f64) -> f32 {
        let span = self.y.span();
        if span == 0.0 {
            0.0
        } else {
            ((value - self.y.min) / span) as f32
        }
    }

    fn y(&self, value: f64) -> f32 {
        f32::from(self.rect.bottom()) - self.height() * self.level(value)
    }
}

fn polyline(points: &[Point<Pixels>], width: f32, color: Hsla) -> Option<Prim> {
    if points.len() < 2 {
        return None;
    }
    let mut path = PathBuilder::stroke(px(width));
    path.add_polygon(points, false);
    path.build().ok().map(|path| Prim::Path(path, color))
}

fn polygon(points: &[Point<Pixels>], color: Hsla) -> Option<Prim> {
    if points.len() < 3 {
        return None;
    }
    let mut path = PathBuilder::fill();
    path.add_polygon(points, true);
    path.build().ok().map(|path| Prim::Path(path, color))
}

/// The series of `chart` for one drawing: the plot or the selector's
/// mini chart.
fn build_series(
    chart: &StatChart,
    kind: ChartKind,
    scale: &Scale,
    alpha: &[f32],
    width: f32,
    selected: Option<usize>,
) -> Vec<Prim> {
    let n = chart.x.len();
    if n == 0 {
        return Vec::new();
    }
    let (start, end) = chart.visible_indices(scale.range);
    let (lo, hi) = (start.saturating_sub(2), (end + 2).min(n - 1));
    let color = |k: usize| -> Hsla {
        let c: Hsla = rgb(chart.lines[k].color()).into();
        c.opacity(alpha.get(k).copied().unwrap_or(1.0))
    };
    let on = |k: usize| alpha.get(k).copied().unwrap_or(1.0) > 0.0;
    let mut prims = Vec::new();
    match kind {
        ChartKind::Linear | ChartKind::DoubleLinear => {
            let ratios = line_ratios(chart, kind);
            for (k, line) in chart.lines.iter().enumerate().filter(|(k, _)| on(*k)) {
                let points: Vec<Point<Pixels>> = (lo..=hi)
                    .filter(|&i| line.y[i] >= 0)
                    .map(|i| {
                        point(
                            px(scale.x(chart, i)),
                            px(scale.y(line.y[i] as f64 * ratios[k])),
                        )
                    })
                    .collect();
                prims.extend(polyline(&points, width, color(k)));
            }
        }
        ChartKind::Bar | ChartKind::StackBar => {
            let rect_w = f32::from(scale.rect.size.width) as f64;
            let (left, step) = bar_columns(chart, scale.range, rect_w, lo);
            let origin = f32::from(scale.rect.left());
            let bottom = f32::from(scale.rect.bottom());
            let column_left = |i: usize| origin + (left + (i - lo) as f64 * step) as f32;
            let step = step as f32;
            if kind == ChartKind::Bar {
                for (k, line) in chart.lines.iter().enumerate().filter(|(k, _)| on(*k)) {
                    let a = alpha[k];
                    let mut points = Vec::with_capacity((hi - lo + 1) * 2);
                    for i in lo..=hi {
                        let top = bottom - scale.height() * scale.level(line.y[i] as f64) * a;
                        let right = if i == hi {
                            f32::from(scale.rect.right())
                        } else {
                            column_left(i) + step
                        };
                        points.push(point(px(column_left(i)), px(top)));
                        points.push(point(px(right), px(top)));
                    }
                    prims.extend(polyline(&points, width, color(k)));
                }
            } else {
                let mut bottoms = vec![0f32; hi - lo + 1];
                let dim = selected.is_some();
                for (k, line) in chart.lines.iter().enumerate().filter(|(k, _)| on(*k)) {
                    for i in lo..=hi {
                        if line.y[i] <= 0 {
                            continue;
                        }
                        let h = scale.height() * scale.level(line.y[i] as f64) * alpha[k];
                        let stack = &mut bottoms[i - lo];
                        let bounds = Bounds::new(
                            point(px(column_left(i)), px(bottom - *stack - h)),
                            size(px(step.max(0.5)), px(h)),
                        );
                        *stack += h;
                        let c = if dim && selected != Some(i) {
                            color(k).opacity(alpha[k] * 0.5)
                        } else {
                            color(k)
                        };
                        prims.push(Prim::Quad(fill(bounds, c)));
                    }
                }
            }
        }
        ChartKind::StackLinear => {
            let count = hi - lo + 1;
            let mut offsets = vec![0f32; count];
            let sums: Vec<f32> = (lo..=hi)
                .map(|i| {
                    chart
                        .lines
                        .iter()
                        .enumerate()
                        .map(|(k, l)| l.y[i].max(0) as f32 * alpha.get(k).copied().unwrap_or(1.0))
                        .sum()
                })
                .collect();
            // Stacked from the bottom, first line lowest (tdesktop).
            let bottom = f32::from(scale.rect.bottom());
            for (k, line) in chart.lines.iter().enumerate().filter(|(k, _)| on(*k)) {
                let mut upper = Vec::with_capacity(count);
                let mut lower = Vec::with_capacity(count);
                for i in lo..=hi {
                    let j = i - lo;
                    let share = if sums[j] > 0.0 {
                        line.y[i].max(0) as f32 * alpha[k] / sums[j]
                    } else {
                        0.0
                    };
                    let x = px(scale.x(chart, i));
                    lower.push(point(x, px(bottom - scale.height() * offsets[j])));
                    offsets[j] += share;
                    upper.push(point(x, px(bottom - scale.height() * offsets[j])));
                }
                lower.reverse();
                upper.extend(lower);
                prims.extend(polygon(&upper, color(k)));
            }
        }
    }
    prims
}

fn cached_series(
    cache: &SharedCache,
    key: GeomKey,
    build: impl FnOnce() -> Vec<Prim>,
) -> Vec<Prim> {
    let mut cache = cache.borrow_mut();
    if cache.key.as_ref() != Some(&key) {
        cache.prims = build();
        cache.key = Some(key);
    }
    cache.prims.clone()
}

fn paint_prims(prims: Vec<Prim>, window: &mut Window) {
    for prim in prims {
        match prim {
            Prim::Path(path, color) => window.paint_path(path, color),
            Prim::Quad(quad) => window.paint_quad(quad),
        }
    }
}

/// A caption at `origin` (top-left), right-aligned to `origin.x` or
/// centered on it per `anchor`. With `fade`, a caption crossing its left
/// or right edge fades out as it leaves (tdesktop `PaintBottomLine`).
pub(super) fn caption(
    text: &str,
    origin: Point<Pixels>,
    color: Hsla,
    anchor: Anchor,
    fade: Option<Bounds<Pixels>>,
    window: &mut Window,
    cx: &mut App,
) {
    if text.is_empty() {
        return;
    }
    let text = SharedString::from(text.to_string());
    let shape = |color: Hsla, window: &mut Window| {
        let run = TextRun {
            len: text.len(),
            font: window.text_style().font(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        window
            .text_system()
            .shape_line(text.clone(), px(CAPTION_SIZE), &[run], None)
    };
    let mut line = shape(color, window);
    let x = match anchor {
        Anchor::Left => origin.x,
        Anchor::Right => origin.x - line.width,
        Anchor::Center => origin.x - line.width / 2.,
    };
    if let Some(edges) = fade {
        let width = f32::from(line.width).max(1.);
        let ramp = width / 4.;
        let left = f32::from(x - edges.left());
        let right = f32::from(edges.right() - (x + line.width));
        let alpha = (1. + left.min(0.) / ramp).min(1. + right.min(0.) / ramp);
        if alpha <= 0. {
            return;
        }
        if alpha < 1. {
            line = shape(color.opacity(color.a * alpha), window);
        }
    }
    let _ = line.paint(
        point(x, origin.y),
        px(CAPTION_LINE),
        TextAlign::Left,
        None,
        window,
        cx,
    );
}

#[derive(Clone, Copy)]
pub(super) enum Anchor {
    Left,
    Right,
    Center,
}

pub(super) struct Palette {
    rule: Hsla,
    caption: Hsla,
    surface: Hsla,
    /// The range selector: outside the window, its frame, the grips.
    inactive: Hsla,
    handle: Hsla,
    grip: Hsla,
}

pub(super) fn palette(cx: &App) -> Palette {
    let theme = cx.theme();
    Palette {
        rule: theme.border,
        caption: theme.muted_foreground,
        surface: theme.background,
        inactive: theme.muted.opacity(0.75),
        handle: theme.muted_foreground.opacity(0.45),
        grip: theme.background,
    }
}

/// What the plot's paint needs from the frame's snapshot.
#[derive(Clone)]
pub(super) struct PlotSnap {
    pub range: Limits,
    pub y: Limits,
    pub alpha: Vec<f32>,
    pub rulers: Vec<(Limits, f32)>,
    pub labels: Vec<(usize, f32)>,
    /// The point whose details show, and their opacity.
    pub details: Option<(usize, f32)>,
    pub local: Option<LocalZoomView>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn paint_plot(
    bounds: Bounds<Pixels>,
    chart: &StatChart,
    kind: ChartKind,
    snap: &PlotSnap,
    cache: &SharedCache,
    colors: &Palette,
    window: &mut Window,
    cx: &mut App,
) {
    let rect = plot_rect(bounds);
    let scale = Scale {
        rect,
        range: snap.range,
        y: snap.y,
    };
    // A local zoom fades the areas, rulers and dates out as the pie
    // comes in.
    let zoom = snap.local.as_ref().map_or(0.0, |local| local.progress);
    let fade = 1.0 - zoom;
    let rulers: Vec<(Vec<quill::stat_chart::Ruler>, f32)> = snap
        .rulers
        .iter()
        .map(|(limits, alpha)| (chart_rulers(chart, kind, *limits), alpha * fade))
        .filter(|(_, alpha)| *alpha > 0.0)
        .collect();
    let rulers_above = matches!(kind, ChartKind::StackBar | ChartKind::StackLinear);
    let ruler_lines = |window: &mut Window| {
        for (set, alpha) in &rulers {
            for ruler in set {
                let y = scale.y(ruler.value);
                if y < f32::from(rect.top()) - 0.5 || y > f32::from(rect.bottom()) + 0.5 {
                    continue;
                }
                window.paint_quad(fill(
                    Bounds::new(point(rect.left(), px(y)), size(rect.size.width, px(1.))),
                    colors.rule.opacity(colors.rule.a * alpha),
                ));
            }
        }
    };
    if !rulers_above {
        ruler_lines(window);
    }
    let selected = snap
        .details
        .filter(|_| kind == ChartKind::StackBar)
        .map(|(i, _)| i);
    let alpha: Vec<f32> = snap.alpha.iter().map(|a| a * fade).collect();
    let key = GeomKey {
        rect,
        range: snap.range,
        y: snap.y,
        alpha: alpha.clone(),
        selected,
    };
    let prims = if fade > 0.0 {
        cached_series(cache, key, || {
            build_series(chart, kind, &scale, &alpha, LINE_WIDTH, selected)
        })
    } else {
        Vec::new()
    };
    // Lines may overshoot the plot's height a little; never its sides.
    let mask = Bounds::new(
        point(rect.left(), bounds.top()),
        size(rect.size.width, bounds.size.height),
    );
    window.with_content_mask(Some(ContentMask { bounds: mask }), |window| {
        paint_prims(prims, window);
        if rulers_above {
            ruler_lines(window);
        }
        if let Some((i, details_alpha)) = snap.details {
            let alpha: Vec<f32> = alpha.iter().map(|a| a * details_alpha).collect();
            paint_selection(chart, kind, &scale, i, &alpha, colors, window);
        }
    });
    if let Some(local) = snap.local.as_ref() {
        super::pie_paint::paint_pie(rect, chart, local, window, cx);
    }
    // Ruler captions: left in the first line's color on a double chart,
    // the second line's scale on the right.
    let double = kind == ChartKind::DoubleLinear && chart.lines.len() == 2;
    let line_color = |k: usize, alpha: f32| -> Hsla {
        let c: Hsla = rgb(chart.lines[k].color()).into();
        c.opacity(snap.alpha.get(k).copied().unwrap_or(1.0) * alpha)
    };
    for (set, alpha) in &rulers {
        for ruler in set {
            let y = scale.y(ruler.value);
            if y < f32::from(rect.top()) - 0.5 || y > f32::from(rect.bottom()) + 0.5 {
                continue;
            }
            let top = px(y - CAPTION_LINE - 2.);
            let left_color = if double {
                line_color(0, *alpha)
            } else {
                colors.caption.opacity(colors.caption.a * alpha)
            };
            caption(
                &ruler.caption,
                point(rect.left(), top),
                left_color,
                Anchor::Left,
                None,
                window,
                cx,
            );
            if double {
                caption(
                    &ruler.right_caption,
                    point(rect.right(), top),
                    line_color(1, *alpha),
                    Anchor::Right,
                    None,
                    window,
                    cx,
                );
            }
        }
    }
    // Dates under the plot; an old step fades out under the new one.
    let width = f32::from(rect.size.width) as f64;
    let bars = matches!(kind, ChartKind::Bar | ChartKind::StackBar);
    let label_y = rect.bottom() + px(6.);
    let offset = local_offset();
    let labels: Vec<(usize, f32)> = if snap.labels.is_empty() {
        vec![(label_step(chart, snap.range, width), 1.0)]
    } else {
        snap.labels.clone()
    };
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        for (step, alpha) in labels {
            let alpha = alpha * fade;
            if alpha <= 0.0 {
                continue;
            }
            for i in label_indices_for_step(chart, snap.range, width, LABEL_WIDTH, step) {
                let x = if bars {
                    bar_center(chart, &scale, i)
                } else {
                    scale.x(chart, i)
                };
                caption(
                    &chart.day_label(i, offset),
                    point(px(x), label_y),
                    colors.caption.opacity(colors.caption.a * alpha),
                    Anchor::Center,
                    Some(rect),
                    window,
                    cx,
                );
            }
        }
    });
}

/// The viewer's UTC offset for time-of-day labels.
fn local_offset() -> i32 {
    quill::local_time::utc_offset_at(quill::local_time::now_unix())
}

fn bar_center(chart: &StatChart, scale: &Scale, i: usize) -> f32 {
    let (start, _) = chart.visible_indices(scale.range);
    let lo = start.saturating_sub(2);
    let (left, step) = bar_columns(
        chart,
        scale.range,
        f32::from(scale.rect.size.width) as f64,
        lo,
    );
    f32::from(scale.rect.left()) + (left + (i.max(lo) - lo) as f64 * step + step / 2.0) as f32
}

/// The hovered point: a vertical rule and a dot on each line
/// (`paintSelectedXIndex`); stacked bars dim the other columns instead.
fn paint_selection(
    chart: &StatChart,
    kind: ChartKind,
    scale: &Scale,
    i: usize,
    alpha: &[f32],
    colors: &Palette,
    window: &mut Window,
) {
    if i >= chart.x.len() || kind == ChartKind::StackBar {
        return;
    }
    let bars = kind == ChartKind::Bar;
    let x = if bars {
        bar_center(chart, scale, i)
    } else {
        scale.x(chart, i)
    };
    window.paint_quad(fill(
        Bounds::new(
            point(px(x - 0.5), scale.rect.top()),
            size(px(1.), scale.rect.size.height),
        ),
        colors.rule,
    ));
    if kind == ChartKind::StackLinear {
        return;
    }
    let ratios = line_ratios(chart, kind);
    for (k, line) in chart.lines.iter().enumerate() {
        let a = alpha.get(k).copied().unwrap_or(1.0);
        if a <= 0.0 {
            continue;
        }
        let y = if bars {
            f32::from(scale.rect.bottom()) - scale.height() * scale.level(line.y[i] as f64) * a
        } else {
            scale.y(line.y[i] as f64 * ratios[k])
        };
        let c: Hsla = rgb(line.color()).into();
        window.paint_quad(quad(
            Bounds::new(
                point(px(x - DOT_RADIUS), px(y - DOT_RADIUS)),
                size(px(DOT_RADIUS * 2.), px(DOT_RADIUS * 2.)),
            ),
            px(DOT_RADIUS),
            colors.surface.opacity(a),
            px(LINE_WIDTH),
            c.opacity(a),
            BorderStyle::Solid,
        ));
    }
}

/// What the range selector's paint needs.
pub(super) struct FooterSnap {
    pub selector: Limits,
    pub footer_y: Limits,
    pub alpha: Vec<f32>,
    pub local: Option<LocalZoomView>,
}

/// The range selector: the whole chart in miniature (or, zoomed into a
/// pie, the eight days as stacked columns) with a window over the
/// selected part.
pub(super) fn paint_footer(
    bounds: Bounds<Pixels>,
    chart: &StatChart,
    kind: ChartKind,
    snap: &FooterSnap,
    cache: &SharedCache,
    colors: &Palette,
    window: &mut Window,
) {
    let inner = Bounds::new(
        point(bounds.left(), bounds.top() + px(1.)),
        size(bounds.size.width, bounds.size.height - px(2.)),
    );
    let zoom = snap.local.as_ref().map_or(0.0, |local| local.progress);
    let scale = Scale {
        rect: inner,
        range: Limits::new(0.0, 1.0),
        y: snap.footer_y,
    };
    let alpha: Vec<f32> = snap.alpha.iter().map(|a| a * (1.0 - zoom)).collect();
    let key = GeomKey {
        rect: inner,
        range: scale.range,
        y: snap.footer_y,
        alpha: alpha.clone(),
        selected: None,
    };
    let prims = if zoom < 1.0 {
        cached_series(cache, key, || {
            build_series(chart, kind, &scale, &alpha, 1., None)
        })
    } else {
        Vec::new()
    };
    window.with_content_mask(Some(ContentMask { bounds: inner }), |window| {
        paint_prims(prims, window);
        if let Some(local) = snap.local.as_ref() {
            super::pie_paint::paint_zoom_columns(inner, chart, local, &snap.alpha, window);
        }
    });
    let width = f32::from(bounds.size.width) as f64;
    let (left, right) = RangeSelector::new(width, snap.selector).handles();
    let x = |v: f64| bounds.left() + px(v as f32);
    let radius = px(6.);
    // Dim what is outside the window.
    window.paint_quad(quad(
        Bounds::new(
            inner.origin,
            size(
                x(left + FOOTER_SIDE / 2.0) - bounds.left(),
                inner.size.height,
            ),
        ),
        radius,
        colors.inactive,
        px(0.),
        transparent_black(),
        BorderStyle::Solid,
    ));
    let right_edge = x(right + FOOTER_SIDE / 2.0);
    window.paint_quad(quad(
        Bounds::new(
            point(right_edge, inner.top()),
            size(bounds.right() - right_edge, inner.size.height),
        ),
        radius,
        colors.inactive,
        px(0.),
        transparent_black(),
        BorderStyle::Solid,
    ));
    // The window: two handles joined by thin top and bottom edges.
    window.paint_quad(quad(
        Bounds::new(
            point(x(left), bounds.top()),
            size(x(right + FOOTER_SIDE) - x(left), bounds.size.height),
        ),
        radius,
        transparent_black(),
        Edges {
            top: px(1.),
            bottom: px(1.),
            left: px(FOOTER_SIDE as f32),
            right: px(FOOTER_SIDE as f32),
        },
        colors.handle,
        BorderStyle::Solid,
    ));
    for edge in [left, right] {
        let center = x(edge + FOOTER_SIDE / 2.0);
        window.paint_quad(quad(
            Bounds::new(
                point(center - px(1.), bounds.center().y - px(5.)),
                size(px(2.), px(10.)),
            ),
            px(1.),
            colors.grip,
            px(0.),
            transparent_black(),
            BorderStyle::Solid,
        ));
    }
}
