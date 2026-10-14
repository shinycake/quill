//! One statistics chart (tdesktop `statistics/chart_widget.cpp`) on
//! gpui-kit's charts: a header with the shown dates, the plot with the
//! kit's hover tooltip, the range selector under it and the legend that
//! switches lines on and off. A click on a point zooms into its day, or
//! into a pie on a percentage chart. The math lives in
//! `quill::stat_chart`.

mod footer;
mod pie;
mod plots;
mod series;
mod stacked_bars;
mod state;
mod tooltip;

use super::actions::CancelSearch;
use super::chat_theme::text_muted;
use gpui_kit::component::button::*;
use gpui_kit::component::slider::Slider;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use plots::PlotInput;
use quill::stat_chart::{ChartKind, StatChart};
use series::{points, zoomable_point};
use state::ChartState;
use std::rc::Rc;

pub(super) use cache::parsed_chart;

/// Height of the plot (`statisticsChartHeight`).
const PLOT_HEIGHT: f32 = 200.;

type ZoomFn = Rc<dyn Fn(i64, &mut Window, &mut App)>;
type UnzoomFn = Rc<dyn Fn(&mut Window, &mut App)>;

/// The day a chart is zoomed into.
pub(super) struct ZoomedDay {
    /// The point zoomed into, milliseconds.
    pub x: i64,
    /// That day's chart once it loaded.
    pub chart: Option<Rc<StatChart>>,
    /// Why it did not load.
    pub error: Option<SharedString>,
}

/// A chart section of the statistics panel.
#[derive(IntoElement)]
pub(super) struct StatChartView {
    id: SharedString,
    title: SharedString,
    kind: ChartKind,
    chart: Rc<StatChart>,
    on_zoom: Option<ZoomFn>,
    zoomed: Option<ZoomedDay>,
    on_unzoom: Option<UnzoomFn>,
    /// A zoomed day's own chart: its header names the day and offers
    /// "Zoom out".
    day_title: Option<SharedString>,
}

impl StatChartView {
    pub(super) fn new(
        id: impl Into<SharedString>,
        title: impl Into<SharedString>,
        kind: ChartKind,
        chart: Rc<StatChart>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            kind,
            chart,
            on_zoom: None,
            zoomed: None,
            on_unzoom: None,
            day_title: None,
        }
    }

    /// The chart can zoom into a day: a click on a point calls `on_zoom`
    /// with its x; `zoomed` is the day shown now.
    pub(super) fn zoomable(
        mut self,
        on_zoom: impl Fn(i64, &mut Window, &mut App) + 'static,
        zoomed: Option<ZoomedDay>,
        on_unzoom: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_zoom = Some(Rc::new(on_zoom));
        self.zoomed = zoomed;
        self.on_unzoom = Some(Rc::new(on_unzoom));
        self
    }

    fn header(
        &self,
        subtitle: SharedString,
        trailing: Option<AnyElement>,
        cx: &App,
    ) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(cx.theme().foreground)
                            .truncate()
                            .child(self.title.clone()),
                    )
                    .when(!subtitle.is_empty(), |d| {
                        d.child(
                            div()
                                .text_xs()
                                .text_color(text_muted())
                                .truncate()
                                .child(subtitle),
                        )
                    }),
            )
            .children(trailing)
    }

    fn zoom_out_button(&self, id: &str) -> Option<AnyElement> {
        let on_unzoom = self.on_unzoom.clone()?;
        Some(
            Button::new(SharedString::from(format!("{id}-zoom-out")))
                .label("Zoom out")
                .small()
                .outline()
                .on_click(move |_, window, cx| on_unzoom(window, cx))
                .into_any_element(),
        )
    }
}

