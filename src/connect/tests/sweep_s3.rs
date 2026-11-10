//! Sweep fixes (s3-links-tdlib): connect-driver behaviour that needs a
//! sender which can refuse requests.
use super::support::toggle_driver;
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::UsernameCheckResult;

/// Sweep 02-037: a send error in a later step of an ingest does not lose
/// the answer captured from the same envelope.
#[test]
fn a_send_error_later_in_the_ingest_keeps_the_captured_answer() {
    let (_dir, mut driver, sender, sink, seq) = toggle_driver();
    driver.session.my_user_id = Some(7);
    let extra = driver.check_username("quill").expect("sent");
    // A step that runs in every ingest and must send (it fails below):
    // the open supergroup's membership probe.
    let chat = r#"{"@type":"updateNewChat","chat":{"id":-1001,"title":"G","type":{"@type":"chatTypeSupergroup","supergroup_id":1,"is_channel":false}}}"#;
    driver
        .ingest(copy_and_parse(chat, &seq, &sink).expect("parse"))
        .expect("chat");
    driver.session.open_chat = Some(crate::ids::ChatId(-1001));
    sender.refuse(true);
    let json = format!(
        r#"{{"@type":"checkChatUsernameResultOk","@extra":"{}"}}"#,
        extra.as_extra()
    );
    let result = driver.ingest(copy_and_parse(&json, &seq, &sink).expect("parse"));
    assert!(result.is_err(), "the later send really failed");
    assert_eq!(
        driver.session.users_state.username_check,
        Some(("quill".to_string(), UsernameCheckResult::Available)),
        "the verdict is stashed before the failing step"
    );
}

/// Sweep 04-041: a name whose request never left is not marked asked, so
/// the row does not keep its placeholder for the session.
#[test]
fn a_grams_peer_request_that_never_left_is_asked_again() {
    use crate::premium_hub::ton::{TonTx, TonTxKind};
    let (_dir, mut driver, sender, _sink, _seq) = toggle_driver();
    driver.session.payments.hub.grams.entries.push(TonTx {
        id: "t1".into(),
        key: "t1".into(),
        gram_nano: 1,
        is_refund: false,
        date: 1,
        kind: TonTxKind::SuggestedPostPayment { chat_id: 13 },
    });
    driver.session.payments.hub.grams.peers_dirty = true;
    sender.refuse(true);
    assert!(!driver.grams_tick(false), "nothing left");
    assert!(driver.session.payments.hub.grams.asked_chats.is_empty());
    sender.refuse(false);
    assert!(driver.grams_tick(false), "asked on the next tick");
    assert!(
        sender
            .snapshot()
            .iter()
            .any(|json| json.contains(r#""@type":"getChat""#))
    );
    assert!(!driver.grams_tick(false), "and only once");
}

/// Sweep 02-212: one failed `getSecretChat` send does not drop the ids
/// queued behind it.
#[test]
fn a_failed_secret_chat_send_keeps_the_rest_of_the_queue() {
    let (_dir, mut driver, sender, _sink, _seq) = toggle_driver();
    driver.session.users_state.secret_chat_fetch_queue = vec![1, 2, 3];
    sender.refuse(true);
    assert!(driver.maybe_fetch_secret_chat_states().is_err());
    assert_eq!(
        driver.session.users_state.secret_chat_fetch_queue,
        vec![1, 2, 3]
    );
    sender.refuse(false);
    assert!(driver.maybe_fetch_secret_chat_states().is_ok());
    assert_eq!(
        sender
            .snapshot()
            .iter()
            .filter(|json| json.contains(r#""@type":"getSecretChat""#))
            .count(),
        3
    );
}
