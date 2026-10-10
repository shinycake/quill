use super::*;

/// Phase D2: `chatStatisticsChannel` (schema 1.8.67, line 10233) —
/// values, all graph variants, and recent interactions (message +
/// story object types). All 12 graph fields are present, as TDLib
/// always sends them (schema has no optional flags on them).
fn channel_statistics_json() -> String {
    let data_graph = |json_data: &str| {
        format!(
            r#"{{"@type":"statisticalGraphData","json_data":"{}","zoom_token":""}}"#,
            json_data.replace('"', "\\\"")
        )
    };
    format!(
        r#"{{"@type":"chatStatisticsChannel","@extra":"7","period":{{"@type":"dateRange","start_date":1788000000,"end_date":1788604800}},"member_count":{{"@type":"statisticalValue","value":12345.0,"previous_value":11700.0,"growth_rate_percentage":5.5}},"mean_message_view_count":{{"@type":"statisticalValue","value":8421.0,"previous_value":9010.0,"growth_rate_percentage":-6.5}},"mean_message_share_count":{{"@type":"statisticalValue","value":312.0,"previous_value":280.0,"growth_rate_percentage":11.4}},"mean_message_reaction_count":{{"@type":"statisticalValue","value":428.0,"previous_value":390.0,"growth_rate_percentage":9.7}},"mean_story_view_count":{{"@type":"statisticalValue","value":5120.0,"previous_value":4980.0,"growth_rate_percentage":2.8}},"mean_story_share_count":{{"@type":"statisticalValue","value":96.0,"previous_value":104.0,"growth_rate_percentage":-7.7}},"mean_story_reaction_count":{{"@type":"statisticalValue","value":154.0,"previous_value":140.0,"growth_rate_percentage":10.0}},"enabled_notifications_percentage":61.5,"member_count_graph":{member_graph},"join_graph":{{"@type":"statisticalGraphAsync","token":"tok"}},"mute_graph":{{"@type":"statisticalGraphError","error_message":"STATS_GRAPH_NOT_AVAILABLE"}},"view_count_by_hour_graph":{hour_graph},"view_count_by_source_graph":{hour_graph},"join_by_source_graph":{hour_graph},"language_graph":{hour_graph},"message_interaction_graph":{hour_graph},"message_reaction_graph":{hour_graph},"story_interaction_graph":{hour_graph},"story_reaction_graph":{hour_graph},"instant_view_interaction_graph":{hour_graph},"recent_interactions":[{{"@type":"chatStatisticsInteractionInfo","object_type":{{"@type":"chatStatisticsObjectTypeMessage","message_id":201}},"view_count":12402,"forward_count":7,"reaction_count":213}},{{"@type":"chatStatisticsInteractionInfo","object_type":{{"@type":"chatStatisticsObjectTypeStory","story_id":44}},"view_count":987,"forward_count":12,"reaction_count":65}}]}}"#,
        member_graph = data_graph(
            &serde_json::to_string(&serde_json::json!({
                "columns": [["x", 1788000000, 1788086400], ["y0", 11800, 12345]],
                "types": {"x": "x", "y0": "line"},
            }))
            .unwrap()
        ),
        hour_graph = data_graph("{}"),
    )
}

#[test]
fn chat_statistics_channel_parses_values_graphs_and_interactions() {
    let env = parse_envelope(&channel_statistics_json()).unwrap();
    let EnvelopePayload::Groups(GroupsPayload::ChatStatistics { statistics }) = env.payload else {
        panic!("expected statistics");
    };
    let ChatStatistics::Channel(stats) = statistics else {
        panic!("expected channel statistics");
    };
    assert_eq!(
        (stats.period_start, stats.period_end),
        (1788000000, 1788604800)
    );
    assert_eq!(stats.member_count.value, 12345.0);
    assert_eq!(stats.member_count.growth_rate_percentage, 5.5);
    assert_eq!(stats.mean_message_view_count.growth_rate_percentage, -6.5);
    // All mean_* values are required by the schema — the fixture carries
    // real statisticalValue objects for each.
    assert_eq!(stats.mean_message_share_count.value, 312.0);
    assert_eq!(stats.mean_message_reaction_count.value, 428.0);
    assert_eq!(stats.mean_story_share_count.value, 96.0);
    assert_eq!(stats.mean_story_reaction_count.value, 154.0);
    assert_eq!(stats.enabled_notifications_percentage, 61.5);
    // Data graph keeps its json_data for client-side sparklines.
    let StatisticalGraph::Data {
        json_data,
        zoom_token,
    } = &stats.member_count_graph
    else {
        panic!("expected data graph");
    };
    assert!(json_data.contains("\"y0\""));
    assert!(zoom_token.is_empty());
    // Async / Error variants round-trip.
    assert!(matches!(stats.join_graph, StatisticalGraph::Async { .. }));
    let StatisticalGraph::Error { error_message } = &stats.mute_graph else {
        panic!("expected error graph");
    };
    assert_eq!(error_message, "STATS_GRAPH_NOT_AVAILABLE");
    // Recent interactions: message + story object types.
    assert_eq!(stats.recent_interactions.len(), 2);
    assert!(matches!(
        stats.recent_interactions[0].object,
        ChatStatisticsObject::Message { message_id: 201 }
    ));
    assert_eq!(stats.recent_interactions[0].view_count, 12402);
    assert!(matches!(
        stats.recent_interactions[1].object,
        ChatStatisticsObject::Story { story_id: 44 }
    ));
    assert_eq!(stats.recent_interactions[1].forward_count, 12);
}

