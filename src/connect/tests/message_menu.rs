//! Connect-driver tests (recorded TDLib JSON, nothing live): the message
//! menu's Report flow, seen / reacted lists and admin moderation.
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::ids::{ChatId, MessageId};
use crate::platform::MemorySecretStore;
use crate::state::{Audience, MessageReportStage};
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::MessageReadDate;
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

/// Answer the most recent request of `type_name` with `body` (an object
/// without `@extra`).
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

const GROUP_CHAT: &str = r#"{"@type":"updateNewChat","chat":{"id":-100,"title":"Group","type":{"@type":"chatTypeSupergroup","supergroup_id":100,"is_channel":false},"unread_count":0}}"#;
const REACTED_MESSAGE: &str = r#"{"@type":"updateNewMessage","message":{"id":50,"chat_id":-100,"sender_id":{"@type":"messageSenderUser","user_id":8},"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hello","entities":[]}},"interaction_info":{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":null,"reactions":{"@type":"messageReactions","reactions":[{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"👍"},"total_count":2,"is_chosen":false,"used_sender_id":null,"recent_sender_ids":[]}],"are_tags":false,"paid_reactors":[],"can_get_added_reactions":true}}}}"#;

#[test]
fn menu_properties_chain_the_viewer_and_reactor_lookups() {
    let (mut driver, recorder, sink, seq) = group_driver();
    feed(&mut driver, &seq, &sink, GROUP_CHAT);
    feed(&mut driver, &seq, &sink, REACTED_MESSAGE);

    driver
        .fetch_message_menu_actions(ChatId(-100), MessageId(50))
        .unwrap();
    answer(
        &mut driver,
        &recorder,
        &seq,
        &sink,
        "getMessageProperties",
        r#"{"@type":"messageProperties","can_get_viewers":true,"can_get_read_date":false,"can_report_chat":true}"#,
    );
    let viewers = sent_request(&recorder, "getMessageViewers");
    assert_eq!(viewers["chat_id"], -100);
    assert_eq!(viewers["message_id"], 50);
    let reactions = sent_request(&recorder, "getMessageAddedReactions");
    assert!(reactions["reaction_type"].is_null());
    // Not a private chat, so no read-date lookup.
    assert!(
        !recorder
            .snapshot()
            .iter()
            .any(|json| json.contains("getMessageReadDate"))
    );
    let actions = driver.session.message_menu_actions.expect("properties").2;
    assert!(actions.can_report_chat && actions.can_get_viewers);

    answer(
        &mut driver,
        &recorder,
        &seq,
        &sink,
        "getMessageViewers",
        r#"{"@type":"messageViewers","viewers":[{"@type":"messageViewer","user_id":8,"view_date":1700000100},{"@type":"messageViewer","user_id":9,"view_date":1700000200}]}"#,
    );
    answer(
        &mut driver,
        &recorder,
        &seq,
        &sink,
        "getMessageAddedReactions",
        r#"{"@type":"addedReactions","total_count":2,"reactions":[{"@type":"addedReaction","type":{"@type":"reactionTypeEmoji","emoji":"👍"},"sender_id":{"@type":"messageSenderUser","user_id":9},"is_outgoing":false,"date":1700000300}],"next_offset":""}"#,
    );
    let audience = driver.session.message_audience.as_ref().expect("audience");
    assert_eq!(audience.viewers.ready().map(Vec::len), Some(2));
    let page = audience.reactions.ready().expect("reactions");
    assert_eq!(page.total_count, 2);
    assert_eq!(page.reactions[0].date, 1_700_000_300);
}

