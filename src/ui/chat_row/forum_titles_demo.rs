//! Screenshot demo of forum rows in the chat list: recent topic titles with
//! the unread ones in bold, as Telegram Desktop's forum rows show them.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::diagnostics::DiagnosticSink;
use quill::ids::ChatId;
use quill::state::RequestPurpose;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

register_demos![
    DemoSpec::chats(
        "ready-forum-row-topics",
        "screenshot demo — chat list: forum topic titles"
    )
    .setup(QuillApp::demo_forum_row_topics),
];

fn topic(chat: i64, id: i32, name: &str, unread: u32) -> String {
    let read = if unread == 0 { 100 + id } else { 50 };
    format!(
        r#"{{"info":{{"@type":"forumTopicInfo","chat_id":{chat},"forum_topic_id":{id},"name":"{name}","icon":{{"@type":"forumTopicIcon","color":16766590,"custom_emoji_id":"0"}},"is_general":false,"is_closed":false,"is_hidden":false}},"last_message":{{"id":{last},"chat_id":{chat},"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"x","entities":[]}}}}}},"order":"{id}","is_pinned":false,"unread_count":{unread},"last_read_inbox_message_id":{read},"notification_settings":{{"@type":"chatNotificationSettings","use_default_mute_for":true,"mute_for":0}}}}"#,
        last = 100 + id
    )
}

impl QuillApp {
    fn demo_forum_row_topics(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let seq = AtomicU64::new(session.last_seq);
        let sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        // (chat id, title, [(topic id, name, unread)]); the highest topic
        // id is the most recently active.
        let forums: [(i64, &str, &[(i32, &str, u32)]); 3] = [
            (
                71,
                "Dev Forum",
                &[
                    (2, "Announcements", 0),
                    (3, "Design", 0),
                    (4, "Backend", 2),
                    (5, "Release planning", 0),
                    (6, "Off topic", 5),
                ],
            ),
            (
                72,
                "Hiking Club",
                &[(2, "Trips", 0), (3, "Gear", 0), (4, "Photos", 0)],
            ),
            // Its topics have not arrived: the line reads "Loading…".
            (73, "New Forum", &[]),
        ];
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        if let Some(owned) = copy_and_parse(
            r#"{"@type":"updateUser","user":{"id":81,"first_name":"Maya","last_name":"","type":{"@type":"userTypeRegular"}}}"#,
            &seq,
            &sink,
        ) {
            session.apply(owned);
        }
        for (n, (chat, title, topics)) in forums.iter().enumerate() {
            let order = 9_000_000_000_000_000_100_i64 - n as i64;
            let newest = topics.iter().max_by_key(|t| t.0).map_or(0, |t| t.0);
            let last = 100 + newest;
            let said = topics.iter().max_by_key(|t| t.0).map_or("", |t| t.1);
            let updates = [
                format!(
                    r#"{{"@type":"updateNewChat","chat":{{"id":{chat},"title":"{title}","type":{{"@type":"chatTypeSupergroup","supergroup_id":{chat},"is_channel":false}},"unread_count":0}}}}"#
                ),
                format!(
                    r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{chat},"is_forum":true,"has_forum_tabs":true}}}}"#
                ),
                format!(
                    r#"{{"@type":"updateChatPosition","chat_id":{chat},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"{order}","is_pinned":false}}}}"#
                ),
                format!(
                    r#"{{"@type":"updateChatLastMessage","chat_id":{chat},"last_message":{{"id":{last},"chat_id":{chat},"sender_id":{{"@type":"messageSenderUser","user_id":81}},"date":{date},"is_outgoing":false,"topic_id":{{"@type":"messageTopicForum","forum_topic_id":{newest}}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Latest in {said}","entities":[]}}}}}},"positions":[{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"{order}","is_pinned":false}}]}}"#,
                    date = now.saturating_sub(120 + 600 * n as u64)
                ),
            ];
            for json in updates {
                if let Some(owned) = copy_and_parse(&json, &seq, &sink) {
                    session.apply(owned);
                }
            }
            if topics.is_empty() {
                continue;
            }
            let extra = session.request(RequestPurpose::GetForumTopics, Some(ChatId(*chat)));
            let list: Vec<String> = topics
                .iter()
                .map(|(id, name, unread)| topic(*chat, *id, name, *unread))
                .collect();
            let answer = format!(
                r#"{{"@type":"forumTopics","@extra":"{}","total_count":{},"topics":[{}],"next_offset_date":0,"next_offset_message_id":0,"next_offset_forum_topic_id":0}}"#,
                extra.0,
                list.len(),
                list.join(",")
            );
            if let Some(owned) = copy_and_parse(&answer, &seq, &sink) {
                session.apply(owned);
            }
        }
        self.demo_ui
            .seq
            .store(seq.load(Ordering::SeqCst), Ordering::SeqCst);
        self.connection
            .set_note(StatusNote::silent("screenshot demo — forum topic titles"));
    }
}
