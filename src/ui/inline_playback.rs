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
use super::playback_state::{ChatMessage, GifPlayback, GifRun, PlayingGif};
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::ids::{ChatId, FileId, MessageId};
use quill::lang::{keys, tr};
use quill::local_path::sandboxed_display_path;
use quill::telegram::envelope::MessageContent;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

/// The `image/gif` file a message loops in its row: a GIF animation, or a
/// GIF sent as a picture file, which tdesktop plays as an animation too
/// (`DocumentData::isAnimation`). A picture file too large for the viewer
/// keeps its file row (`shows_as_photo`) and never plays there.
pub(super) fn row_gif_file(content: &MessageContent) -> Option<FileId> {
    match content {
        MessageContent::Animation(animation)
            if animation.mime_type == "image/gif" && !animation.is_secret =>
        {
            animation.play_file_id()
        }
        MessageContent::Document(doc)
            if doc.mime_type.eq_ignore_ascii_case("image/gif")
                && quill::media_viewer::shows_as_photo(doc)
                && doc.file_id.0 != 0 =>
        {
            Some(doc.file_id)
        }
        _ => None,
    }
}

impl QuillApp {
    pub(super) fn maybe_autoplay_gif(&mut self, ix: usize, cx: &mut Context<Self>) {
        if self.playback.animation.playing().is_some()
            || self.playback.intent.is_some()
            || self.playback.active.voice().is_some()
            || self.playback.active.audio().is_some()
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
            || session.open_chat_id() != Some(row.message.chat_id)
            || self
                .playback
                .autoplayed_gifs
                .contains(&ChatMessage::new(row.message.chat_id, row.message.id))
        {
            return;
        }
        // MP4 GIFs autoplay on the shared player (`inline_video`); only
        // true `.gif` files are decoded here.
        if self.media_spoiler_covered(&row.message) {
            return;
        }
        let Some(file_id) = row_gif_file(&row.message.content).filter(|id| {
            session
                .media
                .files
                .get(&id.0)
                .and_then(|file| file.usable_path())
                .is_some()
        }) else {
            return;
        };
        let key = ChatMessage::new(row.message.chat_id, row.message.id);
        // One GIF plays at a time; the others wait for it to stop.
        self.playback.autoplayed_gifs.insert(key);
        self.toggle_animation_playback(key.message, file_id, "image/gif".into(), cx);
    }

    /// The GIF file of `chat`'s row `message_id` loops in the window whose
    /// state is in place.
    pub(super) fn gif_playing(&self, chat: ChatId, message_id: MessageId) -> bool {
        self.playback.animation.playing()
            == Some(PlayingGif {
                window: self.windows.scope,
                message: ChatMessage::new(chat, message_id),
            })
    }

    /// The window in place leaves `chat`: the GIF file its row loops stops,
    /// a GIF it waits to play goes, and `chat`'s GIFs may autoplay again
    /// when it comes back. A GIF another window plays goes on.
    pub(super) fn leave_gifs_here(&mut self, chat: Option<ChatId>) {
        let here = self.windows.scope;
        if self
            .playback
            .animation
            .playing()
            .is_some_and(|playing| playing.window == here)
        {
            self.stop_animation_playback();
        }
        self.playback.intent.take_if(|intent| {
            intent.target.kind() == PlayKind::Gif && intent.target.window() == here
        });
        if let Some(chat) = chat {
            self.playback.autoplayed_gifs.retain(|gif| gif.chat != chat);
        }
    }

    /// The window `chat` closed: a GIF its row played stops, its clip
    /// goes, and the chat's autoplay marks go, so its GIFs autoplay when
    /// it opens in another window (left marked, they would not).
    pub(in crate::ui) fn stop_gif_of_window(&mut self, chat: ChatId) {
        if self
            .playback
            .animation
            .playing()
            .is_some_and(|playing| playing.window == Some(chat))
        {
            self.stop_animation_playback();
        }
        self.playback.autoplayed_gifs.retain(|gif| gif.chat != chat);
    }

