use super::*;
use crate::diagnostics::MemorySink;
use crate::telegram::client::copy_and_parse;
use std::sync::atomic::AtomicU64;

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

fn invitation(date: i64, is_active: bool, was_missed: bool) -> String {
    invitation_message(90, 77, date, is_active, was_missed)
}

fn invitation_message(
    id: i64,
    unique_id: i64,
    date: i64,
    is_active: bool,
    was_missed: bool,
) -> String {
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":41,"sender_id":{{"@type":"messageSenderUser","user_id":41}},"is_outgoing":false,"date":{date},"content":{{"@type":"messageGroupCall","unique_id":"{unique_id}","is_active":{is_active},"was_missed":{was_missed},"is_video":true,"duration":0,"other_participant_ids":[{{"@type":"messageSenderUser","user_id":42}},{{"@type":"messageSenderUser","user_id":41}}]}}}}}}"#
    )
}

#[test]
fn a_fresh_invitation_rings_until_it_is_joined() {
    let mut session = session();
    apply(
        &mut session,
        &invitation(crate::local_time::now_unix(), false, false),
    );
    assert_eq!(
        session.calls.conference_invite,
        Some(ConferenceInvite {
            chat_id: ChatId(41),
            message_id: MessageId(90),
            unique_id: 77,
            user_id: 41,
            is_video: true,
            others: vec![42],
        })
    );
    apply(
        &mut session,
        r#"{"@type":"updateMessageContent","chat_id":41,"message_id":90,"new_content":{"@type":"messageGroupCall","unique_id":"77","is_active":true,"was_missed":false,"is_video":true,"duration":0,"other_participant_ids":[]}}"#,
    );
    assert_eq!(session.calls.conference_invite, None);
}

#[test]
fn joined_or_missed_invitations_do_not_ring() {
    let mut session = session();
    let now = crate::local_time::now_unix();
    apply(&mut session, &invitation(now, true, false));
    apply(&mut session, &invitation(now, false, true));
    assert_eq!(session.calls.conference_invite, None);
    assert!(session.calls.conference_invite_declines.is_empty());
}

#[test]
fn an_invitation_older_than_the_ring_timeout_is_declined_not_rung() {
    let mut session = session();
    let now = crate::local_time::now_unix();
    apply(
        &mut session,
        &invitation(now - CONFERENCE_INVITE_RING_SECS - 1, false, false),
    );
    assert_eq!(session.calls.conference_invite, None);
    assert_eq!(
        session.calls.conference_invite_declines,
        vec![(ChatId(41), MessageId(90))]
    );
}

/// A ringing invitation is the current call: a second one is declined,
/// and the same one again is not.
#[test]
fn a_second_invitation_while_one_rings_is_declined() {
    let mut session = session();
    let now = crate::local_time::now_unix();
    apply(&mut session, &invitation(now, false, false));
    let ringing = session.calls.conference_invite.clone();
    apply(&mut session, &invitation(now, false, false));
    assert!(session.calls.conference_invite_declines.is_empty());
    apply(&mut session, &invitation_message(91, 78, now, false, false));
    assert_eq!(session.calls.conference_invite, ringing);
    assert_eq!(
        session.calls.conference_invite_declines,
        vec![(ChatId(41), MessageId(91))]
    );
}

/// An incoming 1:1 call while the invitation rings is declined as busy
/// and does not take the call window.
#[test]
fn an_incoming_call_while_an_invitation_rings_is_busy() {
    let mut session = session();
    apply(
        &mut session,
        &invitation(crate::local_time::now_unix(), false, false),
    );
    apply(
        &mut session,
        r#"{"@type":"updateCall","call":{"@type":"call","id":5,"unique_id":"9","user_id":41,"is_outgoing":false,"is_video":true,"state":{"@type":"callStatePending","is_created":true,"is_received":true}}}"#,
    );
    assert!(session.calls.active_call.is_none());
    assert_eq!(session.calls.busy_decline_queue, vec![(5, 41, true)]);
    assert!(session.calls.conference_invite.is_some());
}

#[test]
fn a_deleted_invitation_stops_ringing() {
    let mut session = session();
    apply(
        &mut session,
        &invitation(crate::local_time::now_unix(), false, false),
    );
    apply(
        &mut session,
        r#"{"@type":"updateDeleteMessages","chat_id":41,"message_ids":[90],"is_permanent":true,"from_cache":false}"#,
    );
    assert_eq!(session.calls.conference_invite, None);
}
