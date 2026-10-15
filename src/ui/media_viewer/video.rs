//! In-viewer video: autoplay on the video player core
//! (`video_player`), the system media session, and the decoded-frame
//! helpers shared with inline playback and calls.

use super::*;

impl QuillApp {
    /// Start viewer playback when the current item is a video whose clip is
    /// local; otherwise trigger `downloadFile` for the clip and park the
    /// request in `viewer_pending_play` (resumed from the poll loop).
    pub(in crate::ui) fn maybe_autoplay_viewer_video(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.viewer.state.current().cloned() else {
            return;
        };
        let path = self.viewer_clip_path(&item);
        // The player opens the file itself: there are no frames to wait for.
        match decide_viewer_video_start(&item, path.is_some(), true) {
            ViewerVideoStart::Nothing => {}
            ViewerVideoStart::ParkDownload => {
                if let Some(play_id) = item.play_file_id {
                    self.viewer.pending_play = Some((item.message_id, play_id));
                    self.request_media_download(play_id, None, cx);
                }
            }
            ViewerVideoStart::PlayNow | ViewerVideoStart::ExtractFrames => {
                self.viewer.pending_play = None;
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
            .is_some_and(|item| item.kind.loops())
    }

    /// Decode extracted viewer frame PNGs into pre-loaded GPUI image handles.
    ///
    /// `img()` resolves `ImageSource::Render` synchronously — the pinned
    /// `gpui-pre-0.3.5` `src/elements/img.rs` `use_data` returns
    /// `Some(Ok(data.to_owned()))` for `Render` immediately — so cycling
    /// frames on the 125 ms tick renders without the async fs-read +
    /// PNG-decode round trip that made path-based (`ImageSource::Resource`)
    /// sources flicker and lag (each new path re-entered
    /// `window.use_asset::<ImgResourceLoader>`, which returns `None` until
    /// the load completes, while the tick fired independently).
    pub(in crate::ui) fn decode_viewer_frames(
        paths: &[PathBuf],
    ) -> Result<Vec<Arc<RenderImage>>, String> {
        paths
            .iter()
            .map(|path| {
                let mut rgba = image::open(path)
                    .map_err(|err| format!("{}: {err}", path.display()))?
                    .into_rgba8();
                // RenderImage stores BGRA, as does the shared call-frame decoder below.
                for pixel in rgba.as_chunks_mut::<4>().0 {
                    pixel.swap(0, 2);
                }
                Ok(Arc::new(RenderImage::new(SmallVec::from_buf([
                    image::Frame::new(rgba),
                ]))))
            })
            .collect()
    }

    /// Phase C2e: decode a `VideoFrame`'s RGBA8 bytes into a GPUI image
    /// handle, exactly like `decode_viewer_frames`. `None` on malformed
    /// bytes — never render garbage.
    pub(in crate::ui) fn video_render_image(
        frame: &quill::calls::engine::VideoFrame,
    ) -> Option<Arc<RenderImage>> {
        let mut rgba = image::RgbaImage::from_raw(
            u32::from(frame.width),
            u32::from(frame.height),
            frame.rgba.clone(),
        )?;
        // GPUI holds RenderImage pixels in BGRA (its own decoder swaps
        // R<->B after into_rgba8); the engine delivers RGBA, so swap here.
        for pixel in rgba.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        Some(Arc::new(RenderImage::new(SmallVec::from_buf([
            image::Frame::new(rgba),
        ]))))
    }

    /// Phase C2e/C2l: cached decoded tile for a video frame, rebuilt
    /// only when the `(seq, is_screen)` key changed. `local` picks the
    /// preview slot, otherwise the peer slot (the peer's camera and
    /// screen share the slot; the key keeps the streams apart).
    pub(in crate::ui) fn cached_video_image(
        &mut self,
        local: bool,
        frame: &quill::calls::engine::VideoFrame,
    ) -> Option<Arc<RenderImage>> {
        let key = (frame.seq, frame.is_screen);
        let slot = if local {
            &mut self.calls.local_image
        } else {
            &mut self.calls.remote_image
        };
        if slot.as_ref().is_some_and(|(k, _)| *k == key) {
            return slot.as_ref().map(|(_, image)| image.clone());
        }
        let image = Self::video_render_image(frame)?;
        // The slot's previous frame stays in the atlas until the sweeper
        // retires it; overwriting it silently would leak it there.
        if let Some((_, old)) = slot.replace((key, image.clone())) {
            crate::ui::image_budget::retire_all([old]);
        }
        Some(image)
    }

    /// Phase C2g: cached decoded tile for one group participant's video
    /// slot, rebuilt only when that slot's frame sequence changed.
    pub(in crate::ui) fn cached_group_video_image(
        &mut self,
        call_id: i32,
        user_id: i64,
        screen: bool,
        frame: &quill::calls::engine::VideoFrame,
    ) -> Option<Arc<RenderImage>> {
        let key = (call_id, user_id, screen);
        if self
            .group_call
            .video_images
            .get(&key)
            .is_some_and(|(seq, _)| *seq == frame.seq)
        {
            return self
                .group_call
                .video_images
                .get(&key)
                .map(|(_, image)| image.clone());
        }
        let image = Self::video_render_image(frame)?;
        // Tiles of any other call are dead weight.
        self.prune_group_video_images(Some(call_id));
        // Same for the tile's previous frame.
        if let Some(old) = replace_tile(
            &mut self.group_call.video_images,
            key,
            (frame.seq, image.clone()),
        ) {
            crate::ui::image_budget::retire_all([old]);
        }
        Some(image)
    }

    /// Drop cached group-call tiles that don't belong to `live_call`
    /// (`None`: no call, drop all), handing them to the atlas sweeper.
    pub(in crate::ui) fn prune_group_video_images(&mut self, live_call: Option<i32>) {
        let images = take_dead_call_tiles(&mut self.group_call.video_images, live_call)
            .into_iter()
            .map(|(_, image)| image);
        crate::ui::image_budget::retire_all(images);
    }

    /// Resume a parked viewer play once `downloadFile` lands the clip.
    /// Routes through `maybe_autoplay_viewer_video`: it re-derives the
    /// current item, clears the pending flag, and either reuses cached
    /// frames or starts async extraction — never straight to playback
    /// with an empty frame cache.
    pub(in crate::ui) fn resume_pending_viewer_video(&mut self, cx: &mut Context<Self>) {
        let Some((message_id, file_id)) = self.viewer.pending_play else {
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
        if !ready {
            return;
        }
        let matches = self
            .viewer
            .state
            .current()
            .is_some_and(|item| item.message_id == message_id && item.kind.is_playable());
        if !matches {
            self.viewer.pending_play = None;
            return;
        }
        self.maybe_autoplay_viewer_video(cx);
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
        // Back from Picture-in-Picture: the same player goes on.
        if let Some(player) = self.take_pip_handoff(item.message_id) {
            self.adopt_viewer_player(item, path, player, cx);
            return;
        }
        // A clip with sound ends Picture-in-Picture (tdesktop
        // `startStreamingPlayer`); a silent loop leaves it playing.
        if !item.kind.loops() {
            self.close_video_pip(cx);
        }
        self.stop_voice_playback();
        self.stop_audio_playback();
        self.stop_video_playback();
        self.stop_animation_playback();
        self.viewer.player = None;
        self.viewer.gif = None;
        let looping = item.kind.loops();
        let duration = item.duration_secs.unwrap_or(0).max(0) as f64;
        // A media-timestamp link opened this clip at a given second.
        let start = self
            .viewer
            .pending_seek
            .take_if(|(id, _)| *id == item.message_id)
            .map_or(0.0, |(_, secs)| secs.max(0.0));
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
                .label("Volume")
        });
        cx.subscribe(&volume, |this, _, event, cx| {
            this.on_viewer_volume_event(event, cx);
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
                .label("Seek")
        });
        cx.subscribe(&slider, |this, _, event, cx| {
            this.on_viewer_seek_event(event, cx);
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
        self.viewer.pending_play = None;
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
    let noun = if item.kind.loops() {
        "animation"
    } else {
        "video"
    };
    match error.kind {
        ErrorKind::Unsupported => format!("{noun} format not supported"),
        ErrorKind::Open | ErrorKind::Decode => format!("couldn't play this {noun}"),
    }
}

/// Store `tile` under `key` and return the frame image it replaced, if
/// any, so the caller can retire that image from the atlas.
fn replace_tile<K: std::hash::Hash + Eq, V>(
    tiles: &mut std::collections::HashMap<K, (u64, V)>,
    key: K,
    tile: (u64, V),
) -> Option<V> {
    tiles.insert(key, tile).map(|(_, old)| old)
}

/// Remove and return every tile whose call isn't `live_call`.
fn take_dead_call_tiles<V>(
    tiles: &mut std::collections::HashMap<(i32, i64, bool), V>,
    live_call: Option<i32>,
) -> Vec<V> {
    let dead: Vec<_> = tiles
        .keys()
        .filter(|(call, _, _)| Some(*call) != live_call)
        .copied()
        .collect();
    dead.into_iter()
        .filter_map(|key| tiles.remove(&key))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{replace_tile, take_dead_call_tiles};
    use std::collections::HashMap;

    #[test]
    fn replacing_a_tile_hands_back_the_old_frame_to_retire() {
        let mut tiles: HashMap<(i32, i64, bool), (u64, u8)> = HashMap::new();
        assert_eq!(replace_tile(&mut tiles, (1, 10, false), (1, 7)), None);
        assert_eq!(replace_tile(&mut tiles, (1, 10, false), (2, 8)), Some(7));
        assert_eq!(tiles.get(&(1, 10, false)), Some(&(2, 8)));
        assert_eq!(tiles.len(), 1);
    }

    #[test]
    fn tiles_empty_when_the_call_ends_and_drop_when_it_changes() {
        let mut tiles: HashMap<(i32, i64, bool), u8> = HashMap::new();
        tiles.insert((1, 10, false), 0);
        tiles.insert((1, 11, true), 1);
        tiles.insert((2, 10, false), 2);
        assert_eq!(take_dead_call_tiles(&mut tiles, Some(2)).len(), 2);
        assert_eq!(tiles.len(), 1);
        assert_eq!(take_dead_call_tiles(&mut tiles, None).len(), 1);
        assert!(tiles.is_empty());
    }
}