#[test]
fn private_chat_asks_the_read_date_and_keeps_privacy_answers() {
    let (mut driver, recorder, sink, seq) = group_driver();
    feed(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
    );
    feed(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":60,"chat_id":7,"is_outgoing":true,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#,
    );
    driver
        .fetch_message_menu_actions(ChatId(7), MessageId(60))
        .unwrap();
    answer(
        &mut driver,
        &recorder,
        &seq,
        &sink,
        "getMessageProperties",
        r#"{"@type":"messageProperties","can_get_viewers":true,"can_get_read_date":true}"#,
    );
    // A private chat reads the date, never the group viewer list.
    assert!(
        !recorder
            .snapshot()
            .iter()
            .any(|json| json.contains("getMessageViewers"))
    );
    answer(
        &mut driver,
        &recorder,
        &seq,
        &sink,
        "getMessageReadDate",
        r#"{"@type":"messageReadDateUserPrivacyRestricted"}"#,
    );
    let audience = driver.session.message_audience.as_ref().unwrap();
    assert_eq!(
        audience.read_date,
        Audience::Ready(MessageReadDate::UserPrivacyRestricted)
    );
}

#[test]
fn a_refused_lookup_is_failed_not_loading_forever() {
    let (mut driver, recorder, sink, seq) = group_driver();
    feed(&mut driver, &seq, &sink, GROUP_CHAT);
    feed(&mut driver, &seq, &sink, REACTED_MESSAGE);
    driver
        .fetch_message_menu_actions(ChatId(-100), MessageId(50))
        .unwrap();
    answer(
        &mut driver,
        &recorder,
        &seq,
        &sink,
        "getMessageProperties",
        r#"{"@type":"messageProperties","can_get_viewers":true}"#,
    );
    answer(
        &mut driver,
        &recorder,
        &seq,
        &sink,
        "getMessageViewers",
        r#"{"@type":"error","code":400,"message":"MESSAGE_TOO_OLD"}"#,
    );
    let audience = driver.session.message_audience.as_ref().unwrap();
    assert_eq!(audience.viewers, Audience::Failed);
}

#[test]
fn report_flow_walks_reason_details_and_done() {
    let (mut driver, recorder, sink, seq) = group_driver();
    feed(&mut driver, &seq, &sink, GROUP_CHAT);
    feed(&mut driver, &seq, &sink, REACTED_MESSAGE);

    driver
        .report_messages(ChatId(-100), &[MessageId(50)], "", "", None)
        .unwrap();
    let first = sent_request(&recorder, "reportChat");
    assert_eq!(first["option_id"], "");
    assert_eq!(first["message_ids"], serde_json::json!([50]));
    assert_eq!(
        driver.session.message_report.as_ref().unwrap().stage,
        MessageReportStage::Checking
    );

    answer(
        &mut driver,
        &recorder,
        &seq,
        &sink,
        "reportChat",
        r#"{"@type":"reportChatResultOptionRequired","title":"Why?","options":[{"@type":"reportOption","id":"c3BhbQ==","text":"Spam"},{"@type":"reportOption","id":"b3RoZXI=","text":"Other"}]}"#,
    );
    let options = match &driver.session.message_report.as_ref().unwrap().stage {
        MessageReportStage::PickOption { title, options } => {
            assert_eq!(title, "Why?");
            options.clone()
        }
        other => panic!("{other:?}"),
    };
    assert_eq!(options.len(), 2);

    // Choosing "Other" echoes its id and asks for details.
    driver
        .report_messages(
            ChatId(-100),
            &[MessageId(50)],
            &options[1].id,
            "",
            Some((options[1].text.clone(), "Why?".into(), options.clone())),
        )
        .unwrap();
    let second = sent_request(&recorder, "reportChat");
    assert_eq!(second["option_id"], "b3RoZXI=");
    assert_eq!(
        driver.session.message_report.as_ref().unwrap().stage,
        MessageReportStage::Sending
    );
    answer(
        &mut driver,
        &recorder,
        &seq,
        &sink,
        "reportChat",
        r#"{"@type":"reportChatResultTextRequired","option_id":"b3RoZXI=","is_optional":false}"#,
    );
    assert_eq!(
        driver.session.message_report.as_ref().unwrap().stage,
        MessageReportStage::TextRequired {
            option_id: "b3RoZXI=".into(),
            is_optional: false
        }
    );
    // Back returns to the reasons.
    assert!(driver.session.message_report_back());
    assert!(matches!(
        driver.session.message_report.as_ref().unwrap().stage,
        MessageReportStage::PickOption { .. }
    ));

    driver
        .report_messages(
            ChatId(-100),
            &[MessageId(50)],
            "b3RoZXI=",
            "scam links",
            None,
        )
        .unwrap();
    let third = sent_request(&recorder, "reportChat");
    assert_eq!(third["text"], "scam links");
    answer(
        &mut driver,
        &recorder,
        &seq,
        &sink,
        "reportChat",
        r#"{"@type":"reportChatResultOk"}"#,
    );
    assert_eq!(
        driver.session.message_report.as_ref().unwrap().stage,
        MessageReportStage::Reported
    );
}

