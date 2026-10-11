//! One statistics chart: header with the visible dates, the plot with
//! rulers and a hover tooltip, the range selector under it and the
//! legend that switches lines on and off (tdesktop
//! `statistics/chart_widget.cpp`). The math lives in `quill::stat_chart`;
//! this module owns the interaction state and the drawing.

mod details;
mod paint;
mod state;

use super::chat_theme::text_muted;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::stat_chart::{ChartKind, StatChart};
use state::ChartState;
use std::rc::Rc;

pub(super) use cache::parsed_chart;

/// Height of the plot (`statisticsChartHeight`).
const PLOT_HEIGHT: f32 = 200.;
/// Height of the range selector (`statisticsChartFooterHeight`).
const FOOTER_HEIGHT: f32 = 42.;

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

    /// The chart can zoom into a day: clicking the tooltip calls
    /// `on_zoom` with the point's x; `zoomed` is the day shown now.
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
            let mut view = self.render_chart(window, cx);
            let id = self.id.clone();
            view = view.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(div().text_xs().text_color(text_muted()).child(note))
                    .children(self.zoom_out_button(&id)),
            );
            return view.into_any_element();
        }
        self.render_chart(window, cx).into_any_element()
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
        let snapshot = state.read(cx).snapshot();
        let (start, end) = chart.visible_indices(snapshot.range);
        let subtitle: SharedString = self
            .day_title
            .clone()
            .unwrap_or_else(|| chart.range_title(start, end).into());
        let trailing = if self.day_title.is_some() {
            self.zoom_out_button(&self.id)
        } else {
            None
        };
        let zoomable = self.on_zoom.is_some();
        let mut section = div()
            .id(ElementId::Name(format!("{}-section", self.id).into()))
            .flex()
            .flex_col()
            .gap_2()
            .child(self.header(subtitle, trailing, cx))
            .child(paint::plot(
                &self.id,
                &state,
                &chart,
                kind,
                &snapshot,
                self.on_zoom.clone().filter(|_| zoomable),
                window,
                cx,
            ));
        if !chart.footer_hidden && chart.x.len() > 1 {
            section = section.child(paint::footer(&self.id, &state, &chart, kind, &snapshot, cx));
        }
        if chart.lines.len() > 1 {
            section = section.child(legend(&self.id, &state, &chart, &snapshot.enabled, cx));
        }
        section
    }
}

/// The line switches under a chart with more than one line
/// (`ChartLinesFilterWidget`): filled when shown, outlined when hidden.
/// The last shown line cannot be switched off.
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
        let label: SharedString = if on {
            format!("\u{2713} {}", line.name).into()
        } else {
            line.name.clone().into()
        };
        row = row.child(
            div()
                .id(ElementId::Name(format!("{id}-line-{index}").into()))
                .debug_selector(|| format!("{id}-line-{index}"))
                .role(gpui_kit::Role::CheckBox)
                .aria_label(SharedString::from(line.name.clone()))
                .tab_index(0)
                .px_2()
                .py_0p5()
                .rounded_full()
                .border_2()
                .border_color(color)
                .text_xs()
                .when(on, |d| {
                    d.bg(color).text_color(cx.theme().primary_foreground)
                })
                .when(!on, |d| d.text_color(color))
                .child(label)
                .on_click(move |_, _, cx| {
                    state.update(cx, |state, cx| state.toggle_line(index, cx));
                }),
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
        let take = |from: usize| &bytes[from.min(bytes.len())..(from + 128).min(bytes.len())];
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
