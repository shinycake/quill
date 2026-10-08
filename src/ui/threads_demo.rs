//! `ready-threads` screenshot demo: a channel whose posts carry comment
//! bars, the post's comment thread in its discussion group, and a group
//! message with replies — injected through the normal reducer, no live
//! Telegram. `QUILL_DEMO_THREADS_VIEW=posts|thread|group` (default `posts`).

use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::state::{RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const ME: i64 = 9;
const DANA: i64 = 1;
const OMAR: i64 = 2;
const LEA: i64 = 3;
const CHANNEL: i64 = 71;
const GROUP: i64 = 72;
/// The channel post whose thread the `thread` view opens.
const POST: i64 = 101;
/// Its copy in the discussion group: the thread root.
const ROOT: i64 = 501;

fn text(t: &str) -> String {
    format!(
        r#"{{"@type":"messageText","text":{{"@type":"formattedText","text":{},"entities":[]}}}}"#,
        serde_json::to_string(t).unwrap_or_default()
    )
}

fn user(id: i64) -> String {
    format!(r#"{{"@type":"messageSenderUser","user_id":{id}}}"#)
}

fn reply_info(count: i32, repliers: &[i64], last_read: i64, last: i64) -> String {
    let repliers: Vec<String> = repliers.iter().map(|id| user(*id)).collect();
    format!(
        r#"{{"@type":"messageReplyInfo","reply_count":{count},"recent_replier_ids":[{}],"last_read_inbox_message_id":{last_read},"last_read_outbox_message_id":0,"last_message_id":{last}}}"#,
        repliers.join(",")
    )
}

fn interaction(views: i32, reply: Option<String>) -> String {
    format!(
        r#"{{"@type":"messageInteractionInfo","view_count":{views},"forward_count":0,"reply_info":{},"reactions":null}}"#,
        reply.unwrap_or_else(|| "null".into())
    )
}

#[allow(clippy::too_many_arguments)]
fn message(
    id: i64,
    chat: i64,
    sender: &str,
    outgoing: bool,
    date: i64,
    thread: Option<i64>,
    interaction_info: &str,
    content: &str,
) -> String {
    let topic = thread.map_or_else(String::new, |thread| {
        format!(r#""topic_id":{{"@type":"messageTopicThread","message_thread_id":{thread}}},"#)
    });
    format!(
        r#"{{"@type":"message","id":{id},"chat_id":{chat},"sender_id":{sender},"is_outgoing":{outgoing},"is_channel_post":{channel},"date":{date},{topic}"interaction_info":{interaction_info},"content":{content}}}"#,
        channel = chat == CHANNEL,
    )
}

fn new_message(message: &str) -> String {
    format!(r#"{{"@type":"updateNewMessage","message":{message}}}"#)
}

pub(super) fn apply_ready_threads(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
    view: &str,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let now = quill::local_time::now_unix() - 7200;
    let channel_sender = format!(r#"{{"@type":"messageSenderChat","chat_id":{CHANNEL}}}"#);
    let mut jsons: Vec<String> = vec![
        format!(
            r#"{{"@type":"updateOption","name":"my_id","value":{{"@type":"optionValueInteger","value":"{ME}"}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{CHANNEL},"title":"Quill News","type":{{"@type":"chatTypeSupergroup","supergroup_id":{CHANNEL},"is_channel":true}},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{GROUP},"title":"Quill News Chat","type":{{"@type":"chatTypeSupergroup","supergroup_id":{GROUP},"is_channel":false}},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{CHANNEL},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"5000","is_pinned":false}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{GROUP},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"4000","is_pinned":false}}}}"#
        ),
    ];
    for (id, first, last) in [
        (ME, "Idan", "Birman"),
        (DANA, "Dana", "Cole"),
        (OMAR, "Omar", "Haddad"),
        (LEA, "Lea", "Stern"),
    ] {
        jsons.push(format!(
            r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":"{first}","last_name":"{last}","usernames":null,"phone_number":"","status":{{"@type":"userStatusRecently"}},"profile_photo":null,"is_contact":true,"type":{{"@type":"userTypeRegular"}}}}}}"#
        ));
    }
    // The channel: three posts — many comments, none yet, one unread.
    let posts = [
        (
            POST,
            "Quill 0.9 is out: threads, comments and a faster history. Tell us what you think below.",
            interaction(12_400, Some(reply_info(128, &[DANA, OMAR, LEA], 0, 2000))),
        ),
        (
            102,
            "Maintenance tonight at 22:00 UTC. Sync may pause for a few minutes.",
            interaction(8_100, Some(reply_info(0, &[], 0, 0))),
        ),
        (
            103,
            "Next up: translations and in-chat search.",
            interaction(5_300, Some(reply_info(1, &[LEA], 0, 2001))),
        ),
    ];
    for (index, (id, body, info)) in posts.iter().enumerate() {
        jsons.push(new_message(&message(
            *id,
            CHANNEL,
            &channel_sender,
            false,
            now + index as i64 * 600,
            None,
            info,
            &text(body),
        )));
    }
    // The discussion group: the auto-forwarded post is the thread root,
    // followed by its comments; one comment has replies of its own.
    jsons.push(new_message(&message(
        ROOT,
        GROUP,
        &channel_sender,
        false,
        now,
        Some(ROOT),
        &interaction(0, Some(reply_info(6, &[DANA, OMAR, LEA], 504, 506))),
        &text(
            "Quill 0.9 is out: threads, comments and a faster history. Tell us what you think below.",
        ),
    )));
    let comments = [
        (502, DANA, "Finally! The comments bar looks great."),
        (
            503,
            OMAR,
            "Does the thread view keep my place when I go back?",
        ),
        (504, LEA, "Yes, it returns to the post."),
        (505, DANA, "Typing in the thread works too, nice."),
        (506, OMAR, "Love it."),
        (507, LEA, "Thanks for shipping this."),
    ];
    for (id, sender, body) in comments {
        let reply = (id == 502).then(|| reply_info(4, &[OMAR, LEA], 0, 507));
        jsons.push(new_message(&message(
            id,
            GROUP,
            &user(sender),
            false,
            now + 300 + (id - 501) * 120,
            Some(ROOT),
            &interaction(0, reply),
            &text(body),
        )));
    }
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    match view {
        "thread" => open_thread(session, &dyn_sink, seq, now),
        "group" => session.open_chat(ChatId(GROUP)),
        _ => session.open_chat(ChatId(CHANNEL)),
    }
}

/// The `getMessageThread` + `getMessageThreadHistory` answers for post 101,
/// applied the way the live driver's requests would be.
fn open_thread(session: &mut Session, sink: &Arc<dyn DiagnosticSink>, seq: &AtomicU64, now: i64) {
    session.open_chat(ChatId(CHANNEL));
    session.begin_thread(ChatId(CHANNEL), MessageId(POST));
    let root = message(
        ROOT,
        GROUP,
        &format!(r#"{{"@type":"messageSenderChat","chat_id":{CHANNEL}}}"#),
        false,
        now,
        Some(ROOT),
        &interaction(0, None),
        &text(
            "Quill 0.9 is out: threads, comments and a faster history. Tell us what you think below.",
        ),
    );
    let info = session.request(
        RequestPurpose::GetMessageThread { message_id: POST },
        Some(ChatId(CHANNEL)),
    );
    let page = session.request(
        RequestPurpose::GetMessageThreadHistory { message_id: POST },
        Some(ChatId(CHANNEL)),
    );
    let comments: Vec<String> = [
        (507, LEA, "Thanks for shipping this."),
        (506, OMAR, "Love it."),
        (505, DANA, "Typing in the thread works too, nice."),
        (504, LEA, "Yes, it returns to the post."),
        (
            503,
            OMAR,
            "Does the thread view keep my place when I go back?",
        ),
        (502, DANA, "Finally! The comments bar looks great."),
    ]
    .iter()
    .map(|(id, sender, body)| {
        message(
            *id,
            GROUP,
            &user(*sender),
            false,
            now + 300 + (id - 501) * 120,
            Some(ROOT),
            &interaction(0, None),
            &text(body),
        )
    })
    .collect();
    let jsons = [
        format!(
            r#"{{"@type":"messageThreadInfo","@extra":"{}","chat_id":{GROUP},"message_thread_id":{ROOT},"reply_info":{},"unread_message_count":3,"messages":[{root}]}}"#,
            info.0,
            reply_info(6, &[DANA, OMAR, LEA], 504, 507)
        ),
        format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":7,"messages":[{},{root}]}}"#,
            page.0,
            comments.join(",")
        ),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, sink) {
            session.apply(owned);
        }
    }
    // The driver moves the view into the discussion group.
    session.open_chat(ChatId(GROUP));
    if let Some(thread) = session.thread.as_mut() {
        thread.needs_chat_switch = false;
        thread.reading_started = true;
    }
}
