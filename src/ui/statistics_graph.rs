//! Statistics graphs as chart sections: a ready graph draws at once, an
//! async one loads when its section is first shown (tdesktop requests
//! them as it fills the page) and stays hidden until it arrives, and a
//! graph with a zoom token opens one day on a tooltip click.

use super::app::QuillApp;
use super::chat_theme::{danger, success, text_muted};
use super::stat_chart::{StatChartView, ZoomedDay, parsed_chart};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::stat_chart::{ChartKind, overview_delta};
use quill::state::{GraphFetch, Session};
use quill::telegram::envelope::{StatisticalGraph, StatisticalValue};

/// Ask once for an async graph; the driver skips one in flight.
fn request_graph(chat_id: i64, token: String, cx: &mut Context<QuillApp>) {
    let weak = cx.weak_entity();
    cx.defer(move |cx| {
        let _ = weak.update(cx, |this, cx| {
            if let Some(live) = this.live.as_mut()
                && matches!(
                    live.driver.load_stat_graph(ChatId(chat_id), &token, 0),
                    Ok(Some(_))
                )
            {
                cx.notify();
            }
        });
    });
}

impl QuillApp {
    /// The session the statistics read and write: the live one, or the
    /// demo's.
    fn stats_session_mut(&mut self) -> Option<&mut Session> {
        match self.live.as_mut() {
            Some(live) => Some(&mut live.driver.session),
            None => self.demo_session.as_mut(),
        }
    }

    /// Zoom a graph into the day at `x`.
    pub(super) fn zoom_stat_graph(
        &mut self,
        chat_id: i64,
        token: &str,
        x: i64,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            if live
                .driver
                .zoom_stat_graph(ChatId(chat_id), token, x)
                .is_err()
            {
                self.connection.status_note = "could not load the chart".into();
            }
        } else if let Some(session) = self.stats_session_mut() {
            session.groups.stats.zoomed.insert(token.to_string(), x);
        }
        cx.notify();
    }

    pub(super) fn unzoom_stat_graph(&mut self, token: &str, cx: &mut Context<Self>) {
        if let Some(session) = self.stats_session_mut() {
            session.groups.stats.zoomed.remove(token);
        }
        cx.notify();
    }

    /// A chart section for `graph` of `chat_id`; `None` while an async
    /// graph loads, for a graph the server could not build, or for JSON
    /// that does not parse (tdesktop leaves those out too).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn stat_graph_section(
        &self,
        chat_id: i64,
        id: &str,
        title: &str,
        graph: &StatisticalGraph,
        kind: ChartKind,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let stats = &self.session()?.groups.stats;
        let (json, zoom_token) = match graph {
            StatisticalGraph::Data {
                json_data,
                zoom_token,
            } => (json_data, zoom_token),
            StatisticalGraph::Async { token } => match stats.graph(chat_id, token, 0) {
                Some(GraphFetch::Loaded(StatisticalGraph::Data {
                    json_data,
                    zoom_token,
                })) => (json_data, zoom_token),
                None => {
                    request_graph(chat_id, token.clone(), cx);
                    return None;
                }
                _ => return None,
            },
            StatisticalGraph::Error { .. } => return None,
        };
        let chart = parsed_chart(json)?;
        let mut view = StatChartView::new(id.to_string(), title.to_string(), kind, chart);
        if !zoom_token.is_empty() {
            let zoomed = stats.zoomed.get(zoom_token).map(|&x| {
                let (chart, error) = match stats.graph(chat_id, zoom_token, x) {
                    Some(GraphFetch::Loaded(StatisticalGraph::Data { json_data, .. })) => {
                        match parsed_chart(json_data) {
                            Some(chart) => (Some(chart), None),
                            None => (None, Some("This day has no chart".into())),
                        }
                    }
                    Some(GraphFetch::Loaded(StatisticalGraph::Error { error_message })) => {
                        (None, Some(error_message.clone().into()))
                    }
                    Some(GraphFetch::Failed(message)) => (None, Some(message.clone().into())),
                    _ => (None, None),
                };
                ZoomedDay { x, chart, error }
            });
            let zoom_weak = cx.weak_entity();
            let unzoom_weak = cx.weak_entity();
            let (zoom_token, unzoom_token) = (zoom_token.clone(), zoom_token.clone());
            view = view.zoomable(
                move |x, _, cx| {
                    let _ = zoom_weak.update(cx, |this, cx| {
                        this.zoom_stat_graph(chat_id, &zoom_token, x, cx);
                    });
                },
                zoomed,
                move |_, cx| {
                    let _ = unzoom_weak.update(cx, |this, cx| {
                        this.unzoom_stat_graph(&unzoom_token, cx);
                    });
                },
            );
        }
        Some(view.into_any_element())
    }
}

/// A graph outside the statistics panel (story viewer, poll results):
/// drawn when ready, a note while the server is still building it.
pub(super) fn stat_graph_block(
    id: &str,
    title: &str,
    graph: &StatisticalGraph,
    kind: ChartKind,
) -> Option<AnyElement> {
    let note = match graph {
        StatisticalGraph::Data { json_data, .. } => {
            let chart = parsed_chart(json_data)?;
            return Some(
                StatChartView::new(id.to_string(), title.to_string(), kind, chart)
                    .into_any_element(),
            );
        }
        StatisticalGraph::Async { .. } => "Still processing. Check back later.".to_string(),
        StatisticalGraph::Error { error_message } => error_message.clone(),
    };
    Some(
        div()
            .flex()
            .flex_col()
            .child(div().text_sm().font_semibold().child(title.to_string()))
            .child(div().text_xs().text_color(text_muted()).child(note))
            .into_any_element(),
    )
}

/// One overview number: the value, its change against the previous
/// period in green or red, and what it counts (tdesktop `FillOverview`).
pub(super) struct OverviewCell {
    pub value: String,
    pub delta: Option<(String, bool)>,
    pub label: &'static str,
}

impl OverviewCell {
    pub(super) fn stat(value: &StatisticalValue, label: &'static str) -> Self {
        Self {
            value: quill::stat_chart::overview_value(value.value),
            delta: overview_delta(
                value.value,
                value.previous_value,
                value.growth_rate_percentage,
            ),
            label,
        }
    }

    pub(super) fn plain(value: String, label: &'static str) -> Self {
        Self {
            value,
            delta: None,
            label,
        }
    }
}

/// The overview cells in two columns.
pub(super) fn overview_grid(cells: Vec<OverviewCell>) -> Div {
    let mut grid = div().grid().grid_cols(2).gap_x_3().gap_y_3();
    for cell in cells {
        let mut top = div()
            .flex()
            .items_baseline()
            .gap_1()
            .child(div().text_lg().font_semibold().child(cell.value));
        if let Some((delta, down)) = cell.delta {
            top = top.child(
                div()
                    .text_xs()
                    .text_color(if down { danger() } else { success() })
                    .child(delta),
            );
        }
        grid = grid.child(
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .child(top)
                .child(div().text_xs().text_color(text_muted()).child(cell.label)),
        );
    }
    grid
}
