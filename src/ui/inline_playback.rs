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
    pub(super) fn maybe_autoplay_gif(&mut self, ix: usize, cx: &mut Context<Self>) {
        if self.playing_animation.is_some()
            || self.pending_gif_play.is_some()
            || self.playing_voice.is_some()
            || self.playing_audio.is_some()
            || self.playing_video.is_some()
            || self.recording_active()
            || self.media_viewer.is_open()
            || self.story_viewer.is_open()
        {
            return;
        }
        let Some(super::history_row::HistoryRow::Single(row)) = self.history_rows.get(ix) else {
            return;
        };
        let Some(session) = self.session() else {
            return;
        };
        if !session.media_prefs.autoplay_gifs
            || session.media_prefs.data_saver
            || session.open_chat != Some(row.message.chat_id)
            || self.autoplayed_gifs.contains(&row.message.id)
        {
            return;
        }
        let quill::telegram::envelope::MessageContent::Animation(animation) = &row.message.content
        else {
            return;
        };
        if animation.is_secret || animation.has_spoiler {
            return;
        }
        // MP4 GIFs autoplay on the native inline player (`inline_video`).
        if super::native_video::supported() && animation.mime_type != "image/gif" {
            return;
        }
        let Some(file_id) = animation.play_file_id().filter(|id| {
            session
                .files
                .get(&id.0)
                .and_then(|file| file.usable_path())
                .is_some()
        }) else {
            return;
        };
        let (id, mime) = (row.message.id, animation.mime_type.clone());
        // ponytail: the existing player supports one visible GIF at a time; concurrent clips need independent playback slots.
        self.autoplayed_gifs.insert(id);
        self.toggle_animation_playback(id, file_id, mime, cx);
    }

    pub(super) fn stop_animation_playback(&mut self) {
        if let Some(cancel) = self.animation_extract_cancel.take() {
            cancel.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        if let Some(slot) = self.animation_extract_child.take() {
            let child = slot.lock().ok().and_then(|mut guard| guard.take());
            if let Some(mut child) = child {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        self.animation_extract_epoch = self.animation_extract_epoch.wrapping_add(1);
        if let Some(file_id) = self.animation_cache_file.take() {
            quill::animation::discard_frame_cache(file_id);
        }
        self.playing_animation = None;
        super::image_budget::retire_all(self.animation_frames.drain(..));
        self.animation_frame = 0;
        self.animation_started_at = None;
        self.animation_tick = false;
        self.pending_gif_play = None;
    }

    pub(super) fn spawn_animation_tick(&mut self, cx: &mut Context<Self>) {
        if self.animation_tick {
            return;
        }
        self.animation_tick = true;
        let epoch = self.animation_extract_epoch;
        self.animation_started_at = Some(std::time::Instant::now());
        cx.spawn(async move |this, cx| {
            loop {
                let delay = this
                    .update(cx, |this, _| {
                        Duration::from_secs_f64(1.0 / this.animation_fps.max(0.01))
                    })
                    .unwrap_or(Duration::from_millis(125));
                cx.background_executor().timer(delay).await;
                let cont = this
                    .update(cx, |this, cx| {
                        if this.animation_extract_epoch != epoch {
                            return false;
                        }
                        let playing =
                            this.playing_animation.is_some() && this.animation_frames.len() > 1;
                        // Muted GIFs hold still behind another app, like
                        // tdesktop; activation redraws.
                        if playing && this.window_active.get() {
                            let elapsed = this
                                .animation_started_at
                                .map(|t| t.elapsed().as_secs_f64())
                                .unwrap_or(0.0);
                            this.animation_frame = (elapsed * this.animation_fps) as usize
                                % this.animation_frames.len();
                            // Only the history shows the GIF.
                            this.notify_conversation(cx);
                        }
                        this.playing_animation.is_some()
                    })
                    .unwrap_or(false);
                if !cont {
                    break;
                }
            }
            let _ = this.update(cx, |this, _| {
                if this.animation_extract_epoch == epoch {
                    this.animation_tick = false;
                }
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
        self.autoplayed_gifs.insert(message_id);
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
            return;
        };
        self.pending_gif_play = None;
        let roots = self.media_display_roots();
        let Some(safe) = sandboxed_display_path(&path, &roots) else {
            self.status_note = "GIF file is outside the account files".into();
            cx.notify();
            return;
        };
        self.stop_animation_playback();
        self.stop_voice_playback();
        self.stop_audio_playback();
        self.stop_video_playback();
        self.stop_viewer_video();
        let epoch = self.animation_extract_epoch;
        let cache = quill::animation::gif_frame_cache_dir(file_id.0).join(epoch.to_string());
        self.animation_cache_file = Some(file_id.0);
        self.playing_animation = Some(message_id);
        self.status_note = "Loading GIF playback…".into();
        let slot = Arc::new(std::sync::Mutex::new(None));
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.animation_extract_child = Some(slot.clone());
        self.animation_extract_cancel = Some(cancel.clone());
        let cache_for_task = cache.clone();
        cx.spawn(async move |this, cx| {
            let extracted = cx.background_executor().spawn(async move {
                let result = quill::animation::full_playback_frames_cancelable(&safe, &mime, &cache_for_task, &slot, &cancel).and_then(|frames| {
                    let decoded = Self::decode_viewer_frames(&frames.frames)?;
                    Ok((decoded, frames.fps))
                });
                if result.is_err() { let _ = std::fs::remove_dir_all(&cache_for_task); }
                result
            }).await;
            let applied = this.update(cx, |this, cx| {
                if this.animation_extract_epoch != epoch || this.playing_animation != Some(message_id) { return false; }
                this.animation_extract_child = None;
                this.animation_extract_cancel = None;
                match extracted {
                    Ok((frames, fps)) => {
                        this.animation_frames = frames;
                        this.animation_fps = fps;
                        this.animation_frame = 0;
                        this.spawn_animation_tick(cx);
                        this.status_note = "Playing GIF".into();
                    }
                    Err(_) => {
                        this.stop_animation_playback();
                        this.status_note = "Could not play GIF. Check that ffmpeg and ffprobe are installed, then retry.".into();
                    }
                }
                cx.notify();
                true
            }).unwrap_or(false);
            if !applied { let _ = std::fs::remove_dir_all(&cache); }
        }).detach();
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
                        if playing && this.window_active.get() {
                            this.video_frame = (this.video_frame + 1) % this.video_frames.len();
                            // Only the history shows the clip.
                            this.notify_conversation(cx);
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
                    .timer(Duration::from_millis(100))
                    .await;
                let cont = this
                    .update(cx, |this, cx| {
                        this.check_recording(cx);
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
