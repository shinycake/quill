//! In-viewer video: autoplay on the video player core
//! (`video_player`), the system media session, and the decoded-frame
//! helpers shared with inline playback and calls.

use super::*;
use crate::ui::play_intent::{PlayKind, PlayTarget};
use quill::lang::{keys, tr, tr_with};

impl QuillApp {
    /// Start viewer playback when the current item is a video whose clip is
    /// local; otherwise download the clip and play it once it lands
    /// (`play_intent`).
    pub(in crate::ui) fn maybe_autoplay_viewer_video(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.viewer.state.current().cloned() else {
            return;
        };
        let path = self.viewer_clip_path(&item);
        // The player opens the file itself: there are no frames to wait for.
        match decide_viewer_video_start(&item, path.is_some()) {
            ViewerVideoStart::Nothing => {}
            ViewerVideoStart::ParkDownload => {
                if let Some(play_id) = item.play_file_id {
                    let target = PlayTarget::Viewer {
                        message: item.message_id,
                    };
                    self.play_when_ready(play_id, target, cx);
                }
            }
            ViewerVideoStart::PlayNow => {
                self.playback.intent = None;
                // `decide_viewer_video_start` only plays a local clip.
                let Some(path) = path else {
                    return;
                };
                self.play_viewer_video(&item, &path, cx);
            }
        }
    }

