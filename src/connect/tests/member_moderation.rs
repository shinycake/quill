//! Connect-driver tests (recorded TDLib JSON, nothing live): removing
//! members, reaction deletion, restrict-instead moderation, ownership
//! transfer and the "who inherits" lookup.
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::ids::{ChatId, MessageId};
use crate::platform::MemorySecretStore;
use crate::state::OwnerLookup;
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::{CanTransferOwnershipResult, MessageSender};
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

type Recorder = Arc<RecordingSender>;

fn group_driver() -> (
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

fn answer(
    driver: &mut ConnectDriver<Recorder>,
    recorder: &RecordingSender,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
    type_name: &str,
    body: &str,
) {
    let request = sent_request(recorder, type_name);
    let extra = request["@extra"].as_str().unwrap().to_string();
    let mut value: Value = serde_json::from_str(body).unwrap();
    value["@extra"] = Value::String(extra);
    feed(driver, seq, sink, &value.to_string());
}

fn count_sent(recorder: &RecordingSender, type_name: &str) -> usize {
    let needle = format!("\"@type\":\"{type_name}\"");
    recorder
        .snapshot()
        .iter()
        .filter(|json| json.contains(&needle))
        .count()
}

const GROUP_CHAT: &str = r#"{"@type":"updateNewChat","chat":{"id":-100,"title":"Group","type":{"@type":"chatTypeSupergroup","supergroup_id":100,"is_channel":false},"unread_count":0}}"#;
const BASIC_CHAT: &str = r#"{"@type":"updateNewChat","chat":{"id":-5,"title":"Small","type":{"@type":"chatTypeBasicGroup","basic_group_id":5},"unread_count":0}}"#;
const MESSAGE: &str = r#"{"@type":"updateNewMessage","message":{"id":50,"chat_id":-100,"sender_id":{"@type":"messageSenderUser","user_id":8},"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hello","entities":[]}}}}"#;

fn supergroup_status(status: &str) -> String {
    format!(
        r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":100,"status":{status}}}}}"#
    )
}

const ADMIN: &str = r#"{"@type":"chatMemberStatusAdministrator","rights":{"@type":"chatAdministratorRights","can_restrict_members":true,"can_delete_messages":true,"can_promote_members":true}}"#;
const CREATOR: &str = r#"{"@type":"chatMemberStatusCreator","is_member":true}"#;

#[test]
fn removing_from_a_supergroup_bans_and_nothing_more() {
    let (mut driver, recorder, sink, seq) = group_driver();
    feed(&mut driver, &seq, &sink, GROUP_CHAT);
    feed(&mut driver, &seq, &sink, &supergroup_status(ADMIN));
    driver
        .remove_chat_member(ChatId(-100), 8)
        .unwrap()
        .expect("sent");
    let ban = sent_request(&recorder, "setChatMemberStatus");
    assert_eq!(ban["status"]["@type"], "chatMemberStatusBanned");
    assert_eq!(ban["status"]["banned_until_date"], 0);
    assert_eq!(ban["member_id"]["user_id"], 8);
    answer(
        &mut driver,
        &recorder,
        &seq,
        &sink,
        "setChatMemberStatus",
        r#"{"@type":"ok"}"#,
    );
    // Like Telegram Desktop: the member stays banned, no unban follows.
    assert_eq!(count_sent(&recorder, "setChatMemberStatus"), 1);
}

#[test]
fn a_failed_ban_reports_the_error() {
    let (mut driver, recorder, sink, seq) = group_driver();
    feed(&mut driver, &seq, &sink, GROUP_CHAT);
    feed(&mut driver, &seq, &sink, &supergroup_status(ADMIN));
    driver.remove_chat_member(ChatId(-100), 8).unwrap();
    answer(
        &mut driver,
        &recorder,
        &seq,
        &sink,
        "setChatMemberStatus",
        r#"{"@type":"error","code":400,"message":"USER_ADMIN_INVALID"}"#,
    );
    assert_eq!(count_sent(&recorder, "setChatMemberStatus"), 1);
    assert!(
        driver
            .session
            .groups
            .member_action_error
            .contains_key(&-100)
    );
}

#[test]
fn removing_without_the_restrict_right_sends_nothing() {
    let (mut driver, recorder, sink, seq) = group_driver();
    feed(&mut driver, &seq, &sink, GROUP_CHAT);
    feed(
        &mut driver,
        &seq,
        &sink,
        &supergroup_status(r#"{"@type":"chatMemberStatusMember"}"#),
    );
    assert_eq!(driver.remove_chat_member(ChatId(-100), 8).unwrap(), None);
    assert_eq!(count_sent(&recorder, "setChatMemberStatus"), 0);
}

#[test]
fn removing_from_a_basic_group_uses_ban_chat_member() {
    let (mut driver, recorder, sink, seq) = group_driver();
    feed(&mut driver, &seq, &sink, BASIC_CHAT);
    // Not known as owner or admin yet: refused.
    assert_eq!(driver.remove_chat_member(ChatId(-5), 8).unwrap(), None);
    feed(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateBasicGroup","basic_group":{{"@type":"basicGroup","id":5,"member_count":3,"status":{CREATOR}}}}}"#
        ),
    );
    driver
        .remove_chat_member(ChatId(-5), 8)
        .unwrap()
        .expect("sent");
    let ban = sent_request(&recorder, "banChatMember");
    assert_eq!(ban["chat_id"], -5);
    assert_eq!(ban["member_id"]["user_id"], 8);
    assert_eq!(ban["banned_until_date"], 0);
    assert_eq!(count_sent(&recorder, "setChatMemberStatus"), 0);
    answer(
        &mut driver,
        &recorder,
        &seq,
        &sink,
        "banChatMember",
        r#"{"@type":"ok"}"#,
    );
    assert_eq!(count_sent(&recorder, "setChatMemberStatus"), 0);
}

#[test]
fn restrict_instead_leaves_the_member_view_only() {
    let (mut driver, recorder, sink, seq) = group_driver();
    feed(&mut driver, &seq, &sink, GROUP_CHAT);
    feed(&mut driver, &seq, &sink, &supergroup_status(ADMIN));
    driver
        .moderate_message(
            ChatId(-100),
            &[MessageId(50)],
            8,
            ModerationChoice {
                delete_all: true,
                ban: true,
                restrict_instead: true,
                ..ModerationChoice::default()
            },
        )
        .unwrap();
    let delete = sent_request(&recorder, "deleteChatMessagesBySender");
    assert_eq!(delete["sender_id"]["user_id"], 8);
    let status = sent_request(&recorder, "setChatMemberStatus");
    assert_eq!(status["status"]["@type"], "chatMemberStatusRestricted");
    assert_eq!(status["status"]["restricted_until_date"], 0);
    assert_eq!(
        status["status"]["permissions"]["can_send_basic_messages"],
        false
    );
}

#[test]
fn deleting_a_reaction_needs_the_message_to_allow_it() {
    let (mut driver, recorder, sink, seq) = group_driver();
    feed(&mut driver, &seq, &sink, GROUP_CHAT);
    feed(&mut driver, &seq, &sink, MESSAGE);
    // No properties yet: nothing goes out.
    let sender = MessageSender::User { user_id: 8 };
    assert_eq!(
        driver
            .delete_message_reactions_from(ChatId(-100), MessageId(50), sender)
            .unwrap(),
        None
    );
    driver
        .fetch_message_menu_actions(ChatId(-100), MessageId(50))
        .unwrap();
    answer(
        &mut driver,
        &recorder,
        &seq,
        &sink,
        "getMessageProperties",
        r#"{"@type":"messageProperties","can_delete_reactions":true}"#,
    );
    driver
        .delete_message_reactions_from(ChatId(-100), MessageId(50), sender)
        .unwrap()
        .expect("sent");
    let sent = sent_request(&recorder, "deleteMessageReactionsFromSender");
    assert_eq!(sent["message_id"], 50);
    assert_eq!(sent["sender_id"]["user_id"], 8);
    answer(
        &mut driver,
        &recorder,
        &seq,
        &sink,
        "deleteMessageReactionsFromSender",
        r#"{"@type":"ok"}"#,
    );
    assert_eq!(
        driver.session.messages.message_action_note.as_deref(),
        Some("reaction deleted")
    );
}

#[test]
fn a_reaction_cannot_be_deleted_when_the_message_forbids_it() {
    let (mut driver, recorder, sink, seq) = group_driver();
    feed(&mut driver, &seq, &sink, GROUP_CHAT);
    feed(&mut driver, &seq, &sink, MESSAGE);
    driver
        .fetch_message_menu_actions(ChatId(-100), MessageId(50))
        .unwrap();
    answer(
        &mut driver,
        &recorder,
        &seq,
        &sink,
        "getMessageProperties",
        r#"{"@type":"messageProperties","can_delete_reactions":false}"#,
    );
    assert_eq!(
        driver
            .delete_message_reactions_from(
                ChatId(-100),
                MessageId(50),
                MessageSender::User { user_id: 8 }
            )
            .unwrap(),
        None
    );
    assert_eq!(count_sent(&recorder, "deleteMessageReactionsFromSender"), 0);
}

fn owner_driver() -> (
    ConnectDriver<Recorder>,
    Recorder,
    Arc<dyn DiagnosticSink>,
    AtomicU64,
) {
    let (mut driver, recorder, sink, seq) = group_driver();
    feed(&mut driver, &seq, &sink, GROUP_CHAT);
    feed(&mut driver, &seq, &sink, &supergroup_status(CREATOR));
    feed(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateUser","user":{"id":8,"first_name":"Bo","last_name":"","type":{"@type":"userTypeRegular"}}}"#,
    );
    (driver, recorder, sink, seq)
}

#[test]
fn transfer_waits_for_the_security_check_and_never_stores_the_password() {
    let (mut driver, recorder, sink, seq) = owner_driver();
    // Before the check: refused, nothing sent.
    assert_eq!(
        driver
            .transfer_chat_ownership(ChatId(-100), 8, "hunter2")
            .unwrap(),
        None
    );
    assert_eq!(count_sent(&recorder, "transferChatOwnership"), 0);

    driver
        .check_can_transfer_ownership()
        .unwrap()
        .expect("sent");
    assert!(driver.session.groups.ownership.check_in_flight);
    answer(
        &mut driver,
        &recorder,
        &seq,
        &sink,
        "canTransferOwnership",
        r#"{"@type":"canTransferOwnershipResultPasswordTooFresh","retry_after":86400}"#,
    );
    assert_eq!(
        driver.session.groups.ownership.can_transfer,
        Some(CanTransferOwnershipResult::PasswordTooFresh { retry_after: 86400 })
    );
    assert_eq!(
        driver
            .transfer_chat_ownership(ChatId(-100), 8, "hunter2")
            .unwrap(),
        None
    );

    driver
        .check_can_transfer_ownership()
        .unwrap()
        .expect("sent");
    answer(
        &mut driver,
        &recorder,
        &seq,
        &sink,
        "canTransferOwnership",
        r#"{"@type":"canTransferOwnershipResultOk"}"#,
    );
    // Empty password, yourself and a missing chat are refused.
    assert_eq!(
        driver.transfer_chat_ownership(ChatId(-100), 8, "").unwrap(),
        None
    );
    driver
        .transfer_chat_ownership(ChatId(-100), 8, "hunter2")
        .unwrap()
        .expect("sent");
    let sent = sent_request(&recorder, "transferChatOwnership");
    assert_eq!(sent["user_id"], 8);
    assert_eq!(sent["password"], "hunter2");
    // The password lives only in the request JSON.
    assert!(!format!("{:?}", driver.session.groups.ownership).contains("hunter2"));
    // A second tap while waiting sends nothing.
    assert_eq!(
        driver
            .transfer_chat_ownership(ChatId(-100), 8, "hunter2")
            .unwrap(),
        None
    );

    answer(
        &mut driver,
        &recorder,
        &seq,
        &sink,
        "transferChatOwnership",
        r#"{"@type":"error","code":400,"message":"PASSWORD_HASH_INVALID"}"#,
    );
    assert!(driver.session.groups.ownership.transfer_in_flight.is_none());
    let error = driver
        .session
        .groups
        .ownership
        .transfer_error
        .clone()
        .unwrap();
    assert!(error.contains("Wrong password"), "{error}");
    assert!(!error.contains("PASSWORD_HASH_INVALID"));
    assert!(!format!("{:?}", driver.session.groups.ownership).contains("hunter2"));

    driver
        .transfer_chat_ownership(ChatId(-100), 8, "correct horse")
        .unwrap()
        .expect("retry");
    assert!(driver.session.groups.ownership.transfer_error.is_none());
    answer(
        &mut driver,
        &recorder,
        &seq,
        &sink,
        "transferChatOwnership",
        r#"{"@type":"ok"}"#,
    );
    let done = driver.session.groups.ownership.transferred.expect("done");
    assert_eq!((done.chat_id, done.user_id), (-100, 8));
}

#[test]
fn only_the_owner_may_transfer() {
    let (mut driver, recorder, sink, seq) = group_driver();
    feed(&mut driver, &seq, &sink, GROUP_CHAT);
    feed(&mut driver, &seq, &sink, &supergroup_status(ADMIN));
    driver.check_can_transfer_ownership().unwrap();
    answer(
        &mut driver,
        &recorder,
        &seq,
        &sink,
        "canTransferOwnership",
        r#"{"@type":"canTransferOwnershipResultOk"}"#,
    );
    assert_eq!(
        driver
            .transfer_chat_ownership(ChatId(-100), 8, "pw")
            .unwrap(),
        None
    );
    assert_eq!(
        driver.fetch_chat_owner_after_leaving(ChatId(-100)).unwrap(),
        None
    );
    assert_eq!(count_sent(&recorder, "transferChatOwnership"), 0);
    assert_eq!(count_sent(&recorder, "getChatOwnerAfterLeaving"), 0);
}

#[test]
fn the_next_owner_is_looked_up_once() {
    let (mut driver, recorder, sink, seq) = owner_driver();
    driver
        .fetch_chat_owner_after_leaving(ChatId(-100))
        .unwrap()
        .expect("sent");
    assert_eq!(
        driver
            .session
            .groups
            .ownership
            .owner_after_leaving
            .get(&-100),
        Some(&OwnerLookup::Loading)
    );
    assert_eq!(
        driver.fetch_chat_owner_after_leaving(ChatId(-100)).unwrap(),
        None
    );
    answer(
        &mut driver,
        &recorder,
        &seq,
        &sink,
        "getChatOwnerAfterLeaving",
        r#"{"@type":"user","id":8,"first_name":"Bo"}"#,
    );
    assert_eq!(
        driver
            .session
            .groups
            .ownership
            .owner_after_leaving
            .get(&-100),
        Some(&OwnerLookup::Loaded(8))
    );
}

#[test]
fn a_failed_owner_lookup_is_reported() {
    let (mut driver, recorder, sink, seq) = owner_driver();
    driver.fetch_chat_owner_after_leaving(ChatId(-100)).unwrap();
    answer(
        &mut driver,
        &recorder,
        &seq,
        &sink,
        "getChatOwnerAfterLeaving",
        r#"{"@type":"error","code":400,"message":"CHAT_ADMIN_REQUIRED"}"#,
    );
    assert!(matches!(
        driver
            .session
            .groups
            .ownership
            .owner_after_leaving
            .get(&-100),
        Some(OwnerLookup::Failed(_))
    ));
}
