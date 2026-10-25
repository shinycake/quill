use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::ids::{ChatId, MessageId};
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

type Fixture = (
    std::path::PathBuf,
    ConnectDriver<Arc<RecordingSender>>,
    Arc<RecordingSender>,
    Arc<dyn DiagnosticSink>,
    AtomicU64,
);

const CHAT: ChatId = ChatId(31);
const REQUEST: MessageId = MessageId(7_340_032);

fn fixture() -> Fixture {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let driver = ready_driver(&recorder, prepared, &sink, &seq);
    (dir, driver, recorder, sink, seq)
}

fn cleanup(f: Fixture) {
    let (dir, driver, ..) = f;
    drop(driver);
    std::fs::remove_dir_all(dir).unwrap();
}

fn ingest(f: &mut Fixture, json: &str) {
    let owned = copy_and_parse(json, &f.4, &f.3).unwrap();
    f.1.ingest(owned).unwrap();
}

fn sent(f: &Fixture) -> Vec<Value> {
    f.2.snapshot()
        .iter()
        .map(|json| serde_json::from_str(json).unwrap())
        .collect()
}

fn last_request(f: &Fixture) -> Value {
    sent(f).pop().unwrap()
}

fn message(id: i64, outgoing: bool, content: &str) -> String {
    format!(
        r#"{{"@type":"message","id":{id},"chat_id":31,"is_outgoing":{outgoing},"sender_id":{{"@type":"messageSenderUser","user_id":{sender}}},"date":1700000000,"content":{content}}}"#,
        sender = if outgoing { 1 } else { 31 },
    )
}

fn request(expired: bool) -> String {
    format!(
        r#"{{"@type":"messageChatHasProtectedContentDisableRequested","is_expired":{expired}}}"#
    )
}

fn protected_chat() -> Fixture {
    let mut f = fixture();
    ingest(
        &mut f,
        r#"{"@type":"updateUser","user":{"id":31,"first_name":"Ada","type":{"@type":"userTypeRegular"}}}"#,
    );
    ingest(
        &mut f,
        r#"{"@type":"updateNewChat","chat":{"id":31,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":31},"unread_count":0,"has_protected_content":true}}"#,
    );
    let toggle = message(
        1_048_576,
        false,
        r#"{"@type":"messageChatHasProtectedContentToggled","request_message_id":0,"old_has_protected_content":false,"new_has_protected_content":true}"#,
    );
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"updateChatLastMessage","chat_id":31,"last_message":{toggle},"positions":[]}}"#
        ),
    );
    f
}

fn answer_properties(f: &mut Fixture, extra: &Value, by_other_user: bool) {
    ingest(
        f,
        &format!(
            r#"{{"@type":"messageProperties","@extra":{extra},"has_protected_content_by_other_user":{by_other_user},"has_protected_content_by_current_user":{mine}}}"#,
            mine = !by_other_user,
        ),
    );
}

#[test]
fn enabling_sharing_you_disabled_turns_protection_off_after_tdlib_answers() {
    let mut f = protected_chat();
    f.1.enable_private_sharing(CHAT).unwrap().unwrap();
    let ask = last_request(&f);
    assert_eq!(ask["@type"], "getMessageProperties");
    assert_eq!(ask["chat_id"], 31);
    assert_eq!(ask["message_id"], 1_048_576);
    let count = sent(&f).len();
    assert_eq!(f.1.enable_private_sharing(CHAT).unwrap(), None);
    assert_eq!(sent(&f).len(), count);

    answer_properties(&mut f, &ask["@extra"], false);
    let toggle = last_request(&f);
    assert_eq!(toggle["@type"], "toggleChatHasProtectedContent");
    assert_eq!(toggle["has_protected_content"], false);
    assert_eq!(f.1.session.groups.private_sharing.request_box_chat(), None);
    assert_eq!(f.1.session.private_sharing_disabled(CHAT), Some(true));
    ingest(
        &mut f,
        r#"{"@type":"updateChatHasProtectedContent","chat_id":31,"has_protected_content":false}"#,
    );
    assert_eq!(f.1.session.private_sharing_disabled(CHAT), Some(false));
    cleanup(f);
}

#[test]
fn enabling_sharing_the_other_person_disabled_asks_before_sending_the_request() {
    let mut f = protected_chat();
    f.1.enable_private_sharing(CHAT).unwrap().unwrap();
    let ask = last_request(&f);
    let count = sent(&f).len();
    answer_properties(&mut f, &ask["@extra"], true);
    assert_eq!(sent(&f).len(), count, "nothing is sent before the box");
    assert_eq!(
        f.1.session.groups.private_sharing.request_box_chat(),
        Some(CHAT)
    );

    f.1.send_sharing_request().unwrap().unwrap();
    let toggle = last_request(&f);
    assert_eq!(toggle["@type"], "toggleChatHasProtectedContent");
    assert_eq!(toggle["chat_id"], 31);
    assert_eq!(toggle["has_protected_content"], false);
    assert_eq!(f.1.session.groups.private_sharing.request_box_chat(), None);
    assert_eq!(f.1.session.private_sharing_disabled(CHAT), Some(true));
    cleanup(f);
}

#[test]
fn closing_the_request_box_sends_nothing() {
    let mut f = protected_chat();
    f.1.enable_private_sharing(CHAT).unwrap().unwrap();
    let ask = last_request(&f);
    answer_properties(&mut f, &ask["@extra"], true);
    let count = sent(&f).len();
    f.1.close_sharing_request_box();
    assert_eq!(f.1.send_sharing_request().unwrap(), None);
    assert_eq!(sent(&f).len(), count);
    cleanup(f);
}

