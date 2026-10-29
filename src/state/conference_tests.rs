use super::*;
use crate::diagnostics::MemorySink;
use crate::telegram::client::copy_and_parse;
use std::sync::atomic::AtomicU64;

/// Envelope numbers, rising across a test as the receive loop's do.
static SEQ: AtomicU64 = AtomicU64::new(0);

fn session() -> Session {
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    Session::new(crate::ids::AccountKey::primary(), sink)
}

fn apply(session: &mut Session, json: &str) {
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let owned = copy_and_parse(json, &SEQ, &sink).expect("envelope");
    session.apply(owned);
}

fn joined_answer(extra: RequestId, group_call_id: i32) -> String {
    format!(
        r#"{{"@type":"groupCallInfo","group_call_id":{group_call_id},"join_payload":"{{}}","@extra":{}}}"#,
        serde_json::to_string(&extra.as_extra()).expect("extra")
    )
}

fn upgrade() -> ConferenceUpgrade {
    ConferenceUpgrade {
        call_id: 5,
        peer_user_id: 41,
        is_video: false,
        invite: vec![42],
        share_link: false,
    }
}

#[test]
fn the_join_answer_tracks_the_conference_with_its_payload() {
    let mut session = session();
    let extra = session.request(RequestPurpose::Calls(CallsPurpose::JoinConference), None);
    session.begin_conference(
        extra,
        ConferenceOrigin::Invitation {
            chat_id: 41,
            message_id: 9,
        },
        true,
    );
    apply(&mut session, &joined_answer(extra, 77));

    assert_eq!(session.calls.pending_conference, None);
    let call = session.calls.active_group_call.as_ref().expect("tracked");
    assert_eq!(call.id, 77);
    assert!(call.is_conference());
    assert!(call.is_muted_self);
    assert_eq!(call.join_payload, "{}");
    assert_eq!(
        call.conference_input(),
        Some(ConferenceInput::Message {
            chat_id: 41,
            message_id: 9
        })
    );
    assert_eq!(session.calls.group_call_fetch_queue, vec![77]);
}

#[test]
fn a_created_conference_keeps_its_upgrade_and_rejoins_by_its_link() {
    let mut session = session();
    let extra = session.request(RequestPurpose::Calls(CallsPurpose::JoinConference), None);
    session.begin_conference(
        extra,
        ConferenceOrigin::Create {
            upgrade: Some(upgrade()),
        },
        false,
    );
    apply(&mut session, &joined_answer(extra, 78));
    let call = session.calls.active_group_call.as_mut().expect("tracked");
    assert_eq!(
        call.conference.as_ref().and_then(|c| c.upgrade.clone()),
        Some(upgrade())
    );
    // Nothing to rejoin with until TDLib reports the link.
    assert_eq!(call.conference_input(), None);
    call.invite_link = "https://t.me/call/abc".into();
    assert_eq!(
        call.conference_input(),
        Some(ConferenceInput::Link("https://t.me/call/abc".into()))
    );
}

#[test]
fn a_failed_join_leaves_nothing_tracked() {
    let mut session = session();
    let extra = session.request(RequestPurpose::Calls(CallsPurpose::JoinConference), None);
    session.begin_conference(
        extra,
        ConferenceOrigin::Link {
            link: "https://t.me/call/abc".into(),
        },
        false,
    );
    apply(
        &mut session,
        &format!(
            r#"{{"@type":"error","code":400,"message":"GROUPCALL_INVALID","@extra":{}}}"#,
            serde_json::to_string(&extra.as_extra()).expect("extra")
        ),
    );
    assert_eq!(session.calls.pending_conference, None);
    assert!(session.calls.active_group_call.is_none());
    assert!(session.calls.group_call_error.is_some());
}

fn call_update(state: &str) -> String {
    format!(
        r#"{{"@type":"updateCall","call":{{"@type":"call","id":5,"unique_id":"9","user_id":41,"is_outgoing":false,"is_video":false,"state":{state}}}}}"#
    )
}

#[test]
fn a_call_taken_into_a_conference_is_followed_without_an_end_screen() {
    let mut session = session();
    apply(
        &mut session,
        &call_update(r#"{"@type":"callStatePending","is_created":true,"is_received":true}"#),
    );
    assert!(session.calls.active_call.is_some());
    apply(
        &mut session,
        &call_update(
            r#"{"@type":"callStateDiscarded","reason":{"@type":"callDiscardReasonUpgradeToGroupCall","invite_link":"https://t.me/call/abc"},"need_rating":false,"need_debug_information":false,"need_log":false}"#,
        ),
    );
    assert!(session.calls.active_call.is_none());
    assert!(session.calls.summary.is_none());
    assert_eq!(
        session.calls.call_upgrade,
        Some(CallUpgrade {
            invite_link: "https://t.me/call/abc".into(),
            muted: false,
        })
    );
}

#[test]
fn a_call_hung_up_shows_its_end_screen() {
    let mut session = session();
    apply(
        &mut session,
        &call_update(r#"{"@type":"callStatePending","is_created":true,"is_received":true}"#),
    );
    apply(
        &mut session,
        &call_update(
            r#"{"@type":"callStateDiscarded","reason":{"@type":"callDiscardReasonHungUp"},"need_rating":false,"need_debug_information":false,"need_log":false}"#,
        ),
    );
    assert!(session.calls.summary.is_some());
    assert_eq!(session.calls.call_upgrade, None);
}
