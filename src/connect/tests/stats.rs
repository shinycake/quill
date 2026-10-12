//! Connect-driver tests: async and zoomed graphs, message statistics with
//! public forwards, recent-post previews and the statistics page.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{AccountKey, ChatId, MessageId, RequestId};
use crate::platform::MemorySecretStore;
use crate::state::{
    GraphFetch, InfoPanelTarget, MessageStatsFetch, RECENT_POSTS_FIRST, RequestPurpose, Session,
    StatsPage,
};
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::StatisticalGraph;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

struct Rig {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    seq: AtomicU64,
    sink: Arc<dyn DiagnosticSink>,
    dir: std::path::PathBuf,
}

impl Rig {
    fn new() -> Self {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        seed_ready_alice(&mut driver, &seq, &sink);
        Self {
            driver,
            recorder,
            seq,
            sink,
            dir,
        }
    }

    fn sent(&self) -> Vec<Value> {
        self.recorder
            .snapshot()
            .into_iter()
            .map(|json| serde_json::from_str::<Value>(&json).unwrap())
            .collect()
    }

    fn last(&self, kind: &str) -> Value {
        self.sent()
            .into_iter()
            .rev()
            .find(|v| v["@type"] == kind)
            .unwrap_or_else(|| panic!("no {kind} sent"))
    }

    fn extra_of(&self, kind: &str) -> RequestId {
        RequestId(self.last(kind)["@extra"].as_str().unwrap().parse().unwrap())
    }

