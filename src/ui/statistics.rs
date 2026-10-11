//! The statistics panel of a channel or group: overview numbers with
//! their change, every chart TDLib returns, recent posts (channels) or
//! top members (groups); a post or story opens its own page
//! (`statistics_posts.rs`). tdesktop: `info/statistics/`.

use super::app::QuillApp;
use super::chat_row::chat_avatar;
use super::chat_theme::text_muted;
use super::statistics_graph::{OverviewCell, overview_grid, stat_graph_block};
use gpui_kit::component::button::*;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::local_path::sandboxed_display_path;
use quill::stat_chart::{ChartKind, day_month_year, percent_label};
use quill::state::{ChatStatisticsFetch, InfoPanelTarget, StatsPage};
use quill::telegram::envelope::{
    ChannelStatistics, ChatKind, ChatStatistics, StatisticalGraph, SupergroupStatistics,
};

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

/// A graph outside the statistics panel, drawn as a line chart (poll
/// votes; tdesktop draws those as `Linear`).
pub(super) fn stats_graph_row(
    label: &str,
    graph: &StatisticalGraph,
    _cx: &mut Context<QuillApp>,
) -> Option<AnyElement> {
    let id = format!("graph-{}", label.to_lowercase().replace(' ', "-"));
    stat_graph_block(&id, label, graph, ChartKind::Linear)
}

/// `1 message` / `2 messages`.
pub(super) fn plural(count: i32, one: &str, other: &str) -> String {
    let count_text = quill::stat_chart::format_decimal(i64::from(count));
    if count == 1 {
        format!("{count_text} {one}")
    } else {
        format!("{count_text} {other}")
    }
}

