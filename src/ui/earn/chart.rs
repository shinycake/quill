//! The revenue pages' charts (tdesktop `info_channel_earn_list.cpp` and
//! `info_bot_earn_list.cpp`): each is the statistics `ChartWidget` with
//! the graph's `currencyRate` set, so ad impressions read as a step line
//! (`ChartViewType::Bar`), TON rewards and Stars revenue as columns
//! (`ChartViewType::StackBar`), each with the range selector under it
//! and the amount in the currency and in US dollars in the tooltip. They
//! are the shared kit chart (`ui/stat_chart`).

use super::super::app::QuillApp;
use super::super::stat_chart::{StatChartView, parsed_chart};
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::stat_chart::{ChartCurrency, ChartKind};
use quill::telegram::envelope::StatisticalGraph;

/// What a chart shows.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum EarnChartKind {
    /// A count over time as a step line (tdesktop `ChartViewType::Bar`).
    Steps,
    /// An amount per period as columns (tdesktop `ChartViewType::StackBar`).
    Columns,
}

impl EarnChartKind {
    fn chart_kind(self) -> ChartKind {
        match self {
            Self::Steps => ChartKind::Bar,
            Self::Columns => ChartKind::StackBar,
        }
    }
}

impl QuillApp {
    /// A titled chart of `graph` for the revenue pages; `None` while an
    /// async graph loads or for a graph that has no data (tdesktop
    /// leaves those out). `cents_per_unit` is the dollar rate of a
    /// revenue chart, 0 for none.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn earn_chart(
        &self,
        chat_id: i64,
        id: &'static str,
        title: &'static str,
        graph: &StatisticalGraph,
        kind: EarnChartKind,
        cents_per_unit: f64,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let json = self.stat_graph_json(chat_id, graph, cx)?;
        let chart = parsed_chart(json)?;
        if chart.x.is_empty() {
            return None;
        }
        let unit = match chart.currency {
            ChartCurrency::Ton => Some(tr(keys::Q_MONEY_AMOUNTS_IN_TON)),
            ChartCurrency::Stars => Some(tr(keys::Q_MONEY_AMOUNTS_IN_STARS)),
            ChartCurrency::None => None,
        };
        Some(
            div()
                .debug_selector(move || id.to_string())
                .child(
                    StatChartView::new(id, title, kind.chart_kind(), chart)
                        .currency(cents_per_unit, unit),
                )
                .into_any_element(),
        )
    }
}
