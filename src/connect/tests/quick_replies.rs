//! Connect-driver tests: quick reply shortcuts (recorded JSON, nothing live).
use super::super::*;
use super::*;
use crate::composer::filter_command_menu_items;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::ids::ChatId;
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
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

fn count_sent(recorder: &Recorder, type_name: &str) -> usize {
    recorder
        .snapshot()
        .iter()
        .filter(|json| json.contains(&format!("\"@type\":\"{type_name}\"")))
        .count()
}

fn seed_shortcuts(
    driver: &mut ConnectDriver<Recorder>,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
) {
    for json in [
        r#"{"@type":"updateQuickReplyShortcut","shortcut":{"@type":"quickReplyShortcut","id":1,"name":"hours","first_message":null,"message_count":2}}"#,
        r#"{"@type":"updateQuickReplyShortcut","shortcut":{"@type":"quickReplyShortcut","id":2,"name":"hello","first_message":null,"message_count":1}}"#,
        r#"{"@type":"updateQuickReplyShortcuts","shortcut_ids":[2,1]}"#,
    ] {
        feed(driver, seq, sink, json);
    }
}

fn private_chat(
    driver: &mut ConnectDriver<Recorder>,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
    id: i64,
) {
    feed(
        driver,
        seq,
        sink,
        &format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"P{id}","type":{{"@type":"chatTypePrivate","user_id":{id}}},"unread_count":0}}}}"#
        ),
    );
}

#[test]
fn shortcuts_load_once_and_updates_build_the_ordered_list() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    driver.ensure_quick_reply_shortcuts().unwrap();
    driver.ensure_quick_reply_shortcuts().unwrap();
    assert_eq!(count_sent(&recorder, "loadQuickReplyShortcuts"), 1);
    seed_shortcuts(&mut driver, &seq, &sink);
    let names: Vec<String> = driver
        .session
        .settings
        .quick_replies
        .list()
        .iter()
        .map(|s| s.name.clone())
        .collect();
    assert_eq!(names, ["hello", "hours"]);
}

#[test]
fn messages_load_once_per_shortcut_and_fill_the_editor() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    seed_shortcuts(&mut driver, &seq, &sink);
    driver.ensure_quick_reply_messages(1).unwrap();
    driver.ensure_quick_reply_messages(1).unwrap();
    let request = sent_request(&recorder, "loadQuickReplyShortcutMessages");
    assert_eq!(request["shortcut_id"], 1);
    assert_eq!(count_sent(&recorder, "loadQuickReplyShortcutMessages"), 1);
    feed(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateQuickReplyShortcutMessages","shortcut_id":1,"messages":[{"@type":"quickReplyMessage","id":10,"can_be_edited":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Open 9 to 5","entities":[]}}},null,{"@type":"quickReplyMessage","id":11,"can_be_edited":true,"content":{"@type":"messagePhoto","caption":{"@type":"formattedText","text":"Map","entities":[]}}}]}"#,
    );
    let messages = driver.session.settings.quick_replies.messages_of(1);
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].summary(), "Open 9 to 5");
    assert_eq!(messages[1].summary(), "Photo: Map");
    assert_eq!(
        driver.session.settings.quick_replies.shortcuts[&1].message_count,
        2
    );
}

#[test]
fn a_deleted_shortcut_disappears_with_its_messages() {
    let (mut driver, _recorder, sink, seq) = driver_parts();
    seed_shortcuts(&mut driver, &seq, &sink);
    feed(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateQuickReplyShortcutDeleted","shortcut_id":1}"#,
    );
    let state = &driver.session.settings.quick_replies;
    assert_eq!(state.list().len(), 1);
    assert!(state.by_name("hours").is_none());
    assert!(state.messages_of(1).is_empty());
}

#[test]
fn adding_renaming_and_deleting_send_the_schema_requests() {
    let (mut driver, recorder, _sink, _seq) = driver_parts();
    driver
        .add_quick_reply_text(" hours ", "We open at 9")
        .unwrap();
    let add = sent_request(&recorder, "addQuickReplyShortcutMessage");
    assert_eq!(add["shortcut_name"], "hours");
    assert_eq!(add["input_message_content"]["text"]["text"], "We open at 9");
    driver.edit_quick_reply_text(1, 10, "We open at 8").unwrap();
    assert_eq!(
        sent_request(&recorder, "editQuickReplyMessage")["message_id"],
        10
    );
    driver.delete_quick_reply_message(1, 10).unwrap();
    assert_eq!(
        sent_request(&recorder, "deleteQuickReplyShortcutMessages")["message_ids"][0],
        10
    );
    driver.rename_quick_reply(1, "opening").unwrap();
    assert_eq!(
        sent_request(&recorder, "setQuickReplyShortcutName")["name"],
        "opening"
    );
    driver.delete_quick_reply_shortcut(1).unwrap();
    assert_eq!(
        sent_request(&recorder, "deleteQuickReplyShortcut")["shortcut_id"],
        1
    );
}

