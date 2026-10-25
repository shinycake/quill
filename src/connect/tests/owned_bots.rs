//! Connect-driver tests: editing an owned bot and creating a managed one
//! (recorded JSON, nothing live).
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::UsernameCheckResult;
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

fn extra_of(request: &serde_json::Value) -> String {
    request["@extra"].as_str().unwrap().to_string()
}

fn answer(extra: &str, body: &str) -> String {
    let mut value: serde_json::Value = serde_json::from_str(body).unwrap();
    value["@extra"] = serde_json::Value::String(extra.to_string());
    value.to_string()
}

#[test]
fn opening_the_editor_asks_for_the_name_and_the_about_text() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    driver.begin_bot_edit(21).unwrap();
    let name = sent_request(&recorder, "getBotName");
    let about = sent_request(&recorder, "getBotInfoShortDescription");
    assert_eq!(name["bot_user_id"], 21);
    assert_eq!(about["language_code"], "");
    feed(
        &mut driver,
        &seq,
        &sink,
        &answer(&extra_of(&name), r#"{"@type":"text","text":"Old"}"#),
    );
    feed(
        &mut driver,
        &seq,
        &sink,
        &answer(&extra_of(&about), r#"{"@type":"text","text":"Short"}"#),
    );
    let edit = driver.session.bots.owned.edit.clone().unwrap();
    assert_eq!(edit.name.as_deref(), Some("Old"));
    assert_eq!(edit.about.as_deref(), Some("Short"));
}

#[test]
fn saving_sends_only_what_changed_and_keeps_the_texts_on_ok() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    driver.begin_bot_edit(21).unwrap();
    let changes = crate::owned_bots::edit_changes("Old", "Short", "New", "Short").unwrap();
    driver.save_bot_edit(21, &changes).unwrap();
    let request = sent_request(&recorder, "setBotName");
    assert_eq!(request["name"], "New");
    assert!(
        recorder
            .snapshot()
            .iter()
            .all(|json| !json.contains("setBotInfoShortDescription"))
    );
    assert_eq!(driver.session.bots.owned.edit.as_ref().unwrap().saving, 1);
    feed(
        &mut driver,
        &seq,
        &sink,
        &answer(&extra_of(&request), r#"{"@type":"ok"}"#),
    );
    let edit = driver.session.bots.owned.edit.clone().unwrap();
    assert_eq!(edit.name.as_deref(), Some("New"));
    assert_eq!((edit.saving, edit.saved), (0, 1));
}

#[test]
fn a_refused_save_is_reported_and_not_kept() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    driver.begin_bot_edit(21).unwrap();
    let changes = crate::owned_bots::edit_changes("Old", "", "Old", "About").unwrap();
    driver.save_bot_edit(21, &changes).unwrap();
    let request = sent_request(&recorder, "setBotInfoShortDescription");
    feed(
        &mut driver,
        &seq,
        &sink,
        &answer(
            &extra_of(&request),
            r#"{"@type":"error","code":400,"message":"BOT_INVALID"}"#,
        ),
    );
    let edit = driver.session.bots.owned.edit.clone().unwrap();
    assert_eq!(edit.saving, 0);
    assert!(edit.about.is_none());
    assert!(edit.error.unwrap().contains("400"));
}

#[test]
fn a_new_photo_goes_out_as_a_local_file() {
    let (mut driver, recorder, _sink, _seq) = driver_parts();
    driver.begin_bot_edit(21).unwrap();
    driver.set_bot_photo(21, "/tmp/a.png").unwrap();
    let request = sent_request(&recorder, "setBotProfilePhoto");
    assert_eq!(request["photo"]["photo"]["path"], "/tmp/a.png");
}

