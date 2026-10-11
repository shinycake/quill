//! Drawing a chart: the series of each kind, rulers with captions, the
//! x-axis dates, the hovered point, and the range selector with its mini
//! chart (tdesktop `view/*_chart_view.cpp`, `chart_rulers_view.cpp`,
//! `ChartWidget::Footer`). Built series are cached per range, scale and
//! size, so a hover redraw replays them instead of re-tessellating.

use super::details::details;
use super::state::{ChartState, Snapshot};
use super::{FOOTER_HEIGHT, PLOT_HEIGHT, ZoomFn};
use gpui_kit::component::ActiveTheme;
use gpui_kit::*;
use quill::stat_chart::{
    ChartKind, FOOTER_SIDE, Limits, RangeSelector, StatChart, bar_columns, chart_rulers,
    label_indices, line_ratios,
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
/// Space between the hovered point and its tooltip.
const POPUP_GAP: f32 = 12.;

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
fn caption(
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
enum Anchor {
    Left,
    Right,
    Center,
}

struct Palette {
    rule: Hsla,
    caption: Hsla,
    surface: Hsla,
}

fn palette(cx: &App) -> Palette {
    let theme = cx.theme();
    Palette {
        rule: theme.border,
        caption: theme.muted_foreground,
        surface: theme.background,
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_plot(
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
    let rulers = chart_rulers(chart, kind, snap.y_target);
    let rulers_above = matches!(kind, ChartKind::StackBar | ChartKind::StackLinear);
    let ruler_lines = |window: &mut Window| {
        for ruler in &rulers {
            let y = scale.y(ruler.value);
            if y < f32::from(rect.top()) - 0.5 || y > f32::from(rect.bottom()) + 0.5 {
                continue;
            }
            window.paint_quad(fill(
                Bounds::new(point(rect.left(), px(y)), size(rect.size.width, px(1.))),
                colors.rule,
            ));
        }
    };
    if !rulers_above {
        ruler_lines(window);
    }
    let selected = snap.hover.filter(|_| kind == ChartKind::StackBar);
    let key = GeomKey {
        rect,
        range: snap.range,
        y: snap.y,
        alpha: snap.alpha.clone(),
        selected,
    };
    let prims = cached_series(cache, key, || {
        build_series(chart, kind, &scale, &snap.alpha, LINE_WIDTH, selected)
    });
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
        if let Some(i) = snap.hover {
            paint_selection(chart, kind, &scale, i, &snap.alpha, colors, window);
        }
    });
    // Ruler captions: left in the first line's color on a double chart,
    // the second line's scale on the right.
    let double = kind == ChartKind::DoubleLinear && chart.lines.len() == 2;
    let line_color = |k: usize| -> Hsla {
        let c: Hsla = rgb(chart.lines[k].color()).into();
        c.opacity(snap.alpha.get(k).copied().unwrap_or(1.0))
    };
    for ruler in &rulers {
        let y = scale.y(ruler.value);
        if y < f32::from(rect.top()) - 0.5 || y > f32::from(rect.bottom()) + 0.5 {
            continue;
        }
        let top = px(y - CAPTION_LINE - 2.);
        let left_color = if double {
            line_color(0)
        } else {
            colors.caption
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
                line_color(1),
                Anchor::Right,
                None,
                window,
                cx,
            );
        }
    }
    // Dates under the plot.
    let width = f32::from(rect.size.width) as f64;
    let bars = matches!(kind, ChartKind::Bar | ChartKind::StackBar);
    let label_y = rect.bottom() + px(6.);
    let offset = local_offset();
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        for i in label_indices(chart, snap.range, width, LABEL_WIDTH) {
            let x = if bars {
                bar_center(chart, &scale, i)
            } else {
                scale.x(chart, i)
            };
            caption(
                &chart.day_label(i, offset),
                point(px(x), label_y),
                colors.caption,
                Anchor::Center,
                Some(rect),
                window,
                cx,
            );
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

/// What the plot's paint needs from the frame's snapshot.
#[derive(Clone)]
struct PlotSnap {
    range: Limits,
    y: Limits,
    y_target: Limits,
    alpha: Vec<f32>,
    hover: Option<usize>,
}

/// The plot with its hover handling, keyboard steps and tooltip.
#[allow(clippy::too_many_arguments)]
pub(super) fn plot(
    id: &str,
    state: &Entity<ChartState>,
    chart: &Rc<StatChart>,
    kind: ChartKind,
    snapshot: &Snapshot,
    on_zoom: Option<ZoomFn>,
    _window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let snap = PlotSnap {
        range: snapshot.range,
        y: snapshot.y,
        y_target: snapshot.y_target,
        alpha: snapshot.alpha.clone(),
        hover: snapshot.hover,
    };
    let bounds_cell = snapshot.plot_bounds.clone();
    let cache = snapshot.plot_cache.clone();
    let colors = palette(cx);
    let paint_chart = chart.clone();
    let canvas = canvas(
        move |_, _, _| {},
        move |bounds, _, window, cx| {
            bounds_cell.set(bounds);
            paint_plot(
                bounds,
                &paint_chart,
                kind,
                &snap,
                &cache,
                &colors,
                window,
                cx,
            );
        },
    )
    .size_full();
    let index_at = {
        let chart = chart.clone();
        let cell = snapshot.plot_bounds.clone();
        move |position: Point<Pixels>, state: &ChartState| -> Option<usize> {
            let rect = plot_rect(cell.get());
            let x = f32::from(position.x - rect.left()) as f64;
            state.index_at(&chart, x, f32::from(rect.size.width) as f64)
        }
    };
    let index_at = Rc::new(index_at);
    let mut element = div()
        .id(ElementId::Name(format!("{id}-plot").into()))
        .debug_selector(|| format!("{id}-plot"))
        .relative()
        .w_full()
        .h(px(PLOT_HEIGHT))
        .track_focus(&snapshot.focus)
        .on_mouse_move({
            let state = state.clone();
            let index_at = index_at.clone();
            move |event: &MouseMoveEvent, _, cx| {
                state.update(cx, |s, cx| {
                    let index = index_at(event.position, s);
                    s.hover(index, cx);
                });
            }
        })
        .on_mouse_down(MouseButton::Left, {
            let state = state.clone();
            let index_at = index_at.clone();
            move |event: &MouseDownEvent, window, cx| {
                state.update(cx, |s, cx| {
                    let index = index_at(event.position, s);
                    s.press(index, cx);
                    window.focus(&s.focus_handle(), cx);
                });
            }
        })
        .on_hover({
            let state = state.clone();
            move |hovered: &bool, _, cx| {
                if !*hovered {
                    state.update(cx, |s, cx| s.leave(cx));
                }
            }
        })
        .on_key_down({
            let state = state.clone();
            let chart = chart.clone();
            let on_zoom = on_zoom.clone();
            move |event: &KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "left" => state.update(cx, |s, cx| s.step(-1, cx)),
                    "right" => state.update(cx, |s, cx| s.step(1, cx)),
                    "escape" => state.update(cx, |s, cx| s.clear(cx)),
                    "enter" => {
                        let selected = state.read(cx).selected();
                        if let (Some(i), Some(on_zoom)) = (selected, on_zoom.as_ref())
                            && let Some(x) = chart.x.get(i)
                        {
                            on_zoom(*x as i64, window, cx);
                        }
                    }
                    _ => return,
                }
                cx.stop_propagation();
            }
        })
        .child(canvas);
    if let Some(index) = snapshot.hover.filter(|i| *i < chart.x.len()) {
        // Beside the point, on the side with more room, so the pointer
        // never sits under the tooltip (tdesktop `setupDetails`).
        let ratio = snapshot.range.ratio(chart.x_frac[index]) as f32;
        let width = f32::from(snapshot.plot_bounds.get().size.width);
        let point_x = width * ratio;
        let place = if ratio > 0.5 {
            Err(px((width - point_x + POPUP_GAP).max(4.)))
        } else {
            Ok(px(if width > 0. { point_x + POPUP_GAP } else { 4. }))
        };
        element = element.child(details(
            id,
            chart,
            kind,
            index,
            &snapshot.enabled,
            place,
            on_zoom,
            cx,
        ));
    }
    element
}

/// The range selector: the whole chart in miniature with a window over
/// the visible part. Drag a handle to resize the window, drag inside to
/// move it, click outside to center it there.
pub(super) fn footer(
    id: &str,
    state: &Entity<ChartState>,
    chart: &Rc<StatChart>,
    kind: ChartKind,
    snapshot: &Snapshot,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let inactive = theme.muted.opacity(0.75);
    let handle = theme.muted_foreground.opacity(0.45);
    let grip = theme.background;
    let range = snapshot.range;
    let footer_y = snapshot.footer_y;
    let alpha = snapshot.alpha.clone();
    let cell = snapshot.footer_bounds.clone();
    let cache = snapshot.footer_cache.clone();
    let paint_chart = chart.clone();
    let drag_state = state.clone();
    let canvas = canvas(
        move |_, _, _| {},
        move |bounds, _, window, _cx| {
            cell.set(bounds);
            let inner = Bounds::new(
                point(bounds.left(), bounds.top() + px(1.)),
                size(bounds.size.width, bounds.size.height - px(2.)),
            );
            let scale = Scale {
                rect: inner,
                range: Limits::new(0.0, 1.0),
                y: footer_y,
            };
            let key = GeomKey {
                rect: inner,
                range: scale.range,
                y: footer_y,
                alpha: alpha.clone(),
                selected: None,
            };
            let prims = cached_series(&cache, key, || {
                build_series(&paint_chart, kind, &scale, &alpha, 1., None)
            });
            window.with_content_mask(Some(ContentMask { bounds: inner }), |window| {
                paint_prims(prims, window);
            });
            let width = f32::from(bounds.size.width) as f64;
            let (left, right) = RangeSelector::new(width, range).handles();
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
                inactive,
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
                inactive,
                px(0.),
                transparent_black(),
                BorderStyle::Solid,
            ));
            // The window: two handles joined by thin top and bottom edges.
            let window_bounds = Bounds::new(
                point(x(left), bounds.top()),
                size(x(right + FOOTER_SIDE) - x(left), bounds.size.height),
            );
            window.paint_quad(quad(
                window_bounds,
                radius,
                transparent_black(),
                Edges {
                    top: px(1.),
                    bottom: px(1.),
                    left: px(FOOTER_SIDE as f32),
                    right: px(FOOTER_SIDE as f32),
                },
                handle,
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
                    grip,
                    px(0.),
                    transparent_black(),
                    BorderStyle::Solid,
                ));
            }
            // Drags keep going outside the selector until release.
            let state = drag_state.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                if phase == DispatchPhase::Bubble && state.read(cx).dragging() {
                    let x = f32::from(event.position.x - bounds.left()) as f64;
                    state.update(cx, |s, cx| s.drag_to(x, cx));
                }
            });
            let state = drag_state.clone();
            window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                if phase == DispatchPhase::Bubble && state.read(cx).dragging() {
                    state.update(cx, |s, _| s.release());
                }
            });
        },
    )
    .size_full();
    let cell = snapshot.footer_bounds.clone();
    let state = state.clone();
    div()
        .id(ElementId::Name(format!("{id}-range").into()))
        .debug_selector(|| format!("{id}-range"))
        .w_full()
        .h(px(FOOTER_HEIGHT))
        .cursor(CursorStyle::ResizeLeftRight)
        .on_mouse_down(MouseButton::Left, move |event: &MouseDownEvent, _, cx| {
            let x = f32::from(event.position.x - cell.get().left()) as f64;
            state.update(cx, |s, cx| s.grab(x, cx));
            cx.stop_propagation();
        })
        .child(canvas)
}
