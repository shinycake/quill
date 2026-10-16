//! The history's looping GIF (a true `image/gif` file, decoded in process by
//! `gif_clip`) and the voice recording tick. Video, round video and MP4 GIFs
//! play on the shared player (`video_player`, `inline_video`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::gif_clip::GifClip;
use gpui_kit::*;
use quill::ids::{FileId, MessageId};
use quill::local_path::sandboxed_display_path;
use std::time::{Duration, Instant};

impl QuillApp {
    pub(super) fn maybe_autoplay_gif(&mut self, ix: usize, cx: &mut Context<Self>) {
        if self.playback.playing_animation.is_some()
            || self.playback.pending_gif_play.is_some()
            || self.playback.playing_voice.is_some()
            || self.playback.playing_audio.is_some()
            || self.recording_active()
            || self.viewer.state.is_open()
            || self.stories.viewer.is_open()
        {
            return;
        }
        let Some(super::history_row::HistoryRow::Single(row)) = self.history.rows.get(ix) else {
            return;
        };
        let Some(session) = self.session() else {
            return;
        };
        if !session.settings.media_prefs.autoplay_gifs
            || session.settings.media_prefs.data_saver
            || session.open_chat != Some(row.message.chat_id)
            || self.playback.autoplayed_gifs.contains(&row.message.id)
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
        // MP4 GIFs autoplay on the shared player (`inline_video`); only
        // true `.gif` files are decoded here.
        if animation.mime_type != "image/gif" {
            return;
        }
        let Some(file_id) = animation.play_file_id().filter(|id| {
            session
                .media
                .files
                .get(&id.0)
                .and_then(|file| file.usable_path())
                .is_some()
        }) else {
            return;
        };
        let (id, mime) = (row.message.id, animation.mime_type.clone());
        // One GIF plays at a time; the others wait for it to stop.
        self.playback.autoplayed_gifs.insert(id);
        self.toggle_animation_playback(id, file_id, mime, cx);
    }

    pub(super) fn stop_animation_playback(&mut self) {
        self.playback.gif_epoch = self.playback.gif_epoch.wrapping_add(1);
        self.playback.playing_animation = None;
        if let Some(clip) = self.playback.gif.take() {
            super::image_budget::retire_all(clip.into_images());
        }
        self.playback.gif_started_at = None;
        self.playback.pending_gif_play = None;
    }

    pub(super) fn toggle_animation_playback(
        &mut self,
        message_id: MessageId,
        file_id: FileId,
        mime: String,
        cx: &mut Context<Self>,
    ) {
        self.playback.autoplayed_gifs.insert(message_id);
        if self.playback.playing_animation == Some(message_id) {
            self.stop_animation_playback();
            self.connection.status_note = "GIF paused".into();
            cx.notify();
            return;
        }
        let path = self.session().and_then(|session| {
            session
                .media
                .files
                .get(&file_id.0)
                .and_then(|file| file.usable_path())
                .map(str::to_string)
        });
        let Some(path) = path else {
            self.playback.pending_gif_play = Some((message_id, file_id, mime));
            self.request_media_download(file_id, None, cx);
            return;
        };
        self.playback.pending_gif_play = None;
        let roots = self.media_display_roots();
        let Some(safe) = sandboxed_display_path(&path, &roots) else {
            self.connection.status_note = "GIF file is outside the account files".into();
            cx.notify();
            return;
        };
        self.stop_animation_playback();
        self.stop_voice_playback();
        self.stop_audio_playback();
        self.stop_viewer_video();
        let epoch = self.playback.gif_epoch;
        self.playback.playing_animation = Some(message_id);
        self.connection.status_note = "Loading GIF playback…".into();
        cx.spawn(async move |this, cx| {
            let decoded = cx
                .background_executor()
                .spawn(async move { GifClip::decode(&safe) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.playback.gif_epoch != epoch
                    || this.playback.playing_animation != Some(message_id)
                {
                    // Stopped or replaced meanwhile: the pictures were never shown.
                    return;
                }
                match decoded {
                    Ok(clip) => {
                        this.playback.gif = Some(clip);
                        this.playback.gif_started_at = Some(Instant::now());
                        this.connection.status_note = "Playing GIF".into();
                    }
                    Err(_) => {
                        this.stop_animation_playback();
                        this.connection.status_note = "Could not play GIF.".into();
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn resume_pending_gif(&mut self, cx: &mut Context<Self>) {
        let Some((message_id, file_id, mime)) = self.playback.pending_gif_play.clone() else {
            return;
        };
        let ready = self.session().is_some_and(|session| {
            session
                .media
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
        if self.recording.voice_tick {
            return;
        }
        self.recording.voice_tick = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let cont = this
                    .update(cx, |this, cx| {
                        this.check_recording(cx);
                        this.check_record_preview();
                        let recording = this.recording_active();
                        if let Some(capture) = this.recording.voice_capture.as_mut() {
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
                this.recording.voice_tick = false;
            });
        })
        .detach();
    }
}