#[test]
fn report_error_ends_the_flow_instead_of_spinning() {
    let (mut driver, recorder, sink, seq) = group_driver();
    feed(&mut driver, &seq, &sink, GROUP_CHAT);
    driver
        .report_messages(ChatId(-100), &[MessageId(50)], "", "", None)
        .unwrap();
    answer(
        &mut driver,
        &recorder,
        &seq,
        &sink,
        "reportChat",
        r#"{"@type":"error","code":400,"message":"MESSAGE_ID_INVALID"}"#,
    );
    assert!(matches!(
        driver.session.message_report.as_ref().unwrap().stage,
        MessageReportStage::Failed(_)
    ));
}

#[test]
fn admin_moderation_sends_each_checked_action() {
    let (mut driver, recorder, sink, seq) = group_driver();
    feed(&mut driver, &seq, &sink, GROUP_CHAT);
    feed(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":100,"status":{"@type":"chatMemberStatusAdministrator","rights":{"@type":"chatAdministratorRights","can_restrict_members":true,"can_delete_messages":true}}}}"#,
    );
    driver
        .moderate_message(
            ChatId(-100),
            &[MessageId(50)],
            8,
            ModerationChoice {
                report_spam: true,
                delete_all: true,
                ban: true,
                ..ModerationChoice::default()
            },
        )
        .unwrap();
    let spam = sent_request(&recorder, "reportSupergroupSpam");
    assert_eq!(spam["supergroup_id"], 100);
    assert_eq!(spam["message_ids"], serde_json::json!([50]));
    let delete = sent_request(&recorder, "deleteChatMessagesBySender");
    assert_eq!(delete["sender_id"]["user_id"], 8);
    let ban = sent_request(&recorder, "setChatMemberStatus");
    assert_eq!(ban["status"]["@type"], "chatMemberStatusBanned");

    answer(
        &mut driver,
        &recorder,
        &seq,
        &sink,
        "deleteChatMessagesBySender",
        r#"{"@type":"ok"}"#,
    );
    assert_eq!(
        driver.session.message_action_note.as_deref(),
        Some("messages deleted")
    );
    answer(
        &mut driver,
        &recorder,
        &seq,
        &sink,
        "reportSupergroupSpam",
        r#"{"@type":"error","code":400,"message":"CHAT_ADMIN_REQUIRED"}"#,
    );
    assert!(
        driver
            .session
            .message_action_note
            .as_deref()
            .is_some_and(|note| note.starts_with("could not report the spam"))
    );
}

#[test]
fn moderation_needs_a_supergroup() {
    let (mut driver, _recorder, sink, seq) = group_driver();
    feed(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
    );
    assert!(
        driver
            .moderate_message(
                ChatId(7),
                &[MessageId(1)],
                7,
                ModerationChoice {
                    delete_all: true,
                    ..ModerationChoice::default()
                },
            )
            .is_err()
    );
}

const VOTED_POLL: &str = r#"{"@type":"updateNewMessage","message":{"id":106,"chat_id":7,"is_outgoing":false,"content":{"@type":"messagePoll","poll":{"@type":"poll","id":9001,"question":{"@type":"formattedText","text":"Lunch?","entities":[]},"options":[{"@type":"pollOption","id":"a","text":{"@type":"formattedText","text":"Sushi","entities":[]},"voter_count":12,"vote_percentage":55,"is_chosen":true},{"@type":"pollOption","id":"b","text":{"@type":"formattedText","text":"Pizza","entities":[]},"voter_count":7,"vote_percentage":32,"is_chosen":false}],"total_voter_count":19,"is_anonymous":true,"allows_multiple_answers":false,"allows_revoting":true,"is_closed":false,"type":{"@type":"pollTypeRegular"}},"description":{"@type":"formattedText","text":"","entities":[]},"can_add_option":false}}}"#;

