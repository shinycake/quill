//! channel statistics panel.

use super::app::QuillApp;
use super::groups::apply_ready_channels;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::state::{ChatStatisticsFetch, RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{ChatKind, ChatStatistics, StatisticalGraph, StatisticalValue};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
/// `ReadyChannelStats` fixture (Phase D2): like `apply_ready_channels`,
/// plus an `updateSupergroupFullInfo` with `can_get_statistics: true`
/// (schema 1.8.67 line 2792) and a `chatStatisticsChannel` response
/// through the real `getChatStatistics` reducer path, so the statistics
/// panel renders loaded data. One graph (`story_reaction_graph`) is a
/// `statisticalGraphAsync` and one (`language_graph`) a
/// `statisticalGraphError` to show the honest states.
pub(super) fn apply_ready_channel_stats(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    apply_ready_channels(session, sink, seq);
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let stats_extra = session.request(RequestPurpose::GetChatStatistics, Some(ChatId(13)));
    let value = |v: f64, previous: f64, growth: f64| {
        format!(
            r#"{{"@type":"statisticalValue","value":{v},"previous_value":{previous},"growth_rate_percentage":{growth}}}"#
        )
    };
    // TDLib `statisticalGraphData.json_data`: columns of ["x",t..] +
    // ["y0",v..]; the panel sparklines the first non-"x" numeric column.
    let graph_data = |values: &[i64]| {
        let mut x_col = vec![serde_json::json!("x")];
        let mut y_col = vec![serde_json::json!("y0")];
        for (i, v) in values.iter().enumerate() {
            x_col.push(serde_json::json!(1_788_000_000i64 + i as i64 * 86_400));
            y_col.push(serde_json::json!(v));
        }
        serde_json::to_string(&serde_json::json!({
            "columns": [x_col, y_col],
            "types": {"x": "x", "y0": "line"},
        }))
        .unwrap_or_default()
        .replace('"', "\\\"")
    };
    let graph = |values: &[i64]| {
        format!(
            r#"{{"@type":"statisticalGraphData","json_data":"{}","zoom_token":""}}"#,
            graph_data(values)
        )
    };
    let async_graph = r#"{"@type":"statisticalGraphAsync","token":"stats-token-1"}"#.to_string();
    let error_graph =
        r#"{"@type":"statisticalGraphError","error_message":"STATS_GRAPH_NOT_AVAILABLE"}"#
            .to_string();
    let jsons = [
        // `updateSupergroupFullInfo` carries its own `supergroup_id`
        // (schema 1.8.67 line 10750) — no pending-request correlation.
        r#"{"@type":"updateSupergroupFullInfo","supergroup_id":13,"supergroup_full_info":{"description":"The demo channel.","member_count":12345,"linked_chat_id":16,"can_get_statistics":true}}"#
            .to_string(),
        format!(
            r#"{{"@type":"chatStatisticsChannel","@extra":"{}","period":{{"@type":"dateRange","start_date":1788000000,"end_date":1788604800}},"member_count":{member_count},"mean_message_view_count":{mean_views},"mean_message_share_count":{mean_shares},"mean_message_reaction_count":{mean_reactions},"mean_story_view_count":{mean_story_views},"mean_story_share_count":{mean_story_shares},"mean_story_reaction_count":{mean_story_reactions},"enabled_notifications_percentage":61.5,"member_count_graph":{members_graph},"join_graph":{join_graph},"mute_graph":{mute_graph},"view_count_by_hour_graph":{hour_graph},"view_count_by_source_graph":{source_graph},"join_by_source_graph":{join_source_graph},"language_graph":{language_graph},"message_interaction_graph":{interaction_graph},"message_reaction_graph":{reaction_graph},"story_interaction_graph":{story_graph},"story_reaction_graph":{story_reaction_graph},"instant_view_interaction_graph":{iv_graph},"recent_interactions":[{{"@type":"chatStatisticsInteractionInfo","object_type":{{"@type":"chatStatisticsObjectTypeMessage","message_id":201}},"view_count":12402,"forward_count":7,"reaction_count":213}},{{"@type":"chatStatisticsInteractionInfo","object_type":{{"@type":"chatStatisticsObjectTypeStory","story_id":44}},"view_count":987,"forward_count":12,"reaction_count":65}}]}}"#,
            stats_extra.0,
            member_count = value(12345.0, 11700.0, 5.5),
            mean_views = value(8421.0, 9010.0, -6.5),
            mean_shares = value(312.0, 280.0, 11.4),
            mean_reactions = value(428.0, 390.0, 9.7),
            mean_story_views = value(5120.0, 4980.0, 2.8),
            mean_story_shares = value(96.0, 104.0, -7.7),
            mean_story_reactions = value(154.0, 140.0, 10.0),
            members_graph = graph(&[11800, 11950, 12080, 12190, 12260, 12310, 12345]),
            join_graph = graph(&[120, 145, 98, 160, 132, 175, 141]),
            mute_graph = graph(&[12, 9, 14, 11, 8, 10, 9]),
            hour_graph = graph(&[120, 90, 60, 45, 55, 110, 230, 410, 620, 780, 850, 900, 870, 820, 790, 760, 800, 880, 950, 990, 940, 700, 420, 210]),
            source_graph = graph(&[5200, 2100, 900, 221]),
            join_source_graph = graph(&[95, 30, 12, 4]),
            language_graph = error_graph,
            interaction_graph = graph(&[4100, 4300, 4050, 4600, 4400, 4700, 4521]),
            reaction_graph = graph(&[380, 410, 395, 450, 430, 470, 428]),
            story_graph = graph(&[4900, 5050, 4980, 5200, 5120]),
            story_reaction_graph = async_graph,
            iv_graph = graph(&[1200, 1350, 1280, 1420, 1390]),
        ),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// Compact view-count formatting for broadcast posts (`👁 1.2K`, `👁 3.4M`).
pub(super) fn format_view_count(count: i32) -> String {
    if count >= 1_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.1}K", count as f64 / 1_000.0)
    } else {
        count.to_string()
    }
}

/// Phase D2: compact formatting for `statisticalValue` counts (f64).
pub(super) fn format_stat_count(value: f64) -> String {
    if value >= 1_000_000.0 {
        format!("{:.1}M", value / 1_000_000.0)
    } else if value >= 1_000.0 {
        format!("{:.1}K", value / 1_000.0)
    } else if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else if value >= 100.0 {
        format!("{:.0}", value)
    } else {
        format!("{:.1}", value)
    }
}

/// Phase D2: one statistics row — muted label, value with growth vs the
/// previous period, e.g. "12.4K (+5.6%)".
pub(super) fn stats_value_row(label: &str, value: &StatisticalValue) -> Div {
    div()
        .flex()
        .flex_row()
        .justify_between()
        .gap_2()
        .child(
            div()
                .text_sm()
                .text_color(text_muted())
                .child(label.to_string()),
        )
        .child(div().text_sm().child(format!(
            "{} ({:+.1}%)",
            format_stat_count(value.value),
            value.growth_rate_percentage
        )))
}

/// Phase D2: "Sep 19" from a unix timestamp (TDLib `dateRange`), UTC.
pub(super) fn format_statistics_day(unix: i32) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    // Howard Hinnant's civil_from_days, epoch-shifted to 1970-01-01.
    let days = (unix as i64).div_euclid(86400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{} {d}", MONTHS[(m - 1) as usize])
}

/// Phase D2: trend sparkline from a `statisticalGraphData` `json_data`
/// payload. TDLib graph JSON:
/// `{"columns":[["x",t1,...],["y0",v1,...],...],"types":{"x":"x","y0":"line"}}`.
/// Takes the first non-"x" numeric column. `None` when there is no usable
/// series — the graph row is then omitted, never a placeholder.
pub(super) fn sparkline_from_graph_json(json_data: &str) -> Option<String> {
    const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let parsed: serde_json::Value = serde_json::from_str(json_data).ok()?;
    let columns = parsed.get("columns")?.as_array()?;
    let series = columns.iter().find_map(|column| {
        let column = column.as_array()?;
        let name = column.first()?.as_str()?;
        if name == "x" {
            return None;
        }
        let values: Vec<f64> = column[1..].iter().filter_map(|v| v.as_f64()).collect();
        (!values.is_empty()).then_some(values)
    })?;
    let min = series.iter().copied().fold(f64::INFINITY, f64::min);
    let max = series.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = (max - min).max(f64::EPSILON);
    Some(
        series
            .iter()
            .map(|value| {
                let level = ((value - min) / span * 7.0).round() as usize;
                BLOCKS[level.min(7)]
            })
            .collect(),
    )
}

/// Phase D2: one labeled graph row. `Data` with a usable series renders a
/// sparkline; `Async` is still processing server-side; `Error` shows the
/// server text; a `Data` payload without a usable series is omitted
/// (`None`).
pub(super) fn stats_graph_row(
    label: &str,
    graph: &StatisticalGraph,
    cx: &mut Context<QuillApp>,
) -> Option<Div> {
    let status: Option<String> = match graph {
        StatisticalGraph::Data {
            json_data,
            zoom_token: _,
        } => {
            return sparkline_from_graph_json(json_data).map(|spark| {
                div()
                    .flex()
                    .flex_row()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(text_muted())
                            .child(label.to_string()),
                    )
                    .child(div().text_xs().child(spark))
            });
        }
        StatisticalGraph::Async { token: _ } => {
            Some("Still processing — check back later.".to_string())
        }
        StatisticalGraph::Error { error_message } => Some(format!("Graph error: {error_message}")),
    };
    status.map(|text| {
        div()
            .flex()
            .flex_row()
            .justify_between()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .text_color(text_muted())
                    .child(label.to_string()),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(text),
            )
    })
}