    /// A `.gif` file (not a GIF animation sent as MP4).
    fn viewer_is_gif_file(item: &MediaViewerItem, path: &std::path::Path) -> bool {
        item.mime_type
            .as_deref()
            .is_some_and(|mime| mime.eq_ignore_ascii_case("image/gif"))
            || path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("gif"))
    }

    /// The viewer clip's transport, while one is open.
    pub(in crate::ui) fn viewer_transport(&self) -> Option<crate::ui::video_player::Transport> {
        self.viewer.player.as_ref().map(|player| player.transport())
    }

    /// The viewer clip plays (or is about to): a GIF image always does.
    pub(in crate::ui) fn viewer_video_playing(&self) -> bool {
        self.viewer.gif.is_some()
            || self
                .viewer_transport()
                .is_some_and(|transport| transport.is_playing())
    }

    /// The current item is a GIF: loops, no sound, no transport.
    pub(in crate::ui) fn viewer_loops(&self) -> bool {
        self.viewer
            .state
            .current()
            .is_some_and(MediaViewerItem::loops)
    }

    /// Play `item`'s clip in the viewer from its start (or the second a
    /// media-timestamp link asked for). Stops every other player first —
    /// one thing plays at a time. GIFs loop silently with no transport.
    pub(in crate::ui) fn play_viewer_video(
        &mut self,
        item: &MediaViewerItem,
        path: &std::path::Path,
        cx: &mut Context<Self>,
    ) {
        use crate::ui::video_player::{Purpose, VideoPlayer};
        // The clip Picture-in-Picture plays (back to the viewer, or
        // opened again from the chat): the same player goes on
        // (tdesktop `continueStreaming`).
        if let Some(player) = self.take_pip_player_for(item, cx) {
            self.adopt_viewer_player(item, path, player, cx);
            return;
        }
        // A clip with sound ends Picture-in-Picture (tdesktop
        // `startStreamingPlayer`); a silent loop leaves it playing.
        if !item.loops() {
            self.sound_starts(cx);
        }
        self.stop_voice_playback();
        self.stop_audio_playback();
        self.stop_animation_playback();
        self.viewer.player = None;
        self.viewer.gif = None;
        let looping = item.loops();
        let duration = item.duration_secs.unwrap_or(0).max(0) as f64;
        // A media-timestamp link opened this clip at a given second; a
        // video photo starts at its main frame.
        let start = self
            .viewer
            .pending_seek
            .take_if(|(id, _)| *id == item.message_id)
            .map(|(_, secs)| secs.max(0.0))
            .or_else(|| {
                item.is_video_photo()
                    .then(|| self.viewer_photo_clip_start())
            })
            .unwrap_or(0.0);
        self.viewer.video = Some(item.message_id);
        self.viewer.video_path = Some(path.to_path_buf());
        self.playback.error = None;
        if looping
            && Self::viewer_is_gif_file(item, path)
            && !crate::ui::video_player::plays_gif_files()
        {
            self.viewer.gif = Some(path.to_path_buf());
            cx.notify();
            return;
        }
        let hint = (duration > 0.0).then_some(duration);
        let purpose = if looping {
            Purpose::ViewerLoop
        } else {
            Purpose::Viewer
        };
        let player = match VideoPlayer::open(path, purpose, hint, cx) {
            Ok(player) => player,
            Err(err) => {
                self.stop_viewer_video();
                self.playback.error = Some(viewer_error_message(item, &err));
                cx.notify();
                return;
            }
        };
        if looping {
            player.set_looping(true);
            player.set_volume(0.0);
        } else {
            player.set_volume(self.playback.volume);
            player.set_rate(self.playback.speed);
        }
        if start > 0.0 {
            player.seek(start);
        }
        player.play();
        self.viewer.player = Some(player);
        if !looping {
            self.create_viewer_sliders(duration, cx);
        }
        self.spawn_viewer_session_tick(cx);
        cx.notify();
    }

    /// MED1: the seek and volume sliders of the player panel.
    pub(super) fn create_viewer_sliders(&mut self, duration: f64, cx: &mut Context<Self>) {
        self.create_viewer_seek_slider(duration, cx);
        // MED1: volume slider (0–100%); applies on release.
        let volume = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(100.0)
                .default_value((self.playback.volume * 100.0).round())
                .label(tr(keys::Q_AV_VOLUME_2))
        });
        // The sliders are this viewer's: their events act on this window's.
        let scope = self.windows.scope;
        cx.subscribe(&volume, move |this, _, event, cx| {
            this.in_window_scope(scope, cx, |this, cx| this.on_viewer_volume_event(event, cx));
        })
        .detach();
        self.viewer.controls.volume = Some(volume);
    }

    /// The seek slider, in seconds over `0..=duration` (0.1 s steps): the
    /// scrubber's `Change` previews, `Release` seeks. Made again when the
    /// player learns a different length (`sync_viewer_seek_slider`).
    pub(super) fn create_viewer_seek_slider(&mut self, duration: f64, cx: &mut Context<Self>) {
        let position = self
            .viewer_transport()
            .map_or(0.0, |transport| transport.position.max(0.0));
        let max = crate::ui::video_player::controls::seek_range(duration);
        let slider = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(max as f32)
                .step(0.1)
                .default_value(position.min(max) as f32)
                .label(tr(keys::Q_AV_SEEK))
        });
        let scope = self.windows.scope;
        cx.subscribe(&slider, move |this, _, event, cx| {
            this.in_window_scope(scope, cx, |this, cx| this.on_viewer_seek_event(event, cx));
        })
        .detach();
        self.viewer.controls.seek = Some(slider);
    }

    /// Pause, keeping the item active so the controls stay and Play
    /// resumes where it stopped.
    pub(in crate::ui) fn pause_viewer_video(&mut self, cx: &mut Context<Self>) {
        if let Some(player) = &self.viewer.player {
            player.pause();
        }
        cx.notify();
    }

    /// Resume where it paused (from the start once it ended).
    pub(in crate::ui) fn resume_viewer_video(&mut self, cx: &mut Context<Self>) {
        match &self.viewer.player {
            Some(player) => player.play(),
            None if self.viewer.gif.is_some() => {}
            None => self.stop_viewer_video(),
        }
        cx.notify();
    }

    /// The viewer Play/Pause button: playing → pause, paused → resume,
    /// never-started → begin from 0 (through `maybe_autoplay_viewer_video`).
    pub(in crate::ui) fn toggle_viewer_video(&mut self, cx: &mut Context<Self>) {
        let playing = self.viewer_video_playing();
        if self.viewer.video.is_none() {
            self.maybe_autoplay_viewer_video(cx);
            return;
        }
        if playing {
            self.pause_viewer_video(cx);
        } else {
            self.resume_viewer_video(cx);
        }
    }

    /// Media keys and the OS Now Playing widget for the viewer's video.
    /// The audio player owns the session while a track is active.
    pub(super) fn drive_viewer_media_session(&mut self, cx: &mut Context<Self>) {
        use quill::viewer_extras::{ViewerMediaAction, viewer_media_action, viewer_now_playing};
        if self.active_playback_id().is_some() {
            return;
        }
        let Some(item) = self.viewer.state.current().cloned() else {
            return;
        };
        if item.kind != MediaViewerKind::Video {
            return;
        }
        let Some(transport) = self.viewer_transport() else {
            return;
        };
        let (playing, duration) = (transport.is_playing(), transport.total());
        for command in quill::media_session::take_commands() {
            match viewer_media_action(command, playing, duration) {
                Some(ViewerMediaAction::Toggle) => self.toggle_viewer_video(cx),
                Some(ViewerMediaAction::Stop) => {
                    self.stop_viewer_video();
                    quill::media_session::publish(None);
                    cx.notify();
                    return;
                }
                Some(ViewerMediaAction::SeekTo(secs)) => self.seek_viewer_to(secs, cx),
                None => {}
            }
        }
        let Some(transport) = self.viewer_transport() else {
            return;
        };
        let (sender, chat) = self.viewer_sender_line(&item).map_or_else(
            || (String::new(), String::new()),
            |(name, _)| {
                let chat = self
                    .session()
                    .and_then(|s| s.chats.get(&item.chat_id.0))
                    .map(|chat| chat.title.clone())
                    .unwrap_or_default();
                (name, chat)
            },
        );
        let info = viewer_now_playing(
            &sender,
            &chat,
            transport.total(),
            transport.position,
            transport.is_playing(),
            self.playback.speed,
        );
        quill::media_session::publish(Some(&info));
    }

    /// Full stop: drop the player and clear all viewer-video state.
    /// Called on viewer close/step and when any other player starts.
    pub(in crate::ui) fn stop_viewer_video(&mut self) {
        self.viewer.player = None;
        self.viewer.gif = None;
        self.viewer.video = None;
        self.viewer.video_path = None;
        self.drop_play_intent(PlayKind::Viewer);
        // The player is gone, so a boost has nothing to restore.
        let _ = self.viewer.controls.clear_clip();
    }
}

/// What the viewer says when a clip can't play (TGX distinguishes an
/// unsupported format from other failures).
pub(super) fn viewer_error_message(
    item: &MediaViewerItem,
    error: &crate::ui::video_player::PlayerError,
) -> String {
    use crate::ui::video_player::ErrorKind;
    let noun = if item.loops() { "animation" } else { "video" };
    match error.kind {
        ErrorKind::Unsupported => tr_with(keys::Q_AV_FORMAT_NOT_SUPPORTED, &[("noun", noun)]),
        ErrorKind::Open | ErrorKind::Decode => {
            tr_with(keys::Q_AV_COULDNT_PLAY_THIS, &[("noun", noun)])
        }
    }
}
