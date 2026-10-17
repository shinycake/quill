//! Connect-driver tests for the poll-option menu (`getPollOptionProperties`,
//! `deletePollOption`) and the "Sent by" row (`getMessageAuthor`).
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::ids::{ChatId, MessageId};
use crate::state::Audience;
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const POLL: &str = r#"{"@type":"updateNewMessage","message":{"id":120,"chat_id":7,"is_outgoing":false,"content":{"@type":"messagePoll","poll":{"@type":"poll","id":9100,"question":{"@type":"formattedText","text":"Drink?","entities":[]},"options":[{"@type":"pollOption","id":"a","text":{"@type":"formattedText","text":"Tea","entities":[]},"voter_count":1,"vote_percentage":100,"is_chosen":false}],"total_voter_count":1,"is_anonymous":true,"allows_multiple_answers":false,"allows_revoting":true,"is_closed":false,"type":{"@type":"pollTypeRegular"}},"description":{"@type":"formattedText","text":"","entities":[]},"can_add_option":true}}}"#;

fn last_json(recorder: &RecordingSender) -> Value {
    serde_json::from_str(&recorder.snapshot().last().cloned().expect("a request")).unwrap()
}

fn sent(recorder: &RecordingSender, kind: &str) -> Option<Value> {
    recorder
        .snapshot()
        .into_iter()
        .filter_map(|json| serde_json::from_str::<Value>(&json).ok())
        .find(|value| value["@type"] == kind)
}

fn ingest(
    driver: &mut ConnectDriver<Arc<RecordingSender>>,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
    json: &str,
) {
    driver
        .ingest(copy_and_parse(json, seq, sink).unwrap())
        .unwrap();
}

