//! Screenshot demo: the main window's bar for a joined voice chat, for the
//! tooltips of its icon-only mute and leave buttons (hover with
//! `QUILL_DEMO_CLICK=m:x,y`; injected, no live Telegram, no call).
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::state::ActiveGroupCall;

register_demos![
    DemoSpec::chats(
        "ready-group-call-bar",
        "screenshot demo — joined voice chat bar in the main window"
    )
    .setup(QuillApp::demo_group_call_bar)
];

impl QuillApp {
    fn demo_group_call_bar(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            session.calls.active_group_call = Some(ActiveGroupCall {
                is_joined: true,
                is_video_chat: true,
                participant_count: 12,
                title: "Weekly sync".into(),
                ..ActiveGroupCall::fresh(4401)
            });
        }
        cx.notify();
    }
}
