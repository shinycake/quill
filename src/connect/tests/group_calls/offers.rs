//! Driver tests: the join payload arrives after the press, as tgcalls makes
//! it off the UI thread. Each site sends once it lands, and a press, a leave
//! or a failure during the wait leaves no half-open call.
use super::*;
use std::time::Instant;

fn sent_count(recorder: &RecordingSender, type_name: &str) -> usize {
    recorder
        .snapshot()
        .iter()
        .filter(|json| json.contains(&format!(r#""@type":"{type_name}""#)))
        .count()
}

fn land(driver: &mut ConnectDriver<Arc<RecordingSender>>, handle: &MockEngine) -> bool {
    handle.release_group_offers();
    driver.pump_call_media(Instant::now())
}

fn tracked(driver: &ConnectDriver<Arc<RecordingSender>>) -> &crate::state::ActiveGroupCall {
    driver
        .session
        .calls
        .active_group_call
        .as_ref()
        .expect("tracked call")
}

fn sharable(handle: &MockEngine, driver: &mut ConnectDriver<Arc<RecordingSender>>) {
    handle.set_devices(vec![MediaDevice {
        id: "screen-0".into(),
        name: "Test screen".into(),
        kind: MediaDeviceKind::Screen,
    }]);
    driver.refresh_call_devices();
    driver
        .session
        .calls
        .active_group_call
        .as_mut()
        .expect("tracked call")
        .is_joined = true;
}

#[test]
fn a_join_goes_out_when_its_payload_lands_with_the_mute_of_that_moment() {
    let (dir, mut driver, recorder, handle, _sink, _seq) = ready_group_call_driver();
    handle.hold_group_offers();
    let extra = driver.join_video_chat(555).expect("join tracked call");
    assert_eq!(sent_count(&recorder, "joinVideoChat"), 0);
    assert!(!driver.pump_call_media(Instant::now()));

    let muted = !tracked(&driver).is_muted_self;
    driver
        .set_group_call_self_mute(muted)
        .expect("a press during the wait is kept for the join");

    assert!(land(&mut driver, &handle), "the UI redraws");
    let join = sent_request(&recorder, "joinVideoChat");
    assert_eq!(join["join_parameters"]["payload"], "mock-group-offer-555");
    assert_eq!(join["join_parameters"]["is_muted"], muted);
    assert_eq!(join["@extra"], extra.as_extra());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_second_press_during_the_wait_is_the_same_join() {
    let (dir, mut driver, recorder, handle, _sink, _seq) = ready_group_call_driver();
    handle.hold_group_offers();
    let first = driver.join_video_chat(555).expect("join tracked call");
    let second = driver.join_video_chat(555).expect("the same join");
    assert_eq!(first, second);
    land(&mut driver, &handle);
    assert_eq!(handle.group_offers().len(), 1);
    assert_eq!(sent_count(&recorder, "joinVideoChat"), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn leaving_during_the_wait_sends_no_join_and_leaves_no_instance() {
    let (dir, mut driver, recorder, handle, _sink, _seq) = ready_group_call_driver();
    handle.hold_group_offers();
    let extra = driver.join_video_chat(555).expect("join tracked call");
    driver.drop_group_call_locally();
    assert!(driver.session.requests.get(extra).is_none());
    assert!(!land(&mut driver, &handle));
    assert_eq!(sent_count(&recorder, "joinVideoChat"), 0);
    assert!(handle.group_leaves().contains(&555));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_offer_that_lands_after_the_call_ended_ends_its_instance() {
    let (dir, mut driver, recorder, handle, _sink, _seq) = ready_group_call_driver();
    handle.hold_group_offers();
    let extra = driver.join_video_chat(555).expect("join tracked call");
    driver.session.leave_group_call_local();
    land(&mut driver, &handle);
    assert_eq!(sent_count(&recorder, "joinVideoChat"), 0);
    assert!(driver.session.requests.get(extra).is_none());
    assert!(handle.group_leaves().contains(&555));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_payload_sends_no_join_and_the_button_starts_over() {
    let (dir, mut driver, recorder, handle, _sink, _seq) = ready_group_call_driver();
    handle.fail_group_offer();
    let extra = driver.join_video_chat(555).expect("the press is taken");
    assert_eq!(sent_count(&recorder, "joinVideoChat"), 0);
    assert!(driver.session.requests.get(extra).is_none());
    assert!(tracked(&driver).transport_error.is_some());
    assert!(handle.group_leaves().contains(&555));

    driver.join_video_chat(555).expect("join again");
    assert_eq!(sent_count(&recorder, "joinVideoChat"), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_rejoin_waits_without_touching_the_missing_instance() {
    let (dir, mut driver, recorder, handle, sink, seq) = ready_group_call_driver();
    let extra = driver.join_video_chat(555).expect("join tracked call");
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &format!(r#"{{"@type":"text","text":"a","@extra":{}}}"#, extra.0),
    );
    let syncs = handle.group_video_syncs().len();
    driver
        .session
        .calls
        .active_group_call
        .as_mut()
        .expect("tracked call")
        .reconnecting = true;
    handle.hold_group_offers();

    driver.rejoin_group_call(true).expect("rejoin starts");
    assert_eq!(sent_count(&recorder, "joinVideoChat"), 1);
    driver.maybe_auto_rejoin_group_call().expect("a no-op");
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateReady"}}"#,
    );
    assert_eq!(handle.group_video_syncs().len(), syncs);
    assert_eq!(tracked(&driver).transport_error, None);
    assert_eq!(tracked(&driver).rejoin_attempts, 0);

    land(&mut driver, &handle);
    assert_eq!(sent_count(&recorder, "joinVideoChat"), 2);
    assert_eq!(tracked(&driver).rejoin_attempts, 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_rejoin_counts_and_is_tried_again() {
    let (dir, mut driver, recorder, handle, _sink, _seq) = ready_group_call_driver();
    driver
        .session
        .calls
        .active_group_call
        .as_mut()
        .expect("tracked call")
        .reconnecting = true;
    handle.fail_group_offer();
    driver.rejoin_group_call(true).expect("the press is taken");
    assert_eq!(sent_count(&recorder, "joinVideoChat"), 0);
    let call = tracked(&driver);
    assert!(call.reconnecting);
    assert_eq!(call.rejoin_attempts, 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn stopping_a_share_during_the_wait_sends_neither_start_nor_end() {
    let (dir, mut driver, recorder, handle, _sink, _seq) = ready_group_call_driver();
    sharable(&handle, &mut driver);
    handle.hold_group_offers();
    let start = driver
        .toggle_group_call_screen_share()
        .expect("start sharing");
    assert!(tracked(&driver).screen_share_pending);
    driver
        .toggle_group_call_screen_share()
        .expect("changed my mind");
    assert!(!tracked(&driver).screen_share_pending);
    assert!(driver.session.requests.get(start).is_none());
    land(&mut driver, &handle);
    assert_eq!(sent_count(&recorder, "startGroupCallScreenSharing"), 0);
    assert_eq!(sent_count(&recorder, "endGroupCallScreenSharing"), 0);
    assert!(!handle.presentation_active(555));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_share_payload_clears_the_pending_share() {
    let (dir, mut driver, recorder, handle, _sink, _seq) = ready_group_call_driver();
    sharable(&handle, &mut driver);
    handle.fail_group_offer();
    driver
        .toggle_group_call_screen_share()
        .expect("the press is taken");
    assert_eq!(sent_count(&recorder, "startGroupCallScreenSharing"), 0);
    let call = tracked(&driver);
    assert!(!call.screen_share_pending);
    assert!(call.transport_error.is_some());
    assert!(!handle.presentation_active(555));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A voice chat opened from a link before its chat loaded: the join still
/// gets its instance and carries the real payload, not an empty one that
/// would show you in a call you can't hear.
#[test]
fn a_voice_chat_in_no_known_chat_joins_with_an_instance_and_its_payload() {
    let (dir, mut driver, recorder, handle, _sink, _seq) = ready_group_call_driver();
    if let Some(chat) = driver.session.chats.get_mut(&51) {
        chat.video_chat = None;
    }
    assert_eq!(
        driver.group_call_chat_id(555),
        None,
        "no chat holds the call"
    );
    driver.join_video_chat(555).expect("join tracked call");
    land(&mut driver, &handle);
    assert_eq!(
        handle.group_offers(),
        [(555, 0)],
        "an instance, keyed by no chat"
    );
    let join = sent_request(&recorder, "joinVideoChat");
    assert_eq!(join["join_parameters"]["payload"], "mock-group-offer-555");
    let _ = std::fs::remove_dir_all(&dir);
}
