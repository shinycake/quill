//! Which charts a statistics page shows, their titles and kinds
//! (tdesktop `FillStatistic`), and the workbook "Export to File" writes
//! from them (`statistics_sheets.cpp`).

use super::{ChartKind, Sheet, StatChart, chart_sheet, overview_sheet};
use crate::telegram::envelope::{ChatStatistics, StatisticalGraph};

/// One chart of a statistics page.
pub struct ChartSlot<'a> {
    /// Element id suffix.
    pub id: &'static str,
    pub title: &'static str,
    pub graph: &'a StatisticalGraph,
    pub kind: ChartKind,
}

fn slot<'a>(
    id: &'static str,
    title: &'static str,
    graph: &'a StatisticalGraph,
    kind: ChartKind,
) -> ChartSlot<'a> {
    ChartSlot {
        id,
        title,
        graph,
        kind,
    }
}

/// Every chart of a chat's statistics, in tdesktop's order.
pub fn chart_slots(statistics: &ChatStatistics) -> Vec<ChartSlot<'_>> {
    use ChartKind::*;
    match statistics {
        ChatStatistics::Channel(s) => vec![
            slot("growth", "Growth", &s.member_count_graph, Linear),
            slot("followers", "Followers", &s.join_graph, Linear),
            slot("mute", "Notifications", &s.mute_graph, Linear),
            slot(
                "hours",
                "Views by hours (UTC)",
                &s.view_count_by_hour_graph,
                Linear,
            ),
            slot(
                "view-sources",
                "Views by source",
                &s.view_count_by_source_graph,
                StackBar,
            ),
            slot(
                "join-sources",
                "New followers by source",
                &s.join_by_source_graph,
                StackBar,
            ),
            slot("languages", "Languages", &s.language_graph, StackLinear),
            slot(
                "interactions",
                "Interactions",
                &s.message_interaction_graph,
                DoubleLinear,
            ),
            slot(
                "iv",
                "IV interactions",
                &s.instant_view_interaction_graph,
                DoubleLinear,
            ),
            slot("reactions", "Reactions", &s.message_reaction_graph, Bar),
            slot(
                "story-interactions",
                "Story interactions",
                &s.story_interaction_graph,
                DoubleLinear,
            ),
            slot(
                "story-reactions",
                "Story reactions",
                &s.story_reaction_graph,
                Bar,
            ),
        ],
        ChatStatistics::Supergroup(s) => vec![
            slot("growth", "Growth", &s.member_count_graph, Linear),
            slot("members", "Group members", &s.join_graph, Linear),
            slot(
                "join-sources",
                "New members by source",
                &s.join_by_source_graph,
                StackBar,
            ),
            slot(
                "languages",
                "Members' primary language",
                &s.language_graph,
                StackLinear,
            ),
            slot("messages", "Messages", &s.message_content_graph, StackBar),
            slot("actions", "Actions", &s.action_graph, DoubleLinear),
            slot("hours", "Views by hours", &s.day_graph, Linear),
            slot("weekdays", "Top days of week", &s.week_graph, StackLinear),
        ],
    }
}

/// The workbook of a chat's statistics: the overview, then one sheet per
/// chart that has data. `chart` resolves a graph (an async one through
/// what has loaded).
pub fn chat_statistics_sheets(
    statistics: &ChatStatistics,
    chart: impl Fn(&StatisticalGraph) -> Option<StatChart>,
) -> Vec<Sheet> {
    let pair = |v: &crate::telegram::envelope::StatisticalValue| (v.value, Some(v.previous_value));
    let values: Vec<(&str, f64, Option<f64>)> = match statistics {
        ChatStatistics::Channel(s) => {
            let mut rows: Vec<(&str, f64, Option<f64>)> = [
                ("Followers", &s.member_count),
                ("Views per post", &s.mean_message_view_count),
                ("Shares per post", &s.mean_message_share_count),
                ("Reactions per post", &s.mean_message_reaction_count),
                ("Views per story", &s.mean_story_view_count),
                ("Shares per story", &s.mean_story_share_count),
                ("Reactions per story", &s.mean_story_reaction_count),
            ]
            .into_iter()
            .map(|(title, v)| (title, pair(v).0, pair(v).1))
            .collect();
            rows.push((
                "Enabled notifications",
                s.enabled_notifications_percentage,
                None,
            ));
            rows
        }
        ChatStatistics::Supergroup(s) => [
            ("Members", &s.member_count),
            ("Messages", &s.message_count),
            ("Viewing members", &s.viewer_count),
            ("Posting members", &s.sender_count),
        ]
        .into_iter()
        .map(|(title, v)| (title, pair(v).0, pair(v).1))
        .collect(),
    };
    let mut sheets = vec![overview_sheet(&values)];
    for slot in chart_slots(statistics) {
        if let Some(data) = chart(slot.graph) {
            sheets.push(chart_sheet(slot.title, &data));
        }
    }
    sheets
}

/// The numbers of a post or story page; `None` is unknown (left blank).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PostCounters {
    pub views: Option<i32>,
    pub reactions: Option<i32>,
    pub public_shares: Option<i32>,
    pub private_shares: Option<i32>,
}

/// The workbook of a post's or story's statistics: the counters, the
/// interaction and reaction charts, and for a poll its votes.
pub fn post_statistics_sheets(
    counters: PostCounters,
    charts: &[(&str, Option<StatChart>)],
) -> Vec<Sheet> {
    let value = |v: Option<i32>| v.map_or(-1.0, f64::from);
    let mut sheets = vec![overview_sheet(&[
        ("Views", value(counters.views), None),
        ("Reactions", value(counters.reactions), None),
        ("Public shares", value(counters.public_shares), None),
        ("Private shares", value(counters.private_shares), None),
    ])];
    for (title, chart) in charts {
        if let Some(chart) = chart {
            sheets.push(chart_sheet(title, chart));
        }
    }
    sheets
}