fn private_chat(
    driver: &mut ConnectDriver<Recorder>,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
) {
    feed(
        driver,
        seq,
        sink,
        r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
    );
}

#[test]
fn retracting_a_vote_sends_an_empty_answer_and_clears_the_mark() {
    let (mut driver, recorder, sink, seq) = group_driver();
    private_chat(&mut driver, &seq, &sink);
    feed(&mut driver, &seq, &sink, VOTED_POLL);
    driver.retract_poll_vote(ChatId(7), MessageId(106)).unwrap();
    let request = sent_request(&recorder, "setPollAnswer");
    assert_eq!(request["message_id"], 106);
    assert_eq!(request["option_ids"], serde_json::json!([]));
    let chosen = match &driver.session.histories[&7].messages[&106].content {
        crate::telegram::envelope::MessageContent::Poll(poll) => poll.poll.chosen_indexes(),
        other => panic!("{other:?}"),
    };
    assert!(chosen.is_empty());
    // Nothing is left to retract now.
    assert!(driver.retract_poll_vote(ChatId(7), MessageId(106)).is_err());
}

#[test]
fn a_fact_check_is_set_with_the_text_and_removed_with_null() {
    let (mut driver, recorder, sink, seq) = group_driver();
    private_chat(&mut driver, &seq, &sink);
    driver
        .set_fact_check(ChatId(7), MessageId(5), "Checked against the report.")
        .unwrap();
    let request = sent_request(&recorder, "setMessageFactCheck");
    assert_eq!(request["text"]["text"], "Checked against the report.");
    driver
        .set_fact_check(ChatId(7), MessageId(5), "  ")
        .unwrap();
    assert!(sent_request(&recorder, "setMessageFactCheck")["text"].is_null());
}

#[test]
fn a_fact_check_update_lands_on_the_loaded_message() {
    let (mut driver, _recorder, sink, seq) = group_driver();
    private_chat(&mut driver, &seq, &sink);
    feed(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateNewMessage","message":{"id":5,"chat_id":7,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Claim","entities":[]}}}}"#,
    );
    assert_eq!(
        driver.session.histories[&7].messages[&5].extras.fact_check,
        ""
    );
    feed(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateMessageFactCheck","chat_id":7,"message_id":5,"fact_check":{"@type":"factCheck","text":{"@type":"formattedText","text":"Context","entities":[]},"country_code":"US"}}"#,
    );
    assert_eq!(
        driver.session.histories[&7].messages[&5].extras.fact_check,
        "Context"
    );
}

#[test]
fn saving_a_notification_tone_sends_the_file_by_id() {
    let (mut driver, recorder, _sink, _seq) = group_driver();
    driver
        .save_notification_tone(crate::ids::FileId(42))
        .unwrap();
    let request = sent_request(&recorder, "addSavedNotificationSound");
    assert_eq!(request["sound"]["@type"], "inputFileId");
    assert_eq!(request["sound"]["id"], 42);
    assert!(
        driver
            .save_notification_tone(crate::ids::FileId(0))
            .is_err()
    );
}

#[test]
fn message_properties_carry_the_new_rights() {
    let (mut driver, _recorder, sink, seq) = group_driver();
    private_chat(&mut driver, &seq, &sink);
    let extra = driver.session.request(
        crate::state::RequestPurpose::GetMessageMenuActions {
            chat_id: ChatId(7),
            message_id: MessageId(5),
        },
        Some(ChatId(7)),
    );
    feed(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"messageProperties","@extra":"{}","can_set_fact_check":true,"can_be_replied_in_another_chat":true}}"#,
            extra.0
        ),
    );
    let (_, _, actions) = driver.session.message_menu_actions.expect("properties");
    assert!(actions.can_set_fact_check);
    assert!(actions.can_be_replied_in_another_chat);
    assert!(!actions.can_be_edited);
}
