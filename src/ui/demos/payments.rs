//! Screenshot demos: payments.

use crate::ui::app::QuillApp;
use crate::ui::demo::seed_ready_media_session;
use crate::ui::inline_playback::apply_ready_video_viewer;
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
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum VideoPlaybackDemo {
    VideoPip,
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
        // file 96). The demo extracts + decodes frames synchronously
        // (blocking ~2 s) for a deterministic capture — real in-viewer
        // playback, not faked: the clock keeps ticking and the 125 ms
        // refresh shows the frame for the current clock position.
        // `viewer_demo_sync_frames` suppresses the async extraction that
        // `open_media_viewer` would otherwise start. The audio
        // engine is skipped (demo), like the audio slice.
        self.viewer.demo_sync_frames = true;
        self.open_media_viewer(ChatId(11), MessageId(204), cx);
        if let Some(item) = self.viewer.state.current().cloned()
            && let Some(path) = self.viewer_clip_path(&item)
        {
            // Demo caches use negative IDs, outside TDLib’s live file-ID range.
            let file_id = -item.play_file_id.map(|id| id.0).unwrap_or(0);
            let cache = quill::video::viewer_frame_cache_dir(file_id);
            let mime = item.mime_type.clone().unwrap_or_default();
            let duration = item.duration_secs.unwrap_or(0);
            let start_timestamp = item.start_timestamp.unwrap_or(0);
            if let Ok(viewer_frames) = quill::video::viewer_playback_frames(
                &path,
                &mime,
                &cache,
                start_timestamp,
                duration,
            ) && let Ok(decoded) = Self::decode_viewer_frames(&viewer_frames.frames)
            {
                self.viewer.video_frames = decoded;
                self.viewer.video_fps = viewer_frames.fps;
                self.viewer.frame_cache_file = Some(file_id);
                self.play_viewer_video(&item, &path, cx);
                if let Some(clock) = self.viewer.clock.as_mut() {
                    clock.seek(5.0);
                }
            }
        }
        // Keep `viewer_demo_sync_frames` true so no background extraction
        // races the synchronously decoded frames.
        self.connection.status_note = "screenshot demo — in-viewer video playback".into();
        if demo == VideoPlaybackDemo::VideoPip {
            if let Some(clock) = self.viewer.clock.as_mut() {
                clock.seek(0.0);
            }
            let weak = cx.entity().downgrade();
            cx.defer(move |cx| {
                let _ = weak.update(cx, |this, cx| this.open_video_pip(cx));
            });
        }
    }
}
