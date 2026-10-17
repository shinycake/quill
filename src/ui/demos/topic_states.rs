//! Screenshot demos: the open forum topic while its first page loads, after
//! it failed, and when it is really empty. The forum has topic tabs, one
//! with a very long name, and the last topic is open so the strip has to
//! scroll it into view (injected, no live Telegram).
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::ids::ChatId;
use quill::state::{RequestPurpose, Session, TopicHistory};
use quill::telegram::client::copy_and_parse;
use std::sync::atomic::Ordering;

const FORUM: i64 = 16;
const OPEN_TOPIC: i32 = 9;

register_demos![
    DemoSpec::chats(
        "ready-topic-loading",
        "screenshot demo — forum topic while its first page loads"
    )
    .setup(QuillApp::demo_topic_loading),
    DemoSpec::chats(
        "ready-topic-failed",
        "screenshot demo — forum topic whose first page failed"
    )
    .setup(QuillApp::demo_topic_failed),
    DemoSpec::chats(
        "ready-topic-empty",
        "screenshot demo — forum topic with no messages"
    )
    .setup(QuillApp::demo_topic_empty),
];

fn topic_json(id: i32, name: &str) -> String {
    format!(
        r#"{{"info":{{"@type":"forumTopicInfo","chat_id":{FORUM},"forum_topic_id":{id},"name":"{name}","icon":{{"@type":"forumTopicIcon","color":16766590,"custom_emoji_id":"0"}},"is_general":false,"is_closed":false,"is_hidden":false}},"last_message":{{"id":{last},"chat_id":{FORUM},"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"x","entities":[]}}}}}},"order":"{id}","is_pinned":false,"unread_count":0,"last_read_inbox_message_id":0,"notification_settings":{{"@type":"chatNotificationSettings","use_default_mute_for":true,"mute_for":0}}}}"#,
        last = 100 + id
    )
}

fn seed_forum(session: &mut Session, app: &QuillApp) {
    let sink = app.demo_ui.sink.clone();
    let seq = &app.demo_ui.seq;
    seq.store(session.last_seq, Ordering::SeqCst);
    let dyn_sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> = sink;
    for json in [
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{FORUM},"title":"Dev Forum","type":{{"@type":"chatTypeSupergroup","supergroup_id":{FORUM},"is_channel":false}},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{FORUM},"is_forum":true,"has_forum_tabs":true}}}}"#
        ),
    ] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    let names = [
        (2, "Announcements"),
        (
            3,
            "Release planning and roadmap discussion for the next quarter",
        ),
        (4, "Design"),
        (5, "Backend"),
        (6, "Frontend"),
        (7, "Off topic"),
        (8, "Questions and answers"),
        (OPEN_TOPIC, "Build infrastructure"),
    ];
    let topics: Vec<String> = names
        .iter()
        .map(|(id, name)| topic_json(*id, name))
        .collect();
    let extra = session.request(RequestPurpose::GetForumTopics, Some(ChatId(FORUM)));
    let answer = format!(
        r#"{{"@type":"forumTopics","@extra":"{}","total_count":{},"topics":[{}],"next_offset_date":0,"next_offset_message_id":0,"next_offset_forum_topic_id":0}}"#,
        extra.0,
        names.len(),
        topics.join(",")
    );
    if let Some(owned) = copy_and_parse(&answer, seq, &dyn_sink) {
        session.apply(owned);
    }
    session.open_chat(ChatId(FORUM));
    session.select_topic(ChatId(FORUM), OPEN_TOPIC);
}

impl QuillApp {
    fn demo_topic_session(&mut self, then: impl FnOnce(&mut Session)) {
        // `seed_forum` reads the demo sink and counter off the app while the
        // session is borrowed, so take the session out for the duration.
        if let Some(mut session) = self.demo_session.take() {
            seed_forum(&mut session, self);
            then(&mut session);
            self.demo_session = Some(session);
        }
    }

    fn demo_topic_loading(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.demo_topic_session(|_| {});
    }

    fn demo_topic_failed(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.demo_topic_session(|session| {
            session
                .threads
                .topic_load_failed
                .insert((FORUM, OPEN_TOPIC));
        });
    }

    fn demo_topic_empty(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.demo_topic_session(|session| {
            let history = TopicHistory {
                loaded_complete: true,
                ..TopicHistory::default()
            };
            session
                .threads
                .topic_histories
                .insert((FORUM, OPEN_TOPIC), history);
        });
    }
}