#[test]
fn a_failed_check_lets_tdlib_decide() {
    let mut f = protected_chat();
    f.1.enable_private_sharing(CHAT).unwrap().unwrap();
    let ask = last_request(&f);
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"error","@extra":{},"code":400,"message":"MESSAGE_NOT_FOUND"}}"#,
            ask["@extra"]
        ),
    );
    let toggle = last_request(&f);
    assert_eq!(toggle["@type"], "toggleChatHasProtectedContent");
    assert_eq!(toggle["has_protected_content"], false);
    cleanup(f);
}

#[test]
fn enabling_sharing_soon_after_disabling_it_here_skips_the_check() {
    let mut f = fixture();
    ingest(
        &mut f,
        r#"{"@type":"updateUser","user":{"id":31,"first_name":"Ada","type":{"@type":"userTypeRegular"}}}"#,
    );
    ingest(
        &mut f,
        r#"{"@type":"updateNewChat","chat":{"id":31,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":31},"unread_count":0}}"#,
    );
    let disable = f.1.set_chat_protected_content(CHAT, true).unwrap().unwrap();
    ingest(
        &mut f,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, disable.as_extra()),
    );
    let last = message(
        2_097_152,
        true,
        r#"{"@type":"messageChatHasProtectedContentToggled","request_message_id":0,"old_has_protected_content":false,"new_has_protected_content":true}"#,
    );
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"updateChatLastMessage","chat_id":31,"last_message":{last},"positions":[]}}"#
        ),
    );
    f.1.enable_private_sharing(CHAT).unwrap().unwrap();
    let toggle = last_request(&f);
    assert_eq!(toggle["@type"], "toggleChatHasProtectedContent");
    assert_eq!(toggle["has_protected_content"], false);
    assert!(
        !sent(&f)
            .iter()
            .any(|request| request["@type"] == "getMessageProperties")
    );
    cleanup(f);
}

#[test]
fn accepting_a_request_names_it_once_and_a_refusal_frees_the_buttons() {
    let mut f = protected_chat();
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"updateNewMessage","message":{}}}"#,
            message(REQUEST.0, false, &request(false))
        ),
    );
    f.1.answer_sharing_request(CHAT, REQUEST, true)
        .unwrap()
        .unwrap();
    let accept = last_request(&f);
    assert_eq!(
        accept["@type"],
        "processChatHasProtectedContentDisableRequest"
    );
    assert_eq!(accept["chat_id"], 31);
    assert_eq!(accept["request_message_id"], REQUEST.0);
    assert_eq!(accept["approve"], true);
    assert!(f.1.session.sharing_request_answering(CHAT, REQUEST));
    assert_eq!(
        f.1.answer_sharing_request(CHAT, REQUEST, false).unwrap(),
        None
    );

    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"error","@extra":{},"code":400,"message":"MESSAGE_ID_INVALID"}}"#,
            accept["@extra"]
        ),
    );
    assert!(f.1.session.chats_state.chat_action_error.is_some());
    f.1.answer_sharing_request(CHAT, REQUEST, false)
        .unwrap()
        .unwrap();
    assert_eq!(last_request(&f)["approve"], false);
    cleanup(f);
}

#[test]
fn only_an_incoming_live_request_can_be_answered() {
    let mut f = protected_chat();
    let mine = MessageId(REQUEST.0 + 1_048_576);
    let expired = MessageId(REQUEST.0 + 2_097_152);
    for (id, outgoing, is_expired) in [(mine, true, false), (expired, false, true)] {
        ingest(
            &mut f,
            &format!(
                r#"{{"@type":"updateNewMessage","message":{}}}"#,
                message(id.0, outgoing, &request(is_expired))
            ),
        );
    }
    let count = sent(&f).len();
    assert_eq!(f.1.answer_sharing_request(CHAT, mine, true).unwrap(), None);
    assert_eq!(
        f.1.answer_sharing_request(CHAT, expired, true).unwrap(),
        None
    );
    assert_eq!(
        f.1.answer_sharing_request(CHAT, REQUEST, true).unwrap(),
        None
    );
    assert_eq!(sent(&f).len(), count);
    cleanup(f);
}

#[test]
fn an_answered_request_keeps_its_card_once_it_expires() {
    let mut f = protected_chat();
    let new_message = |id: i64, outgoing: bool, content: &str| {
        format!(
            r#"{{"@type":"updateNewMessage","message":{}}}"#,
            message(id, outgoing, content)
        )
    };
    ingest(&mut f, &new_message(REQUEST.0, false, &request(false)));
    let view = |f: &Fixture| {
        let session = &f.1.session;
        session
            .histories
            .get(&CHAT.0)
            .and_then(|history| history.messages.get(&REQUEST.0))
            .and_then(|message| session.sharing_request_view(message))
    };
    let live = view(&f).unwrap();
    assert!(live.card && live.buttons);

    let expire = format!(
        r#"{{"@type":"updateMessageContent","chat_id":31,"message_id":{},"new_content":{}}}"#,
        REQUEST.0,
        request(true)
    );
    ingest(&mut f, &expire);
    let unanswered = view(&f).unwrap();
    assert!(!unanswered.card && !unanswered.buttons);

    ingest(
        &mut f,
        &new_message(
            REQUEST.0 + 1_048_576,
            true,
            &format!(
                r#"{{"@type":"messageChatHasProtectedContentToggled","request_message_id":{},"old_has_protected_content":true,"new_has_protected_content":true}}"#,
                REQUEST.0
            ),
        ),
    );
    let answered = view(&f).unwrap();
    assert!(answered.card && !answered.buttons);
    cleanup(f);
}
