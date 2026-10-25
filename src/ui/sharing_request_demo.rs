#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::demo::seed_ready_chats_session;
use super::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::local_time::now_unix;
use quill::state::{EnableSharing, Session};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

register_demos![
    DemoSpec::ready(
        "ready-sharing-request",
        seed_sharing_request_session,
        "screenshot demo — a request to enable sharing with Reject / Accept (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_sharing_request),
];

pub(super) const CHAT: i64 = 4_031;
const ME: i64 = 4_001;
const ADA_TOGGLE: i64 = 1_048_576;
const MY_EXPIRED_REQUEST: i64 = 2_097_152;
const MY_REJECTED_REQUEST: i64 = 3_145_728;
const ADA_REJECTION: i64 = 4_194_304;
pub(super) const ADA_LIVE_REQUEST: i64 = 5_242_880;

pub(super) fn seed_sharing_request_session(sink: Arc<MemorySink>) -> Session {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let mut session = seed_ready_chats_session(sink);
    session.my_user_id = Some(ME);
    let seq = AtomicU64::new(session.last_seq);
    let mut apply = |json: &str| {
        if let Some(owned) = copy_and_parse(json, &seq, &dyn_sink) {
            session.apply(owned);
        }
    };
    apply(
        r#"{"@type":"updateUser","user":{"id":4031,"first_name":"Ada","last_name":"Lovelace","type":{"@type":"userTypeRegular"}}}"#,
    );
    apply(&format!(
        r#"{{"@type":"updateNewChat","chat":{{"id":{CHAT},"title":"Ada Lovelace","type":{{"@type":"chatTypePrivate","user_id":{CHAT}}},"unread_count":0,"has_protected_content":true}}}}"#
    ));
    let now = now_unix();
    let mut message = |id: i64, outgoing: bool, ago: i64, content: String| {
        let sender = if outgoing { ME } else { CHAT };
        apply(&format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{CHAT},"is_outgoing":{outgoing},"sender_id":{{"@type":"messageSenderUser","user_id":{sender}}},"date":{date},"content":{content}}}}}"#,
            date = now.saturating_sub(ago),
        ));
    };
    let toggled = |request: i64, old: bool, new: bool| {
        format!(
            r#"{{"@type":"messageChatHasProtectedContentToggled","request_message_id":{request},"old_has_protected_content":{old},"new_has_protected_content":{new}}}"#
        )
    };
    let request = |expired: bool| {
        format!(
            r#"{{"@type":"messageChatHasProtectedContentDisableRequested","is_expired":{expired}}}"#
        )
    };
    message(ADA_TOGGLE, false, 4 * 86_400, toggled(0, false, true));
    message(MY_EXPIRED_REQUEST, true, 3 * 86_400, request(true));
    message(MY_REJECTED_REQUEST, true, 7200, request(true));
    message(
        ADA_REJECTION,
        false,
        5400,
        toggled(MY_REJECTED_REQUEST, true, true),
    );
    message(ADA_LIVE_REQUEST, false, 600, request(false));
    session.open_chat(ChatId(CHAT));
    session
}

impl QuillApp {
    fn demo_sharing_request(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if std::env::var("QUILL_DEMO_SHARING").as_deref() == Ok("box")
            && let Some(session) = self.demo_session.as_mut()
        {
            session.groups.private_sharing.enable = Some(EnableSharing::AskToRequest(ChatId(CHAT)));
        }
        cx.notify();
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "sharing_request_tests.rs"]
mod tests;
