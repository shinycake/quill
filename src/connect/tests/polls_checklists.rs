//! Connect-driver tests for B15: poll extras (add option, statistics,
//! unread poll votes, creation flags) and checklists.
use super::super::*;
use super::*;
use crate::checklist::ChecklistDraft;
use crate::diagnostics::DiagnosticSink;
use crate::ids::{ChatId, MessageId};
use crate::poll::PollDraft;
use crate::state::{PollStatsFetch, UnreadJumpKind};
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::MessageContent;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const ADDABLE_POLL_JSON: &str = r#"{"@type":"updateNewMessage","message":{"id":120,"chat_id":7,"is_outgoing":false,"content":{"@type":"messagePoll","poll":{"@type":"poll","id":9100,"question":{"@type":"formattedText","text":"Drink?","entities":[]},"options":[{"@type":"pollOption","id":"a","text":{"@type":"formattedText","text":"Tea","entities":[]},"voter_count":1,"vote_percentage":100,"is_chosen":false}],"total_voter_count":1,"is_anonymous":true,"allows_multiple_answers":false,"allows_revoting":true,"is_closed":false,"type":{"@type":"pollTypeRegular"}},"description":{"@type":"formattedText","text":"","entities":[]},"can_add_option":true}}}"#;

const CHECKLIST_JSON: &str = r#"{"@type":"updateNewMessage","message":{"id":130,"chat_id":7,"is_outgoing":false,"content":{"@type":"messageChecklist","list":{"@type":"checklist","title":{"@type":"formattedText","text":"Trip","entities":[]},"tasks":[{"@type":"checklistTask","id":1,"text":{"@type":"formattedText","text":"Tickets","entities":[]},"completed_by":{"@type":"messageSenderUser","user_id":7},"completion_date":1700000000},{"@type":"checklistTask","id":4,"text":{"@type":"formattedText","text":"Hotel","entities":[]},"completed_by":null,"completion_date":0}],"others_can_add_tasks":true,"can_add_tasks":true,"others_can_mark_tasks_as_done":true,"can_mark_tasks_as_done":true}}}}"#;

