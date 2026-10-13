//! The plot and the range selector as elements: their canvases and the
//! pointer and keyboard handling. tdesktop shows a point's details on a
//! press and follows a drag; pressing the shown point again hides them.

use super::details::{details, pie_details};
use super::paint::{FooterSnap, PlotSnap, paint_footer, paint_plot, palette, plot_rect};
use super::state::{ChartState, Snapshot};
use super::{FOOTER_HEIGHT, PLOT_HEIGHT, ZoomFn};
use gpui_kit::*;
use quill::stat_chart::{ChartKind, StatChart};
use std::rc::Rc;

/// Space between the point and its details box.
const POPUP_GAP: f32 = 12.;

/// Zoom into point `index`: a percentage chart turns into a pie, others
/// ask the server for that day (when they can).
fn zoom_into(
    state: &Entity<ChartState>,
    chart: &StatChart,
    index: usize,
    on_zoom: Option<&ZoomFn>,
    window: &mut Window,
    cx: &mut App,
) {
    if state.read(cx).zooms_locally() {
        state.update(cx, |s, cx| s.start_local_zoom(index, cx));
    } else if let (Some(on_zoom), Some(x)) = (on_zoom, chart.x.get(index)) {
        state.update(cx, |s, cx| s.hide_details(cx));
        on_zoom(*x as i64, window, cx);
    }
}

