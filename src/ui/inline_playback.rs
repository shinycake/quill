//! inline animation/voice playback ticks.

use super::app::QuillApp;
use super::demo::{demo_file_json, demo_media_allowlist, demo_thumb_png_path};
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, FileId, MessageId};
use quill::local_path::sandboxed_display_path;
use quill::state::{RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;
pub(super) fn apply_ready_gifs(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let frame = demo_media_allowlist()
        .join("demo-gif-1.png")
        .to_string_lossy()
        .into_owned();
    let clip = demo_media_allowlist()
        .join("demo-gif.gif")
        .to_string_lossy()
        .into_owned();
    let thumb = demo_file_json(61, &frame, true);
    let pending = demo_file_json(62, "", false);
    let local_clip = demo_file_json(63, &clip, true);
    let history = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":501,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageAnimation","animation":{{"@type":"animation","duration":1,"width":240,"height":140,"file_name":"demo-gif.gif","mime_type":"image/gif","has_stickers":false,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":240,"height":140,"file":{thumb}}},"animation":{local_clip}}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    let waiting = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":502,"chat_id":11,"is_outgoing":true,"content":{{"@type":"messageAnimation","animation":{{"@type":"animation","duration":2,"width":240,"height":140,"file_name":"saved.mp4","mime_type":"video/mp4","has_stickers":false,"minithumbnail":null,"thumbnail":null,"animation":{pending}}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    for json in [history, waiting] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.gifs.open = true;
    session.gifs.loading = true;
    let extra = session.request(RequestPurpose::GetSavedAnimations, None);
    let saved = format!(
        r#"{{"@type":"animations","@extra":"{}","animations":[{{"@type":"animation","duration":1,"width":240,"height":140,"file_name":"demo-gif.gif","mime_type":"image/gif","has_stickers":false,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":240,"height":140,"file":{thumb}}},"animation":{local_clip}}},{{"@type":"animation","duration":2,"width":240,"height":140,"file_name":"saved.mp4","mime_type":"video/mp4","has_stickers":false,"minithumbnail":null,"thumbnail":null,"animation":{pending}}}]}}"#,
        extra.0
    );
    if let Some(owned) = copy_and_parse(&saved, seq, &dyn_sink) {
        session.apply(owned);
    }
}

pub(super) fn apply_ready_video(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let thumb_path = demo_thumb_png_path();
    let clip_path = demo_media_allowlist()
        .join("demo-gif.gif")
        .to_string_lossy()
        .into_owned();
    let thumb = demo_file_json(71, &thumb_path, true);
    let clip = demo_file_json(72, &clip_path, true);
    let pending = demo_file_json(73, "", false);
    let playing = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":601,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageVideo","video":{{"@type":"video","duration":12,"width":640,"height":360,"file_name":"clip.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":true,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":240,"height":140,"file":{thumb}}},"video":{clip}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":"Beach clip","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    let waiting = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":602,"chat_id":11,"is_outgoing":true,"content":{{"@type":"messageVideo","video":{{"@type":"video","duration":3,"width":320,"height":180,"file_name":"pending.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":false,"minithumbnail":null,"thumbnail":null,"video":{pending}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":"","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    let drop_seed = r#"{"@type":"updateDeleteMessages","chat_id":11,"message_ids":[101,102,103],"is_permanent":true,"from_cache":false}"#;
    for json in [playing, waiting, drop_seed.to_string()] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

/// Demo fixture (Parity slice 5): inject a downloaded video message into
/// chat 11 of a Media-seeded demo session. The thumbnail (file 97) and the
/// full clip (file 96, `demo-clip.mp4`) are both local/completed so the
/// viewer opens straight onto playback. The 12 s duration is fixture data
/// for the screenshot — the real fixture clip is 1 s.
pub(super) fn apply_ready_video_viewer(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let clip_path = demo_media_allowlist()
        .join("demo-clip-12s.mp4")
        .to_string_lossy()
        .into_owned();
    let thumb_path = demo_thumb_png_path();
    let clip = demo_file_json(96, &clip_path, true);
    let thumb = demo_file_json(97, &thumb_path, true);
    let json = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":204,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageVideo","video":{{"@type":"video","duration":12,"width":320,"height":180,"file_name":"demo-clip-12s.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":true,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":240,"height":140,"file":{thumb}}},"video":{clip}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":"Demo clip","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
        session.apply(owned);
    }
}

pub(super) fn apply_ready_video_note(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let thumb_path = demo_thumb_png_path();
    let clip_path = demo_media_allowlist()
        .join("demo-clip.mp4")
        .to_string_lossy()
        .into_owned();
    let thumb = demo_file_json(91, &thumb_path, true);
    let clip = demo_file_json(92, &clip_path, true);
    let pending = demo_file_json(93, "", false);
    let playing = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":611,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageVideoNote","video_note":{{"@type":"videoNote","duration":8,"waveform":"","length":240,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":120,"height":120,"file":{thumb}}},"speech_recognition_result":null,"video":{clip}}},"is_viewed":false,"is_secret":false}}}}}}"#
    );
    let waiting = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":612,"chat_id":11,"is_outgoing":true,"content":{{"@type":"messageVideoNote","video_note":{{"@type":"videoNote","duration":3,"waveform":"","length":240,"minithumbnail":null,"thumbnail":null,"speech_recognition_result":null,"video":{pending}}},"is_viewed":true,"is_secret":false}}}}}}"#
    );
    let drop_seed = r#"{"@type":"updateDeleteMessages","chat_id":11,"message_ids":[101,102,103],"is_permanent":true,"from_cache":false}"#;
    for json in [playing, waiting, drop_seed.to_string()] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

pub(super) fn apply_ready_video_note_send(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let clip_path = demo_media_allowlist()
        .join("demo-video-note.mp4")
        .to_string_lossy()
        .into_owned();
    let thumb_path = demo_thumb_png_path();
    let clip = demo_file_json(94, &clip_path, true);
    let thumb = demo_file_json(95, &thumb_path, true);
    let sent = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":721,"chat_id":11,"is_outgoing":true,"content":{{"@type":"messageVideoNote","video_note":{{"@type":"videoNote","duration":1,"waveform":"","length":240,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":240,"height":240,"file":{thumb}}},"speech_recognition_result":null,"video":{clip}}},"is_viewed":true,"is_secret":false}}}}}}"#
    );
    let drop_seed = r#"{"@type":"updateDeleteMessages","chat_id":11,"message_ids":[101,102,103],"is_permanent":true,"from_cache":false}"#;
    for json in [sent, drop_seed.to_string()] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

pub(super) fn apply_ready_video_send(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let clip_path = demo_media_allowlist()
        .join("demo-clip.mp4")
        .to_string_lossy()
        .into_owned();
    let thumb_path = demo_thumb_png_path();
    let clip = demo_file_json(81, &clip_path, true);
    let thumb = demo_file_json(82, &thumb_path, true);
    let sent = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":701,"chat_id":11,"is_outgoing":true,"content":{{"@type":"messageVideo","video":{{"@type":"video","duration":1,"width":320,"height":180,"file_name":"demo-clip.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":true,"minithumbnail":null,"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":240,"height":140,"file":{thumb}}},"video":{clip}}},"alternative_videos":[],"storyboards":[],"cover":null,"start_timestamp":0,"caption":{{"@type":"formattedText","text":"Sent clip","entities":[]}},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}}}}"#
    );
    let drop_seed = r#"{"@type":"updateDeleteMessages","chat_id":11,"message_ids":[101,102,103],"is_permanent":true,"from_cache":false}"#;
    for json in [sent, drop_seed.to_string()] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

impl QuillApp {
    pub(super) fn stop_animation_playback(&mut self) {
        if let Some(file_id) = self.animation_cache_file.take() {
            quill::animation::discard_frame_cache(file_id);
        }
        self.playing_animation = None;
        self.animation_frames.clear();
        self.animation_frame = 0;
        self.pending_gif_play = None;
    }

    pub(super) fn spawn_animation_tick(&mut self, cx: &mut Context<Self>) {
        if self.animation_tick {
            return;
        }
        self.animation_tick = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(400))
                    .await;
                let cont = this
                    .update(cx, |this, cx| {
                        let playing =
                            this.playing_animation.is_some() && this.animation_frames.len() > 1;
                        if playing {
                            this.animation_frame =
                                (this.animation_frame + 1) % this.animation_frames.len();
                            cx.notify();
                        }
                        this.playing_animation.is_some()
                    })
                    .unwrap_or(false);
                if !cont {
                    break;
                }
            }
            let _ = this.update(cx, |this, _| {
                this.animation_tick = false;
            });
        })
        .detach();
    }

    pub(super) fn toggle_animation_playback(
        &mut self,
        message_id: MessageId,
        file_id: FileId,
        mime: String,
        cx: &mut Context<Self>,
    ) {
        if self.playing_animation == Some(message_id) {
            self.stop_animation_playback();
            self.status_note = "GIF paused".into();
            cx.notify();
            return;
        }
        let path = self.session().and_then(|session| {
            session
                .files
                .get(&file_id.0)
                .and_then(|file| file.usable_path())
                .map(str::to_string)
        });
        let Some(path) = path else {
            self.pending_gif_play = Some((message_id, file_id, mime));
            self.request_media_download(file_id, None, cx);
            self.status_note = "downloading GIF".into();
            return;
        };
        self.pending_gif_play = None;
        let roots = self.media_display_roots();
        let Some(safe) = sandboxed_display_path(&path, &roots) else {
            self.status_note = "GIF file is outside the account files".into();
            cx.notify();
            return;
        };
        let cache = quill::animation::gif_frame_cache_dir(file_id.0);
        match quill::animation::playback_frames(&safe, &mime, &cache) {
            Ok(frames) if !frames.is_empty() => {
                self.stop_voice_playback();
                self.stop_audio_playback();
                self.stop_video_playback();
                self.stop_viewer_video();
                self.pending_gif_play = None;
                if self.animation_cache_file.is_some_and(|id| id != file_id.0)
                    && let Some(old) = self.animation_cache_file.take()
                {
                    quill::animation::discard_frame_cache(old);
                }
                self.animation_cache_file = Some(file_id.0);
                self.playing_animation = Some(message_id);
                self.animation_frames = frames;
                self.animation_frame = 0;
                self.spawn_animation_tick(cx);
                self.status_note = "playing GIF".into();
            }
            _ => {
                self.status_note = "could not play GIF".into();
            }
        }
        cx.notify();
    }

    pub(super) fn stop_video_playback(&mut self) {
        if let Some(file_id) = self.video_cache_file.take() {
            quill::video::discard_frame_cache(file_id);
        }
        self.playing_video = None;
        self.video_frames.clear();
        self.video_frame = 0;
        self.pending_video_play = None;
    }

    pub(super) fn spawn_video_tick(&mut self, cx: &mut Context<Self>) {
        if self.video_tick {
            return;
        }
        self.video_tick = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(400))
                    .await;
                let cont = this
                    .update(cx, |this, cx| {
                        let playing = this.playing_video.is_some() && this.video_frames.len() > 1;
                        if playing {
                            this.video_frame = (this.video_frame + 1) % this.video_frames.len();
                            cx.notify();
                        }
                        this.playing_video.is_some()
                    })
                    .unwrap_or(false);
                if !cont {
                    break;
                }
            }
            let _ = this.update(cx, |this, _| {
                this.video_tick = false;
            });
        })
        .detach();
    }

    pub(super) fn toggle_video_playback(
        &mut self,
        message_id: MessageId,
        file_id: FileId,
        mime: String,
        start_timestamp: i32,
        mark_opened: Option<ChatId>,
        cx: &mut Context<Self>,
    ) {
        if self.playing_video == Some(message_id) {
            self.stop_video_playback();
            self.status_note = "video paused".into();
            cx.notify();
            return;
        }
        let path = self.session().and_then(|session| {
            session
                .files
                .get(&file_id.0)
                .and_then(|file| file.usable_path())
                .map(str::to_string)
        });
        let Some(path) = path else {
            self.pending_video_play =
                Some((message_id, file_id, mime, start_timestamp, mark_opened));
            self.request_media_download(file_id, None, cx);
            self.status_note = "downloading video".into();
            return;
        };
        self.pending_video_play = None;
        let roots = self.media_display_roots();
        let Some(safe) = sandboxed_display_path(&path, &roots) else {
            self.status_note = "video file is outside the account files".into();
            cx.notify();
            return;
        };
        let cache = quill::video::video_frame_cache_dir(file_id.0);
        match quill::video::playback_frames(&safe, &mime, &cache, start_timestamp) {
            Ok(frames) if !frames.is_empty() => {
                self.stop_voice_playback();
                self.stop_audio_playback();
                self.stop_animation_playback();
                self.stop_viewer_video();
                self.pending_video_play = None;
                if self.video_cache_file.is_some_and(|id| id != file_id.0)
                    && let Some(old) = self.video_cache_file.take()
                {
                    quill::video::discard_frame_cache(old);
                }
                self.video_cache_file = Some(file_id.0);
                self.playing_video = Some(message_id);
                self.video_frames = frames;
                self.video_frame = 0;
                self.spawn_video_tick(cx);
                if let Some(chat_id) = mark_opened {
                    self.mark_voice_opened(chat_id, message_id);
                }
                self.status_note = "playing video".into();
            }
            _ => {
                self.status_note = "could not play video".into();
            }
        }
        cx.notify();
    }

    pub(super) fn resume_pending_video(&mut self, cx: &mut Context<Self>) {
        let Some((message_id, file_id, mime, start_timestamp, mark_opened)) =
            self.pending_video_play.clone()
        else {
            return;
        };
        let ready = self.session().is_some_and(|session| {
            session
                .files
                .get(&file_id.0)
                .and_then(|file| file.usable_path())
                .is_some()
        });
        if ready {
            self.toggle_video_playback(message_id, file_id, mime, start_timestamp, mark_opened, cx);
        }
    }

    pub(super) fn resume_pending_gif(&mut self, cx: &mut Context<Self>) {
        let Some((message_id, file_id, mime)) = self.pending_gif_play.clone() else {
            return;
        };
        let ready = self.session().is_some_and(|session| {
            session
                .files
                .get(&file_id.0)
                .and_then(|file| file.usable_path())
                .is_some()
        });
        if ready {
            self.toggle_animation_playback(message_id, file_id, mime, cx);
        }
    }

    pub(super) fn spawn_voice_tick(&mut self, cx: &mut Context<Self>) {
        if self.voice_tick {
            return;
        }
        self.voice_tick = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                let cont = this
                    .update(cx, |this, cx| {
                        let recording = this.recording_active();
                        if let Some(capture) = this.voice_capture.as_mut() {
                            capture.sample_bar();
                        }
                        if recording {
                            this.sync_voice_action();
                            cx.notify();
                        }
                        recording
                    })
                    .unwrap_or(false);
                if !cont {
                    break;
                }
            }
            let _ = this.update(cx, |this, _| {
                this.voice_tick = false;
            });
        })
        .detach();
    }
}
