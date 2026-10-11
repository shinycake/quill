//! Statistics fixtures for UI tests: a channel's overview with one graph
//! of every kind (in Telegram's chart JSON), an async graph that has
//! loaded, a failed one, recent posts and a loaded zoomed day.

use quill::state::{
    ChatStatisticsFetch, GraphFetch, GraphKey, MessageStatsFetch, Session, StatsPostPreview,
};
use quill::telegram::envelope::{
    ChannelStatistics, ChatStatistics, ChatStatisticsInteractionInfo, ChatStatisticsObject,
    MessageStatisticsView, PublicForwardKind, PublicForwardView, StatisticalGraph,
    StatisticalValue,
};
use serde_json::json;

/// 2024-10-01 00:00 UTC.
const START_MS: i64 = 1_727_740_800_000;
const DAY_MS: i64 = 86_400_000;
pub(crate) const ZOOM_TOKEN: &str = "zoom-growth";
pub(crate) const ASYNC_TOKEN: &str = "async-mute";

fn wave(i: usize, base: f64, amp: f64, period: f64) -> i64 {
    (base + amp * ((i as f64) / period * std::f64::consts::TAU).sin()).round() as i64
}

/// Chart JSON with `days` daily points (or hourly when `step_ms` is an
/// hour) and the given series.
pub(crate) fn chart_json(
    points: usize,
    step_ms: i64,
    series: &[(&str, &str, &str, Vec<i64>)],
    extra: serde_json::Value,
) -> String {
    let mut x = vec![json!("x")];
    x.extend((0..points).map(|i| json!(START_MS + i as i64 * step_ms)));
    let mut columns = vec![serde_json::Value::Array(x)];
    let mut names = serde_json::Map::new();
    let mut colors = serde_json::Map::new();
    for (id, name, color, values) in series {
        let mut column = vec![json!(id)];
        column.extend(values.iter().map(|v| json!(v)));
        columns.push(serde_json::Value::Array(column));
        names.insert((*id).into(), json!(name));
        colors.insert((*id).into(), json!(color));
    }
    let mut root = json!({"columns": columns, "names": names, "colors": colors, "types": {}});
    if let Some(extra) = extra.as_object() {
        for (key, value) in extra {
            root[key] = value.clone();
        }
    }
    root.to_string()
}

fn data(json: String, zoom: &str) -> StatisticalGraph {
    StatisticalGraph::Data {
        json_data: json,
        zoom_token: zoom.to_string(),
    }
}

fn value(value: f64, previous: f64) -> StatisticalValue {
    StatisticalValue {
        value,
        previous_value: previous,
        growth_rate_percentage: (value - previous) / previous * 100.0,
    }
}

fn days(n: usize, f: impl Fn(usize) -> i64) -> Vec<i64> {
    (0..n).map(f).collect()
}

/// One day of the growth chart, hour by hour.
pub(crate) fn zoomed_day_json() -> String {
    chart_json(
        24,
        3_600_000,
        &[(
            "y0",
            "Followers",
            "BLUE#3497ED",
            days(24, |i| 12_000 + i as i64 * 4),
        )],
        json!({}),
    )
}