impl RenderOnce for StatChartView {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // Zoomed into a day that loaded: that day's chart takes the place.
        if let Some(day) = self.zoomed.take() {
            let index = self.chart.x.iter().position(|x| *x as i64 == day.x);
            let day_title: SharedString = index
                .map(|i| self.chart.range_title(i, i))
                .unwrap_or_default()
                .into();
            if let Some(chart) = day.chart {
                return StatChartView {
                    id: format!("{}-day-{}", self.id, day.x).into(),
                    title: self.title.clone(),
                    kind: self.kind,
                    chart,
                    on_zoom: None,
                    zoomed: None,
                    on_unzoom: self.on_unzoom.clone(),
                    day_title: Some(day_title),
                }
                .into_any_element();
            }
            // Still loading, or failed: the overview stays with a note.
            let note: SharedString = day.error.unwrap_or_else(|| "Loading the day…".into());
            let id = self.id.clone();
            return self
                .render_chart(window, cx)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(div().text_xs().text_color(text_muted()).child(note))
                        .children(self.zoom_out_button(&id)),
                )
                .into_any_element();
        }
        self.render_chart(window, cx).into_any_element()
    }
}

/// Zoom into point `index`: the server's day chart, or a pie of the days
/// around it on a percentage chart. A point with nothing in it does not
/// zoom (tdesktop).
fn zoom_into(
    state: &Entity<ChartState>,
    chart: &StatChart,
    kind: ChartKind,
    index: usize,
    on_zoom: Option<&ZoomFn>,
    window: &mut Window,
    cx: &mut App,
) {
    let positive = points(chart, kind, &state.read(cx).enabled, index, index)
        .first()
        .is_some_and(zoomable_point);
    if !positive {
        return;
    }
    if let Some(on_zoom) = on_zoom {
        if let Some(x) = chart.x.get(index) {
            on_zoom(*x as i64, window, cx);
        }
    } else if state.read(cx).zooms_locally() {
        state.update(cx, |s, cx| s.enter_pie(index, cx));
    }
}

impl StatChartView {
    fn render_chart(&self, window: &mut Window, cx: &mut App) -> Stateful<Div> {
        let chart = self.chart.clone();
        let kind = self.kind;
        let state = window.use_keyed_state(ElementId::Name(self.id.clone()), cx, |_, cx| {
            ChartState::new(&chart, kind, cx)
        });
        state.update(cx, |state, cx| state.sync(&chart, kind, cx));
        // Copy what the frame needs out of the state, so no borrow of it
        // is held while the elements below are built.
        let (enabled, visible, pie_days, key_point, hovered, focus, range, pie_slider, local) = {
            let s = state.read(cx);
            (
                s.enabled.clone(),
                s.visible(cx),
                s.pie_days(cx),
                s.key_point,
                s.hovered.clone(),
                s.focus.clone(),
                s.range.clone(),
                s.pie.as_ref().map(|p| p.slider.clone()),
                s.zooms_locally(),
            )
        };
        let zoomable = self.on_zoom.is_some() || local;
        let subtitle: SharedString = match (pie_days, self.day_title.clone()) {
            (Some((first, last)), _) => chart.range_title(first, last).into(),
            (None, Some(day)) => day,
            (None, None) => chart.range_title(visible.0, visible.1).into(),
        };
        let trailing = if pie_days.is_some() {
            // The pie's "Zoom out" (tdesktop `processLocalZoom`).
            let state = state.clone();
            let selector = format!("{}-pie-zoom-out", self.id);
            Some(
                div()
                    .debug_selector(move || selector.clone())
                    .child(
                        Button::new(SharedString::from(format!("{}-pie-zoom-out", self.id)))
                            .label("Zoom out")
                            .small()
                            .outline()
                            .on_click(move |_, _, cx| state.update(cx, |s, cx| s.exit_pie(cx))),
                    )
                    .into_any_element(),
            )
        } else if self.day_title.is_some() {
            self.zoom_out_button(&self.id)
        } else {
            None
        };

        let plot = match pie_days {
            Some(days) => pie::pie(&self.id, &chart, &enabled, days),
            None => plots::plot(PlotInput {
                id: self.id.clone(),
                chart: chart.clone(),
                kind,
                points: points(&chart, kind, &enabled, visible.0, visible.1),
                enabled: enabled.clone(),
                hovered: hovered.clone(),
                zoomable,
                height: PLOT_HEIGHT,
            }),
        };
        let id = self.id.clone();
        let mut section = div()
            .id(ElementId::Name(format!("{id}-section").into()))
            .flex()
            .flex_col()
            .gap_2()
            .child(self.header(subtitle, trailing, cx))
            .child(self.plot_frame(plot, &state, hovered, focus, zoomable, pie_days.is_some()));
        if let (Some(index), None) = (key_point, pie_days) {
            section = section.child(self.key_readout(&chart, &enabled, index, zoomable, cx));
        }
        if let Some(slider) = pie_slider {
            section = section.child(Slider::new(&slider));
        } else if let Some(range) = range {
            let shown = chart
                .lines
                .iter()
                .enumerate()
                .filter(|(k, _)| enabled.get(*k).copied().unwrap_or(true))
                .map(|(k, line)| (k, rgb(line.color()).into()))
                .collect();
            let back = chart.x.len().saturating_sub(1);
            section = section.child(footer::footer(
                &id,
                kind,
                points(&chart, kind, &enabled, 0, back),
                &enabled,
                shown,
                visible,
                &range,
                cx,
            ));
        }
        if chart.lines.len() > 1 {
            section = section.child(legend(&id, &state, &chart, &enabled, cx));
        }
        section
    }