impl QuillApp {
    /// Phase D2: the channel/group statistics view (`getChatStatistics`,
    /// schema 1.8.67 line 15760), opened from the info panel when
    /// `supergroupFullInfo.can_get_statistics` is true. Honest about
    /// absent data: loading / error / still-processing states render as
    /// text, and graphs without a usable series are omitted — no
    /// placeholder numbers anywhere.
    pub(super) fn chat_statistics_panel(&self, chat_id: i64, cx: &mut Context<Self>) -> AnyElement {
        let session = self.session();
        let (title, is_channel) = session
            .and_then(|s| s.chats.get(&chat_id))
            .map(|chat| {
                let is_channel = matches!(
                    chat.kind,
                    ChatKind::Supergroup {
                        is_channel: true,
                        ..
                    }
                );
                (chat.title.clone(), is_channel)
            })
            .unwrap_or_else(|| (format!("Chat {chat_id}"), false));
        let mut body = div()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .child(div().text_lg().font_semibold().child(title))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(if is_channel {
                        "Channel statistics"
                    } else {
                        "Group statistics"
                    }),
            );
        match session.and_then(|s| s.chat_statistics.get(&chat_id).cloned()) {
            None | Some(ChatStatisticsFetch::Loading) => {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Loading statistics…"),
                );
            }
            Some(ChatStatisticsFetch::Failed(message)) => {
                let retry_chat = chat_id;
                body = body.child(div().text_sm().child(message)).child(
                    Button::new("statistics-retry")
                        .label("Retry")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.refresh_statistics(retry_chat, cx);
                        })),
                );
            }
            Some(ChatStatisticsFetch::Loaded(statistics)) => {
                body = self.statistics_body(chat_id, &statistics, is_channel, cx);
            }
        }
        body.into_any_element()
    }

    /// Phase D2: the loaded-statistics body, shared by the channel and
    /// supergroup variants.
    pub(super) fn statistics_body(
        &self,
        chat_id: i64,
        statistics: &ChatStatistics,
        is_channel: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let session = self.session();
        let display_name = |user_id: i64| -> String {
            session
                .and_then(|s| s.user(user_id))
                .map(|u| u.display_name())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| format!("User {user_id}"))
        };
        let noun = if is_channel { "Subscribers" } else { "Members" };
        let mut body = div();
        let (period_start, period_end, values, graphs, footer) = match statistics {
            ChatStatistics::Channel(stats) => {
                let values = vec![
                    (noun, stats.member_count),
                    ("Mean views", stats.mean_message_view_count),
                    ("Mean shares", stats.mean_message_share_count),
                    ("Mean reactions", stats.mean_message_reaction_count),
                    ("Mean story views", stats.mean_story_view_count),
                    ("Mean story shares", stats.mean_story_share_count),
                    ("Mean story reactions", stats.mean_story_reaction_count),
                ];
                let graphs: Vec<(&str, &StatisticalGraph)> = vec![
                    ("Members", &stats.member_count_graph),
                    ("Joins", &stats.join_graph),
                    ("Views by hour", &stats.view_count_by_hour_graph),
                    ("Message interactions", &stats.message_interaction_graph),
                ];
                let mut footer: Vec<String> = stats
                    .recent_interactions
                    .iter()
                    .map(|info| {
                        let kind = match info.object {
                            quill::telegram::envelope::ChatStatisticsObject::Message {
                                message_id,
                            } => format!("Post {message_id}"),
                            quill::telegram::envelope::ChatStatisticsObject::Story { story_id } => {
                                format!("Story {story_id}")
                            }
                        };
                        format!(
                            "{kind} — {} views · {} forwards · {} reactions",
                            format_stat_count(info.view_count as f64),
                            format_stat_count(info.forward_count as f64),
                            format_stat_count(info.reaction_count as f64),
                        )
                    })
                    .collect();
                if footer.is_empty() {
                    footer.push("No recent interactions yet.".to_string());
                }
                (
                    stats.period_start,
                    stats.period_end,
                    values,
                    graphs,
                    ("Recent interactions".to_string(), footer),
                )
            }
            ChatStatistics::Supergroup(stats) => {
                let values = vec![
                    (noun, stats.member_count),
                    ("Messages", stats.message_count),
                    ("Viewers", stats.viewer_count),
                    ("Senders", stats.sender_count),
                ];
                let graphs: Vec<(&str, &StatisticalGraph)> = vec![
                    ("Members", &stats.member_count_graph),
                    ("Joins", &stats.join_graph),
                    ("Messages by day", &stats.day_graph),
                    ("Messages by week", &stats.week_graph),
                ];
                let mut footer: Vec<String> = Vec::new();
                for sender in &stats.top_senders {
                    footer.push(format!(
                        "{} — {} messages",
                        display_name(sender.user_id),
                        format_stat_count(sender.sent_message_count as f64),
                    ));
                }
                for admin in &stats.top_administrators {
                    footer.push(format!(
                        "{} — {} deleted · {} banned · {} restricted",
                        display_name(admin.user_id),
                        admin.deleted_message_count,
                        admin.banned_user_count,
                        admin.restricted_user_count,
                    ));
                }
                for inviter in &stats.top_inviters {
                    footer.push(format!(
                        "{} — {} added",
                        display_name(inviter.user_id),
                        inviter.added_member_count,
                    ));
                }
                if footer.is_empty() {
                    footer.push("No top contributors yet.".to_string());
                }
                (
                    stats.period_start,
                    stats.period_end,
                    values,
                    graphs,
                    (
                        "Top senders · administrators · inviters".to_string(),
                        footer,
                    ),
                )
            }
        };
        body = body.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(format!(
                    "{} – {}",
                    format_statistics_day(period_start),
                    format_statistics_day(period_end)
                )),
        );
        for (label, value) in values {
            body = body.child(stats_value_row(label, &value));
        }
        body = body.child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(cx.theme().muted_foreground)
                .child("Notifications enabled")
                .child(div().text_sm().child(match statistics {
                    ChatStatistics::Channel(stats) => {
                        format!("{:.1}%", stats.enabled_notifications_percentage)
                    }
                    ChatStatistics::Supergroup(_) => "—".to_string(),
                })),
        );
        for (label, graph) in graphs {
            if let Some(row) = stats_graph_row(label, graph, cx) {
                body = body.child(row);
            }
        }
        let (footer_title, footer_lines) = footer;
        let mut footer_div = div().flex().flex_col().gap_1().child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(cx.theme().muted_foreground)
                .child(footer_title),
        );
        for line in footer_lines {
            footer_div = footer_div.child(div().text_sm().child(line));
        }
        body = body.child(footer_div);
        let refresh_chat = chat_id;
        body.child(
            Button::new("statistics-refresh")
                .label("Refresh")
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.refresh_statistics(refresh_chat, cx);
                })),
        )
    }

    /// Phase D2: explicit statistics refresh — clears the cached result
    /// and re-sends `getChatStatistics`.
    pub(super) fn refresh_statistics(&mut self, chat_id: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if let Err(err) = live.driver.refresh_chat_statistics(ChatId(chat_id), false) {
                self.status_note = format!("statistics refresh failed: {err:?}");
            }
        }
        cx.notify();
    }
}
