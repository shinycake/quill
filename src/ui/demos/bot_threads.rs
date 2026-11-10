//! Screenshot demos: a bot whose users create threads (injected, no live
//! Telegram). The "All" view ends with the "New Thread" card; the second
//! demo sends from there, which opens the new "New Chat" thread with the
//! message in it.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::bots::apply_ready_bot_chat;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::ids::ChatId;
use quill::state::RequestPurpose;
use quill::telegram::client::copy_and_parse;
use std::sync::atomic::Ordering;

const BOT: i64 = 21;

register_demos![
    DemoSpec::chats(
        "ready-bot-threads",
        "screenshot demo — bot with threads: the New Thread card in All"
    )
    .setup(QuillApp::demo_bot_threads),
    DemoSpec::chats(
        "ready-bot-thread-sent",
        "screenshot demo — bot with threads: a send from All opens its new thread"
    )
    .setup(QuillApp::demo_bot_thread_sent),
];

impl QuillApp {
    fn demo_bot_threads(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        apply_ready_bot_chat(session, &self.demo_ui.sink, &self.demo_ui.seq);
        let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
            self.demo_ui.sink.clone();
        let user = r#"{"@type":"updateUser","user":{"id":21,"first_name":"Demo","type":{"@type":"userTypeBot","can_be_edited":false,"can_join_groups":false,"can_read_all_group_messages":false,"has_main_web_app":false,"has_topics":true,"has_topic_sidebar":false,"allows_users_to_create_topics":true,"can_manage_bots":false,"is_inline":false,"inline_query_placeholder":"","supports_guest_queries":false,"is_guard":false,"need_location":false,"can_connect_to_business":false,"can_be_added_to_attachment_menu":false,"active_user_count":0}}}"#;
        let extra = session.request(RequestPurpose::GetForumTopics, Some(ChatId(BOT)));
        let topic = |id: i32, name: &str, color: i32| {
            format!(
                r#"{{"info":{{"@type":"forumTopicInfo","chat_id":{BOT},"forum_topic_id":{id},"name":"{name}","icon":{{"@type":"forumTopicIcon","color":{color},"custom_emoji_id":"0"}},"is_general":false,"is_closed":false,"is_hidden":false,"is_outgoing":true}},"order":"{id}","is_pinned":false,"unread_count":0,"last_read_inbox_message_id":0,"notification_settings":{{"@type":"chatNotificationSettings","use_default_mute_for":true,"mute_for":0}}}}"#
            )
        };
        let topics = format!(
            r#"{{"@type":"forumTopics","@extra":"{}","total_count":2,"topics":[{},{}],"next_offset_date":0,"next_offset_message_id":0,"next_offset_forum_topic_id":0}}"#,
            extra.0,
            topic(2, "Weather in Paris", 0x8EEE98),
            topic(3, "Trip ideas", 0xFFD67E),
        );
        for json in [user, topics.as_str()] {
            if let Some(owned) = copy_and_parse(json, &self.demo_ui.seq, &sink) {
                session.apply(owned);
            }
        }
    }

    fn demo_bot_thread_sent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_bot_threads(window, cx);
        let text = "Plan a weekend in Lisbon";
        self.composer
            .update(cx, |input, cx| input.set_value(text, window, cx));
        self.submit_composer(text.to_string(), window, cx);
    }
}
