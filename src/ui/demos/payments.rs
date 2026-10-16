//! Screenshot demos: payments.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::demo::seed_ready_media_session;
use crate::ui::demos::video_fixtures::apply_ready_video_viewer;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use std::sync::atomic::Ordering;

register_demos![
    DemoSpec::ready(
        "ready-video-pip",
        seed_ready_media_session,
        "screenshot demo — in-viewer video playback"
    )
    .setup(|app, window, cx| app.demo_video_playback(VideoPlaybackDemo::VideoPip, window, cx)),
    DemoSpec::ready(
        "ready-video-controls",
        seed_ready_media_session,
        "screenshot demo — the viewer's video controller"
    )
    .setup(|app, window, cx| app.demo_video_playback(VideoPlaybackDemo::Controls, window, cx)),
    DemoSpec::ready(
        "ready-video-speed",
        seed_ready_media_session,
        "screenshot demo — the viewer's speed popover"
    )
    .setup(|app, window, cx| app.demo_video_playback(VideoPlaybackDemo::Speed, window, cx)),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum VideoPlaybackDemo {
    VideoPip,
    /// The controller with the pointer over the scrubber (thumb and time
    /// bubble) at 1.5×.
    Controls,
    /// The speed popover open.
    Speed,
}

impl QuillApp {
    fn demo_video_playback(
        &mut self,
        demo: VideoPlaybackDemo,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_video_viewer(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        // The Media seed plus a downloaded 12 s video (204, "Demo clip",
        // file 96), played by the viewer's player. Under
        // `QUILL_DEMO_CAPTURE` the player shows a still first picture
        // decoded synchronously (at most 2 s), so the capture is
        // deterministic; otherwise it really plays (with sound).
        self.open_media_viewer(ChatId(11), MessageId(204), cx);
        if let Some(player) = &self.viewer.player {
            // A seek there and back: the picture shown is the decoded
            // first one, after the seek path ran.
            player.seek(5.0);
            player.seek(0.0);
        }
        self.connection.status_note = "screenshot demo — in-viewer video playback".into();
        match demo {
            VideoPlaybackDemo::VideoPip => {
                let weak = cx.entity().downgrade();
                cx.defer(move |cx| {
                    let _ = weak.update(cx, |this, cx| this.open_video_pip(cx));
                });
            }
            VideoPlaybackDemo::Controls => {
                self.set_playback_speed(1.5, cx);
                self.viewer.controls.hover = Some(0.42);
            }
            VideoPlaybackDemo::Speed => {
                self.set_playback_speed(1.5, cx);
                self.viewer.controls.demo_speed_open = true;
            }
        }
    }
}
