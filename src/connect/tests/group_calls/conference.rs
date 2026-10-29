//! Driver tests: conference calls (`parity:calls-conference`): joining an
//! invitation, a 1:1 call taken into a conference, following the other
//! side's upgrade, rejoining, and the engine's frame crypto and ssrc owners.
use super::*;
use crate::state::ConferenceInput;

type Harness = (
    std::path::PathBuf,
    ConnectDriver<Arc<RecordingSender>>,
    Arc<RecordingSender>,
    MockEngine,
    Arc<dyn DiagnosticSink>,
    std::sync::atomic::AtomicU64,
);

fn conference_driver() -> Harness {
    let (dir, mut driver, recorder, sink, seq) = call_driver();
    let mock = MockEngine::new();
    let handle = mock.clone();
    driver.set_call_engine(Box::new(mock));
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
    );
    (dir, driver, recorder, handle, sink, seq)
}

fn feed(harness: &mut Harness, json: &str) {
    let (_, driver, _, _, sink, seq) = harness;
    ingest_call_json(driver, seq, sink, json);
}

fn answer(extra: &Value, group_call_id: i32) -> String {
    format!(
        r#"{{"@type":"groupCallInfo","group_call_id":{group_call_id},"join_payload":"answer-{group_call_id}","@extra":{extra}}}"#
    )
}

fn conference_update(id: i32, invite_link: &str, need_rejoin: bool) -> String {
    format!(
        r#"{{"@type":"updateGroupCall","group_call":{{"@type":"groupCall","id":{id},"unique_id":"9{id}","title":"","invite_link":"{invite_link}","paid_message_star_count":0,"scheduled_start_date":0,"enabled_start_notification":false,"is_active":true,"is_video_chat":false,"is_live_story":false,"is_rtmp_stream":false,"is_joined":true,"need_rejoin":{need_rejoin},"is_owned":true,"can_be_managed":false,"participant_count":2,"has_hidden_listeners":false,"loaded_all_participants":true,"message_sender_id":null,"recent_speakers":[],"is_my_video_enabled":false,"is_my_video_paused":false,"can_enable_video":true,"mute_new_participants":false,"can_toggle_mute_new_participants":false,"can_send_messages":true,"are_messages_allowed":true,"can_toggle_are_messages_allowed":false,"can_delete_messages":false,"record_duration":0,"is_video_recorded":false,"duration":0}}}}"#
    )
}

fn one_to_one(state: &str) -> String {
    format!(
        r#"{{"@type":"updateCall","call":{{"@type":"call","id":5,"unique_id":"105","user_id":41,"is_outgoing":true,"is_video":false,"state":{state}}}}}"#
    )
}

const READY_SUPPORTING_CONFERENCE: &str = r#"{"@type":"callStateReady","protocol":{"@type":"callProtocol","udp_p2p":true,"udp_reflector":true,"min_layer":65,"max_layer":92,"library_versions":["13.0.0"]},"servers":[],"config":"{}","encryption_key":"AQIDBA==","emojis":[],"allow_p2p":false,"is_group_call_supported":true,"custom_parameters":"{}"}"#;