    pub(super) fn stop_animation_playback(&mut self) {
        // The stage owns its decode: leaving it stops the decode before its
        // next frame, and the clip hands its pictures to the atlas sweeper.
        self.playback.animation = GifPlayback::Idle;
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
        let open_chat = self.session().and_then(|session| session.open_chat_id());
        if let Some(chat) = open_chat {
            self.playback
                .autoplayed_gifs
                .insert(ChatMessage::new(chat, message_id));
        }
        if open_chat.is_some_and(|chat| self.gif_playing(chat, message_id)) {
            self.stop_animation_playback();
            self.set_note(StatusNote::silent(tr(keys::Q_AV_GIF_PAUSED)));
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
        let Some(chat_id) = self.session().and_then(|session| session.open_chat_id()) else {
            return;
        };
        let window = self.windows.scope;
        self.play_inline_on_demand(window, chat_id, message_id, file_id, demand, cx);
    }

    fn play_gif_file(&mut self, message_id: MessageId, file_id: FileId, cx: &mut Context<Self>) {
        let Some(chat_id) = self.session().and_then(|session| session.open_chat_id()) else {
            return;
        };
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
                    window: self.windows.scope,
                },
                cx,
            );
            return;
        };
        self.playback.intent = None;
        let roots = self.media_display_roots();
        let Some(safe) = sandboxed_display_path(&path, &roots) else {
            self.set_note(StatusNote::error(tr(keys::Q_AV_GIF_FILE_IS_OUTSIDE_THE)));
            cx.notify();
            return;
        };
        self.stop_animation_playback();
        self.stop_voice_playback();
        self.stop_audio_playback();
        self.stop_viewer_video();
        let run = self.spawn_gif_decode(chat_id, message_id, file_id, safe, cx);
        self.playback.animation = GifPlayback::Waiting {
            playing: PlayingGif {
                window: self.windows.scope,
                message: ChatMessage::new(chat_id, message_id),
            },
            run,
        };
        self.set_note(StatusNote::silent(tr(keys::Q_AV_LOADING_GIF_PLAYBACK)));
        cx.notify();
    }

    /// Decode the GIF off the UI thread. The run goes into the playback
    /// stage (`GifPlayback`), which owns it: stopping drops it, and with it
    /// the landing of a decode nobody waits for any more.
    pub(super) fn spawn_gif_decode(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        file_id: FileId,
        path: PathBuf,
        cx: &mut Context<Self>,
    ) -> GifRun {
        let key = ChatMessage::new(chat_id, message_id);
        let cancel = Arc::new(AtomicBool::new(false));
        let max_edge = self.gif_decode_edge(message_id, cx);
        let decoding = cancel.clone();
        // The decode lands in the window that started it: its open chat is
        // the row's chat, in a chat window too.
        let task = self.spawn_scoped(cx, async move |this, cx| {
            let (first, first_in) = futures_channel::oneshot::channel();
            let decode = cx.background_executor().spawn(async move {
                GifClip::decode(&path, max_edge, &decoding, |clip| {
                    let _ = first.send(clip);
                })
            });
            // The first picture plays while the rest decode.
            if let Ok(clip) = first_in.await {
                let _ = this.update(cx, |this, cx| {
                    if this.session().and_then(|session| session.open_chat_id()) == Some(chat_id) {
                        this.gif_started(chat_id, message_id, clip, cx);
                    }
                });
            }
            let decoded = decode.await;
            let landed = this.update(cx, |this, cx| this.gif_decoded(key, file_id, decoded, cx));
            // Its window closed meanwhile: the row it played went with it,
            // so the decode ends and nothing stays marked as playing.
            if landed.is_err() {
                let _ = this.main().update(cx, |this, cx| {
                    if this
                        .playback
                        .animation
                        .playing()
                        .is_some_and(|playing| playing.message == key)
                    {
                        this.stop_animation_playback();
                        cx.notify();
                    }
                });
            }
        });
        GifRun::new(cancel, task)
    }

    fn open_chat_animation(&self, message_id: MessageId) -> bool {
        self.session().is_some_and(|session| {
            session
                .open_chat_id()
                .and_then(|chat| session.histories.get(&chat.0))
                .and_then(|history| history.messages.get(&message_id.0))
                .is_some_and(|message| matches!(message.content, MessageContent::Animation(_)))
        })
    }

    /// The longer side to decode the GIF at: its tile in device pixels
    /// (`InlineTile::decode_edge`, capped at `INLINE_MAX_EDGE`).
    fn gif_decode_edge(&self, message_id: MessageId, cx: &mut Context<Self>) -> u32 {
        let scale = self.players.inline_videos.scale();
        let message = self.session().and_then(|session| {
            let chat = session.open_chat_id()?;
            session.histories.get(&chat.0)?.messages.get(&message_id.0)
        });
        let (width, height, caption) = match message {
            Some(message) => match &message.content {
                MessageContent::Animation(animation) => (
                    animation.width,
                    animation.height,
                    self.inline_caption_width(message, cx),
                ),
                MessageContent::Document(doc) => {
                    doc.thumbnail.as_ref().map_or((0, 0, 0), |thumb| {
                        (
                            thumb.width,
                            thumb.height,
                            self.inline_caption_width(message, cx),
                        )
                    })
                }
                _ => (0, 0, 0),
            },
            None => (0, 0, 0),
        };
        InlineTile::media(MediaFrameKind::Gif, width, height, caption).decode_edge(scale)
    }

    /// The GIF's first picture is in: it plays from here.
    fn gif_started(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        clip: Arc<GifClip>,
        cx: &mut Context<Self>,
    ) {
        if !self.gif_playing(chat_id, message_id) || self.playback.animation.clip().is_some() {
            return;
        }
        self.playback.animation.begin_clip(clip, Instant::now());
        self.set_note(StatusNote::silent(tr(keys::Q_AV_PLAYING_GIF)));
        self.notify_conversation(cx);
        cx.notify();
    }

    fn gif_decoded(
        &mut self,
        key: ChatMessage,
        file_id: FileId,
        decoded: Result<Arc<GifClip>, GifError>,
        cx: &mut Context<Self>,
    ) {
        let message_id = key.message;
        // Playing is the open chat's: the same id in another chat is not
        // this decode's row.
        let in_open_chat =
            self.session().and_then(|session| session.open_chat_id()) == Some(key.chat);
        if !in_open_chat || !self.gif_playing(key.chat, message_id) {
            // Another chat is open: this decode's row is not on screen.
            return;
        }
        match decoded {
            // Already playing since its first picture: the rest is in.
            Ok(clip)
                if self
                    .playback
                    .animation
                    .clip()
                    .is_some_and(|playing| Arc::ptr_eq(playing, &clip)) => {}
            // The first picture plays: a failure in the rest leaves it be.
            Err(_) if self.playback.animation.clip().is_some() => {
                self.playback.animation.decode_ended();
                return;
            }
            Ok(clip) => self.gif_started(key.chat, message_id, clip, cx),
            // Only a stop cancels the decode, and a stop drops this task
            // with its stage: nothing lands from it.
            Err(GifError::Cancelled) => {
                self.stop_animation_playback();
                return;
            }
            // Labelled `image/gif`, but a video: the inline player takes an
            // animation; a picture file stays still.
            Err(GifError::NotGif) => {
                self.stop_animation_playback();
                if self.open_chat_animation(message_id) {
                    self.play_open_chat_clip(message_id, file_id, Demand::Loop, cx);
                }
                return;
            }
            Err(_) => {
                self.stop_animation_playback();
                self.set_note(StatusNote::error(tr(keys::Q_AV_COULD_NOT_PLAY_GIF)));
            }
        }
        // The pictures that came are all there is.
        self.playback.animation.decode_ended();
        self.notify_conversation(cx);
        cx.notify();
    }

    pub(super) fn spawn_voice_tick(&mut self, cx: &mut Context<Self>) {
        if self.recording.tick.is_some() {
            return;
        }
        // The tick is the app's; each step runs as the recording's window.
        // window-scope: `recording_tick_in_window` enters the recording's window itself.
        let task = self.spawn_screen(cx, async move |this, cx| {
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
                this.recording.tick = None;
            });
        });
        self.recording.tick = Some(task);
    }
}

