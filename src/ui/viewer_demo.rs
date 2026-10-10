//! Screenshot-demo fixtures for the media viewer's GIF playback and Shared
//! Media paging (injected through the real reducers, no live Telegram).

use super::app::QuillApp;
use super::demo::{
    demo_file_json, demo_media_allowlist, demo_thumb_png_path, seed_ready_custom_emoji_session,
    seed_ready_media_session,
};
use super::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::state::{RequestPurpose, Session, SharedMediaTab};
use quill::telegram::client::copy_and_parse;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// A looping GIF whose caption carries the demo custom emoji (id 4242,
/// resolved by `seed_ready_custom_emoji_session`).
fn apply_viewer_gif(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let frame = demo_media_allowlist()
        .join("demo-gif-1.png")
        .to_string_lossy()
        .into_owned();
    let clip = demo_media_allowlist()
        .join("demo-gif.gif")
        .to_string_lossy()
        .into_owned();
    let thumb = demo_file_json(301, &frame, true);
    let animation = demo_file_json(302, &clip, true);
    let json = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":301,"chat_id":11,"date":1790632500,"is_outgoing":false,"content":{{"@type":"messageAnimation","animation":{{"@type":"animation","duration":1,"width":240,"height":140,"file_name":"demo-gif.gif","mime_type":"image/gif","has_stickers":false,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":240,"height":140,"file":{thumb}}},"animation":{animation}}},"caption":{{"@type":"formattedText","text":"Looping GIF 😀 with a custom emoji","entities":[{{"@type":"textEntity","offset":12,"length":2,"type":{{"@type":"textEntityTypeCustomEmoji","custom_emoji_id":"4242"}}}}]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

/// The Shared Media panel on its Media tab: the newest nine photos of a
/// thirty-photo chat (older ones are still to load).
fn apply_viewer_shared(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let chat_id = ChatId(11);
    let tab = session.shared_media.open_for(chat_id);
    assert_eq!(tab, SharedMediaTab::Media);
    let generation = session.shared_media.begin_fetch(tab);
    let extra = session.request(
        RequestPurpose::GetSharedMedia { tab, generation },
        Some(chat_id),
    );
    let thumb = demo_thumb_png_path();
    let messages: Vec<String> = (0..9)
        .map(|i| {
            let id = 320 - i * 10;
            let file = demo_file_json(400 + i, &thumb, true);
            let caption = if i == 3 {
                // The custom emoji sits at UTF-16 offset 6.
                r#"{"@type":"formattedText","text":"Photo 😀 with a custom emoji","entities":[{"@type":"textEntity","offset":6,"length":2,"type":{"@type":"textEntityTypeCustomEmoji","custom_emoji_id":"4242"}}]}"#.to_string()
            } else {
                format!(r#"{{"@type":"formattedText","text":"Photo {id}","entities":[]}}"#)
            };
            format!(
                r#"{{"id":{id},"chat_id":11,"date":{date},"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{file},"width":240,"height":160,"progressive_sizes":[]}}]}},"caption":{caption},"has_spoiler":false,"is_secret":false}}}}"#,
                date = 1790631000 + id,
            )
        })
        .collect();
    let found = format!(
        r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":30,"next_from_message_id":240,"messages":[{}]}}"#,
        extra.0,
        messages.join(",")
    );
    if let Some(owned) = copy_and_parse(&found, seq, &dyn_sink) {
        session.apply(owned);
    }
}

/// A downloaded 12 s video with a formatted caption (bold and a link),
/// sent on a known date so the viewer header shows the sender line.
fn apply_viewer_extras(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let clip_path = demo_media_allowlist()
        .join("demo-clip-12s.mp4")
        .to_string_lossy()
        .into_owned();
    let clip = demo_file_json(96, &clip_path, true);
    let thumb = demo_file_json(97, &demo_thumb_png_path(), true);
    let caption = "Sunday hike: full route at https://example.com/hike";
    let json = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":205,"chat_id":11,"date":1790632500,"is_outgoing":false,"content":{{"@type":"messageVideo","video":{{"@type":"video","duration":12,"width":320,"height":180,"file_name":"demo-clip-12s.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":true,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":240,"height":140,"file":{thumb}}},"video":{clip}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":"{caption}","entities":[{{"@type":"textEntity","offset":0,"length":12,"type":{{"@type":"textEntityTypeBold"}}}},{{"@type":"textEntity","offset":27,"length":24,"type":{{"@type":"textEntityTypeUrl"}}}}]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

register_demos![
    // A GIF looping in the viewer, with a custom emoji in its caption.
    DemoSpec::ready(
        "ready-viewer-gif",
        seed_ready_custom_emoji_session,
        "screenshot demo — viewer GIF loop / Shared Media paging"
    )
    .setup(|app, _, cx| app.demo_setup_viewer_extras(ViewerDemo::Gif, cx)),
    // A playing video in the viewer with a formatted caption, the saved
    // toast and the open speed dial.
    DemoSpec::ready(
        "ready-viewer-extras",
        seed_ready_media_session,
        "screenshot demo — in-viewer video playback"
    )
    .setup(|app, _, cx| app.demo_setup_viewer_extras(ViewerDemo::Extras, cx)),
    // The viewer paging over the Shared Media panel's photos.
    DemoSpec::ready(
        "ready-viewer-shared",
        seed_ready_custom_emoji_session,
        "screenshot demo — viewer GIF loop / Shared Media paging"
    )
    .setup(|app, _, cx| app.demo_setup_viewer_extras(ViewerDemo::Shared, cx)),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum ViewerDemo {
    Gif,
    Extras,
    Shared,
}

impl QuillApp {
    fn demo_setup_viewer_extras(&mut self, demo: ViewerDemo, cx: &mut Context<Self>) {
        if demo == ViewerDemo::Gif {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_viewer_gif(session, &self.demo_sink, &self.demo_seq);
            }
            // As the video demo: frames are extracted and decoded here so the
            // capture is deterministic; the viewer then loops them for real.
            self.viewer_demo_sync_frames = true;
            self.open_media_viewer(ChatId(11), MessageId(301), cx);
            if let Some(item) = self.media_viewer.current().cloned()
                && let Some(path) = self.viewer_clip_path(&item)
            {
                let file_id = -item.play_file_id.map(|id| id.0).unwrap_or(0);
                let cache = quill::video::viewer_frame_cache_dir(file_id);
                let mime = item.mime_type.clone().unwrap_or_default();
                let slot = Arc::new(Mutex::new(None));
                let cancel = AtomicBool::new(false);
                if let Ok(frames) = quill::animation::viewer_loop_frames_cancelable(
                    &path, &mime, &cache, &slot, &cancel,
                ) && let Ok(decoded) = Self::decode_viewer_frames(&frames.frames)
                {
                    self.viewer_video_frames = decoded;
                    self.viewer_video_fps = frames.fps;
                    self.viewer_frame_cache_file = Some(file_id);
                    self.play_viewer_video(&item, &path, cx);
                    if let Some(clock) = self.viewer_clock.as_mut() {
                        clock.seek(0.3);
                    }
                }
            }
            self.status_note = "screenshot demo — GIF looping in the viewer".into();
        }
        if demo == ViewerDemo::Extras {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_viewer_extras(session, &self.demo_sink, &self.demo_seq);
            }
            // As the video demo: decode the frames here so the capture is
            // deterministic, then play for real from the 5 s mark.
            self.viewer_demo_sync_frames = true;
            self.open_media_viewer(ChatId(11), MessageId(205), cx);
            if let Some(item) = self.media_viewer.current().cloned()
                && let Some(path) = self.viewer_clip_path(&item)
            {
                let file_id = -item.play_file_id.map(|id| id.0).unwrap_or(0);
                let cache = quill::video::viewer_frame_cache_dir(file_id);
                let mime = item.mime_type.clone().unwrap_or_default();
                let duration = item.duration_secs.unwrap_or(0);
                let start_timestamp = item.start_timestamp.unwrap_or(0);
                if let Ok(frames) = quill::video::viewer_playback_frames(
                    &path,
                    &mime,
                    &cache,
                    start_timestamp,
                    duration,
                ) && let Ok(decoded) = Self::decode_viewer_frames(&frames.frames)
                {
                    self.viewer_video_frames = decoded;
                    self.viewer_video_fps = frames.fps;
                    self.viewer_frame_cache_file = Some(file_id);
                    self.playback_speed = 1.5;
                    self.play_viewer_video(&item, &path, cx);
                    if let Some(clock) = self.viewer_clock.as_mut() {
                        clock.seek(5.0);
                    }
                }
            }
            self.show_saved_toast(
                std::path::PathBuf::from("/Users/demo/Downloads/hike.mp4"),
                true,
                cx,
            );
            // `QUILL_DEMO_NO_DIAL=1` keeps the dial closed, to see the
            // caption and toast it would cover.
            self.viewer_extra.demo_speed_dial_open =
                std::env::var_os("QUILL_DEMO_NO_DIAL").is_none();
            self.status_note = "screenshot demo — viewer extras".into();
        }
        if demo == ViewerDemo::Shared {
            if let Some(session) = self.demo_session.as_mut() {
                self.demo_seq.store(session.last_seq, Ordering::SeqCst);
                apply_viewer_shared(session, &self.demo_sink, &self.demo_seq);
            }
            // The fourth item carries the custom emoji caption.
            self.open_shared_media_viewer(MessageId(290), cx);
            self.status_note = "screenshot demo — viewer paging over Shared Media".into();
        }
    }
}
