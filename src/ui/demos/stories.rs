//! Screenshot demos: stories.

use crate::ui::app::QuillApp;
use crate::ui::groups::{apply_ready_channels, apply_ready_channels_admin};
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::sponsored::apply_ready_sponsored;
use gpui_kit::*;
use std::sync::atomic::Ordering;

register_demos![
    // Broadcast channel demo (injected, no live Telegram): the ungated demo
    // channel (id 13) renders broadcast posts with channel author + view
    // counts, composer hidden for the non-admin viewer, and the join/leave
    // footer.
    DemoSpec::chats(
        "ready-channels",
        "screenshot demo — broadcast channel posts + join footer"
    )
    .setup(QuillApp::demo_ready_channels),
    // Broadcast channel demo (injected, no live Telegram): the demo channel
    // (id 13) with the viewer as an administrator
    // (`rights.can_post_messages: true`), so the composer is visible above
    // the broadcast posts (Phase 2.3).
    DemoSpec::chats(
        "ready-channels-admin",
        "screenshot demo — broadcast channel, admin composer"
    )
    .setup(QuillApp::demo_ready_channels_admin),
    // Channel sponsored / recommended rows + report flow (injected, no live Telegram).
    // Fixture/proof surface only; the channel opens normally in live use.
    DemoSpec::chats(
        "ready-sponsored",
        "screenshot demo — sponsored / recommended channel rows"
    )
    .setup(QuillApp::demo_ready_sponsored),
];

impl QuillApp {
    fn demo_ready_channels(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_channels(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.connection.status_note = "screenshot demo — broadcast channel".into();
    }

    fn demo_ready_channels_admin(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer.update(cx, |input, cx| {
            input.set_value("admin post — hello from the channel", window, cx);
        });
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_channels_admin(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.connection.status_note = "screenshot demo — broadcast channel admin".into();
    }

    fn demo_ready_sponsored(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_sponsored(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.connection.status_note = "screenshot demo — sponsored messages".into();
    }
}
