//! The history's looping GIF (a true `image/gif` file, decoded in process by
//! `gif_clip`) and the voice recording tick. Video, round video and MP4 GIFs
//! play on the shared player (`video_player`, `inline_video`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::gif_clip::{GifClip, GifError};
use super::inline_players::Demand;
use super::inline_video::InlineTile;
use super::message_media::MediaFrameKind;
use super::play_intent::{PlayKind, PlayTarget};
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::ids::{FileId, MessageId};
use quill::lang::{keys, tr};
use quill::local_path::sandboxed_display_path;
use quill::telegram::envelope::MessageContent;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

impl QuillApp {
    pub(super) fn maybe_autoplay_gif(&mut self, ix: usize, cx: &mut Context<Self>) {
        if self.playback.playing_animation.is_some()
            || self
                .playback
                .intent
                .as_ref()
                .is_some_and(|intent| intent.target.kind() == PlayKind::Gif)
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
        let MessageContent::Animation(animation) = &row.message.content else {
            return;
        };
        if animation.is_secret || self.media_spoiler_covered(&row.message) {
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
        // A decode still running stops before its next frame.
        for cancel in self.playback.gif_decodes.values() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.playback.playing_animation = None;
        // The clip hands its pictures to the atlas sweeper when it drops.
        self.playback.gif = None;
        self.playback.gif_started_at = None;
        self.drop_play_intent(PlayKind::Gif);
    }

    /// The GIF's play disc: an `image/gif` file loops here, decoded in
    /// process; an MP4 GIF plays on demand on the inline player.
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
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_AV_GIF_PAUSED)));
            cx.notify();
            return;
        }
        if mime != "image/gif" {
            self.play_open_chat_clip(message_id, file_id, Demand::Loop, cx);
            return;
        }
        self.play_gif_file(message_id, file_id, cx);
    }

    /// A clip of the open chat on demand on the inline player.
    fn play_open_chat_clip(
        &mut self,
        message_id: MessageId,
        file_id: FileId,
        demand: Demand,
        cx: &mut Context<Self>,
    ) {
        let Some(chat_id) = self.session().and_then(|session| session.open_chat) else {
            return;
        };
        self.play_inline_on_demand(chat_id, message_id, file_id, demand, cx);
    }

    fn play_gif_file(&mut self, message_id: MessageId, file_id: FileId, cx: &mut Context<Self>) {
        let path = self.session().and_then(|session| {
            session
                .media
                .files
                .get(&file_id.0)
                .and_then(|file| file.usable_path())
                .map(str::to_string)
        });
        let Some(path) = path else {
            self.play_when_ready(
                file_id,
                PlayTarget::Gif {
                    message: message_id,
                },
                cx,
            );
            return;
        };
        self.playback.intent = None;
        let roots = self.media_display_roots();
        let Some(safe) = sandboxed_display_path(&path, &roots) else {
            self.connection
                .set_note(StatusNote::error(tr(keys::Q_AV_GIF_FILE_IS_OUTSIDE_THE)));
            cx.notify();
            return;
        };
        self.stop_animation_playback();
        self.stop_voice_playback();
        self.stop_audio_playback();
        self.stop_viewer_video();
        self.playback.playing_animation = Some(message_id);
        self.connection
            .set_note(StatusNote::silent(tr(keys::Q_AV_LOADING_GIF_PLAYBACK)));
        self.spawn_gif_decode(message_id, file_id, safe, cx);
        cx.notify();
    }

    /// Decode the GIF off the UI thread. One decode per GIF: one still
    /// running (stopped, then started again before it saw the stop) is
    /// taken back instead of starting a second.
    fn spawn_gif_decode(
        &mut self,
        message_id: MessageId,
        file_id: FileId,
        path: PathBuf,
        cx: &mut Context<Self>,
    ) {
        if let Some(cancel) = self.playback.gif_decodes.get(&message_id) {
            cancel.store(false, Ordering::Relaxed);
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.playback.gif_decodes.insert(message_id, cancel.clone());
        let max_edge = self.gif_decode_edge(message_id, cx);
        Self::spawn_main(cx, async move |this, cx| {
            let decoded = cx
                .background_executor()
                .spawn(async move { GifClip::decode(&path, max_edge, &cancel) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.gif_decoded(message_id, file_id, decoded, cx);
            });
        })
        .detach();
    }

    /// The longer side to decode the GIF at: its tile in device pixels
    /// (`InlineTile::decode_edge`, capped at `INLINE_MAX_EDGE`).
    fn gif_decode_edge(&self, message_id: MessageId, cx: &mut Context<Self>) -> u32 {
        let scale = self.playback.inline_videos.scale();
        let message = self.session().and_then(|session| {
            let chat = session.open_chat?;
            session.histories.get(&chat.0)?.messages.get(&message_id.0)
        });
        let (width, height, caption) = match message {
            Some(message) => match &message.content {
                MessageContent::Animation(animation) => (
                    animation.width,
                    animation.height,
                    self.inline_caption_width(message, cx),
                ),
                _ => (0, 0, 0),
            },
            None => (0, 0, 0),
        };
        InlineTile::media(MediaFrameKind::Gif, width, height, caption).decode_edge(scale)
    }

    fn gif_decoded(
        &mut self,
        message_id: MessageId,
        file_id: FileId,
        decoded: Result<GifClip, GifError>,
        cx: &mut Context<Self>,
    ) {
        self.playback.gif_decodes.remove(&message_id);
        if self.playback.playing_animation != Some(message_id) || self.playback.gif.is_some() {
            // Stopped or replaced meanwhile: the pictures were never shown.
            return;
        }
        match decoded {
            Ok(clip) => {
                self.playback.gif = Some(Rc::new(clip));
                self.playback.gif_started_at = Some(Instant::now());
                self.connection
                    .set_note(StatusNote::silent(tr(keys::Q_AV_PLAYING_GIF)));
            }
            // Started again after this decode saw the stop: decode anew.
            Err(GifError::Cancelled) => {
                self.playback.playing_animation = None;
                self.play_gif_file(message_id, file_id, cx);
                return;
            }
            // Labelled `image/gif`, but a video: the inline player takes it.
            Err(GifError::NotGif) => {
                self.stop_animation_playback();
                self.play_open_chat_clip(message_id, file_id, Demand::Loop, cx);
                return;
            }
            Err(_) => {
                self.stop_animation_playback();
                self.connection
                    .set_note(StatusNote::error(tr(keys::Q_AV_COULD_NOT_PLAY_GIF)));
            }
        }
        self.notify_conversation(cx);
        cx.notify();
    }

    pub(super) fn spawn_voice_tick(&mut self, cx: &mut Context<Self>) {
        if self.recording.voice_tick {
            return;
        }
        self.recording.voice_tick = true;
        // The tick is the app's; each step runs as the recording's window.
        Self::spawn_main(cx, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let cont = this
                    .update(cx, |this, cx| this.recording_tick_in_window(cx))
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
