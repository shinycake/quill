//! Screenshot demo of the live PiP (injected, no live Telegram, no call):
//! the demo channel's playing RTMP stream (`group_call_stream_demo`) in
//! the PiP window with its LIVE badge and controls, its stream window
//! closed. Capture the PiP with `QUILL_DEMO_CAPTURE_WINDOWS=1`.
#![deny(clippy::indexing_slicing)]

use super::live_pip::LivePipSource;
use crate::ui::app::QuillApp;
use crate::ui::group_call_stream_layout::demo::{CALL, demo_stream_feed, seed_rtmp_stream};
use crate::ui::groups::apply_ready_channels;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::video_player::{Purpose, VideoPlayer};
use gpui_kit::*;
use quill::calls::engine::StreamState;
use quill::diagnostics::DiagnosticSink;
use std::sync::Arc;
use std::sync::atomic::Ordering;

register_demos![
    DemoSpec::chats(
        "ready-rtmp-pip",
        "screenshot demo — live stream in Picture-in-Picture"
    )
    .setup(QuillApp::demo_rtmp_pip),
];

impl QuillApp {
    fn demo_rtmp_pip(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_channels(session, &self.demo_ui.sink, &self.demo_ui.seq);
            let sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
            seed_rtmp_stream(session, StreamState::Live, &sink, &self.demo_ui.seq);
        }
        let Some(feed) = demo_stream_feed(cx) else {
            return;
        };
        // As after the stream window's PiP button: the window stays
        // closed while the PiP plays its player.
        self.group_call.window_closed_by_user = Some(CALL);
        let player = VideoPlayer::live(feed.sink.clone(), Purpose::Live { tile: false }, cx);
        self.group_call.stream.player = Some(player.clone());
        self.group_call.stream.demo_feed = Some(feed);
        self.open_live_pip(player, LivePipSource::Stream { call_id: CALL }, None, cx);
        cx.notify();
    }
}
