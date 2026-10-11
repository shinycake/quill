//! channel statistics panel.

use super::app::QuillApp;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::state::ChatStatisticsFetch;
use quill::telegram::envelope::{ChatKind, ChatStatistics, StatisticalGraph, StatisticalValue};

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
        match session.and_then(|s| s.groups.chat_statistics.get(&chat_id).cloned()) {
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
        if let Some(live) = self.live.as_mut()
            && let Err(err) = live.driver.refresh_chat_statistics(ChatId(chat_id), false)
        {
            self.connection.status_note = format!("statistics refresh failed: {err:?}");
        }
        cx.notify();
    }
}
