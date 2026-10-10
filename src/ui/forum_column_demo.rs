//! `ready-forum-column` screenshot demo: the forum's topic column next to the
//! chat list, a topic with a message that has replies, and that thread open
//! inside the topic. Injected through the normal reducer; no live Telegram.
//! `QUILL_DEMO_FORUM_COLUMN_VIEW=topics|topic|thread` (default `topics`).

use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::state::{RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

/// The topic the `topic` and `thread` views open.
const TOPIC: i32 = 2;
/// The topic message whose replies the `thread` view opens.
const ROOT: i64 = 201;

fn apply(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64, json: &str) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    if let Some(owned) = copy_and_parse(json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

fn user(id: i64, first: &str, last: &str) -> String {
    format!(
        r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":"{first}","last_name":"{last}","usernames":null,"phone_number":"","status":null,"profile_photo":null,"is_contact":true,"type":{{"@type":"userTypeRegular"}}}}}}"#
    )
}

fn topic_message(
    id: i64,
    sender: i64,
    date: i64,
    reply_to: Option<i64>,
    replies: i32,
    body: &str,
) -> String {
    let reply = reply_to.map_or_else(String::new, |message_id| {
        format!(
            r#""reply_to":{{"@type":"messageReplyToMessage","chat_id":16,"message_id":{message_id}}},"#
        )
    });
    let info = if replies > 0 {
        format!(
            r#""interaction_info":{{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":{{"@type":"messageReplyInfo","reply_count":{replies},"recent_replier_ids":[{{"@type":"messageSenderUser","user_id":7}},{{"@type":"messageSenderUser","user_id":6}}],"last_read_inbox_message_id":0,"last_read_outbox_message_id":0,"last_message_id":{}}},"reactions":null}},"#,
            ROOT + replies as i64
        )
    } else {
        String::new()
    };
    format!(
        r#"{{"id":{id},"chat_id":16,"sender_id":{{"@type":"messageSenderUser","user_id":{sender}}},"is_outgoing":false,"date":{date},"topic_id":{{"@type":"messageTopicForum","forum_topic_id":{TOPIC}}},{reply}{info}"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":{},"entities":[]}}}}}}"#,
        serde_json::to_string(body).unwrap_or_default()
    )
}

pub(super) fn apply_ready_forum_column(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
    view: &str,
) {
    apply(session, sink, seq, &user(6, "Dana", "Levi"));
    apply(session, sink, seq, &user(7, "Omar", "Haddad"));
    super::groups_forum::seed_forum_chat_16(session, sink, seq);
    session.open_chat(ChatId(16));
    if view == "topics" {
        return;
    }
    session.select_topic(ChatId(16), TOPIC);
    let now = quill::local_time::now_unix() - 3_600;
    let extra = session.request_for_topic(RequestPurpose::GetTopicHistory, Some(ChatId(16)), TOPIC);
    let messages = [
        topic_message(
            ROOT,
            6,
            now,
            None,
            3,
            "v2.1 is rolling out this week. Anything you want in the notes?",
        ),
        topic_message(
            202,
            7,
            now + 600,
            None,
            0,
            "Thanks, Dana. Testing the beta now.",
        ),
    ]
    .join(",");
    apply(
        session,
        sink,
        seq,
        &format!(
            r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":2,"next_from_message_id":0,"messages":[{messages}]}}"#,
            extra.0
        ),
    );
    if view == "thread" {
        open_thread(session, sink, seq, now);
    }
}

/// The `getMessageThread` and `getMessageThreadHistory` answers for the
/// topic message, applied the way the live driver's requests would be.
fn open_thread(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64, now: i64) {
    session.begin_thread(ChatId(16), MessageId(ROOT));
    let info = session.request(
        RequestPurpose::GetMessageThread { message_id: ROOT },
        Some(ChatId(16)),
    );
    let page = session.request(
        RequestPurpose::GetMessageThreadHistory { message_id: ROOT },
        Some(ChatId(16)),
    );
    let root = topic_message(
        ROOT,
        6,
        now,
        None,
        0,
        "v2.1 is rolling out this week. Anything you want in the notes?",
    );
    let replies = [
        topic_message(205, 6, now + 1_500, None, 0, "Noted, I will add it."),
        topic_message(
            204,
            7,
            now + 1_200,
            None,
            0,
            "A line about the faster history would be good.",
        ),
        topic_message(
            203,
            6,
            now + 900,
            None,
            0,
            "Beta feedback so far: scrolling feels smoother.",
        ),
    ]
    .join(",");
    apply(
        session,
        sink,
        seq,
        &format!(
            r#"{{"@type":"messageThreadInfo","@extra":"{}","chat_id":16,"message_thread_id":{ROOT},"reply_info":{{"@type":"messageReplyInfo","reply_count":3,"recent_replier_ids":[],"last_read_inbox_message_id":0,"last_read_outbox_message_id":0,"last_message_id":205}},"unread_message_count":0,"messages":[{root}]}}"#,
            info.0
        ),
    );
    apply(
        session,
        sink,
        seq,
        &format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":4,"messages":[{replies},{root}]}}"#,
            page.0
        ),
    );
}
