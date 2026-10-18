//! Screenshot demo of the top-people strip in the empty search
//! (`QUILL_DEMO_TOP_PEOPLE=few` shows a strip that fits one row).

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::diagnostics::DiagnosticSink;
use quill::ids::ChatId;
use quill::state::SearchStatus;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

register_demos![
    DemoSpec::chats(
        "ready-top-people",
        "screenshot demo — search: top people strip"
    )
    .setup(QuillApp::demo_top_people),
];

const PEOPLE: [(i64, &str, i32); 9] = [
    (31, "Alice Cooper", 3),
    (32, "Bob Stone", 0),
    (33, "Carol Reyes", 128),
    (34, "Dmitri Volkov", 0),
    (35, "Elena Marsh", 1),
    (36, "Farid Haddad", 0),
    (37, "Grace Lin", 0),
    (38, "Hugo Becker", 12),
    (39, "Ines Duarte", 0),
];

impl QuillApp {
    fn demo_top_people(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        let few = std::env::var("QUILL_DEMO_TOP_PEOPLE").is_ok_and(|v| v == "few");
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        let seq = AtomicU64::new(session.last_seq);
        let sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        let mut top = Vec::new();
        for (index, (id, title, unread)) in PEOPLE.iter().enumerate() {
            if few && index >= 4 {
                break;
            }
            let json = format!(
                r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{{"@type":"chatTypePrivate","user_id":{id}}},"unread_count":{unread},"last_read_inbox_message_id":0,"last_read_outbox_message_id":0}}}}"#
            );
            if let Some(owned) = copy_and_parse(&json, &seq, &sink) {
                session.apply(owned);
            }
            top.push(ChatId(*id));
        }
        self.demo_ui
            .seq
            .store(seq.load(Ordering::SeqCst), Ordering::SeqCst);
        if let Some(chat) = session.chats.get_mut(&35) {
            chat.is_marked_as_unread = true;
        }
        if let Some(chat) = session.chats.get_mut(&33) {
            chat.notification_settings =
                quill::telegram::envelope::ChatNotificationSettings::default()
                    .with_mute_for(quill::telegram::envelope::MUTE_FOREVER);
        }
        session.open_search();
        session.search.search.top_chats = top;
        session.search.search.chat_ids = vec![ChatId(11), ChatId(12)];
        session.search.search.recents = true;
        session.search.search.status = SearchStatus::Ready;
        self.connection
            .set_note(StatusNote::silent("screenshot demo — top people strip"));
    }
}
