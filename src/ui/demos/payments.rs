//! Screenshot demos: payments.

use crate::ui::app::QuillApp;
use crate::ui::demo::seed_ready_media_session;
use crate::ui::inline_playback::apply_ready_video_viewer;
use crate::ui::message_media::{apply_ready_dice, apply_ready_location};
use crate::ui::payments::apply_ready_payments;
use crate::ui::polls::apply_ready_poll;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use std::sync::atomic::Ordering;

register_demos![
    // Dice demo (injected, no live Telegram): a few `messageDice` rows —
    // 🎲 with values 4 / 6 and a 🎯 — showing the large static emoji
    // face plus the rolled value (Phase 4.4). No roll animation.
    DemoSpec::chats("ready-dice", "screenshot demo — dice rolls").setup(QuillApp::demo_ready_dice),
    // Location / venue / contact demo (injected, no live Telegram): a
    // plain `messageLocation` (coordinates + accuracy), a
    // `messageLiveLocation` (live-period / expires / heading /
    // proximity-alert state), a `messageVenue` (title + address +
    // provider), and a `messageContact` (name + phone + vCard +
    // `user_id`), each with a tappable "Open map" link (Phase 4.3).
    DemoSpec::chats(
        "ready-location",
        "screenshot demo — location / venue / contact"
    )
    .setup(QuillApp::demo_ready_location),
    // Media viewer demo (injected, no live Telegram): the ReadyMedia
    // photo chat with the viewer overlay open on the downloaded photo
    // (Phase 4.5).
    DemoSpec::ready(
        "ready-media-viewer",
        seed_ready_media_session,
        "screenshot demo — fullscreen media viewer"
    )
    .setup(QuillApp::demo_ready_media_viewer),
    // Slice P1 payments demo (injected, no live Telegram): a bot chat
    // with a `messageInvoice` (Buy button), a `messagePaymentSuccessful`
    // row, a paid invoice with a receipt link, and a seeded
    // `paymentForm` with the checkout dialog open (regular provider,
    // order fields, a saved credential, terms).
    DemoSpec::chats(
        "ready-payments",
        "screenshot demo — payments: invoice + checkout"
    )
    .setup(QuillApp::demo_ready_payments),
    // Poll demo (injected, no live Telegram): an open regular poll with a
    // voted option (percentage bars + counts, tapping an option flips the
    // chosen mark locally) and a closed poll (results, no voting
    // affordance) (Phase 4.2).
    DemoSpec::chats("ready-poll", "screenshot demo — polls: voted + closed")
        .setup(QuillApp::demo_ready_poll),
    DemoSpec::ready(
        "ready-video-pip",
        seed_ready_media_session,
        "screenshot demo — in-viewer video playback"
    )
    .setup(|app, window, cx| app.demo_video_playback(VideoPlaybackDemo::VideoPip, window, cx)),
    // Video-playback demo (injected, no live Telegram): the ReadyMedia
    // seed plus a downloaded video (message 204); the viewer opens on it
    // with playback faked mid-track (no audio — the tick
    // advances the elapsed label, like the seek-bars demo). The clip's
    // 12 s duration is fixture data for the screenshot.
    // (Parity slice 5.)
    DemoSpec::ready(
        "ready-video-playback",
        seed_ready_media_session,
        "screenshot demo — in-viewer video playback"
    )
    .setup(|app, window, cx| app.demo_video_playback(
        VideoPlaybackDemo::VideoPlayback,
        window,
        cx
    )),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum VideoPlaybackDemo {
    VideoPlayback,
    VideoPip,
}

impl QuillApp {
    fn demo_ready_dice(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_dice(session, &self.demo_sink, &self.demo_seq);
        }
        self.status_note = "screenshot demo — dice rolls".into();
    }

    fn demo_ready_location(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_location(session, &self.demo_sink, &self.demo_seq);
        }
        self.status_note = "screenshot demo — location / venue / contact".into();
    }

    fn demo_ready_media_viewer(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        // The Media seed opens chat 11 with a downloaded photo
        // (message 201, "Loaded photo") and a pending one (202, loading
        // state); the document (203) is not viewer-openable.
        self.open_media_viewer(ChatId(11), MessageId(201), cx);
        self.status_note = "screenshot demo — fullscreen media viewer".into();
    }

    fn demo_ready_payments(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Slice P1: invoice card, payment rows, and the checkout dialog.
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_payments(session, &self.demo_sink, &self.demo_seq);
        }
        self.open_payment_dialog(window, cx);
        self.status_note = "screenshot demo — payments: invoice + checkout".into();
    }

    fn demo_ready_poll(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_poll(session, &self.demo_sink, &self.demo_seq);
        }
        self.status_note = "screenshot demo — polls: voted + closed".into();
    }

    fn demo_video_playback(
        &mut self,
        demo: VideoPlaybackDemo,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_video_viewer(session, &self.demo_sink, &self.demo_seq);
        }
        // The Media seed plus a downloaded 12 s video (204, "Demo clip",
        // file 96). The demo extracts + decodes frames synchronously
        // (blocking ~2 s) for a deterministic capture — real in-viewer
        // playback, not faked: the clock keeps ticking and the 125 ms
        // refresh shows the frame for the current clock position.
        // `viewer_demo_sync_frames` suppresses the async extraction that
        // `open_media_viewer` would otherwise start. The audio
        // engine is skipped (demo), like the audio slice.
        self.viewer_demo_sync_frames = true;
        self.open_media_viewer(ChatId(11), MessageId(204), cx);
        if let Some(item) = self.media_viewer.current().cloned()
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
                self.viewer_video_frames = decoded;
                self.viewer_video_fps = viewer_frames.fps;
                self.viewer_frame_cache_file = Some(file_id);
                self.play_viewer_video(&item, &path, cx);
                if let Some(clock) = self.viewer_clock.as_mut() {
                    clock.seek(5.0);
                }
            }
        }
        // Keep `viewer_demo_sync_frames` true so no background extraction
        // races the synchronously decoded frames.
        self.status_note = "screenshot demo — in-viewer video playback".into();
        if demo == VideoPlaybackDemo::VideoPip {
            if let Some(clock) = self.viewer_clock.as_mut() {
                clock.seek(0.0);
            }
            let weak = cx.entity().downgrade();
            cx.defer(move |cx| {
                let _ = weak.update(cx, |this, cx| this.open_video_pip(cx));
            });
        }
    }
}