#[test]
fn bad_names_and_empty_texts_send_nothing() {
    let (mut driver, recorder, _sink, _seq) = driver_parts();
    let before = recorder.snapshot().len();
    assert!(driver.add_quick_reply_text("two words", "hi").is_err());
    assert!(driver.add_quick_reply_text("ok", "   ").is_err());
    assert!(driver.rename_quick_reply(1, "").is_err());
    assert!(driver.edit_quick_reply_text(1, 2, "").is_err());
    assert_eq!(recorder.snapshot().len(), before);
}

#[test]
fn the_slash_menu_offers_shortcuts_in_a_private_chat_with_a_person() {
    let (mut driver, _recorder, sink, seq) = driver_parts();
    seed_shortcuts(&mut driver, &seq, &sink);
    private_chat(&mut driver, &seq, &sink, 31);
    let items = driver.session.command_menu_items(ChatId(31));
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].command, "hello");
    assert_eq!(items[0].description, "1 message");
    assert_eq!(items[0].quick_reply, Some(2));
    let filtered = filter_command_menu_items(&items, "HO");
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].quick_reply, Some(1));
}

#[test]
fn the_slash_menu_skips_bots_and_empty_shortcuts_but_not_saved_messages() {
    let (mut driver, _recorder, sink, seq) = driver_parts();
    seed_shortcuts(&mut driver, &seq, &sink);
    feed(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateQuickReplyShortcut","shortcut":{"@type":"quickReplyShortcut","id":3,"name":"empty","first_message":null,"message_count":0}}"#,
    );
    private_chat(&mut driver, &seq, &sink, 31);
    private_chat(&mut driver, &seq, &sink, 32);
    driver.session.my_user_id = Some(32);
    driver.session.bots.bot_user_ids.insert(33);
    private_chat(&mut driver, &seq, &sink, 33);
    assert!(
        driver
            .session
            .command_menu_items(ChatId(31))
            .iter()
            .all(|item| item.command != "empty")
    );
    // Saved Messages (the chat with yourself) lists them, as in tdesktop.
    assert_eq!(driver.session.command_menu_items(ChatId(32)).len(), 2);
    assert!(driver.session.command_menu_items(ChatId(33)).is_empty());
}

#[test]
fn choosing_a_shortcut_sends_it_to_the_chat() {
    let (mut driver, recorder, _sink, _seq) = driver_parts();
    driver.send_quick_reply(ChatId(31), 2).unwrap();
    driver.send_quick_reply(ChatId(31), 2).unwrap();
    let sent: Vec<serde_json::Value> = recorder
        .snapshot()
        .iter()
        .map(|json| serde_json::from_str::<serde_json::Value>(json).unwrap())
        .filter(|v| v["@type"] == "sendQuickReplyShortcutMessages")
        .collect();
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0]["chat_id"], 31);
    assert_eq!(sent[0]["shortcut_id"], 2);
    assert_ne!(sent[0]["sending_id"], sent[1]["sending_id"]);
}

#[test]
fn a_refused_request_is_reported_and_a_late_answer_is_harmless() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    driver.add_quick_reply_text("hours", "Hi").unwrap();
    let request = sent_request(&recorder, "addQuickReplyShortcutMessage");
    let extra = request["@extra"].as_str().unwrap().to_string();
    feed(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"quickReplyMessage","@extra":"{extra}","id":5,"can_be_edited":true,"content":{{"@type":"messageText","text":{{"text":"Hi","entities":[]}}}}}}"#
        ),
    );
    driver.delete_quick_reply_shortcut(9).unwrap();
    let request = sent_request(&recorder, "deleteQuickReplyShortcut");
    let extra = request["@extra"].as_str().unwrap().to_string();
    feed(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{extra}","code":400,"message":"SHORTCUT_INVALID"}}"#
        ),
    );
    assert!(driver.session.settings.quick_replies.error.is_some());
}

#[test]
fn the_shortcuts_are_requested_once_the_session_is_ready() {
    let (_driver, recorder, _sink, _seq) = driver_parts();
    assert_eq!(count_sent(&recorder, "loadQuickReplyShortcuts"), 1);
}

#[test]
fn the_limits_follow_the_tdlib_options_and_default_to_tdesktops() {
    let (mut driver, _recorder, sink, seq) = driver_parts();
    let state = &driver.session.settings.quick_replies;
    assert_eq!((state.shortcut_limit(), state.message_limit()), (100, 20));
    for json in [
        r#"{"@type":"updateOption","name":"quick_reply_shortcut_count_max","value":{"@type":"optionValueInteger","value":"5"}}"#,
        r#"{"@type":"updateOption","name":"quick_reply_shortcut_message_count_max","value":{"@type":"optionValueInteger","value":"3"}}"#,
    ] {
        feed(&mut driver, &seq, &sink, json);
    }
    let state = &driver.session.settings.quick_replies;
    assert_eq!((state.shortcut_limit(), state.message_limit()), (5, 3));
}