#[test]
fn chat_statistics_missing_graph_is_parse_error() {
    // A missing (null) graph is `MissingField`, not a silent empty
    // graph — TDLib always sends all 12 (schema has no optional
    // flags), so a null one means a protocol change we must surface
    // rather than fabricate.
    let json = channel_statistics_json().replace(
            r#""mute_graph":{"@type":"statisticalGraphError","error_message":"STATS_GRAPH_NOT_AVAILABLE"}"#,
            r#""mute_graph":null"#,
        );
    assert!(parse_envelope(&json).is_err());
}

#[test]
fn chat_statistics_missing_value_is_parse_error() {
    // A null required `statisticalValue` is a parse error, not
    // fabricated zeros — the schema marks all of these required.
    let json = channel_statistics_json().replace(
            r#""mean_message_share_count":{"@type":"statisticalValue","value":312.0,"previous_value":280.0,"growth_rate_percentage":11.4}"#,
            r#""mean_message_share_count":null"#,
        );
    assert!(parse_envelope(&json).is_err());
}

#[test]
fn chat_statistics_supergroup_parses_top_lists() {
    // Phase D2: `chatStatisticsSupergroup` (schema 1.8.67, line 10208)
    // with top senders / administrators / inviters.
    let json = r#"{"@type":"chatStatisticsSupergroup","@extra":"7","period":{"@type":"dateRange","start_date":1788000000,"end_date":1788604800},"member_count":{"@type":"statisticalValue","value":420.0,"previous_value":400.0,"growth_rate_percentage":5.0},"message_count":{"@type":"statisticalValue","value":1234.0,"previous_value":1100.0,"growth_rate_percentage":12.2},"viewer_count":{"@type":"statisticalValue","value":380.0,"previous_value":360.0,"growth_rate_percentage":5.6},"sender_count":{"@type":"statisticalValue","value":95.0,"previous_value":90.0,"growth_rate_percentage":5.6},"member_count_graph":{"@type":"statisticalGraphData","json_data":"{}","zoom_token":""},"join_graph":{"@type":"statisticalGraphData","json_data":"{}","zoom_token":""},"join_by_source_graph":{"@type":"statisticalGraphData","json_data":"{}","zoom_token":""},"language_graph":{"@type":"statisticalGraphData","json_data":"{}","zoom_token":""},"message_content_graph":{"@type":"statisticalGraphData","json_data":"{}","zoom_token":""},"action_graph":{"@type":"statisticalGraphData","json_data":"{}","zoom_token":""},"day_graph":{"@type":"statisticalGraphData","json_data":"{}","zoom_token":""},"week_graph":{"@type":"statisticalGraphData","json_data":"{}","zoom_token":""},"top_senders":[{"@type":"chatStatisticsMessageSenderInfo","user_id":31,"sent_message_count":250,"average_character_count":120}],"top_administrators":[{"@type":"chatStatisticsAdministratorActionsInfo","user_id":32,"deleted_message_count":3,"banned_user_count":1,"restricted_user_count":0}],"top_inviters":[{"@type":"chatStatisticsInviterInfo","user_id":33,"added_member_count":7}]}"#;
    let env = parse_envelope(json).unwrap();
    let EnvelopePayload::Groups(GroupsPayload::ChatStatistics { statistics }) = env.payload else {
        panic!("expected statistics");
    };
    let ChatStatistics::Supergroup(stats) = statistics else {
        panic!("expected supergroup statistics");
    };
    assert_eq!(stats.member_count.value, 420.0);
    assert_eq!(stats.message_count.growth_rate_percentage, 12.2);
    assert_eq!(stats.top_senders.len(), 1);
    assert_eq!(stats.top_senders[0].user_id, 31);
    assert_eq!(stats.top_senders[0].sent_message_count, 250);
    assert_eq!(stats.top_administrators[0].deleted_message_count, 3);
    assert_eq!(stats.top_inviters[0].added_member_count, 7);
}

#[test]
fn chat_statistics_unknown_variant_is_parse_error() {
    // Unknown future `ChatStatistics` constructors fail parsing at the
    // envelope level rather than silently becoming `Unknown` (and
    // dropping the statistics response).
    let env = parse_envelope(r#"{"@type":"chatStatisticsQuantum"}"#);
    assert!(env.is_err());
}
