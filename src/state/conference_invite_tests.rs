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
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":90,"chat_id":41,"sender_id":{{"@type":"messageSenderUser","user_id":41}},"is_outgoing":false,"date":{date},"content":{{"@type":"messageGroupCall","unique_id":"77","is_active":{is_active},"was_missed":{was_missed},"is_video":true,"duration":0,"other_participant_ids":[{{"@type":"messageSenderUser","user_id":42}},{{"@type":"messageSenderUser","user_id":41}}]}}}}}}"#
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
    apply(&mut session, &invitation(now, false, false));
    let ringing = session.calls.conference_invite.clone();
    apply(
        &mut session,
        &invitation(now - CONFERENCE_INVITE_RING_SECS - 1, false, false),
    );
    assert_eq!(session.calls.conference_invite, ringing);
    assert_eq!(
        session.calls.conference_invite_declines,
        vec![(ChatId(41), MessageId(90))]
    );
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
