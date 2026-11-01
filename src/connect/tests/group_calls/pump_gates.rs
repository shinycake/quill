use super::*;
use crate::calls::tile_pin::TileKey;
use crate::locks::LockExt;

const CAMERA: &str = r#"{"@type":"groupCallParticipantVideoInfo","source_groups":[{"@type":"groupCallVideoSourceGroup","semantics":"SIM","source_ids":[111]}],"endpoint_id":"ep-42","is_paused":false}"#;
const SCREEN: &str = r#"{"@type":"groupCallParticipantVideoInfo","source_groups":[{"@type":"groupCallVideoSourceGroup","semantics":"SIM","source_ids":[222]}],"endpoint_id":"ep-42-screen","is_paused":false}"#;
const UNRELATED: &str =
    r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateReady"}}"#;

fn in_call_with_a_camera() -> GroupCallDriverHarness {
    let (dir, mut driver, recorder, handle, sink, seq) = ready_group_call_driver();
    let extra = driver.join_video_chat(555).expect("join tracked call");
    let answer = format!(
        r#"{{"@type":"text","text":"join-answer","@extra":{}}}"#,
        extra.0
    );
    ingest_call_json(&mut driver, &seq, &sink, &answer);
    let camera = group_participant_json(42, false, CAMERA, "null");
    ingest_call_json(&mut driver, &seq, &sink, &camera);
    (dir, driver, recorder, handle, sink, seq)
}

#[test]
fn an_unrelated_update_leaves_feeds_and_subscriptions_alone() {
    let (dir, mut driver, _recorder, handle, sink, seq) = in_call_with_a_camera();
    let syncs = handle.group_video_syncs().len();
    assert!(syncs > 0);
    driver.live_feeds.lock_or_recover().clear();

    ingest_call_json(&mut driver, &seq, &sink, UNRELATED);

    assert_eq!(handle.group_video_syncs().len(), syncs);
    assert!(driver.live_feeds.lock_or_recover().is_empty());

    let both = group_participant_json(42, false, CAMERA, SCREEN);
    ingest_call_json(&mut driver, &seq, &sink, &both);

    let sources = handle.group_video_syncs();
    assert_eq!(sources.len(), syncs + 1);
    assert_eq!(sources.last().map(|(_, sources)| sources.len()), Some(2));
    assert_eq!(driver.live_feeds.lock_or_recover().len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_rejoin_subscribes_its_new_instance_with_nothing_changed() {
    let (dir, mut driver, _recorder, handle, sink, seq) = in_call_with_a_camera();
    let syncs = handle.group_video_syncs().len();
    let call = driver.session.calls.group_call_mut(555).expect("tracked");
    call.transport_ready = false;
    call.join_payload = "rejoin-answer".to_string();

    ingest_call_json(&mut driver, &seq, &sink, UNRELATED);

    let sources = handle.group_video_syncs();
    assert_eq!(sources.len(), syncs + 1);
    assert_eq!(sources.last().map(|(_, sources)| sources.len()), Some(1));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_subscription_is_tried_again_on_the_next_update() {
    let (dir, mut driver, _recorder, handle, sink, seq) = in_call_with_a_camera();
    let syncs = handle.group_video_syncs().len();
    handle.fail_next_group_video_sync();
    let both = group_participant_json(42, false, CAMERA, SCREEN);
    ingest_call_json(&mut driver, &seq, &sink, &both);
    assert_eq!(handle.group_video_syncs().len(), syncs);

    ingest_call_json(&mut driver, &seq, &sink, UNRELATED);

    let sources = handle.group_video_syncs();
    assert_eq!(sources.len(), syncs + 1);
    assert_eq!(sources.last().map(|(_, sources)| sources.len()), Some(2));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_large_tile_alone_is_asked_for_more_than_a_thumbnail() {
    let (dir, mut driver, _recorder, handle, sink, seq) = in_call_with_a_camera();
    let both = group_participant_json(42, false, CAMERA, SCREEN);
    ingest_call_json(&mut driver, &seq, &sink, &both);
    let large = |handle: &MockEngine| {
        let syncs = handle.group_video_syncs();
        let sources = &syncs.last().expect("a sync").1;
        assert_eq!(sources.len(), 2);
        sources
            .iter()
            .filter(|source| source.large)
            .map(|source| source.endpoint.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(large(&handle), Vec::<String>::new());

    let tile = TileKey {
        participant: MessageSender::User { user_id: 42 },
        screen: true,
    };
    let syncs = handle.group_video_syncs().len();
    driver.set_group_call_large_tile(Some(tile));
    assert_eq!(handle.group_video_syncs().len(), syncs + 1);
    assert_eq!(large(&handle), vec!["ep-42-screen".to_string()]);

    driver.set_group_call_large_tile(Some(tile));
    assert_eq!(
        handle.group_video_syncs().len(),
        syncs + 1,
        "only on change"
    );

    driver.set_group_call_large_tile(None);
    assert_eq!(handle.group_video_syncs().len(), syncs + 2);
    assert_eq!(large(&handle), Vec::<String>::new());
    let _ = std::fs::remove_dir_all(&dir);
}