#[cfg(test)]
mod tests {
    use super::row_gif_file;
    use quill::ids::FileId;
    use quill::telegram::envelope::{AlbumCoverThumb, DocumentContent, MessageContent};

    fn picture_file(name: &str, mime: &str, size: i64) -> DocumentContent {
        DocumentContent {
            file_name: name.into(),
            mime_type: mime.into(),
            caption: String::new(),
            caption_entities: Vec::new(),
            file_id: FileId(7),
            thumbnail: Some(AlbumCoverThumb {
                width: 320,
                height: 240,
                file_id: FileId(8),
            }),
            minithumbnail: None,
            size,
        }
    }

    #[test]
    fn a_gif_sent_as_a_picture_file_loops_in_its_row() {
        let gif = picture_file("loop.gif", "image/gif", 2 << 20);
        assert!(quill::media_viewer::shows_as_photo(&gif));
        assert_eq!(
            row_gif_file(&MessageContent::Document(gif)),
            Some(FileId(7))
        );
        let png = picture_file("still.png", "image/png", 2 << 20);
        assert_eq!(row_gif_file(&MessageContent::Document(png)), None);
    }

    #[test]
    fn a_gif_file_too_large_for_the_viewer_keeps_its_file_row_and_does_not_play() {
        let huge = picture_file("huge.gif", "image/gif", 65 << 20);
        assert!(!quill::media_viewer::shows_as_photo(&huge));
        assert_eq!(row_gif_file(&MessageContent::Document(huge)), None);
    }
}
