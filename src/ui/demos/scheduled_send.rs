//! Screenshot demos: a message scheduled in a private chat (injected, no
//! live Telegram). TDLib answers the send with the scheduled copy and then
//! saves it; the chat's history stays as it was, the composer shows the
//! scheduled-messages button, and the second demo opens that list.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::ids::ChatId;
use quill::telegram::client::copy_and_parse;
use std::sync::atomic::Ordering;

const CHAT: i64 = 31;

register_demos![
    DemoSpec::chats(
        "ready-scheduled-send",
        "screenshot demo — a scheduled send stays out of the chat's history"
    )
    .setup(QuillApp::demo_scheduled_send),
    DemoSpec::chats(
        "ready-scheduled-send-list",
        "screenshot demo — a scheduled send in the scheduled-messages list"
    )
    .setup(QuillApp::demo_scheduled_send_list),
];

fn text_message(id: i64, outgoing: bool, date: i64, extra: &str, text: &str) -> String {
    format!(
        r#"{{"id":{id},"chat_id":{CHAT},"is_outgoing":{outgoing},"date":{date},{extra}"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}}}"#
    )
}

impl QuillApp {
    fn demo_scheduled_send(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
            self.demo_ui.sink.clone();
        session.open_chat(ChatId(CHAT));
        let now = quill::local_time::now_unix();
        let scheduled = format!(
            r#""scheduling_state":{{"@type":"messageSchedulingStateSendAtDate","send_date":{}}},"#,
            now + 3 * 3600
        );
        let pending = format!(
            r#"{scheduled}"sending_state":{{"@type":"messageSendingStatePending","sending_id":0}},"#
        );
        let later = "Happy birthday! Call you tonight";
        let jsons = [
            format!(
                r#"{{"@type":"updateUser","user":{{"id":{CHAT},"first_name":"Maya","type":{{"@type":"userTypeRegular"}}}}}}"#
            ),
            format!(
                r#"{{"@type":"updateNewChat","chat":{{"id":{CHAT},"title":"Maya","type":{{"@type":"chatTypePrivate","user_id":{CHAT}}},"unread_count":0}}}}"#
            ),
            format!(
                r#"{{"@type":"updateChatPosition","chat_id":{CHAT},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"50","is_pinned":false}}}}"#
            ),
            format!(
                r#"{{"@type":"updateNewMessage","message":{}}}"#,
                text_message(401, false, now - 600, "", "Are you free later today?")
            ),
            format!(
                r#"{{"@type":"updateNewMessage","message":{}}}"#,
                text_message(402, true, now - 540, "", "Busy until the evening")
            ),
            // The scheduled send: its temporary copy, then the saved one.
            format!(
                r#"{{"@type":"updateNewMessage","message":{}}}"#,
                text_message(-1, true, now, &pending, later)
            ),
            format!(
                r#"{{"@type":"updateMessageSendSucceeded","old_message_id":-1,"message":{}}}"#,
                text_message(1_048_580, true, now, &scheduled, later)
            ),
            format!(
                r#"{{"@type":"updateChatHasScheduledMessages","chat_id":{CHAT},"has_scheduled_messages":true}}"#
            ),
        ];
        for json in jsons {
            if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &sink) {
                session.apply(owned);
            }
        }
    }

    fn demo_scheduled_send_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_scheduled_send(window, cx);
        self.open_scheduled_dialog(cx);
    }
}
