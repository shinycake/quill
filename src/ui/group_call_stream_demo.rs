//! Screenshot demos of a channel's live stream window (injected, no live
//! Telegram, no call): the demo channel (id 13) with a photo and a running
//! RTMP stream that is connecting, playing a still picture, or not
//! broadcasting. The stream window opens beside the main window; capture
//! it with `QUILL_DEMO_CAPTURE_WINDOWS=1`.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::demo::{demo_file_json, demo_media_allowlist};
use crate::ui::groups::apply_ready_channels;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::calls::engine::{GroupCallMode, StreamState, VideoFrame};
use quill::diagnostics::DiagnosticSink;
use quill::state::{ActiveGroupCall, Session, VideoChatInfo};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// The demo channel and its stream.
pub(super) const CHANNEL: i64 = 13;
pub(super) const CALL: i32 = 4401;
const PHOTO_FILE: i32 = 4402;

register_demos![
    DemoSpec::chats(
        "ready-rtmp-connecting",
        "screenshot demo — live stream window, connecting"
    )
    .setup(QuillApp::demo_rtmp_connecting),
    DemoSpec::chats(
        "ready-rtmp-live",
        "screenshot demo — live stream window, playing"
    )
    .setup(QuillApp::demo_rtmp_live),
    DemoSpec::chats(
        "ready-rtmp-no-stream",
        "screenshot demo — live stream window, nobody broadcasting"
    )
    .setup(QuillApp::demo_rtmp_no_stream),
];

impl QuillApp {
    fn demo_rtmp_connecting(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.demo_rtmp(StreamState::Connecting, cx);
    }

    fn demo_rtmp_live(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.group_call.stream.demo_frame = demo_stream_frame().map(Arc::new);
        self.demo_rtmp(StreamState::Live, cx);
    }

    fn demo_rtmp_no_stream(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.demo_rtmp(StreamState::NoStream, cx);
    }

    fn demo_rtmp(&mut self, stream: StreamState, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_channels(session, &self.demo_ui.sink, &self.demo_ui.seq);
            let sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
            seed_rtmp_stream(session, stream, &sink, &self.demo_ui.seq);
        }
        // The capture shows the controls, as with the mouse over the picture.
        self.group_call.stream.controls.show_now();
        cx.notify();
    }
}

/// The demo channel gets a photo and a joined RTMP live stream in `stream`.
pub(super) fn seed_rtmp_stream(
    session: &mut Session,
    stream: StreamState,
    sink: &Arc<dyn DiagnosticSink>,
    seq: &AtomicU64,
) {
    let photo = demo_media_allowlist()
        .join("showcase/av07.png")
        .to_string_lossy()
        .into_owned();
    let file = format!(
        r#"{{"@type":"updateFile","file":{}}}"#,
        demo_file_json(PHOTO_FILE, &photo, true)
    );
    if let Some(owned) = copy_and_parse(&file, seq, sink) {
        session.apply(owned);
    }
    if let Some(chat) = session.chats.get_mut(&CHANNEL) {
        chat.photo_file_id = Some(PHOTO_FILE);
        chat.video_chat = Some(VideoChatInfo {
            group_call_id: CALL,
            has_participants: true,
            default_participant_id: None,
        });
    }
    session.calls.active_group_call = Some(ActiveGroupCall {
        is_rtmp_stream: true,
        is_joined: true,
        stream_mode: GroupCallMode::Rtmp,
        stream,
        viewer_count: 12_480,
        ..ActiveGroupCall::fresh(CALL)
    });
}

/// A still picture for the playing stream: the showcase photo as a frame.
pub(super) fn demo_stream_frame() -> Option<VideoFrame> {
    let path = demo_media_allowlist().join("showcase/p2.jpg");
    let rgba = image::open(path).ok()?.to_rgba8();
    Some(VideoFrame {
        seq: 1,
        width: u16::try_from(rgba.width()).ok()?,
        height: u16::try_from(rgba.height()).ok()?,
        rgba: rgba.into_raw(),
        is_local: false,
        participant_user_id: None,
        is_screen: false,
    })
}