#[test]
fn the_username_verdict_answers_only_the_latest_check() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    driver.check_new_bot_username("firstbot").unwrap();
    let first = sent_request(&recorder, "checkBotUsername");
    driver.check_new_bot_username("secondbot").unwrap();
    // The first answer arrives late and is dropped.
    feed(
        &mut driver,
        &seq,
        &sink,
        &answer(
            &extra_of(&first),
            r#"{"@type":"checkChatUsernameResultOk"}"#,
        ),
    );
    assert!(driver.session.bots.owned.create.check.is_none());
    let sent = recorder.snapshot();
    let second: serde_json::Value = serde_json::from_str(sent.last().unwrap()).unwrap();
    assert_eq!(second["username"], "secondbot");
    feed(
        &mut driver,
        &seq,
        &sink,
        &answer(
            &extra_of(&second),
            r#"{"@type":"checkChatUsernameResultUsernameOccupied"}"#,
        ),
    );
    assert_eq!(
        driver.session.bots.owned.create.check,
        Some(("secondbot".to_string(), UsernameCheckResult::Occupied))
    );
}

#[test]
fn creating_a_bot_records_the_new_user() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    driver
        .create_managed_bot(5, "Cool", "coolbot", true)
        .unwrap();
    let request = sent_request(&recorder, "createBot");
    assert_eq!(request["manager_bot_user_id"], 5);
    assert_eq!(request["via_link"], true);
    assert!(driver.session.bots.owned.create.creating.is_some());
    feed(
        &mut driver,
        &seq,
        &sink,
        &answer(
            &extra_of(&request),
            r#"{"@type":"user","id":900,"first_name":"Cool","type":{"@type":"userTypeBot","can_be_edited":true}}"#,
        ),
    );
    let create = &driver.session.bots.owned.create;
    assert_eq!(create.created, Some(900));
    assert!(create.creating.is_none());
}

#[test]
fn creating_from_the_keyboard_button_is_not_via_link() {
    let (mut driver, recorder, _sink, _seq) = driver_parts();
    driver
        .create_managed_bot(5, "Cool", "coolbot", false)
        .unwrap();
    let request = sent_request(&recorder, "createBot");
    assert_eq!(request["via_link"], false);
    assert_eq!(request["username"], "coolbot");
}

#[test]
fn a_failed_create_keeps_the_dialog_open_with_a_reason() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    driver
        .create_managed_bot(5, "Cool", "coolbot", true)
        .unwrap();
    let request = sent_request(&recorder, "createBot");
    feed(
        &mut driver,
        &seq,
        &sink,
        &answer(
            &extra_of(&request),
            r#"{"@type":"error","code":400,"message":"BOT_INVALID"}"#,
        ),
    );
    let create = &driver.session.bots.owned.create;
    assert!(create.created.is_none());
    assert!(create.creating.is_none());
    assert!(create.error.as_deref().unwrap().contains("BotFather"));
    assert_eq!(driver.session.payments.hub.limit_toast, None);
}

#[test]
fn the_bot_limit_shows_the_premium_toast_over_the_open_dialog() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    driver
        .create_managed_bot(5, "Cool", "coolbot", true)
        .unwrap();
    let request = sent_request(&recorder, "createBot");
    feed(
        &mut driver,
        &seq,
        &sink,
        &answer(
            &extra_of(&request),
            r#"{"@type":"error","code":400,"message":"BOT_CREATE_LIMIT_EXCEEDED"}"#,
        ),
    );
    let create = &driver.session.bots.owned.create;
    assert!(create.creating.is_none());
    assert_eq!(create.error, None);
    assert_eq!(
        driver.session.payments.hub.limit_toast,
        Some(crate::premium_limits::PremiumLimitKind::OwnedBots)
    );
}

#[test]
fn the_bot_type_tells_who_may_edit_and_who_may_manage() {
    let (mut driver, _recorder, sink, seq) = driver_parts();
    feed(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateUser","user":{"id":21,"first_name":"Mine","type":{"@type":"userTypeBot","can_be_edited":true,"can_manage_bots":false}}}"#,
    );
    feed(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateUser","user":{"id":22,"first_name":"Maker","type":{"@type":"userTypeBot","can_be_edited":false,"can_manage_bots":true}}}"#,
    );
    let mine = driver.session.user(21).unwrap();
    assert!(mine.can_be_edited && !mine.can_manage_bots);
    let maker = driver.session.user(22).unwrap();
    assert!(!maker.can_be_edited && maker.can_manage_bots);
}
