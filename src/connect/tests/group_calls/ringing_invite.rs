//! Driver tests: a ringing conference invitation is the current call
//! (tdesktop `Instance::inCall`, `calls_instance.cpp`): an incoming 1:1
//! call and a second invitation are declined as busy, placing a call or
//! joining a voice chat or another conference declines it, and an
//! invitation to the conference already current is left alone.
use super::*;
use crate::state::{ActiveGroupCall, ConferenceCall};

type Harness = (
    std::path::PathBuf,
    ConnectDriver<Arc<RecordingSender>>,
    Arc<RecordingSender>,
    Arc<dyn DiagnosticSink>,
    std::sync::atomic::AtomicU64,
);

fn ringing_driver() -> Harness {
    let (dir, mut driver, recorder, sink, seq) = call_driver();
    driver.set_call_engine(Box::new(MockEngine::new()));
    seed_ready_call_user(&mut driver, &seq, &sink);
    let mut harness = (dir, driver, recorder, sink, seq);
    feed(&mut harness, &invitation(41, 90, 77));
    assert!(harness.1.session.calls.conference_invite.is_some(), "rings");
    harness
}

fn feed(harness: &mut Harness, json: &str) {
    let (_, driver, _, sink, seq) = harness;
    ingest_call_json(driver, seq, sink, json);
}

fn invitation(chat_id: i64, message_id: i64, unique_id: i64) -> String {
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{message_id},"chat_id":{chat_id},"sender_id":{{"@type":"messageSenderUser","user_id":{chat_id}}},"is_outgoing":false,"date":{},"content":{{"@type":"messageGroupCall","unique_id":"{unique_id}","is_active":false,"was_missed":false,"is_video":false,"duration":0,"other_participant_ids":[]}}}}}}"#,
        crate::local_time::now_unix()
    )
}

/// Every `declineGroupCallInvitation` sent, as (chat, message).
fn declines(recorder: &RecordingSender) -> Vec<(i64, i64)> {
    recorder
        .snapshot()
        .iter()
        .map(|json| serde_json::from_str::<Value>(json).unwrap())
        .filter(|value| value["@type"] == "declineGroupCallInvitation")
        .map(|value| {
            (
                value["chat_id"].as_i64().unwrap(),
                value["message_id"].as_i64().unwrap(),
            )
        })
        .collect()
}

fn ringing(harness: &Harness) -> Option<(i64, i64)> {
    harness
        .1
        .session
        .calls
        .conference_invite
        .as_ref()
        .map(|invite| (invite.chat_id.0, invite.message_id.0))
}

#[test]
fn an_incoming_call_while_an_invitation_rings_is_declined_busy() {
    let mut harness = ringing_driver();
    feed(
        &mut harness,
        r#"{"@type":"updateCall","call":{"@type":"call","id":5,"unique_id":"105","user_id":41,"is_outgoing":false,"is_video":false,"state":{"@type":"callStatePending","is_created":true,"is_received":true}}}"#,
    );
    assert!(harness.1.session.calls.active_call.is_none(), "never rings");
    assert_eq!(sent_request(&harness.2, "discardCall")["call_id"], 5);
    assert_eq!(harness.1.session.calls.busy_declined, vec![(41, false)]);
    assert_eq!(ringing(&harness), Some((41, 90)), "the invitation rings on");
    assert!(declines(&harness.2).is_empty());
    let _ = std::fs::remove_dir_all(&harness.0);
}

#[test]
fn a_second_invitation_is_declined_and_the_first_rings_on() {
    let mut harness = ringing_driver();
    // Another conference, from another chat with the same message id.
    feed(&mut harness, &invitation(42, 90, 78));
    assert_eq!(declines(&harness.2), vec![(42, 90)]);
    assert_eq!(ringing(&harness), Some((41, 90)));
    // The ringing message again (getDifference), or another invitation
    // to the same conference: neither is declined.
    feed(&mut harness, &invitation(41, 90, 77));
    feed(&mut harness, &invitation(43, 95, 77));
    assert_eq!(declines(&harness.2), vec![(42, 90)]);
    assert_eq!(ringing(&harness), Some((41, 90)));
    let _ = std::fs::remove_dir_all(&harness.0);
}

#[test]
fn placing_a_call_declines_the_ringing_invitation() {
    let mut harness = ringing_driver();
    harness
        .1
        .start_call(41, false)
        .expect("createCall goes out");
    assert_eq!(declines(&harness.2), vec![(41, 90)]);
    assert_eq!(ringing(&harness), None);
    let _ = std::fs::remove_dir_all(&harness.0);
}

#[test]
fn joining_a_voice_chat_declines_the_ringing_invitation() {
    let mut harness = ringing_driver();
    harness.1.session.calls.active_group_call = Some(ActiveGroupCall::fresh(555));
    harness.1.join_video_chat(555).expect("the join starts");
    assert_eq!(declines(&harness.2), vec![(41, 90)]);
    assert_eq!(ringing(&harness), None);
    let _ = std::fs::remove_dir_all(&harness.0);
}

#[test]
fn joining_another_conference_declines_the_ringing_invitation() {
    let mut harness = ringing_driver();
    harness
        .1
        .join_conference_link("https://t.me/call/abc".into())
        .expect("the join starts");
    assert_eq!(declines(&harness.2), vec![(41, 90)]);
    assert_eq!(ringing(&harness), None);
    let _ = std::fs::remove_dir_all(&harness.0);
}

#[test]
fn accepting_the_ringing_invitation_does_not_decline_it() {
    let mut harness = ringing_driver();
    harness
        .1
        .accept_group_call_invitation(41, 90)
        .expect("accept sends");
    assert!(declines(&harness.2).is_empty());
    assert_eq!(ringing(&harness), None, "answered");
    let _ = std::fs::remove_dir_all(&harness.0);
}

fn in_conference(harness: &mut Harness, unique_id: i64) {
    harness.1.session.calls.conference_invite = None;
    harness.1.session.calls.active_group_call = Some(ActiveGroupCall {
        is_joined: true,
        unique_id,
        conference: Some(ConferenceCall {
            input: None,
            upgrade: None,
            migrated_from: None,
            link_open: false,
        }),
        ..ActiveGroupCall::fresh(555)
    });
}

#[test]
fn an_invitation_to_the_conference_you_are_in_is_left_alone() {
    let mut harness = ringing_driver();
    in_conference(&mut harness, 78);
    feed(&mut harness, &invitation(42, 91, 78));
    assert!(
        declines(&harness.2).is_empty(),
        "the re-inviter sees no decline"
    );
    assert_eq!(
        ringing(&harness),
        None,
        "and it does not ring over the call"
    );
    // Another conference while you are in this one: declined as busy.
    feed(&mut harness, &invitation(43, 92, 79));
    assert_eq!(declines(&harness.2), vec![(43, 92)]);
    let _ = std::fs::remove_dir_all(&harness.0);
}

#[test]
fn a_failed_busy_decline_leaves_the_voice_chat_alone() {
    let mut harness = ringing_driver();
    in_conference(&mut harness, 78);
    feed(&mut harness, &invitation(43, 92, 79));
    let extra = sent_request(&harness.2, "declineGroupCallInvitation")["@extra"].clone();
    feed(
        &mut harness,
        &format!(
            r#"{{"@type":"error","code":400,"message":"MESSAGE_ID_INVALID","@extra":{extra}}}"#
        ),
    );
    assert_eq!(harness.1.session.calls.group_call_error, None);
    let _ = std::fs::remove_dir_all(&harness.0);
}