    /// The plot's frame: focusable, a click zooms into the point the
    /// tooltip shows, the arrow keys walk the points and Enter zooms.
    fn plot_frame(
        &self,
        plot: AnyElement,
        state: &Entity<ChartState>,
        hovered: Rc<std::cell::Cell<Option<usize>>>,
        focus: FocusHandle,
        zoomable: bool,
        pie: bool,
    ) -> impl IntoElement {
        let id = self.id.clone();
        let chart = self.chart.clone();
        let kind = self.kind;
        let on_zoom = self.on_zoom.clone();
        div()
            .id(ElementId::Name(format!("{id}-plot").into()))
            .debug_selector({
                let id = id.clone();
                move || format!("{id}-plot")
            })
            .role(gpui_kit::Role::Group)
            .aria_label(self.title.clone())
            .track_focus(&focus)
            .tab_index(0)
            .relative()
            .w_full()
            .h(px(PLOT_HEIGHT))
            .when(zoomable && !pie, |d| d.cursor_pointer())
            .on_hover({
                let hovered = hovered.clone();
                move |inside: &bool, _, _| {
                    if !*inside {
                        hovered.set(None);
                    }
                }
            })
            .on_mouse_down(MouseButton::Left, {
                let focus = focus.clone();
                move |_, window, cx| window.focus(&focus, cx)
            })
            .on_click({
                let state = state.clone();
                let chart = chart.clone();
                let on_zoom = on_zoom.clone();
                move |_, window, cx| {
                    if pie || !zoomable {
                        return;
                    }
                    if let Some(index) = hovered.get() {
                        zoom_into(&state, &chart, kind, index, on_zoom.as_ref(), window, cx);
                    }
                }
            })
            // Escape is bound to the app-wide cancel action, which closes
            // the panel; the focused plot takes it first while it has a pie
            // or a picked point to drop.
            .on_action({
                let state = state.clone();
                move |_: &CancelSearch, _, cx| {
                    if pie {
                        state.update(cx, |s, cx| s.exit_pie(cx));
                    } else if state.read(cx).key_point.is_some() {
                        state.update(cx, |s, cx| s.clear_key_point(cx));
                    } else {
                        cx.propagate();
                    }
                }
            })
            .on_key_down({
                let state = state.clone();
                move |event: &KeyDownEvent, window, cx| {
                    match event.keystroke.key.as_str() {
                        "left" if !pie => state.update(cx, |s, cx| s.step(-1, cx)),
                        "right" if !pie => state.update(cx, |s, cx| s.step(1, cx)),
                        "enter" if !pie && zoomable => {
                            let Some(index) = state.read(cx).key_point else {
                                return;
                            };
                            zoom_into(&state, &chart, kind, index, on_zoom.as_ref(), window, cx);
                        }
                        _ => return,
                    }
                    cx.stop_propagation();
                }
            })
            .child(plot)
    }

