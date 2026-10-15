//! Screenshot demo of the "Stream with other apps" box: the demo channel
//! (id 13, where the viewer is made an administrator with video chat rights) has no
//! video chat, and the server's answer is in. Placeholder values only; no
//! request is sent.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::groups::apply_ready_channels;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::calls::rtmp_start::{RtmpInfo, RtmpStart};
use quill::ids::ChatId;
use quill::telegram::envelope::ChannelMemberStatus;
use std::sync::atomic::Ordering;

/// The demo channel.
pub(super) const CHANNEL: i64 = 13;

register_demos![
    DemoSpec::chats(
        "ready-rtmp-start-box",
        "screenshot demo — Stream with other apps"
    )
    .setup(QuillApp::demo_rtmp_start_box)
];

impl QuillApp {
    fn demo_rtmp_start_box(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_channels(session, &self.demo_ui.sink, &self.demo_ui.seq);
            seed_rtmp_start(session);
        }
        cx.notify();
    }
}

/// The answer is in: the box has its Server URL and Stream Key.
pub(super) fn seed_rtmp_start(session: &mut quill::state::Session) {
    if let Some(chat) = session.chats.get_mut(&CHANNEL) {
        chat.video_chat = None;
        // The viewer administers the channel with video chat rights.
        chat.set_member_status(ChannelMemberStatus::Administrator, Some(true));
        chat.set_admin_can_manage_video_chats(Some(true));
    }
    let mut start = RtmpStart::new(ChatId(CHANNEL));
    start.accept(
        ChatId(CHANNEL),
        RtmpInfo {
            url: "rtmps://dc4-1.rtmp.t.me/s/".into(),
            key: "0000000000:demo-placeholder-key".into(),
        },
    );
    session.calls.rtmp_start = Some(start);
}