/// tdesktop `FormatText`: the non-zero parts joined with commas.
fn join_counts(parts: &[(i32, &str, &str)]) -> String {
    parts
        .iter()
        .filter(|(count, _, _)| *count > 0)
        .map(|(count, one, other)| plural(*count, one, other))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn section_title(text: impl Into<SharedString>) -> Div {
    div().text_sm().font_semibold().child(text.into())
}

pub(super) fn divider(cx: &App) -> Div {
    div().h(px(1.)).w_full().bg(cx.theme().border)
}

impl QuillApp {
    /// The statistics panel of `chat_id`: the overview, or the page of a
    /// post or story opened from it.
    pub(super) fn chat_statistics_panel(&self, chat_id: i64, cx: &mut Context<Self>) -> AnyElement {
        let page = self
            .session()
            .map(|s| s.groups.stats.page(chat_id))
            .unwrap_or_default();
        let body = match page {
            StatsPage::Overview => self.statistics_overview(chat_id, cx),
            StatsPage::Message(message_id) => self.message_stats_page(chat_id, message_id, cx),
            StatsPage::Story(story_id) => self.story_stats_page(chat_id, story_id, cx),
        };
        div()
            .id("statistics-panel")
            .flex()
            .flex_col()
            .gap_4()
            .p_3()
            .child(body)
            .into_any_element()
    }

    fn statistics_overview(&self, chat_id: i64, cx: &mut Context<Self>) -> AnyElement {
        let session = self.session();
        let is_channel = session
            .and_then(|s| s.chats.get(&chat_id))
            .is_some_and(|chat| {
                matches!(
                    chat.kind,
                    ChatKind::Supergroup {
                        is_channel: true,
                        ..
                    }
                )
            });
        match session.and_then(|s| s.groups.chat_statistics.get(&chat_id).cloned()) {
            None | Some(ChatStatisticsFetch::Loading) => div()
                .flex()
                .flex_col()
                .items_center()
                .gap_2()
                .py_6()
                .child(Spinner::new())
                .child(div().text_sm().font_semibold().child("Loading statistics…"))
                .child(
                    div()
                        .text_xs()
                        .text_color(text_muted())
                        .text_center()
                        .child("It can take a few moments to put your statistics together."),
                )
                .into_any_element(),
            Some(ChatStatisticsFetch::Failed(message)) => div()
                .flex()
                .flex_col()
                .gap_2()
                .child(div().text_sm().child(message))
                .child(
                    Button::new("statistics-retry")
                        .label("Retry")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.refresh_statistics(chat_id, cx);
                        })),
                )
                .into_any_element(),
            Some(ChatStatisticsFetch::Loaded(statistics)) => self
                .statistics_body(chat_id, &statistics, is_channel, cx)
                .into_any_element(),
        }
    }

    /// The loaded statistics, channel or group.
    fn statistics_body(
        &self,
        chat_id: i64,
        statistics: &ChatStatistics,
        is_channel: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let (start, end) = match statistics {
            ChatStatistics::Channel(s) => (s.period_start, s.period_end),
            ChatStatistics::Supergroup(s) => (s.period_start, s.period_end),
        };
        let mut body = div().flex().flex_col().gap_4().child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(section_title("Overview"))
                        .when(start > 0 && end > 0, |d| {
                            d.child(div().text_xs().text_color(text_muted()).child(format!(
                                "{} \u{2014} {}",
                                day_month_year(i64::from(start)),
                                day_month_year(i64::from(end))
                            )))
                        }),
                )
                .child(overview_grid(overview_cells(statistics, is_channel))),
        );
        if let ChatStatistics::Channel(stats) = statistics {
            body = body.child(overview_grid(channel_story_cells(stats)));
        }
        for (id, title, graph, kind) in chart_slots(statistics) {
            if let Some(section) = self.stat_graph_section(
                chat_id,
                &format!("stats-{chat_id}-{id}"),
                title,
                graph,
                kind,
                cx,
            ) {
                body = body.child(divider(cx)).child(section);
            }
        }
        match statistics {
            ChatStatistics::Channel(stats) => {
                if let Some(section) = self.recent_posts_section(chat_id, stats, cx) {
                    body = body.child(divider(cx)).child(section);
                }
            }
            ChatStatistics::Supergroup(stats) => {
                for section in self.top_member_sections(stats, cx) {
                    body = body.child(divider(cx)).child(section);
                }
            }
        }
        body.child(
            Button::new("statistics-refresh")
                .label("Refresh")
                .ghost()
                .small()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.refresh_statistics(chat_id, cx);
                })),
        )
    }

    /// "Top members", "Top admins", "Top inviters" of a group; a row
    /// opens the member's profile.
    fn top_member_sections(
        &self,
        stats: &SupergroupStatistics,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let senders: Vec<(i64, String)> = stats
            .top_senders
            .iter()
            .map(|s| {
                (
                    s.user_id,
                    join_counts(&[
                        (s.sent_message_count, "message", "messages"),
                        (
                            s.average_character_count,
                            "symbol per message",
                            "symbols per message",
                        ),
                    ]),
                )
            })
            .collect();
        let admins: Vec<(i64, String)> = stats
            .top_administrators
            .iter()
            .map(|a| {
                (
                    a.user_id,
                    join_counts(&[
                        (a.deleted_message_count, "deletion", "deletions"),
                        (a.banned_user_count, "ban", "bans"),
                        (a.restricted_user_count, "restriction", "restrictions"),
                    ]),
                )
            })
            .collect();
        let inviters: Vec<(i64, String)> = stats
            .top_inviters
            .iter()
            .map(|i| {
                (
                    i.user_id,
                    plural(i.added_member_count, "invitation", "invitations"),
                )
            })
            .collect();
        [
            ("top-members", "Top members", senders),
            ("top-admins", "Top admins", admins),
            ("top-inviters", "Top inviters", inviters),
        ]
        .into_iter()
        .filter(|(_, _, rows)| !rows.is_empty())
        .map(|(id, title, rows)| self.member_list(id, title, rows, cx))
        .collect()
    }

    fn member_list(
        &self,
        id: &'static str,
        title: &'static str,
        rows: Vec<(i64, String)>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let session = self.session();
        let roots = self.media_display_roots();
        let mut list = div().flex().flex_col().gap_1().child(section_title(title));
        for (index, (user_id, status)) in rows.into_iter().enumerate() {
            let name = session
                .and_then(|s| s.user(user_id))
                .map(|u| u.display_name())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| format!("User {user_id}"));
            let photo = session
                .and_then(|s| s.user_photo_path(user_id))
                .and_then(|path| sandboxed_display_path(path, &roots));
            list = list.child(
                stats_row(SharedString::from(format!("{id}-{index}")), cx)
                    .aria_label(SharedString::from(name.clone()))
                    .child(chat_avatar(&name, photo.as_deref(), 32.))
                    .child(row_text(name, status))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_info_panel_target(InfoPanelTarget::User(user_id), window, cx);
                    })),
            );
        }
        list.into_any_element()
    }

    /// Phase D2: explicit statistics refresh — clears the cached result
    /// and re-sends `getChatStatistics`.
    pub(super) fn refresh_statistics(&mut self, chat_id: i64, cx: &mut Context<Self>) {
        let is_dark = super::chat_theme::is_dark_palette();
        if let Some(live) = self.live.as_mut()
            && let Err(err) = live
                .driver
                .refresh_chat_statistics(ChatId(chat_id), is_dark)
        {
            self.connection.status_note = format!("statistics refresh failed: {err:?}");
        }
        cx.notify();
    }
}

/// A clickable row of a statistics list: avatar or preview, then text.
pub(super) fn stats_row(id: SharedString, cx: &App) -> Stateful<Div> {
    let selector = id.to_string();
    div()
        .id(ElementId::Name(id))
        .debug_selector(|| selector)
        .role(gpui_kit::Role::Button)
        .tab_index(0)
        .flex()
        .items_center()
        .gap_2()
        .px_1()
        .py_1()
        .rounded_md()
        .hover(|style| style.bg(cx.theme().secondary_hover))
}

/// Name over a muted status line, truncating.
pub(super) fn row_text(title: impl Into<SharedString>, status: impl Into<SharedString>) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .child(
            div()
                .text_sm()
                .truncate()
                .child(super::bidi_line::one_line_plain(title.into().to_string())),
        )
        .child(
            div()
                .text_xs()
                .text_color(text_muted())
                .truncate()
                .child(status.into()),
        )
}