#[test]
fn deleting_an_option_waits_for_the_properties() {
    let (dir, mut driver, recorder, _sink, dyn_sink, seq) = poll_driver();
    ingest(&mut driver, &seq, &dyn_sink, POLL);
    let (chat, message) = (ChatId(7), MessageId(120));
    // Nothing was asked yet: no request leaves the client.
    let before = recorder.snapshot().len();
    assert_eq!(driver.delete_poll_option(chat, message, "a").unwrap(), None);
    assert_eq!(recorder.snapshot().len(), before);
    let extra = driver
        .fetch_poll_option_properties(chat, message, "a")
        .unwrap();
    let asked = last_json(&recorder);
    assert_eq!(asked["@type"], "getPollOptionProperties");
    assert_eq!(asked["poll_option_id"], "a");
    assert_eq!(asked["message_id"], 120);
    // Still unanswered.
    assert_eq!(driver.delete_poll_option(chat, message, "a").unwrap(), None);
    ingest(
        &mut driver,
        &seq,
        &dyn_sink,
        &format!(
            r#"{{"@type":"pollOptionProperties","@extra":"{}","can_be_deleted":false,"can_be_replied":true}}"#,
            extra.0
        ),
    );
    let menu = driver.session.messages.poll_option_menu.clone().unwrap();
    assert!(menu.properties.is_some_and(|p| p.can_be_replied));
    assert_eq!(driver.delete_poll_option(chat, message, "a").unwrap(), None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_allowed_delete_sends_the_request_and_a_failure_leaves_a_note() {
    let (dir, mut driver, recorder, _sink, dyn_sink, seq) = poll_driver();
    ingest(&mut driver, &seq, &dyn_sink, POLL);
    let (chat, message) = (ChatId(7), MessageId(120));
    let extra = driver
        .fetch_poll_option_properties(chat, message, "a")
        .unwrap();
    ingest(
        &mut driver,
        &seq,
        &dyn_sink,
        &format!(
            r#"{{"@type":"pollOptionProperties","@extra":"{}","can_be_deleted":true,"can_be_replied":true}}"#,
            extra.0
        ),
    );
    // Another option of the same poll is not what the answer was about.
    assert_eq!(driver.delete_poll_option(chat, message, "b").unwrap(), None);
    let delete = driver
        .delete_poll_option(chat, message, "a")
        .unwrap()
        .expect("sent");
    let request = last_json(&recorder);
    assert_eq!(request["@type"], "deletePollOption");
    assert_eq!(request["option_id"], "a");
    assert_eq!(request["chat_id"], 7);
    assert_eq!(request["message_id"], 120);
    ingest(
        &mut driver,
        &seq,
        &dyn_sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"POLL_OPTION_INVALID"}}"#,
            delete.0
        ),
    );
    assert!(driver.session.messages.message_action_note.is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_author_is_asked_only_when_the_message_allows_it() {
    let (dir, mut driver, recorder, _sink, dyn_sink, seq) = poll_driver();
    ingest(&mut driver, &seq, &dyn_sink, POLL);
    let (chat, message) = (ChatId(7), MessageId(120));
    let extra = driver.fetch_message_menu_actions(chat, message).unwrap();
    ingest(
        &mut driver,
        &seq,
        &dyn_sink,
        &format!(r#"{{"@type":"messageProperties","@extra":"{}"}}"#, extra.0),
    );
    assert!(sent(&recorder, "getMessageAuthor").is_none());
    let extra = driver.fetch_message_menu_actions(chat, message).unwrap();
    ingest(
        &mut driver,
        &seq,
        &dyn_sink,
        &format!(
            r#"{{"@type":"messageProperties","@extra":"{}","can_get_author":true}}"#,
            extra.0
        ),
    );
    let asked = sent(&recorder, "getMessageAuthor").expect("getMessageAuthor was sent");
    assert_eq!(asked["message_id"], 120);
    let audience = driver.session.messages.message_audience.clone().unwrap();
    assert!(audience.author.is_loading());
    let author_extra = asked["@extra"].as_str().unwrap().to_string();
    ingest(
        &mut driver,
        &seq,
        &dyn_sink,
        &format!(
            r#"{{"@type":"user","@extra":"{author_extra}","id":900,"first_name":"Nora","last_name":"Vale","type":{{"@type":"userTypeRegular"}},"status":{{"@type":"userStatusOffline","was_online":1}}}}"#
        ),
    );
    let audience = driver.session.messages.message_audience.clone().unwrap();
    assert_eq!(audience.author, Audience::Ready(900));
    assert_eq!(
        driver.session.user(900).map(|u| u.display_name()),
        Some("Nora Vale".to_string())
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_author_lookup_shows_nothing() {
    let (dir, mut driver, recorder, _sink, dyn_sink, seq) = poll_driver();
    ingest(&mut driver, &seq, &dyn_sink, POLL);
    let (chat, message) = (ChatId(7), MessageId(120));
    let extra = driver.fetch_message_menu_actions(chat, message).unwrap();
    ingest(
        &mut driver,
        &seq,
        &dyn_sink,
        &format!(
            r#"{{"@type":"messageProperties","@extra":"{}","can_get_author":true}}"#,
            extra.0
        ),
    );
    let asked = sent(&recorder, "getMessageAuthor").expect("getMessageAuthor was sent");
    ingest(
        &mut driver,
        &seq,
        &dyn_sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"MESSAGE_AUTHOR_UNAVAILABLE"}}"#,
            asked["@extra"].as_str().unwrap()
        ),
    );
    let audience = driver.session.messages.message_audience.clone().unwrap();
    assert_eq!(audience.author, Audience::Failed);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_reply_to_an_option_survives_in_the_draft() {
    use crate::composer::ReplyItem;
    use crate::telegram::SendReply;
    let (dir, mut driver, recorder, _sink, dyn_sink, seq) = poll_driver();
    ingest(&mut driver, &seq, &dyn_sink, POLL);
    driver.select_chat(ChatId(7)).unwrap();
    let reply = SendReply {
        message_id: MessageId(120),
        quote: None,
        source_chat: None,
        item: ReplyItem::PollOption("a".to_string()),
    };
    driver
        .note_composer_draft(ChatId(7), "tea please", Some(reply), 1_000, false)
        .unwrap();
    let draft = sent(&recorder, "setChatDraftMessage").expect("the draft was saved");
    assert_eq!(
        draft["draft_message"]["reply_to"]["poll_option_id"], "a",
        "{draft}"
    );
    assert_eq!(draft["draft_message"]["reply_to"]["checklist_task_id"], 0);
    let saved = driver.session.chats.get(&7).unwrap().draft.clone().unwrap();
    assert_eq!(saved.item, ReplyItem::PollOption("a".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

fn answer(extra: crate::ids::RequestId, deletable: bool, link: bool) -> String {
    format!(
        r#"{{"@type":"pollOptionProperties","@extra":"{}","can_be_deleted":{deletable},"can_be_replied":true,"can_get_link":{link}}}"#,
        extra.0
    )
}

#[test]
fn a_late_answer_for_another_option_is_dropped() {
    let (dir, mut driver, recorder, _sink, dyn_sink, seq) = poll_driver();
    ingest(&mut driver, &seq, &dyn_sink, POLL);
    let (chat, message) = (ChatId(7), MessageId(120));
    // Two quick right-clicks on different options of one poll.
    let for_a = driver
        .fetch_poll_option_properties(chat, message, "a")
        .unwrap();
    let for_b = driver
        .fetch_poll_option_properties(chat, message, "b")
        .unwrap();
    // A's answer arrives last-but-one: it allows deleting A. The menu is on B.
    ingest(&mut driver, &seq, &dyn_sink, &answer(for_a, true, true));
    let menu = driver.session.messages.poll_option_menu.clone().unwrap();
    assert_eq!(menu.option_id, "b");
    assert_eq!(menu.properties, None, "A's answer is not B's");
    let before = recorder.snapshot().len();
    assert_eq!(driver.delete_poll_option(chat, message, "b").unwrap(), None);
    assert_eq!(driver.delete_poll_option(chat, message, "a").unwrap(), None);
    assert_eq!(
        driver
            .get_item_link(
                chat,
                message,
                &crate::composer::ReplyItem::PollOption("b".into())
            )
            .unwrap(),
        None
    );
    assert_eq!(recorder.snapshot().len(), before);
    // B's own answer says no.
    ingest(&mut driver, &seq, &dyn_sink, &answer(for_b, false, false));
    assert_eq!(driver.delete_poll_option(chat, message, "b").unwrap(), None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_option_link_is_asked_only_when_allowed() {
    use crate::composer::ReplyItem;
    let (dir, mut driver, recorder, _sink, dyn_sink, seq) = poll_driver();
    ingest(&mut driver, &seq, &dyn_sink, POLL);
    let (chat, message) = (ChatId(7), MessageId(120));
    let item = ReplyItem::PollOption("a".into());
    let extra = driver
        .fetch_poll_option_properties(chat, message, "a")
        .unwrap();
    ingest(&mut driver, &seq, &dyn_sink, &answer(extra, false, true));
    driver
        .get_item_link(chat, message, &item)
        .unwrap()
        .expect("sent");
    let request = last_json(&recorder);
    assert_eq!(request["@type"], "getMessageLink");
    assert_eq!(request["poll_option_id"], "a");
    let _ = std::fs::remove_dir_all(&dir);
}
