//! Screenshot demos of starting a live stream in the demo channel (id 13,
//! where the viewer is made an administrator with the video chat right):
//! the create confirmation and the schedule box. No request is sent.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::groups::apply_ready_channels;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::calls::group_call_create::CreateKind;
use quill::ids::ChatId;
use quill::telegram::envelope::{ChannelMemberStatus, ChatKind};
use std::sync::atomic::Ordering;

const CHANNEL: i64 = 13;

register_demos![
    DemoSpec::chats(
        "ready-group-call-create",
        "screenshot demo — start a live stream"
    )
    .setup(QuillApp::demo_group_call_create),
    DemoSpec::chats(
        "ready-group-call-schedule",
        "screenshot demo — schedule a live stream"
    )
    .setup(QuillApp::demo_group_call_schedule),
];

impl QuillApp {
    fn demo_group_call_manager(&mut self) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        apply_ready_channels(session, &self.demo_ui.sink, &self.demo_ui.seq);
        let Some(chat) = session.chats.get_mut(&CHANNEL) else {
            return;
        };
        chat.video_chat = None;
        if let ChatKind::Supergroup { supergroup_id, .. } = chat.kind {
            session
                .groups
                .supergroup_member_status
                .insert(supergroup_id, ChannelMemberStatus::Administrator);
            session
                .groups
                .supergroup_manage_video_chats_right
                .insert(supergroup_id, true);
        }
    }

    fn demo_group_call_create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_group_call_manager();
        self.begin_group_call_create(ChatId(CHANNEL), CreateKind::Now, window, cx);
    }

    fn demo_group_call_schedule(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_group_call_manager();
        self.begin_group_call_create(ChatId(CHANNEL), CreateKind::Schedule, window, cx);
    }
}