/// A channel with statistics loaded for `chat_id`: every chart kind, an
/// async chart already loaded, recent posts with previews.
pub(crate) fn seed_channel_statistics(session: &mut Session, chat_id: i64) {
    let n = 60;
    let interactions = chart_json(
        n,
        DAY_MS,
        &[
            (
                "y0",
                "Views",
                "BLUE#3497ED",
                days(n, |i| wave(i, 8_000., 2_500., 9.)),
            ),
            (
                "y1",
                "Shares",
                "GOLDEN#F5BD25",
                days(n, |i| wave(i, 40., 18., 13.)),
            ),
        ],
        json!({"y_scaled": true}),
    );
    let stats = ChannelStatistics {
        period_start: (START_MS / 1000) as i32,
        period_end: ((START_MS + 59 * DAY_MS) / 1000) as i32,
        member_count: value(12_345., 11_700.),
        mean_message_view_count: value(8_421., 9_010.),
        mean_message_share_count: value(312., 280.),
        mean_message_reaction_count: value(428., 390.),
        mean_story_view_count: value(5_120., 4_980.),
        mean_story_share_count: value(96., 104.),
        mean_story_reaction_count: value(154., 140.),
        enabled_notifications_percentage: 61.5,
        member_count_graph: data(
            chart_json(
                n,
                DAY_MS,
                &[(
                    "y0",
                    "Followers",
                    "BLUE#3497ED",
                    days(n, |i| 11_000 + i as i64 * 22 + wave(i, 0., 60., 7.)),
                )],
                json!({"subchart": {"show": true, "defaultZoom": [START_MS + 30 * DAY_MS, START_MS + 59 * DAY_MS]}}),
            ),
            ZOOM_TOKEN,
        ),
        join_graph: data(
            chart_json(
                n,
                DAY_MS,
                &[
                    (
                        "y0",
                        "Joined",
                        "GREEN#61c752",
                        days(n, |i| wave(i, 90., 40., 11.)),
                    ),
                    (
                        "y1",
                        "Left",
                        "RED#e05356",
                        days(n, |i| wave(i, 30., 12., 6.)),
                    ),
                ],
                json!({}),
            ),
            "",
        ),
        mute_graph: StatisticalGraph::Async {
            token: ASYNC_TOKEN.to_string(),
        },
        view_count_by_hour_graph: data(
            chart_json(
                24,
                3_600_000,
                &[
                    (
                        "y0",
                        "Views",
                        "BLUE#3497ED",
                        days(24, |i| wave(i, 900., 500., 24.)),
                    ),
                    (
                        "y1",
                        "Last week",
                        "LIGHTBLUE#58a8ed",
                        days(24, |i| wave(i, 800., 420., 24.)),
                    ),
                ],
                json!({}),
            ),
            "",
        ),
        view_count_by_source_graph: data(
            chart_json(
                n,
                DAY_MS,
                &[
                    (
                        "y0",
                        "Followers",
                        "BLUE#3497ED",
                        days(n, |i| wave(i, 5_000., 900., 7.)),
                    ),
                    (
                        "y1",
                        "Search",
                        "GREEN#61c752",
                        days(n, |i| wave(i, 1_200., 300., 9.)),
                    ),
                    (
                        "y2",
                        "Other channels",
                        "ORANGE#f28c39",
                        days(n, |i| wave(i, 800., 200., 5.)),
                    ),
                    (
                        "y3",
                        "Other",
                        "PURPLE#9f79e8",
                        days(n, |i| wave(i, 300., 90., 4.)),
                    ),
                ],
                json!({"stacked": true}),
            ),
            "",
        ),
        join_by_source_graph: StatisticalGraph::Error {
            error_message: "STATS_GRAPH_NOT_AVAILABLE".into(),
        },
        language_graph: data(
            chart_json(
                n,
                DAY_MS,
                &[
                    (
                        "y0",
                        "English",
                        "BLUE#3497ED",
                        days(n, |i| wave(i, 6_000., 400., 10.)),
                    ),
                    (
                        "y1",
                        "Spanish",
                        "GOLDEN#eba52d",
                        days(n, |i| wave(i, 2_000., 300., 8.)),
                    ),
                    (
                        "y2",
                        "German",
                        "GREEN#61c752",
                        days(n, |i| wave(i, 1_200., 200., 12.)),
                    ),
                    (
                        "y3",
                        "Other",
                        "CYAN#40d0ca",
                        days(n, |i| wave(i, 800., 150., 6.)),
                    ),
                ],
                json!({"percentage": true, "stacked": true}),
            ),
            "",
        ),
        message_interaction_graph: data(interactions.clone(), ""),
        message_reaction_graph: data(
            chart_json(
                n,
                DAY_MS,
                &[
                    (
                        "y0",
                        "\u{1F44D}",
                        "GREEN#61c752",
                        days(n, |i| wave(i, 120., 60., 8.)),
                    ),
                    (
                        "y1",
                        "\u{2764}",
                        "RED#e05356",
                        days(n, |i| wave(i, 80., 40., 6.)),
                    ),
                ],
                json!({}),
            ),
            "",
        ),
        story_interaction_graph: data(interactions, ""),
        story_reaction_graph: StatisticalGraph::Error {
            error_message: "STATS_GRAPH_NOT_AVAILABLE".into(),
        },
        instant_view_interaction_graph: StatisticalGraph::Error {
            error_message: "STATS_GRAPH_NOT_AVAILABLE".into(),
        },
        recent_interactions: (0..14)
            .map(|i| ChatStatisticsInteractionInfo {
                object: ChatStatisticsObject::Message {
                    message_id: (100 + i) << 20,
                },
                view_count: 12_000 - i as i32 * 500,
                forward_count: 20 - i as i32,
                reaction_count: 200 - i as i32 * 9,
            })
            .collect(),
    };
    session.groups.chat_statistics.insert(
        chat_id,
        ChatStatisticsFetch::Loaded(Box::new(ChatStatistics::Channel(Box::new(stats)))),
    );
    session.groups.stats.graphs.insert(
        GraphKey {
            chat_id,
            token: ASYNC_TOKEN.into(),
            x: 0,
        },
        GraphFetch::Loaded(data(
            chart_json(
                n,
                DAY_MS,
                &[
                    (
                        "y0",
                        "Muted",
                        "RED#e05356",
                        days(n, |i| wave(i, 14., 6., 5.)),
                    ),
                    (
                        "y1",
                        "Unmuted",
                        "GREEN#61c752",
                        days(n, |i| wave(i, 9., 4., 7.)),
                    ),
                ],
                json!({}),
            ),
            "",
        )),
    );
    for i in 0..14i64 {
        session.groups.stats.posts.insert(
            (chat_id, (100 + i) << 20),
            Some(StatsPostPreview {
                text: format!("Release notes, part {}", i + 1),
                date: ((START_MS + (59 - i) * DAY_MS) / 1000) as i32,
                views: 12_000 - i as i32 * 500,
                forwards: 20 - i as i32,
            }),
        );
    }
}

