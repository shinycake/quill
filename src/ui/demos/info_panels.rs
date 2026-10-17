//! Screenshot demos: the info panel of a channel, a group and a bot.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::state::{InfoPanelTarget, Session};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

register_demos![
    // A verified broadcast channel the viewer is a plain subscriber of, with
    // its info panel open: header badge, title and description.
    DemoSpec::chats(
        "ready-info-channel",
        "screenshot demo — verified channel info panel"
    )
    .setup(QuillApp::demo_info_channel),
];

const CHANNEL: i64 = 71;

fn apply_info_channel(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let jsons = [
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{CHANNEL},"title":"Quill News","type":{{"@type":"chatTypeSupergroup","supergroup_id":{CHANNEL},"is_channel":true}},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{CHANNEL},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"80","is_pinned":false}}}}"#
        ),
        format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{CHANNEL},"is_channel":true,"usernames":{{"@type":"usernames","active_usernames":["quillnews"],"disabled_usernames":[],"editable_username":"quillnews","collectible_usernames":[]}},"status":{{"@type":"chatMemberStatusMember"}},"member_count":12345,"verification_status":{{"@type":"verificationStatus","is_verified":true,"is_scam":false,"is_fake":false,"bot_verification_icon_custom_emoji_id":"0"}}}}}}"#
        ),
        format!(
            r#"{{"@type":"updateSupergroupFullInfo","supergroup_id":{CHANNEL},"supergroup_full_info":{{"description":"Release notes and tips for Quill. Read the changelog at https://example.com/changelog or write to news@example.com.","member_count":12345}}}}"#
        ),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(ChatId(CHANNEL));
}

impl QuillApp {
    fn demo_info_channel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_info_channel(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.open_info_panel_target(InfoPanelTarget::Supergroup(CHANNEL), window, cx);
        self.connection
            .set_note(StatusNote::silent("screenshot demo — channel info panel"));
        cx.notify();
    }
}