    fn answer(&mut self, extra: RequestId, json: &str) {
        let json = json.replacen('{', &format!(r#"{{"@extra":"{}","#, extra.0), 1);
        self.driver
            .ingest(copy_and_parse(&json, &self.seq, &self.sink).unwrap())
            .unwrap();
    }

    fn error(&mut self, extra: RequestId) {
        self.answer(
            extra,
            r#"{"@type":"error","code":400,"message":"STATS_UNAVAILABLE"}"#,
        );
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

const GRAPH: &str =
    r#"{"@type":"statisticalGraphData","json_data":"{\"columns\":[]}","zoom_token":"z"}"#;

#[test]
fn async_graph_loads_once_and_lands_by_request() {
    let mut rig = Rig::new();
    let extra = rig
        .driver
        .load_stat_graph(ChatId(-100), "tok", 0)
        .unwrap()
        .expect("sends");
    let sent = rig.last("getStatisticalGraph");
    assert_eq!(sent["chat_id"], -100);
    assert_eq!(sent["token"], "tok");
    assert_eq!(sent["x"], 0);
    let stats = &rig.driver.session.groups.stats;
    assert_eq!(stats.graph(-100, "tok", 0), Some(&GraphFetch::Loading));
    // In flight: nothing new goes out.
    assert!(
        rig.driver
            .load_stat_graph(ChatId(-100), "tok", 0)
            .unwrap()
            .is_none()
    );
    rig.answer(extra, GRAPH);
    assert!(matches!(
        rig.driver.session.groups.stats.graph(-100, "tok", 0),
        Some(GraphFetch::Loaded(StatisticalGraph::Data { zoom_token, .. })) if zoom_token == "z"
    ));
    // Loaded: kept.
    assert!(
        rig.driver
            .load_stat_graph(ChatId(-100), "tok", 0)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        rig.driver.load_stat_graph(ChatId(-100), "", 0),
        Err(ConnectSendError::InvalidRequest)
    );
}

#[test]
fn zoom_loads_the_day_and_a_failure_can_be_retried() {
    let mut rig = Rig::new();
    let x = 1_727_740_800_000;
    let extra = rig
        .driver
        .zoom_stat_graph(ChatId(-100), "zoom", x)
        .unwrap()
        .expect("sends");
    assert_eq!(rig.last("getStatisticalGraph")["x"], x);
    assert_eq!(rig.driver.session.groups.stats.zoomed.get("zoom"), Some(&x));
    rig.error(extra);
    assert!(matches!(
        rig.driver.session.groups.stats.graph(-100, "zoom", x),
        Some(GraphFetch::Failed(_))
    ));
    // Asking again after a failure sends again.
    let again = rig.driver.zoom_stat_graph(ChatId(-100), "zoom", x).unwrap();
    assert!(again.is_some());
    rig.driver.unzoom_stat_graph("zoom");
    assert!(rig.driver.session.groups.stats.zoomed.is_empty());
}

fn forwards_page(next: &str, ids: &[i64]) -> String {
    let forwards: Vec<String> = ids
        .iter()
        .map(|id| {
            format!(
                r#"{{"@type":"publicForwardMessage","message":{{"@type":"message","id":{id},"chat_id":-300,"date":5,"interaction_info":{{"@type":"messageInteractionInfo","view_count":40,"forward_count":0}}}}}}"#
            )
        })
        .collect();
    format!(
        r#"{{"@type":"publicForwards","total_count":3,"next_offset":"{next}","forwards":[{}]}}"#,
        forwards.join(",")
    )
}

#[test]
fn message_statistics_open_the_page_and_page_through_forwards() {
    let mut rig = Rig::new();
    let stats_extra = rig
        .driver
        .fetch_message_statistics(ChatId(-100), MessageId(42 << 20), true)
        .unwrap()
        .expect("sends");
    let request = rig.last("getMessageStatistics");
    assert_eq!(request["chat_id"], -100);
    assert_eq!(request["message_id"], 42i64 << 20);
    assert_eq!(request["is_dark"], true);
    let forwards = rig.last("getMessagePublicForwards");
    assert_eq!(forwards["offset"], "");
    let stats = &rig.driver.session.groups.stats;
    assert_eq!(stats.page(-100), StatsPage::Message(42 << 20));
    let state = stats.message_for(-100, 42 << 20).expect("tracked");
    assert_eq!(state.statistics, MessageStatsFetch::Loading);
    assert!(state.forwards.loading);

    rig.answer(
        stats_extra,
        &format!(
            r#"{{"@type":"messageStatistics","message_interaction_graph":{GRAPH},"message_reaction_graph":{{"@type":"statisticalGraphAsync","token":"later"}}}}"#
        ),
    );
    let forwards_extra = rig.extra_of("getMessagePublicForwards");
    rig.answer(forwards_extra, &forwards_page("p2", &[1, 2]));
    let state = rig
        .driver
        .session
        .groups
        .stats
        .message_for(-100, 42 << 20)
        .unwrap();
    assert!(matches!(state.statistics, MessageStatsFetch::Loaded(_)));
    assert_eq!(state.forwards.forwards.len(), 2);
    assert_eq!(state.forwards.forwards[0].view_count, 40);
    assert_eq!(state.forwards.total, 3);
    assert!(!state.forwards.loading);

    let more = rig
        .driver
        .load_more_message_forwards()
        .unwrap()
        .expect("next page");
    assert_eq!(rig.last("getMessagePublicForwards")["offset"], "p2");
    assert!(rig.driver.load_more_message_forwards().unwrap().is_none());
    rig.answer(more, &forwards_page("", &[3]));
    let state = rig.driver.session.groups.stats.message.as_ref().unwrap();
    assert_eq!(state.forwards.forwards.len(), 3);
    assert!(rig.driver.load_more_message_forwards().unwrap().is_none());

    // Opening the panel again starts on the overview.
    rig.driver
        .set_info_panel(Some(InfoPanelTarget::Statistics(-100)));
    assert_eq!(
        rig.driver.session.groups.stats.page(-100),
        StatsPage::Overview
    );
}

#[test]
fn a_failed_message_statistics_load_is_shown_and_retried() {
    let mut rig = Rig::new();
    let extra = rig
        .driver
        .fetch_message_statistics(ChatId(-100), MessageId(7), false)
        .unwrap()
        .unwrap();
    rig.error(extra);
    let state = rig.driver.session.groups.stats.message.as_ref().unwrap();
    assert!(matches!(state.statistics, MessageStatsFetch::Failed(_)));
    let before = rig.sent().len();
    rig.driver
        .retry_message_statistics(ChatId(-100), MessageId(7), false)
        .unwrap()
        .expect("resends");
    assert!(rig.sent().len() > before);
    let state = rig.driver.session.groups.stats.message.as_ref().unwrap();
    assert_eq!(state.statistics, MessageStatsFetch::Loading);
    // A late answer for another post is dropped.
    let other = rig.driver.session.request(
        RequestPurpose::Groups(crate::state::GroupsPurpose::GetMessageStatistics {
            chat_id: -100,
            message_id: 8,
        }),
        Some(ChatId(-100)),
    );
    rig.answer(
        other,
        &format!(
            r#"{{"@type":"messageStatistics","message_interaction_graph":{GRAPH},"message_reaction_graph":{GRAPH}}}"#
        ),
    );
    let state = rig.driver.session.groups.stats.message.as_ref().unwrap();
    assert_eq!(state.message_id, 7);
    assert_eq!(state.statistics, MessageStatsFetch::Loading);
}

#[test]
fn recent_post_previews_are_fetched_once() {
    let mut rig = Rig::new();
    let extra = rig
        .driver
        .fetch_stats_post(ChatId(-100), MessageId(9 << 20))
        .unwrap()
        .expect("sends");
    assert_eq!(rig.last("getMessage")["message_id"], 9i64 << 20);
    assert!(
        rig.driver
            .fetch_stats_post(ChatId(-100), MessageId(9 << 20))
            .unwrap()
            .is_none()
    );
    rig.answer(
        extra,
        r#"{"@type":"message","id":9437184,"chat_id":-100,"date":1727740800,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Launch day","entities":[]}},"interaction_info":{"@type":"messageInteractionInfo","view_count":1200,"forward_count":7}}"#,
    );
    let preview = rig.driver.session.groups.stats.posts[&(-100, 9 << 20)]
        .clone()
        .expect("preview");
    assert_eq!(preview.text, "Launch day");
    assert_eq!(preview.views, 1200);
    assert_eq!(preview.forwards, 7);
    let missing = rig
        .driver
        .fetch_stats_post(ChatId(-100), MessageId(10 << 20))
        .unwrap()
        .unwrap();
    rig.error(missing);
    assert_eq!(
        rig.driver.session.groups.stats.posts[&(-100, 10 << 20)],
        None
    );
}

#[test]
fn recent_posts_show_ten_then_thirty_more() {
    let mut rig = Rig::new();
    let stats = &mut rig.driver.session.groups.stats;
    assert_eq!(stats.recent_posts_shown(-100, 50), RECENT_POSTS_FIRST);
    assert_eq!(stats.recent_posts_shown(-100, 4), 4);
    stats.show_more_recent_posts(-100, 50);
    assert_eq!(stats.recent_posts_shown(-100, 50), 40);
    stats.show_more_recent_posts(-100, 50);
    assert_eq!(stats.recent_posts_shown(-100, 50), 50);
    stats.reset(-100);
    assert_eq!(stats.recent_posts_shown(-100, 50), RECENT_POSTS_FIRST);
}

#[test]
fn message_properties_carry_the_statistics_flag() {
    let rig = Rig::new();
    let json = r#"{"@type":"messageProperties","can_get_statistics":true}"#;
    let envelope = copy_and_parse(json, &rig.seq, &rig.sink).unwrap();
    match &envelope.envelope.payload {
        crate::telegram::envelope::EnvelopePayload::Messages(
            crate::telegram::envelope::MessagesPayload::MessageProperties(actions),
        ) => assert!(actions.can_get_statistics),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn poll_votes_are_asked_once_for_a_poll_post() {
    let mut rig = Rig::new();
    let extra = rig
        .driver
        .fetch_stats_poll_votes(ChatId(-100), MessageId(3 << 20), true)
        .unwrap()
        .expect("sends");
    let sent = rig.last("getPollVoteStatistics");
    assert_eq!(sent["message_id"], 3i64 << 20);
    assert_eq!(sent["is_dark"], true);
    assert!(
        rig.driver
            .fetch_stats_poll_votes(ChatId(-100), MessageId(3 << 20), true)
            .unwrap()
            .is_none()
    );
    rig.answer(
        extra,
        &format!(r#"{{"@type":"pollVoteStatistics","vote_graph":{GRAPH}}}"#),
    );
    assert!(matches!(
        rig.driver.session.messages.poll_stats.get(&(-100, 3 << 20)),
        Some(crate::state::PollStatsFetch::Loaded(_))
    ));
}

#[test]
fn the_open_panel_downloads_recent_post_thumbnails() {
    use crate::ids::FileId;
    use crate::state::{ChatStatisticsFetch, StatsPostPreview};
    use crate::telegram::envelope::{
        ChannelStatistics, ChatStatistics, ChatStatisticsInteractionInfo, ChatStatisticsObject,
        StatisticalValue,
    };
    let mut rig = Rig::new();
    let session = &mut rig.driver.session;
    let value = StatisticalValue {
        value: 1.0,
        previous_value: 1.0,
        growth_rate_percentage: 0.0,
    };
    let none = StatisticalGraph::Error {
        error_message: String::new(),
    };
    let stats = ChannelStatistics {
        period_start: 0,
        period_end: 0,
        member_count: value,
        mean_message_view_count: value,
        mean_message_share_count: value,
        mean_message_reaction_count: value,
        mean_story_view_count: value,
        mean_story_share_count: value,
        mean_story_reaction_count: value,
        enabled_notifications_percentage: 0.0,
        member_count_graph: none.clone(),
        join_graph: none.clone(),
        mute_graph: none.clone(),
        view_count_by_hour_graph: none.clone(),
        view_count_by_source_graph: none.clone(),
        join_by_source_graph: none.clone(),
        language_graph: none.clone(),
        message_interaction_graph: none.clone(),
        message_reaction_graph: none.clone(),
        story_interaction_graph: none.clone(),
        story_reaction_graph: none.clone(),
        instant_view_interaction_graph: none,
        recent_interactions: vec![ChatStatisticsInteractionInfo {
            object: ChatStatisticsObject::Message {
                message_id: 9 << 20,
            },
            view_count: 1,
            forward_count: 0,
            reaction_count: 0,
        }],
    };
    session.groups.chat_statistics.insert(
        -100,
        ChatStatisticsFetch::Loaded(Box::new(ChatStatistics::Channel(Box::new(stats)))),
    );
    session.groups.stats.posts.insert(
        (-100, 9 << 20),
        Some(StatsPostPreview {
            text: "Photo".into(),
            date: 1,
            views: 1,
            forwards: 0,
            thumbs: vec![FileId(55), FileId(56)],
            is_poll: false,
        }),
    );
    assert!(session.stats_thumb_file_ids().is_empty(), "panel closed");
    session.set_open_info_panel(Some(InfoPanelTarget::Statistics(-100)));
    assert_eq!(session.stats_thumb_file_ids(), vec![FileId(55)]);
    // The post's own page asks for every candidate.
    session.open_message_stats_page(-100, 9 << 20);
    assert_eq!(session.stats_thumb_file_ids(), vec![FileId(55), FileId(56)]);
}