/// The first overview: followers, notifications, views per post and
/// per story for a channel; members, messages, viewing and posting
/// members for a group.
fn overview_cells(statistics: &ChatStatistics, is_channel: bool) -> Vec<OverviewCell> {
    match statistics {
        ChatStatistics::Channel(s) => vec![
            OverviewCell::stat(
                &s.member_count,
                if is_channel { "Followers" } else { "Members" },
            ),
            OverviewCell::plain(
                percent_label(s.enabled_notifications_percentage),
                "Enabled notifications",
            ),
            OverviewCell::stat(&s.mean_message_view_count, "Views per post"),
            OverviewCell::stat(&s.mean_story_view_count, "Views per story"),
        ],
        ChatStatistics::Supergroup(s) => vec![
            OverviewCell::stat(&s.member_count, "Members"),
            OverviewCell::stat(&s.message_count, "Messages"),
            OverviewCell::stat(&s.viewer_count, "Viewing members"),
            OverviewCell::stat(&s.sender_count, "Posting members"),
        ],
    }
}

/// A channel's second overview: shares and reactions per post and per
/// story (reactions per post only when the channel has any).
fn channel_story_cells(s: &ChannelStatistics) -> Vec<OverviewCell> {
    let post_reactions = s.mean_message_reaction_count.value != 0.0
        || s.mean_message_reaction_count.previous_value != 0.0;
    let mut cells = vec![
        OverviewCell::stat(&s.mean_message_share_count, "Shares per post"),
        OverviewCell::stat(&s.mean_story_share_count, "Shares per story"),
    ];
    if post_reactions {
        cells.push(OverviewCell::stat(
            &s.mean_message_reaction_count,
            "Reactions per post",
        ));
    }
    cells.push(OverviewCell::stat(
        &s.mean_story_reaction_count,
        "Reactions per story",
    ));
    cells
}

/// Every chart of the statistics, with its title and how tdesktop draws
/// it (`FillStatistic`).
fn chart_slots(
    statistics: &ChatStatistics,
) -> Vec<(&'static str, &'static str, &StatisticalGraph, ChartKind)> {
    use ChartKind::*;
    match statistics {
        ChatStatistics::Channel(s) => vec![
            ("growth", "Growth", &s.member_count_graph, Linear),
            ("followers", "Followers", &s.join_graph, Linear),
            ("mute", "Notifications", &s.mute_graph, Linear),
            (
                "hours",
                "Views by hours (UTC)",
                &s.view_count_by_hour_graph,
                Linear,
            ),
            (
                "view-sources",
                "Views by source",
                &s.view_count_by_source_graph,
                StackBar,
            ),
            (
                "join-sources",
                "New followers by source",
                &s.join_by_source_graph,
                StackBar,
            ),
            ("languages", "Languages", &s.language_graph, StackLinear),
            (
                "interactions",
                "Interactions",
                &s.message_interaction_graph,
                DoubleLinear,
            ),
            (
                "iv",
                "IV interactions",
                &s.instant_view_interaction_graph,
                DoubleLinear,
            ),
            ("reactions", "Reactions", &s.message_reaction_graph, Bar),
            (
                "story-interactions",
                "Story interactions",
                &s.story_interaction_graph,
                DoubleLinear,
            ),
            (
                "story-reactions",
                "Story reactions",
                &s.story_reaction_graph,
                Bar,
            ),
        ],
        ChatStatistics::Supergroup(s) => vec![
            ("growth", "Growth", &s.member_count_graph, Linear),
            ("members", "Group members", &s.join_graph, Linear),
            (
                "join-sources",
                "New members by source",
                &s.join_by_source_graph,
                StackBar,
            ),
            (
                "languages",
                "Members' primary language",
                &s.language_graph,
                StackLinear,
            ),
            ("messages", "Messages", &s.message_content_graph, StackBar),
            ("actions", "Actions", &s.action_graph, DoubleLinear),
            ("hours", "Views by hours", &s.day_graph, Linear),
            ("weekdays", "Top days of week", &s.week_graph, StackLinear),
        ],
    }
}

#[cfg(all(test, feature = "demo-capture"))]
#[path = "statistics_fixture.rs"]
pub(crate) mod fixture;

#[cfg(all(test, feature = "demo-capture"))]
#[path = "statistics_tests.rs"]
mod ui_tests;

#[cfg(test)]
mod tests {
    use super::{join_counts, plural};

    #[test]
    fn member_rows_join_the_non_zero_counts() {
        assert_eq!(plural(1, "ban", "bans"), "1 ban");
        assert_eq!(plural(1200, "ban", "bans"), "1,200 bans");
        assert_eq!(
            join_counts(&[
                (3, "deletion", "deletions"),
                (0, "ban", "bans"),
                (1, "restriction", "restrictions")
            ]),
            "3 deletions, 1 restriction"
        );
        assert_eq!(join_counts(&[(0, "ban", "bans")]), "");
    }
}
