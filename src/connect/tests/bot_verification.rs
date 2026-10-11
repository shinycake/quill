//! Connect-driver tests: the owner's verify and unverify requests, and the
//! verification fields TDLib sends back (recorded JSON, nothing live).
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use crate::telegram::requests::MessageSenderRef;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

type Recorder = Arc<RecordingSender>;

fn driver_parts() -> (
    ConnectDriver<Recorder>,
    Recorder,
    Arc<dyn DiagnosticSink>,
    AtomicU64,
) {
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink;
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    (driver, recorder, dyn_sink, seq)
}

fn feed(
    driver: &mut ConnectDriver<Recorder>,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
    json: &str,
) {
    driver
        .ingest(copy_and_parse(json, seq, sink).unwrap())
        .unwrap();
}

#[test]
fn verifying_a_user_sends_the_bot_the_sender_and_the_description() {
    let (mut driver, recorder, _sink, _seq) = driver_parts();
    driver
        .set_bot_verification(77, MessageSenderRef::User(12), "Checked by Acme")
        .unwrap();
    let request = sent_request(&recorder, "setMessageSenderBotVerification");
    assert_eq!(request["bot_user_id"], 77);
    assert_eq!(request["verified_id"]["@type"], "messageSenderUser");
    assert_eq!(request["verified_id"]["user_id"], 12);
    assert_eq!(request["custom_description"]["text"], "Checked by Acme");
}

#[test]
fn unverifying_a_channel_names_the_chat() {
    let (mut driver, recorder, _sink, _seq) = driver_parts();
    driver
        .remove_bot_verification(77, MessageSenderRef::Chat(-1001))
        .unwrap();
    let request = sent_request(&recorder, "removeMessageSenderBotVerification");
    assert_eq!(request["verified_id"]["@type"], "messageSenderChat");
    assert_eq!(request["verified_id"]["chat_id"], -1001);
}

#[test]
fn a_bot_id_of_zero_is_refused() {
    let (mut driver, _recorder, _sink, _seq) = driver_parts();
    assert!(
        driver
            .set_bot_verification(0, MessageSenderRef::User(12), "")
            .is_err()
    );
}

#[test]
fn the_verifying_bots_icon_reaches_the_chat_row() {
    let (mut driver, _recorder, sink, seq) = driver_parts();
    feed(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":31,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":31},"unread_count":0}}"#,
    );
    feed(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateUser","user":{"id":31,"first_name":"Ada","type":{"@type":"userTypeRegular"},"verification_status":{"@type":"verificationStatus","is_verified":false,"is_scam":false,"is_fake":false,"bot_verification_icon_custom_emoji_id":"5368324170671202286"}}}"#,
    );
    let chat = driver.session.chats.get(&31).unwrap().clone();
    assert_eq!(
        driver.session.chat_bot_verification_icon(&chat),
        Some(5368324170671202286)
    );
}

#[test]
fn only_the_owner_gets_verification_parameters() {
    let (mut driver, _recorder, sink, seq) = driver_parts();
    feed(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateUserFullInfo","user_id":77,"user_full_info":{"@type":"userFullInfo","bot_info":{"@type":"botInfo","short_description":"","description":"","commands":[],"verification_parameters":{"@type":"botVerificationParameters","icon_custom_emoji_id":"42","organization_name":"Acme","default_custom_description":null,"can_set_custom_description":true}}}}"#,
    );
    feed(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateUserFullInfo","user_id":78,"user_full_info":{"@type":"userFullInfo","bot_info":{"@type":"botInfo","short_description":"","description":"","commands":[],"verification_parameters":null}}}"#,
    );
    let params = driver.session.bot_verification_parameters(77).unwrap();
    assert_eq!(params.organization_name, "Acme");
    assert!(params.can_set_custom_description);
    assert!(driver.session.bot_verification_parameters(78).is_none());
}

#[test]
fn full_info_carries_the_description_and_the_owner_parameters() {
    let (mut driver, _recorder, sink, seq) = driver_parts();
    feed(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateUser","user":{"id":31,"first_name":"Ada","type":{"@type":"userTypeRegular"}}}"#,
    );
    feed(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateUserFullInfo","user_id":31,"user_full_info":{"@type":"userFullInfo","bio":null,"bot_verification":{"@type":"botVerification","bot_user_id":77,"icon_custom_emoji_id":"42","custom_description":{"@type":"formattedText","text":"Verified by Acme","entities":[]}}}}"#,
    );
    let verification = driver.session.user_bot_verification(31).unwrap();
    assert_eq!(verification.bot_user_id, 77);
    assert_eq!(verification.description, "Verified by Acme");
}