fn last_json(recorder: &RecordingSender) -> Value {
    serde_json::from_str(&recorder.snapshot().last().cloned().expect("a request")).unwrap()
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
fn add_poll_option_guards_and_request() {
    let (dir, mut driver, recorder, _sink, dyn_sink, seq) = poll_driver();
    ingest(&mut driver, &seq, &dyn_sink, ADDABLE_POLL_JSON);
    // Not addable (can_add_option false), missing, and not a poll.
    assert_invalid(driver.add_poll_option(ChatId(7), MessageId(106), "Tacos"));
    assert_invalid(driver.add_poll_option(ChatId(7), MessageId(404), "Tacos"));
    assert_invalid(driver.add_poll_option(ChatId(7), MessageId(50), "Tacos"));
    // Empty and duplicate (case-insensitive) texts never leave the client.
    assert_invalid(driver.add_poll_option(ChatId(7), MessageId(120), "   "));
    assert_invalid(driver.add_poll_option(ChatId(7), MessageId(120), "TEA"));
    let before = recorder.snapshot().len();
    let extra = driver
        .add_poll_option(ChatId(7), MessageId(120), "  Coffee ")
        .unwrap();
    assert_eq!(recorder.snapshot().len(), before + 1);
    let v = last_json(&recorder);
    assert_eq!(v["@type"], "addPollOption");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["chat_id"], 7);
    assert_eq!(v["message_id"], 120);
    assert_eq!(v["option"]["text"]["text"], "Coffee");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn add_poll_option_error_surfaces_a_note() {
    let (dir, mut driver, _recorder, _sink, dyn_sink, seq) = poll_driver();
    ingest(&mut driver, &seq, &dyn_sink, ADDABLE_POLL_JSON);
    let extra = driver
        .add_poll_option(ChatId(7), MessageId(120), "Coffee")
        .unwrap();
    ingest(
        &mut driver,
        &seq,
        &dyn_sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"POLL_OPTION_DUPLICATE"}}"#,
            extra.0
        ),
    );
    assert_eq!(
        driver.session.message_action_note.as_deref(),
        Some("Could not add the option. Please try again.")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn poll_creation_flags_reach_the_wire() {
    let (dir, mut driver, recorder, _sink, _dyn_sink, _seq) = poll_driver();
    let draft = PollDraft {
        question: "Lunch?".into(),
        options: vec!["Sushi".into(), "Pizza".into()],
        is_anonymous: true,
        allows_revoting: true,
        allow_adding_options: true,
        hide_results_until_closes: true,
        members_only: true,
        close_date: crate::local_time::now_unix() + 7200,
        ..Default::default()
    };
    assert_eq!(draft.validate(), None);
    driver.send_poll_draft(ChatId(7), &draft, None).unwrap();
    let v = last_json(&recorder);
    let content = &v["input_message_content"];
    assert_eq!(content["type"]["allow_adding_options"], true);
    assert_eq!(content["hide_results_until_closes"], true);
    assert_eq!(content["members_only"], true);
    assert_eq!(content["open_period"], 0);
    assert_eq!(content["close_date"], draft.close_date);
    // Quizzes never carry "allow adding options".
    let quiz = PollDraft {
        is_quiz: true,
        quiz_correct: Some(0),
        ..draft.clone()
    };
    driver.send_poll_draft(ChatId(7), &quiz, None).unwrap();
    let v = last_json(&recorder);
    assert_eq!(
        v["input_message_content"]["type"]["@type"],
        "inputPollTypeQuiz"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn poll_vote_statistics_land_in_the_session() {
    let (dir, mut driver, recorder, _sink, dyn_sink, seq) = poll_driver();
    assert_invalid(driver.fetch_poll_vote_statistics(ChatId(7), MessageId(50), false));
    let extra = driver
        .fetch_poll_vote_statistics(ChatId(7), MessageId(106), true)
        .unwrap()
        .expect("request sent");
    let v = last_json(&recorder);
    assert_eq!(v["@type"], "getPollVoteStatistics");
    assert_eq!(v["is_dark"], true);
    assert!(matches!(
        driver.session.poll_stats.get(&(7, 106)),
        Some(PollStatsFetch::Loading)
    ));
    // A second call while loading sends nothing.
    assert!(
        driver
            .fetch_poll_vote_statistics(ChatId(7), MessageId(106), true)
            .unwrap()
            .is_none()
    );
    ingest(
        &mut driver,
        &seq,
        &dyn_sink,
        &format!(
            r#"{{"@type":"pollVoteStatistics","@extra":"{}","vote_graph":{{"@type":"statisticalGraphData","json_data":"{{}}","zoom_token":""}}}}"#,
            extra.0
        ),
    );
    assert!(matches!(
        driver.session.poll_stats.get(&(7, 106)),
        Some(PollStatsFetch::Loaded(_))
    ));
    // A failed refetch lands in `Failed`.
    driver.session.poll_stats.remove(&(7, 106));
    let extra = driver
        .fetch_poll_vote_statistics(ChatId(7), MessageId(106), false)
        .unwrap()
        .unwrap();
    ingest(
        &mut driver,
        &seq,
        &dyn_sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"nope"}}"#,
            extra.0
        ),
    );
    assert!(matches!(
        driver.session.poll_stats.get(&(7, 106)),
        Some(PollStatsFetch::Failed(_))
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unread_poll_votes_badge_jump_and_read_all() {
    let (dir, mut driver, recorder, _sink, dyn_sink, seq) = poll_driver();
    ingest(
        &mut driver,
        &seq,
        &dyn_sink,
        r#"{"@type":"updateChatUnreadPollVoteCount","chat_id":7,"unread_poll_vote_count":3}"#,
    );
    assert_eq!(
        driver.session.chats.get(&7).unwrap().unread_poll_vote_count,
        3
    );
    // Reading a vote reports the chat's new counter on the message update.
    ingest(
        &mut driver,
        &seq,
        &dyn_sink,
        r#"{"@type":"updateMessageContainsUnreadPollVotes","chat_id":7,"message_id":106,"contains_unread_poll_votes":false,"unread_poll_vote_count":2}"#,
    );
    assert_eq!(
        driver.session.chats.get(&7).unwrap().unread_poll_vote_count,
        2
    );
    // A new chat carries its count.
    ingest(
        &mut driver,
        &seq,
        &dyn_sink,
        r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Polls","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0,"unread_poll_vote_count":5}}"#,
    );
    assert_eq!(
        driver
            .session
            .chats
            .get(&21)
            .unwrap()
            .unread_poll_vote_count,
        5
    );

    driver.session.open_chat = Some(ChatId(7));
    driver
        .jump_to_unread_marker(UnreadJumpKind::PollVote)
        .unwrap();
    let v = last_json(&recorder);
    assert_eq!(v["@type"], "searchChatMessages");
    assert_eq!(v["filter"]["@type"], "searchMessagesFilterUnreadPollVote");
    driver
        .read_all_unread_markers(UnreadJumpKind::PollVote)
        .unwrap();
    let v = last_json(&recorder);
    assert_eq!(v["@type"], "readAllChatPollVotes");
    assert_eq!(v["chat_id"], 7);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn checklist_message_parses_into_the_history() {
    let (dir, mut driver, _recorder, _sink, dyn_sink, seq) = poll_driver();
    ingest(&mut driver, &seq, &dyn_sink, CHECKLIST_JSON);
    let message = driver
        .session
        .histories
        .get(&7)
        .and_then(|history| history.messages.get(&130))
        .unwrap();
    let MessageContent::Checklist(content) = &message.content else {
        panic!("expected a checklist message");
    };
    assert_eq!(content.list.title, "Trip");
    assert_eq!(content.list.done_count(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn checklist_toggle_and_add_need_premium_and_permission() {
    let (dir, mut driver, recorder, _sink, dyn_sink, seq) = poll_driver();
    ingest(&mut driver, &seq, &dyn_sink, CHECKLIST_JSON);
    // Without Premium nothing is sent.
    driver.session.premium_option = Some(false);
    assert_invalid(driver.toggle_checklist_task(ChatId(7), MessageId(130), 4));
    assert_invalid(driver.add_checklist_tasks(ChatId(7), MessageId(130), &["Pack".into()]));
    driver.session.premium_option = Some(true);
    // Not a checklist / unknown task.
    assert_invalid(driver.toggle_checklist_task(ChatId(7), MessageId(106), 4));
    assert_invalid(driver.toggle_checklist_task(ChatId(7), MessageId(130), 99));
    // Open task -> done, done task -> not done.
    driver
        .toggle_checklist_task(ChatId(7), MessageId(130), 4)
        .unwrap();
    let v = last_json(&recorder);
    assert_eq!(v["@type"], "markChecklistTasksAsDone");
    assert_eq!(v["marked_as_done_task_ids"], serde_json::json!([4]));
    assert_eq!(v["marked_as_not_done_task_ids"], serde_json::json!([]));
    driver
        .toggle_checklist_task(ChatId(7), MessageId(130), 1)
        .unwrap();
    let v = last_json(&recorder);
    assert_eq!(v["marked_as_done_task_ids"], serde_json::json!([]));
    assert_eq!(v["marked_as_not_done_task_ids"], serde_json::json!([1]));
    // New tasks are numbered after the highest id (4).
    assert_invalid(driver.add_checklist_tasks(ChatId(7), MessageId(130), &["  ".into()]));
    driver
        .add_checklist_tasks(
            ChatId(7),
            MessageId(130),
            &["Pack".into(), " Leave ".into()],
        )
        .unwrap();
    let v = last_json(&recorder);
    assert_eq!(v["@type"], "addChecklistTasks");
    assert_eq!(v["tasks"][0]["id"], 5);
    assert_eq!(v["tasks"][1]["id"], 6);
    assert_eq!(v["tasks"][1]["text"]["text"], "Leave");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn checklist_mutation_errors_surface_a_note() {
    let (dir, mut driver, _recorder, _sink, dyn_sink, seq) = poll_driver();
    ingest(&mut driver, &seq, &dyn_sink, CHECKLIST_JSON);
    driver.session.premium_option = Some(true);
    let extra = driver
        .toggle_checklist_task(ChatId(7), MessageId(130), 4)
        .unwrap();
    ingest(
        &mut driver,
        &seq,
        &dyn_sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"x"}}"#,
            extra.0
        ),
    );
    assert_eq!(
        driver.session.message_action_note.as_deref(),
        Some("Could not update the checklist (error 400)")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn send_checklist_draft_is_premium_gated() {
    let (dir, mut driver, recorder, _sink, _dyn_sink, _seq) = poll_driver();
    let draft = ChecklistDraft {
        title: "Trip".into(),
        tasks: vec!["Tickets".into(), "  ".into(), "Hotel".into()],
        others_can_add_tasks: true,
        others_can_mark_tasks_as_done: false,
    };
    driver.session.premium_option = Some(false);
    assert_invalid(driver.send_checklist_draft(ChatId(7), &draft, None));
    driver.session.premium_option = Some(true);
    let invalid = ChecklistDraft {
        title: "  ".into(),
        ..draft.clone()
    };
    assert_invalid(driver.send_checklist_draft(ChatId(7), &invalid, None));
    driver
        .send_checklist_draft(ChatId(7), &draft, None)
        .unwrap();
    let v = last_json(&recorder);
    assert_eq!(v["@type"], "sendMessage");
    let list = &v["input_message_content"]["checklist"];
    assert_eq!(v["input_message_content"]["@type"], "inputMessageChecklist");
    assert_eq!(list["title"]["text"], "Trip");
    assert_eq!(list["tasks"].as_array().unwrap().len(), 2);
    assert_eq!(list["tasks"][1]["text"]["text"], "Hotel");
    assert_eq!(list["others_can_add_tasks"], true);
    assert_eq!(list["others_can_mark_tasks_as_done"], false);
    let _ = std::fs::remove_dir_all(&dir);
}
