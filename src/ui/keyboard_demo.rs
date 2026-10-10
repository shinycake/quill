//! `ready-reply-keyboard` screenshot demo: a bot chat whose keyboard comes
//! from `updateChatReplyMarkup` (its message is not in the loaded history),
//! with text, share-phone, share-users and share-chat buttons, and the
//! share dialogs. `QUILL_DEMO_KEYBOARD_VIEW=keyboard|hidden|phone|users|chat|confirm`
//! (default `keyboard`).

use super::app::QuillApp;
use super::bots::apply_ready_bot_chat;
use super::request_share::RequestShareKind;
use gpui_kit::gpui::Context;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{RequestChatSpec, RequestUsersSpec};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const BOT_CHAT: i64 = 21;
const KEYBOARD_MESSAGE: i64 = 9000;

fn apply(session: &mut Session, sink: &Arc<dyn DiagnosticSink>, seq: &AtomicU64, json: &str) {
    if let Some(owned) = copy_and_parse(json, seq, sink) {
        session.apply(owned);
    }
}

pub(super) fn apply_ready_reply_keyboard(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    apply_ready_bot_chat(session, sink, seq);
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let people = [(31, "Mia Chen"), (32, "Leo Park")];
    for (id, name) in people {
        let (first, last) = name.split_once(' ').unwrap_or((name, ""));
        for json in [
            format!(
                r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":"{first}","last_name":"{last}","usernames":null,"phone_number":"","status":{{"@type":"userStatusRecently"}},"type":{{"@type":"userTypeRegular"}}}}}}"#
            ),
            format!(
                r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{name}","type":{{"@type":"chatTypePrivate","user_id":{id}}},"unread_count":0}}}}"#
            ),
            format!(
                r#"{{"@type":"updateChatPosition","chat_id":{id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"{order}","is_pinned":false}}}}"#,
                order = 100 + id
            ),
        ] {
            apply(session, &dyn_sink, seq, &json);
        }
    }
    for (id, title) in [(41, "Weekend Hike"), (42, "Design Team")] {
        for json in [
            format!(
                r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{{"@type":"chatTypeBasicGroup","basic_group_id":{id}}},"unread_count":0}}}}"#
            ),
            format!(
                r#"{{"@type":"updateChatPosition","chat_id":{id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"{order}","is_pinned":false}}}}"#,
                order = 200 + id
            ),
        ] {
            apply(session, &dyn_sink, seq, &json);
        }
    }
    // The keyboard's message is not in the history window.
    let keyboard = format!(
        r#"{{"@type":"updateChatReplyMarkup","chat_id":{BOT_CHAT},"reply_markup_message":{{"id":{KEYBOARD_MESSAGE},"chat_id":{BOT_CHAT},"is_outgoing":false,"reply_markup":{{"@type":"replyMarkupShowKeyboard","rows":[[{{"@type":"keyboardButton","text":"Yes","type":{{"@type":"keyboardButtonTypeText"}}}},{{"@type":"keyboardButton","text":"No","type":{{"@type":"keyboardButtonTypeText"}}}}],[{{"@type":"keyboardButton","text":"Share my number","type":{{"@type":"keyboardButtonTypeRequestPhoneNumber"}}}}],[{{"@type":"keyboardButton","text":"Pick friends","type":{{"@type":"keyboardButtonTypeRequestUsers","id":1,"restrict_user_is_bot":true,"user_is_bot":false,"restrict_user_is_premium":false,"user_is_premium":false,"max_quantity":2,"request_name":false,"request_username":false,"request_photo":false}}}},{{"@type":"keyboardButton","text":"Pick a group","type":{{"@type":"keyboardButtonTypeRequestChat","id":2,"chat_is_channel":false,"restrict_chat_is_forum":false,"chat_is_forum":false,"restrict_chat_has_username":false,"chat_has_username":false,"chat_is_created":false,"bot_is_member":false,"request_title":false,"request_username":false,"request_photo":false}}}}]],"is_persistent":false,"resize_keyboard":true,"one_time":false,"is_personal":false,"force_reply":false,"input_field_placeholder":""}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Choose an option","entities":[]}}}}}}}}"#
    );
    apply(session, &dyn_sink, seq, &keyboard);
}

impl QuillApp {
    /// Open the dialog or state the scene `QUILL_DEMO_KEYBOARD_VIEW` names.
    pub(super) fn demo_setup_reply_keyboard(&mut self, cx: &mut Context<Self>) {
        let view = std::env::var("QUILL_DEMO_KEYBOARD_VIEW").unwrap_or_default();
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui
                .seq
                .store(session.last_seq, std::sync::atomic::Ordering::SeqCst);
            apply_ready_reply_keyboard(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        let chat = ChatId(BOT_CHAT);
        let message = MessageId(KEYBOARD_MESSAGE);
        match view.as_str() {
            "hidden" => {
                self.message_ui
                    .collapsed_keyboards
                    .insert((BOT_CHAT, KEYBOARD_MESSAGE));
            }
            "phone" => self.open_request_share(
                chat,
                message,
                RequestShareKind::Phone { bot_user_id: 21 },
                cx,
            ),
            "users" | "confirm" => {
                self.open_request_share(
                    chat,
                    message,
                    RequestShareKind::Users(RequestUsersSpec {
                        id: 1,
                        user_is_bot: Some(false),
                        user_is_premium: None,
                        max_quantity: 2,
                    }),
                    cx,
                );
                if view == "confirm" {
                    self.demo_select_request_peers(&[ChatId(31), ChatId(32)]);
                }
            }
            "chat" => self.open_request_share(
                chat,
                message,
                RequestShareKind::Chat(RequestChatSpec {
                    id: 2,
                    chat_is_channel: false,
                    chat_is_forum: None,
                    chat_has_username: None,
                    chat_is_created: false,
                    bot_is_member: false,
                }),
                cx,
            ),
            _ => {}
        }
        self.connection.status_note = "screenshot demo — bot reply keyboard".into();
    }
}
