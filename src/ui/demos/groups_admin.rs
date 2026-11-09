//! Screenshot demos: groups admin.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::groups::apply_ready_group_manage;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::diagnostics::DiagnosticSink;
use quill::ids::ChatId;
use quill::state::{GroupsPurpose, InfoPanelTarget, RequestPurpose};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::Ordering;

register_demos![
    // Group info demo (injected, no live Telegram): the
    // `ReadyGroupManage` demo supergroup (id 61, viewer an admin with
    // `can_change_info`) with the info panel open on its "Manage group"
    // entry.
    DemoSpec::chats("ready-group-info-edit", "screenshot demo — group info edit")
        .setup(QuillApp::demo_ready_group_info_edit),
    // The join-request box of that group after Add to Group was refused.
    // `QUILL_DEMO_JOIN_ERROR=gone` (default): the request was withdrawn
    // or handled elsewhere; `=other`: any other refusal.
    DemoSpec::chats(
        "ready-join-request-refused",
        "screenshot demo — join request refused"
    )
    .setup(QuillApp::demo_join_request_refused),
];

impl QuillApp {
    fn demo_ready_group_info_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // The group-management fixture (viewer 777 is an admin with
        // `can_change_info`) with the info panel open instead of the
        // member dialog.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_group_manage(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.open_info_panel_target(InfoPanelTarget::Supergroup(61), window, cx);
        self.set_note(StatusNote::silent("screenshot demo — group info edit"));
        cx.notify();
    }

    fn demo_join_request_refused(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let gone = std::env::var("QUILL_DEMO_JOIN_ERROR").as_deref() != Ok("other");
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_group_manage(session, &self.demo_ui.sink, &self.demo_ui.seq);
            let dyn_sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
            let chat = ChatId(61);
            let page = session.request(RequestPurpose::GetChatJoinRequests, Some(chat));
            let row = |user: i64| {
                format!(
                    r#"{{"@type":"chatJoinRequest","user_id":{user},"date":1788500000,"bio":""}}"#
                )
            };
            let process = session.request(
                RequestPurpose::Groups(GroupsPurpose::ProcessChatJoinRequest { user_id: 2 }),
                Some(chat),
            );
            let message = if gone {
                "HIDE_REQUESTER_MISSING"
            } else {
                "CHANNELS_TOO_MUCH"
            };
            for json in [
                format!(
                    r#"{{"@type":"chatJoinRequests","@extra":"{}","total_count":3,"requests":[{},{},{}]}}"#,
                    page.0,
                    row(2),
                    row(5),
                    row(6)
                ),
                format!(
                    r#"{{"@type":"error","@extra":"{}","code":400,"message":"{message}"}}"#,
                    process.0
                ),
            ] {
                if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &dyn_sink) {
                    session.apply(owned);
                }
            }
        }
        self.open_join_requests_dialog(ChatId(61), window, cx);
        cx.notify();
    }
}
