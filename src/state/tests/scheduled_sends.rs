//! State reducer tests: a scheduled send stays out of the chat's history
//! from its send answer until TDLib sends it at its time.
use super::common::*;
use super::*;

const SCHEDULED: &str =
    r#""scheduling_state":{"@type":"messageSchedulingStateSendAtDate","send_date":1800000000}"#;

fn message(id: i64, scheduled: bool, pending: bool) -> String {
    let state = if scheduled { SCHEDULED } else { r#""x":0"# };
    let sending = if pending {
        r#""sending_state":{"@type":"messageSendingStatePending","sending_id":0},"#
    } else {
        ""
    };
    format!(
        r#"{{"@type":"message","id":{id},"chat_id":7,"date":1700000000,"is_outgoing":true,{sending}{state},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"later","entities":[]}}}}}}"#
    )
}

fn history_ids(session: &Session) -> Vec<i64> {
    session
        .histories
        .get(&7)
        .map(|h| h.messages.keys().copied().collect())
        .unwrap_or_default()
}

fn scheduled_ids(session: &Session) -> Vec<i64> {
    session.scheduled_in(ChatId(7)).map(|m| m.id.0).collect()
}

#[test]
fn scheduled_send_lives_in_the_scheduled_list_until_it_goes_out() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let send = session.request(RequestPurpose::SendMessage, Some(ChatId(7)));
    // The send answer: TDLib's temporary scheduled copy.
    let answer = message(-1, true, true).replacen(
        r#"{"@type":"message","#,
        &format!(r#"{{"@type":"message","@extra":"{}","#, send.0),
        1,
    );
    apply_json(&mut session, &seq, &sink, &answer);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateNewMessage","message":{}}}"#,
            message(-1, true, true)
        ),
    );
    assert_eq!(history_ids(&session), Vec::<i64>::new());
    assert_eq!(scheduled_ids(&session), vec![-1]);

    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateMessageSendSucceeded","old_message_id":-1,"message":{}}}"#,
            message(1_048_580, true, false)
        ),
    );
    assert_eq!(
        history_ids(&session),
        Vec::<i64>::new(),
        "a saved scheduled send must not show in the chat as sent"
    );
    assert_eq!(scheduled_ids(&session), vec![1_048_580]);
    assert!(
        session
            .messages
            .draft_clears
            .iter()
            .any(|ack| ack.chat_id == ChatId(7) && ack.old_message_id == MessageId(-1)),
        "the draft it went from is let go"
    );

    // At its time TDLib deletes the scheduled copy and the real message
    // arrives.
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateDeleteMessages","chat_id":7,"message_ids":[1048580],"is_permanent":true,"from_cache":false}"#,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateNewMessage","message":{}}}"#,
            message(50, false, false)
        ),
    );
    assert_eq!(scheduled_ids(&session), Vec::<i64>::new());
    assert_eq!(history_ids(&session), vec![50]);
}

#[test]
fn failed_scheduled_send_stays_in_the_scheduled_list() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateNewMessage","message":{}}}"#,
            message(-2, true, true)
        ),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateMessageSendFailed","old_message_id":-2,"error":{{"@type":"error","code":400,"message":"SCHEDULE_DATE_INVALID"}},"message":{}}}"#,
            message(-3, true, false)
        ),
    );
    assert_eq!(history_ids(&session), Vec::<i64>::new());
    assert_eq!(scheduled_ids(&session), vec![-3]);
    assert!(
        session
            .messages
            .failed_sends
            .iter()
            .any(|ack| ack.old_message_id == MessageId(-2)),
        "the draft stays"
    );
}
