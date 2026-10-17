#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::state::{RequestPurpose, Session};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

register_demos![
    DemoSpec::chats(
        "ready-thread",
        "screenshot demo — a channel post's comment thread"
    )
    .setup(QuillApp::demo_ready_thread),
];

const CHANNEL: ChatId = ChatId(13);
const POST: MessageId = MessageId(101);
pub(super) const DISCUSSION: ChatId = ChatId(14);
const THREAD: i64 = 501;

fn reply(id: i64, sender: Option<i64>, minute: i64, text: &str) -> String {
    let (outgoing, sender) = match sender {
        Some(user) => (
            false,
            format!(r#""sender_id":{{"@type":"messageSenderUser","user_id":{user}}},"#),
        ),
        None => (true, String::new()),
    };
    let date = 1_790_631_720 + minute * 60;
    format!(
        r#"{{"@type":"message","id":{id},"chat_id":14,{sender}"is_outgoing":{outgoing},"date":{date},"topic_id":{{"@type":"messageTopicThread","message_thread_id":{THREAD}}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}}}"#
    )
}

/// Open the comments of the demo channel's post: the root and three
/// replies, the last one ours.
pub(super) fn apply_ready_thread(session: &mut Session, seq: &AtomicU64, sink: &Arc<MemorySink>) {
    let sink: Arc<dyn DiagnosticSink> = sink.clone();
    let apply = |session: &mut Session, json: &str| {
        if let Some(owned) = quill::telegram::client::copy_and_parse(json, seq, &sink) {
            session.apply(owned);
        }
    };
    for json in [
        r#"{"@type":"updateUser","user":{"id":21,"first_name":"Maya","last_name":"Chen","accent_color_id":2,"type":{"@type":"userTypeRegular"}}}"#,
        r#"{"@type":"updateUser","user":{"id":22,"first_name":"Leo","last_name":"Park","accent_color_id":5,"type":{"@type":"userTypeRegular"}}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":14,"title":"Demo channel chat","type":{"@type":"chatTypeSupergroup","supergroup_id":14,"is_channel":false},"unread_count":0}}"#,
    ] {
        apply(session, json);
    }
    session.begin_thread(CHANNEL, POST);
    let info = session.request(
        RequestPurpose::GetMessageThread { message_id: POST.0 },
        Some(CHANNEL),
    );
    let root = format!(
        r#"{{"@type":"message","id":{THREAD},"chat_id":14,"is_outgoing":false,"is_channel_post":true,"date":1790631600,"topic_id":{{"@type":"messageTopicThread","message_thread_id":{THREAD}}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Release notes are up. Questions welcome below.","entities":[]}}}}}}"#
    );
    apply(
        session,
        &format!(
            r#"{{"@type":"messageThreadInfo","@extra":"{}","chat_id":14,"message_thread_id":{THREAD},"reply_info":{{"@type":"messageReplyInfo","reply_count":3,"recent_replier_ids":[],"last_read_inbox_message_id":504,"last_read_outbox_message_id":504,"last_message_id":504}},"unread_message_count":0,"messages":[{root}]}}"#,
            info.0
        ),
    );
    session.open_chat(DISCUSSION);
    for page in [
        vec![
            reply(504, None, 7, "Thanks, updating tonight."),
            reply(503, Some(22), 5, "Works on Linux too."),
            reply(502, Some(21), 3, "Does the new sync need a restart?"),
            root,
        ],
        Vec::new(),
    ] {
        let extra = session.request(
            RequestPurpose::GetMessageThreadHistory { message_id: POST.0 },
            Some(CHANNEL),
        );
        apply(
            session,
            &format!(
                r#"{{"@type":"messages","@extra":"{}","total_count":4,"messages":[{}]}}"#,
                extra.0,
                page.join(",")
            ),
        );
    }
}

impl QuillApp {
    fn demo_ready_thread(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_thread(session, &self.demo_ui.seq, &self.demo_ui.sink);
        }
        cx.notify();
    }
}