    /// The point picked with the arrow keys, read out under the plot the
    /// way the tooltip reads it.
    fn key_readout(
        &self,
        chart: &StatChart,
        enabled: &[bool],
        index: usize,
        zoomable: bool,
        cx: &App,
    ) -> impl IntoElement {
        let shares = chart.has_percentages || self.kind == ChartKind::StackLinear;
        let point = points(chart, self.kind, enabled, index, index);
        let theme = cx.theme();
        div()
            .id(ElementId::Name(format!("{}-details", self.id).into()))
            .debug_selector({
                let id = self.id.clone();
                move || format!("{id}-details")
            })
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .text_color(theme.popover_foreground)
            .children(point.first().map(|p| {
                tooltip::body(chart, p, enabled, shares, zoomable && zoomable_point(p), cx)
            }))
    }
}

/// The line switches under a chart with more than one line
/// (`ChartLinesFilterWidget`), as kit toggles keyed by each line's
/// color. The last shown line cannot be switched off.
fn legend(
    id: &str,
    state: &Entity<ChartState>,
    chart: &StatChart,
    enabled: &[bool],
    cx: &App,
) -> impl IntoElement {
    let mut row = div().flex().flex_wrap().gap_1();
    for (index, line) in chart.lines.iter().enumerate() {
        let on = enabled.get(index).copied().unwrap_or(true);
        let color: Hsla = rgb(line.color()).into();
        let state = state.clone();
        let swatch = div()
            .flex_none()
            .size(px(10.))
            .rounded_sm()
            .border_2()
            .border_color(color)
            .when(on, |d| d.bg(color));
        let item_id = format!("{id}-line-{index}");
        row = row.child(
            div().debug_selector(move || item_id.clone()).child(
                Toggle::new(SharedString::from(format!("{id}-line-{index}")))
                    .small()
                    .outline()
                    .checked(on)
                    .tooltip(SharedString::from(line.name.clone()))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .text_xs()
                            .text_color(if on {
                                cx.theme().foreground
                            } else {
                                cx.theme().muted_foreground
                            })
                            .child(swatch)
                            .child(line.name.clone()),
                    )
                    .on_click(move |_, _, cx| {
                        state.update(cx, |state, cx| state.toggle_line(index, cx));
                    }),
            ),
        );
    }
    row
}

mod cache {
    //! Parsed charts by JSON, so a panel redraw does not re-parse every
    //! graph. Keyed by the JSON buffer's address and length, checked
    //! against a sample of its bytes.
    use quill::stat_chart::{StatChart, parse_chart};
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;

    const LIMIT: usize = 64;

    struct Entry {
        sample: u64,
        chart: Option<Rc<StatChart>>,
    }

    thread_local! {
        static CHARTS: RefCell<HashMap<(usize, usize), Entry>> = RefCell::new(HashMap::new());
    }

    fn sample(json: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let bytes = json.as_bytes();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let take = |from: usize| {
            bytes
                .get(from.min(bytes.len())..(from + 128).min(bytes.len()))
                .unwrap_or_default()
        };
        take(0).hash(&mut hasher);
        take(bytes.len() / 2).hash(&mut hasher);
        take(bytes.len().saturating_sub(128)).hash(&mut hasher);
        hasher.finish()
    }

    /// The chart in `json`, or `None` when it does not parse.
    pub(in crate::ui) fn parsed_chart(json: &str) -> Option<Rc<StatChart>> {
        let key = (json.as_ptr() as usize, json.len());
        let sample = sample(json);
        CHARTS.with(|charts| {
            let mut charts = charts.borrow_mut();
            if let Some(entry) = charts.get(&key)
                && entry.sample == sample
            {
                return entry.chart.clone();
            }
            if charts.len() >= LIMIT {
                charts.clear();
            }
            let chart = parse_chart(json).ok().map(Rc::new);
            charts.insert(
                key,
                Entry {
                    sample,
                    chart: chart.clone(),
                },
            );
            chart
        })
    }
}