/// The growth chart zoomed into its last day, loaded.
pub(crate) fn seed_zoomed_day(session: &mut Session, chat_id: i64) -> i64 {
    let x = START_MS + 59 * DAY_MS;
    session
        .groups
        .stats
        .zoomed
        .insert(ZOOM_TOKEN.to_string(), x);
    session.groups.stats.graphs.insert(
        GraphKey {
            chat_id,
            token: ZOOM_TOKEN.into(),
            x,
        },
        GraphFetch::Loaded(data(zoomed_day_json(), "")),
    );
    x
}

/// A post's statistics opened and loaded, with two public shares.
pub(crate) fn seed_message_statistics(session: &mut Session, chat_id: i64, from_chat: i64) -> i64 {
    let message_id = 100 << 20;
    session.open_message_stats_page(chat_id, message_id);
    let state = session.groups.stats.message.as_mut().expect("opened");
    let interactions = chart_json(
        30,
        DAY_MS,
        &[
            (
                "y0",
                "Views",
                "BLUE#3497ED",
                days(30, |i| 3_000 / (i as i64 + 1)),
            ),
            (
                "y1",
                "Shares",
                "GOLDEN#F5BD25",
                days(30, |i| 30 / (i as i64 + 1)),
            ),
        ],
        json!({}),
    );
    state.statistics = MessageStatsFetch::Loaded(MessageStatisticsView {
        interaction_graph: data(interactions, ""),
        reaction_graph: StatisticalGraph::Async {
            token: "message-reactions".into(),
        },
    });
    state.forwards.total = 2;
    state.forwards.forwards = vec![
        PublicForwardView {
            kind: PublicForwardKind::Message {
                chat_id: from_chat,
                message_id: 5 << 20,
            },
            date: 1,
            view_count: 640,
        },
        PublicForwardView {
            kind: PublicForwardKind::Message {
                chat_id: from_chat,
                message_id: 6 << 20,
            },
            date: 2,
            view_count: 0,
        },
    ];
    message_id
}
