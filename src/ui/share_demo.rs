//! Screenshot demos for the share box, the forward bar and "send as"
//! (English fixtures, injected updates, no live Telegram).

use super::app::QuillApp;
use super::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::composer::ForwardDraft;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::state::{RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Extra chats for the share box: a friend, a group where the user can post
/// as several identities, a channel and a premium-only channel.
pub(super) fn apply_ready_share(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let jsons = [
        r#"{"@type":"updateUser","user":{"id":1,"first_name":"Alex","last_name":"Morgan","type":{"@type":"userTypeRegular"}}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":21,"title":"Maya Cohen","type":{"@type":"chatTypePrivate","user_id":21},"unread_count":0}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":22,"title":"Design Team","type":{"@type":"chatTypeSupergroup","supergroup_id":22,"is_channel":false},"unread_count":0,"message_sender_id":{"@type":"messageSenderUser","user_id":1}}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":23,"title":"Design Updates","type":{"@type":"chatTypeSupergroup","supergroup_id":23,"is_channel":true},"unread_count":0}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":24,"title":"Pixel Weekly","type":{"@type":"chatTypeSupergroup","supergroup_id":24,"is_channel":true},"unread_count":0}}"#,
        r#"{"@type":"updateChatPosition","chat_id":21,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"18","is_pinned":false}}"#,
        r#"{"@type":"updateChatPosition","chat_id":22,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"16","is_pinned":false}}"#,
        r#"{"@type":"updateNewMessage","message":{"id":7,"chat_id":22,"is_outgoing":false,"date":1790632200,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Final mockups are in the shared folder.","entities":[]}}}}"#,
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// The `getChatAvailableMessageSenders` answer for the group.
fn apply_send_as_options(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let extra = session.request(
        RequestPurpose::GetChatAvailableMessageSenders,
        Some(ChatId(22)),
    );
    let json = format!(
        r#"{{"@type":"chatMessageSenders","@extra":"{}","senders":[{{"@type":"chatMessageSender","sender":{{"@type":"messageSenderUser","user_id":1}},"needs_premium":false}},{{"@type":"chatMessageSender","sender":{{"@type":"messageSenderChat","chat_id":22}},"needs_premium":false}},{{"@type":"chatMessageSender","sender":{{"@type":"messageSenderChat","chat_id":23}},"needs_premium":false}},{{"@type":"chatMessageSender","sender":{{"@type":"messageSenderChat","chat_id":24}},"needs_premium":true}}]}}"#,
        extra.0
    );
    if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

register_demos![
    // B4: the share box with two destinations ticked and a comment.
    DemoSpec::chats(
        "ready-share-box",
        "screenshot demo — share box, forward bar and send as (injected)"
    )
    .setup(|app, window, cx| app.demo_setup_share(ShareDemo::ShareBox, window, cx)),
    // B4: the forward bar above the destination chat's composer.
    DemoSpec::chats(
        "ready-forward-bar",
        "screenshot demo — share box, forward bar and send as (injected)"
    )
    .setup(|app, window, cx| app.demo_setup_share(ShareDemo::ForwardBar, window, cx)),
    // B4: the composer's "send as" identity list.
    DemoSpec::chats(
        "ready-send-as",
        "screenshot demo — share box, forward bar and send as (injected)"
    )
    .setup(|app, window, cx| app.demo_setup_share(ShareDemo::SendAs, window, cx)),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum ShareDemo {
    ShareBox,
    ForwardBar,
    SendAs,
}

impl QuillApp {
    fn demo_setup_share(&mut self, demo: ShareDemo, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_share(session, &self.demo_sink, &self.demo_seq);
            if matches!(demo, ShareDemo::SendAs) {
                apply_send_as_options(session, &self.demo_sink, &self.demo_seq);
                session.open_chat(ChatId(22));
            }
            if matches!(demo, ShareDemo::ForwardBar) {
                session.open_chat(ChatId(21));
            }
        }
        let mut draft =
            ForwardDraft::from_message(ChatId(11), MessageId(101), false).expect("forward 101");
        match demo {
            ShareDemo::ShareBox => {
                draft.toggle(ChatId(11), MessageId(103), false);
                self.share.pending_forward = Some(draft);
                self.share.forward_picker_open = true;
                self.share.selection.toggle(ChatId(21));
                self.share.selection.toggle(ChatId(22));
                self.share.comment_input.update(cx, |input, cx| {
                    input.set_value("Thought you'd like this", window, cx);
                });
                self.status_note = "screenshot demo — share box (two chats ticked)".into();
            }
            ShareDemo::ForwardBar => {
                self.share.pending_forward = Some(draft);
                self.share.forward_bar_dest = Some(ChatId(21));
                self.composer.update(cx, |input, cx| {
                    input.set_value("Check this out", window, cx);
                    input.focus(window, cx);
                });
                self.status_note = "screenshot demo — forward bar in the destination".into();
            }
            ShareDemo::SendAs => {
                self.composer_ui.send_as_open = true;
                self.status_note = "screenshot demo — send as".into();
            }
        }
    }
}