/// The plot with its press-to-show details, keyboard steps and the pie
/// of a local zoom.
#[allow(clippy::too_many_arguments)]
pub(super) fn plot(
    id: &str,
    state: &Entity<ChartState>,
    chart: &Rc<StatChart>,
    kind: ChartKind,
    snapshot: &Snapshot,
    on_zoom: Option<ZoomFn>,
    cx: &mut App,
) -> impl IntoElement {
    let snap = PlotSnap {
        range: snapshot.range,
        y: snapshot.y,
        alpha: snapshot.alpha.clone(),
        rulers: snapshot.rulers.clone(),
        labels: snapshot.labels.clone(),
        details: snapshot.details.map(|i| (i, snapshot.details_alpha)),
        local: snapshot.local.clone(),
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
    let cell = snapshot.plot_bounds.clone();
    let index_at = Rc::new(move |position: Point<Pixels>, state: &ChartState| {
        let rect = plot_rect(cell.get());
        let x = f32::from(position.x - rect.left()) as f64;
        state.index_at(x, f32::from(rect.size.width) as f64)
    });
    let cell = snapshot.plot_bounds.clone();
    let pie_offset = move |position: Point<Pixels>| {
        let center = plot_rect(cell.get()).center();
        (
            f32::from(position.x - center.x) as f64,
            f32::from(position.y - center.y) as f64,
        )
    };
    let local = snapshot.local.as_ref().is_some_and(|l| !l.leaving);
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
                if local {
                    let offset = pie_offset(event.position);
                    state.update(cx, |s, cx| s.pie_hover(Some(offset), cx));
                } else if event.pressed_button == Some(MouseButton::Left) {
                    state.update(cx, |s, cx| {
                        let index = index_at(event.position, s);
                        s.drag_point(index, cx);
                    });
                }
            }
        })
        .on_mouse_down(MouseButton::Left, {
            let state = state.clone();
            let index_at = index_at.clone();
            move |event: &MouseDownEvent, window, cx| {
                if local {
                    return;
                }
                state.update(cx, |s, cx| {
                    let index = index_at(event.position, s);
                    s.press(index, cx);
                    window.focus(&s.focus_handle(), cx);
                });
            }
        })
        .on_mouse_up(MouseButton::Left, {
            let state = state.clone();
            move |_: &MouseUpEvent, _, cx| state.update(cx, |s, _| s.release_press())
        })
        .on_mouse_up_out(MouseButton::Left, {
            let state = state.clone();
            move |_: &MouseUpEvent, _, cx| state.update(cx, |s, _| s.release_press())
        })
        .on_hover({
            let state = state.clone();
            move |hovered: &bool, _, cx| {
                if !*hovered && local {
                    state.update(cx, |s, cx| s.pie_hover(None, cx));
                }
            }
        })
        .on_key_down({
            let state = state.clone();
            let chart = chart.clone();
            let on_zoom = on_zoom.clone();
            move |event: &KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "left" if !local => state.update(cx, |s, cx| s.step(-1, cx)),
                    "right" if !local => state.update(cx, |s, cx| s.step(1, cx)),
                    "escape" if local => state.update(cx, |s, cx| s.exit_local_zoom(cx)),
                    "escape" => state.update(cx, |s, cx| s.hide_details(cx)),
                    "enter" if !local => {
                        let Some(index) = state.read(cx).selected() else {
                            return;
                        };
                        zoom_into(&state, &chart, index, on_zoom.as_ref(), window, cx);
                    }
                    _ => return,
                }
                cx.stop_propagation();
            }
        })
        .child(canvas);
    if let Some(local) = snapshot.local.as_ref()
        && let Some(k) = local.hover
        && let Some(line) = chart.lines.get(k)
    {
        let (first, last) = local.indices;
        let last = last.min(line.y.len().saturating_sub(1));
        let total: i64 = line.y.get(first..=last).unwrap_or_default().iter().sum();
        let value =
            (total as f64 * f64::from(snapshot.alpha.get(k).copied().unwrap_or(1.0))) as i64;
        if value > 0 {
            element = element.child(pie_details(id, &line.name, value, line.color(), cx));
        }
    } else if let Some(index) = snapshot.details.filter(|i| *i < chart.x.len())
        && snapshot.local.is_none()
    {
        // Beside the point, on the side with more room, so the pointer
        // never sits under the box (tdesktop `setupDetails`).
        let ratio = snapshot.range.ratio(chart.x_frac[index]) as f32;
        let width = f32::from(snapshot.plot_bounds.get().size.width);
        let point_x = width * ratio;
        let place = if ratio > 0.5 {
            Err(px((width - point_x + POPUP_GAP).max(4.)))
        } else {
            Ok(px(if width > 0. { point_x + POPUP_GAP } else { 4. }))
        };
        let zoomable = on_zoom.is_some() || state.read(cx).zooms_locally();
        let on_click: Option<Rc<dyn Fn(&mut Window, &mut App)>> = zoomable.then(|| {
            let state = state.clone();
            let chart = chart.clone();
            let on_zoom = on_zoom.clone();
            Rc::new(move |window: &mut Window, cx: &mut App| {
                zoom_into(&state, &chart, index, on_zoom.as_ref(), window, cx);
            }) as Rc<dyn Fn(&mut Window, &mut App)>
        });
        element = element.child(details(
            id,
            chart,
            kind,
            index,
            &snapshot.enabled,
            place,
            on_click,
            snapshot.details_alpha,
            cx,
        ));
    }
    element
}

/// The range selector. Drag a handle to resize the window, drag inside
/// to move it, click outside to slide it there.
pub(super) fn footer(
    id: &str,
    state: &Entity<ChartState>,
    chart: &Rc<StatChart>,
    kind: ChartKind,
    snapshot: &Snapshot,
    cx: &App,
) -> impl IntoElement {
    let snap = FooterSnap {
        selector: snapshot.selector,
        footer_y: snapshot.footer_y,
        alpha: snapshot.alpha.clone(),
        local: snapshot.local.clone(),
    };
    let colors = palette(cx);
    let cell = snapshot.footer_bounds.clone();
    let cache = snapshot.footer_cache.clone();
    let paint_chart = chart.clone();
    let drag_state = state.clone();
    let canvas = canvas(
        move |_, _, _| {},
        move |bounds, _, window, _cx| {
            cell.set(bounds);
            paint_footer(bounds, &paint_chart, kind, &snap, &cache, &colors, window);
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