fn sent_count(recorder: &RecordingSender, type_name: &str) -> usize {
    recorder
        .snapshot()
        .iter()
        .filter(|json| json.contains(&format!(r#""@type":"{type_name}""#)))
        .count()
}

fn bound_call(driver: &ConnectDriver<Arc<RecordingSender>>) -> Option<(i32, Option<i32>)> {
    driver
        .conference_crypto
        .as_ref()
        .map(|known| (known.key, known.crypto.group_call_id()))
}

#[test]
fn an_invitation_is_accepted_with_an_encrypted_instance_renamed_by_the_answer() {
    let mut harness = conference_driver();
    let request = harness
        .1
        .accept_group_call_invitation(3, 42)
        .expect("accept sends");
    let join = sent_request(&harness.2, "joinGroupCall");
    assert_eq!(join["input_group_call"]["@type"], "inputGroupCallMessage");
    assert_eq!(join["input_group_call"]["chat_id"], 3);
    assert_eq!(join["input_group_call"]["message_id"], 42);
    assert_eq!(
        join["join_parameters"]["payload"],
        "mock-conference-offer--1"
    );
    assert_eq!(
        harness.3.conferences().first().map(|(key, _)| *key),
        Some(-1),
        "the instance waits under the provisional key"
    );
    assert_eq!(bound_call(&harness.1), Some((-1, None)));
    // A second join while the first is in flight is refused.
    assert_eq!(
        harness.1.accept_group_call_invitation(3, 43),
        Err(ConnectSendError::InvalidRequest)
    );

    let extra = serde_json::to_string(&request.as_extra()).unwrap();
    let extra: Value = serde_json::from_str(&extra).unwrap();
    feed(&mut harness, &answer(&extra, 555));
    assert_eq!(harness.3.rekeys(), vec![(-1, 555)]);
    assert_eq!(bound_call(&harness.1), Some((555, Some(555))));
    let call = harness.1.session.calls.active_group_call.as_ref().unwrap();
    assert_eq!(call.id, 555);
    assert!(call.is_conference());
    assert_eq!(
        sent_request(&harness.2, "getGroupCall")["group_call_id"],
        555
    );
    assert!(
        harness
            .3
            .group_connects()
            .iter()
            .any(|(id, payload, _)| *id == 555 && payload == "answer-555"),
        "the pump connects with the join answer"
    );

    // Leaving drops the frames still in flight.
    let crypto = harness
        .1
        .conference_crypto
        .as_ref()
        .map(|known| known.crypto.clone())
        .unwrap();
    harness.1.leave_group_call().expect("leave sends");
    assert_eq!(crypto.group_call_id(), None);
    assert!(harness.1.conference_crypto.is_none());
    let _ = std::fs::remove_dir_all(&harness.0);
}

#[test]
fn a_refused_join_drops_the_instance() {
    let mut harness = conference_driver();
    let request = harness
        .1
        .join_conference_link("https://t.me/call/abc".into())
        .expect("join sends");
    assert_eq!(
        sent_request(&harness.2, "joinGroupCall")["input_group_call"]["link"],
        "https://t.me/call/abc"
    );
    let extra = serde_json::to_string(&request.as_extra()).unwrap();
    feed(
        &mut harness,
        &format!(
            r#"{{"@type":"error","code":400,"message":"GROUPCALL_INVALID","@extra":{extra}}}"#
        ),
    );
    assert!(harness.3.conferences().is_empty());
    assert!(harness.1.conference_crypto.is_none());
    assert!(harness.1.session.calls.active_group_call.is_none());
    assert!(harness.1.session.calls.group_call_error.is_some());
    let _ = std::fs::remove_dir_all(&harness.0);
}

#[test]
fn without_an_engine_or_during_a_call_nothing_is_joined() {
    let (dir, mut driver, recorder, sink, seq) = call_driver();
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
    );
    assert_eq!(
        driver.accept_group_call_invitation(3, 42),
        Err(ConnectSendError::CallsUnavailable)
    );
    assert_eq!(sent_count(&recorder, "joinGroupCall"), 0);
    let _ = std::fs::remove_dir_all(&dir);

    let mut harness = conference_driver();
    feed(
        &mut harness,
        &one_to_one(r#"{"@type":"callStatePending","is_created":true,"is_received":true}"#),
    );
    assert_eq!(
        harness.1.accept_group_call_invitation(3, 42),
        Err(ConnectSendError::InvalidRequest)
    );
    // "Add people" waits until the call is established.
    assert_eq!(
        harness.1.upgrade_call_to_conference(vec![42], false),
        Err(ConnectSendError::InvalidRequest)
    );
    assert_eq!(sent_count(&harness.2, "joinGroupCall"), 0);
    assert_eq!(sent_count(&harness.2, "createGroupCall"), 0);
    let _ = std::fs::remove_dir_all(&harness.0);
}

#[test]
fn add_people_takes_the_call_into_a_new_conference_and_ends_it_with_the_link() {
    let mut harness = conference_driver();
    feed(
        &mut harness,
        &one_to_one(r#"{"@type":"callStatePending","is_created":true,"is_received":true}"#),
    );
    feed(&mut harness, &one_to_one(READY_SUPPORTING_CONFERENCE));
    let request = harness
        .1
        .upgrade_call_to_conference(vec![41, 42], false)
        .expect("create sends");
    let create = sent_request(&harness.2, "createGroupCall");
    assert_eq!(
        create["join_parameters"]["payload"],
        "mock-conference-offer--1"
    );
    let extra = serde_json::to_string(&request.as_extra()).unwrap();
    let extra: Value = serde_json::from_str(&extra).unwrap();
    feed(&mut harness, &answer(&extra, 600));
    // The 1:1 call goes on until the conference has its link.
    assert_eq!(sent_count(&harness.2, "discardCall"), 0);
    assert!(harness.1.session.calls.active_call.is_some());

    feed(
        &mut harness,
        &conference_update(600, "https://t.me/call/xyz", false),
    );
    let discard = sent_request(&harness.2, "discardCall");
    assert_eq!(discard["call_id"], 5);
    assert_eq!(discard["invite_link"], "https://t.me/call/xyz");
    // The other side follows the link; only the others are invited.
    let invite = sent_request(&harness.2, "inviteGroupCallParticipant");
    assert_eq!(invite["group_call_id"], 600);
    assert_eq!(invite["user_id"], 42);
    assert_eq!(sent_count(&harness.2, "inviteGroupCallParticipant"), 1);
    // Later updates don't do it again.
    feed(
        &mut harness,
        &conference_update(600, "https://t.me/call/xyz", false),
    );
    assert_eq!(sent_count(&harness.2, "discardCall"), 1);

    // The 1:1 call ends with no end screen: the conference goes on.
    feed(
        &mut harness,
        &one_to_one(
            r#"{"@type":"callStateDiscarded","reason":{"@type":"callDiscardReasonHungUp"},"need_rating":true,"need_debug_information":false,"need_log":false}"#,
        ),
    );
    assert!(harness.1.session.calls.active_call.is_none());
    assert!(harness.1.session.calls.summary.is_none());
    assert_eq!(
        harness
            .1
            .session
            .calls
            .active_group_call
            .as_ref()
            .map(|call| call.id),
        Some(600)
    );
    let _ = std::fs::remove_dir_all(&harness.0);
}

#[test]
fn the_other_side_of_an_upgraded_call_joins_by_its_link() {
    let mut harness = conference_driver();
    feed(
        &mut harness,
        &one_to_one(r#"{"@type":"callStatePending","is_created":true,"is_received":true}"#),
    );
    feed(
        &mut harness,
        &one_to_one(
            r#"{"@type":"callStateDiscarded","reason":{"@type":"callDiscardReasonUpgradeToGroupCall","invite_link":"https://t.me/call/xyz"},"need_rating":false,"need_debug_information":false,"need_log":false}"#,
        ),
    );
    assert!(harness.1.session.calls.summary.is_none());
    let join = sent_request(&harness.2, "joinGroupCall");
    assert_eq!(join["input_group_call"]["@type"], "inputGroupCallLink");
    assert_eq!(join["input_group_call"]["link"], "https://t.me/call/xyz");
    assert!(harness.1.session.calls.pending_conference.is_some());

    // The link's join fails: the 1:1 call ends as failed, on its end
    // screen (tdesktop `finishByMigration`).
    let extra = join["@extra"].clone();
    feed(
        &mut harness,
        &format!(
            r#"{{"@type":"error","code":400,"message":"GROUPCALL_INVALID","@extra":{extra}}}"#
        ),
    );
    let summary = harness
        .1
        .session
        .calls
        .summary
        .as_ref()
        .expect("end screen");
    assert_eq!(summary.call_id, 5);
    assert_eq!(summary.end_line, crate::state::CONFERENCE_FOLLOW_FAILED);
    assert!(harness.1.session.calls.pending_conference.is_none());
    let _ = std::fs::remove_dir_all(&harness.0);
}

#[test]
fn a_dead_conference_gives_way_to_a_new_join() {
    let mut harness = conference_driver();
    let request = harness
        .1
        .accept_group_call_invitation(3, 42)
        .expect("accept sends");
    let extra = serde_json::to_string(&request.as_extra()).unwrap();
    let extra: Value = serde_json::from_str(&extra).unwrap();
    feed(&mut harness, &answer(&extra, 555));
    // A live conference refuses another join.
    assert_eq!(
        harness
            .1
            .join_conference_link("https://t.me/call/new".into()),
        Err(ConnectSendError::InvalidRequest)
    );
    // A failed screen share leaves it working: still refused.
    harness
        .1
        .session
        .calls
        .active_group_call
        .as_mut()
        .unwrap()
        .transport_error = Some("screen share failed".into());
    assert_eq!(
        harness
            .1
            .join_conference_link("https://t.me/call/new".into()),
        Err(ConnectSendError::InvalidRequest)
    );
    // Its rejoins are spent: the new link replaces it, and it is left.
    let call = harness.1.session.calls.active_group_call.as_mut().unwrap();
    call.is_joined = true;
    call.reconnecting = true;
    call.rejoin_attempts = crate::state::MAX_GROUP_CALL_REJOIN_ATTEMPTS;
    harness
        .1
        .join_conference_link("https://t.me/call/new".into())
        .expect("join sends");
    let join = sent_request(&harness.2, "joinGroupCall");
    assert_eq!(join["input_group_call"]["link"], "https://t.me/call/new");
    assert!(harness.1.session.calls.active_group_call.is_none());
    assert!(harness.3.group_leaves().contains(&555));
    assert_eq!(sent_count(&harness.2, "leaveGroupCall"), 1);
    assert_eq!(bound_call(&harness.1), Some((-1, None)));
    let _ = std::fs::remove_dir_all(&harness.0);
}

#[test]
fn a_dropped_conference_rejoins_by_its_link_under_its_id() {
    let mut harness = conference_driver();
    let request = harness
        .1
        .accept_group_call_invitation(3, 42)
        .expect("accept sends");
    let extra = serde_json::to_string(&request.as_extra()).unwrap();
    let extra: Value = serde_json::from_str(&extra).unwrap();
    feed(&mut harness, &answer(&extra, 555));
    feed(
        &mut harness,
        &conference_update(555, "https://t.me/call/abc", true),
    );
    let call = harness.1.session.calls.active_group_call.as_ref().unwrap();
    assert_eq!(
        call.conference_input(),
        Some(ConferenceInput::Link("https://t.me/call/abc".into()))
    );
    let rejoin = sent_request(&harness.2, "joinGroupCall");
    assert_eq!(rejoin["input_group_call"]["@type"], "inputGroupCallLink");
    assert_eq!(rejoin["input_group_call"]["link"], "https://t.me/call/abc");
    assert_eq!(
        rejoin["join_parameters"]["payload"],
        "mock-conference-offer-555"
    );
    assert_eq!(sent_count(&harness.2, "joinVideoChat"), 0);
    // The same crypto, still bound to the call.
    assert_eq!(bound_call(&harness.1), Some((555, Some(555))));
    let _ = std::fs::remove_dir_all(&harness.0);
}

#[test]
fn the_engine_learns_who_sends_each_audio_source() {
    let mut harness = conference_driver();
    let request = harness
        .1
        .accept_group_call_invitation(3, 42)
        .expect("accept sends");
    let extra = serde_json::to_string(&request.as_extra()).unwrap();
    let extra: Value = serde_json::from_str(&extra).unwrap();
    feed(&mut harness, &answer(&extra, 555));
    feed(
        &mut harness,
        r#"{"@type":"updateGroupCallParticipant","group_call_id":555,"participant":{"@type":"groupCallParticipant","participant_id":{"@type":"messageSenderUser","user_id":41},"audio_source_id":1234,"screen_sharing_audio_source_id":0,"video_info":null,"screen_sharing_video_info":null,"bio":"","is_current_user":false,"is_speaking":false,"is_hand_raised":false,"can_be_muted_for_all_users":false,"can_be_unmuted_for_all_users":false,"can_be_muted_for_current_user":false,"can_be_unmuted_for_current_user":false,"is_muted_for_all_users":false,"is_muted_for_current_user":false,"can_unmute_self":false,"volume_level":10000,"order":"a1"}}"#,
    );
    let owners = harness.3.ssrc_owners();
    let (id, last) = owners.last().expect("owners set");
    assert_eq!(*id, 555);
    assert_eq!(last.owner(1234), Some(41));
    let _ = std::fs::remove_dir_all(&harness.0);
}

#[test]
fn an_invitation_during_a_call_is_declined_and_one_accepted_stops_ringing() {
    let invitation = |id: i64| {
        format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":41,"sender_id":{{"@type":"messageSenderUser","user_id":41}},"is_outgoing":false,"date":{},"content":{{"@type":"messageGroupCall","unique_id":"77","is_active":false,"was_missed":false,"is_video":false,"duration":0,"other_participant_ids":[]}}}}}}"#,
            crate::local_time::now_unix()
        )
    };
    let mut harness = conference_driver();
    feed(&mut harness, &invitation(90));
    assert!(harness.1.session.calls.conference_invite.is_some());
    harness
        .1
        .accept_group_call_invitation(41, 90)
        .expect("accept sends");
    assert_eq!(harness.1.session.calls.conference_invite, None);
    let _ = std::fs::remove_dir_all(&harness.0);

    let mut harness = conference_driver();
    feed(
        &mut harness,
        &one_to_one(r#"{"@type":"callStatePending","is_created":true,"is_received":true}"#),
    );
    feed(&mut harness, &invitation(91));
    let decline = sent_request(&harness.2, "declineGroupCallInvitation");
    assert_eq!(decline["chat_id"], 41);
    assert_eq!(decline["message_id"], 91);
    assert_eq!(harness.1.session.calls.conference_invite, None);
    let _ = std::fs::remove_dir_all(&harness.0);
}

#[test]
fn an_invitation_older_than_the_ring_timeout_is_declined() {
    let date = crate::local_time::now_unix() - crate::state::CONFERENCE_INVITE_RING_SECS - 1;
    let mut harness = conference_driver();
    feed(
        &mut harness,
        &format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":92,"chat_id":41,"sender_id":{{"@type":"messageSenderUser","user_id":41}},"is_outgoing":false,"date":{date},"content":{{"@type":"messageGroupCall","unique_id":"77","is_active":false,"was_missed":false,"is_video":false,"duration":0,"other_participant_ids":[]}}}}}}"#
        ),
    );
    let decline = sent_request(&harness.2, "declineGroupCallInvitation");
    assert_eq!(decline["message_id"], 92);
    assert_eq!(harness.1.session.calls.conference_invite, None);
    let _ = std::fs::remove_dir_all(&harness.0);
}
