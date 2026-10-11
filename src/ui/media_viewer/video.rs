//! In-viewer video: autoplay, frame extraction and decoding, native
//! playback, the audio track and the system media session.

use super::*;

impl QuillApp {
    /// Start viewer playback when the current item is a video whose clip is
    /// local; otherwise trigger `downloadFile` for the clip and park the
    /// request in `viewer_pending_play` (resumed from the poll loop).
    ///
    /// Parity slice 5: the clip's frames are extracted (async — ffmpeg takes
    /// ~2 s for a 12 s clip), decoded into pre-loaded image handles, and
    /// rendered in-viewer; the audio engine plays the sound. If frames
    /// are already cached for this file (e.g. the screenshot demo decoded
    /// them synchronously), playback starts at once. Every start path
    /// routes through `decide_viewer_video_start` so a local clip without
    /// cached frames always goes through extraction — never straight to
    /// playback with an empty frame cache.
    pub(in crate::ui) fn maybe_autoplay_viewer_video(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.viewer.state.current().cloned() else {
            return;
        };
        let file_id = item.play_file_id.map(|id| id.0).unwrap_or(0);
        let path = self.viewer_clip_path(&item);
        let frames_ready =
            self.viewer.frame_cache_file == Some(file_id) && !self.viewer.video_frames.is_empty();
        match decide_viewer_video_start(&item, path.is_some(), frames_ready) {
            ViewerVideoStart::Nothing => {}
            ViewerVideoStart::ParkDownload => {
                if let Some(play_id) = item.play_file_id {
                    self.viewer.pending_play = Some((item.message_id, play_id));
                    self.request_media_download(play_id, None, cx);
                }
            }
            ViewerVideoStart::PlayNow | ViewerVideoStart::ExtractFrames
                if Self::viewer_uses_native(&item, path.as_deref())
                    && !self.viewer.demo_sync_frames =>
            {
                self.viewer.pending_play = None;
                let path = path.expect("clip checked local by decide_viewer_video_start");
                self.play_native_viewer_video(&item, &path, cx);
            }
            ViewerVideoStart::PlayNow => {
                self.viewer.pending_play = None;
                let path = path.expect("clip checked local by decide_viewer_video_start");
                self.play_viewer_video(&item, &path, cx);
            }
            ViewerVideoStart::ExtractFrames => {
                self.viewer.pending_play = None;
                // The screenshot demo extracts + decodes frames synchronously
                // itself; don't start a redundant background extraction.
                if self.viewer.demo_sync_frames {
                    return;
                }
                let path = path.expect("clip checked local by decide_viewer_video_start");
                self.extract_viewer_frames(&item, &path, cx);
            }
        }
    }

    /// Whether the native player (macOS AVPlayer) takes this clip. `.gif`
    /// files are not a video format for it, so they use the ffmpeg frames.
    fn viewer_uses_native(item: &MediaViewerItem, path: Option<&std::path::Path>) -> bool {
        let is_gif_file = item
            .mime_type
            .as_deref()
            .is_some_and(|mime| mime.eq_ignore_ascii_case("image/gif"))
            || path
                .and_then(|path| path.extension())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("gif"));
        crate::ui::native_video::supported() && !(item.kind.loops() && is_gif_file)
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
        *slot = Some((key, image.clone()));
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
        self.group_call
            .video_images
            .insert(key, (frame.seq, image.clone()));
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

    /// Kill a running viewer frame extraction (ffmpeg child), if any, and
    /// invalidate its completion. Called on viewer close/step and before a
    /// fresh extraction starts, so an abandoned extraction can't run to
    /// completion on a discarded cache dir.
    pub(in crate::ui) fn kill_viewer_extraction(&mut self) {
        // Signal cancellation first: the worker checks this before spawning
        // and right after publishing the child, so a kill that lands before
        // ffmpeg publishes still aborts the run instead of orphaning it.
        if let Some(cancel) = self.viewer.extract_cancel.take() {
            cancel.store(true, Ordering::SeqCst);
        }
        if let Some(slot) = self.viewer.extract_child.take() {
            // Take the child out of the lock before kill/wait: the worker
            // only holds the lock briefly around `try_wait`.
            let child = slot.lock().ok().and_then(|mut guard| guard.take());
            drop(slot);
            if let Some(mut child) = child {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        self.viewer.extract_epoch = self.viewer.extract_epoch.wrapping_add(1);
    }

    /// Extract the clip's frames on a background thread, decode them into
    /// pre-loaded image handles, then start playback if the viewer is still
    /// on the same item. The thumbnail stays visible with a loading hint
    /// meanwhile. The ffmpeg child is published so close/step can kill it;
    /// a completion from a killed or superseded run is dropped by epoch.
    pub(in crate::ui) fn extract_viewer_frames(
        &mut self,
        item: &MediaViewerItem,
        path: &std::path::Path,
        cx: &mut Context<Self>,
    ) {
        let file_id = item.play_file_id.map(|id| id.0).unwrap_or(0);
        if self
            .viewer
            .frame_cache_file
            .is_some_and(|cached| cached != file_id)
            && let Some(old) = self.viewer.frame_cache_file.take()
        {
            quill::video::discard_viewer_frame_cache(old);
        }
        self.viewer.frame_cache_file = Some(file_id);
        crate::ui::image_budget::retire_all(self.viewer.video_frames.drain(..));
        self.viewer.extracting = true;
        // A step between two videos goes through `stop_viewer_video` first,
        // but cancel explicitly anyway: a fresh run must not share the
        // previous run's slot or epoch.
        self.kill_viewer_extraction();
        let epoch = self.viewer.extract_epoch;
        let slot: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
        self.viewer.extract_child = Some(slot.clone());
        let cancel: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
        self.viewer.extract_cancel = Some(cancel.clone());
        cx.notify();

        let cache = quill::video::viewer_frame_cache_dir(file_id);
        let mime = item.mime_type.clone().unwrap_or_default();
        let duration = item.duration_secs.unwrap_or(0);
        let start_timestamp = item.start_timestamp.unwrap_or(0);
        let message_id = item.message_id;
        let looping = item.kind.loops();
        let path = path.to_path_buf();
        let item = item.clone();
        let extract_path = path.clone();
        let task_slot = slot.clone();
        let task_cancel = cancel.clone();
        #[allow(clippy::let_underscore_future)]
        let _ = cx.spawn(async move |this, cx| {
            let extracted = cx
                .background_executor()
                .spawn(async move {
                    let frames = if looping {
                        quill::animation::viewer_loop_frames_cancelable(
                            &extract_path,
                            &mime,
                            &cache,
                            &task_slot,
                            &task_cancel,
                        )?
                    } else {
                        quill::video::viewer_playback_frames_cancelable(
                            &extract_path,
                            &mime,
                            &cache,
                            start_timestamp,
                            duration,
                            &task_slot,
                            &task_cancel,
                        )?
                    };
                    // Decode on the background thread: the render path needs
                    // pre-loaded handles, and decoding up to 600 PNGs must
                    // not block the UI thread.
                    let decoded = Self::decode_viewer_frames(&frames.frames)?;
                    Ok::<_, String>((decoded, frames.fps))
                })
                .await;
            this.update(cx, |this, cx| {
                // Stale completion (viewer closed/stepped, or a newer
                // extraction started): drop silently — no error note, no
                // playback, and crucially don't clear a newer run's
                // loading state.
                if this.viewer.extract_epoch != epoch {
                    return;
                }
                this.viewer.extracting = false;
                // Drop the slot only if it's still ours (`stop_viewer_video`
                // may have taken it to kill the child).
                if this
                    .viewer
                    .extract_child
                    .as_ref()
                    .is_some_and(|current| Arc::ptr_eq(current, &slot))
                {
                    this.viewer.extract_child = None;
                }
                let still_current = this
                    .viewer
                    .state
                    .current()
                    .is_some_and(|current| current.message_id == message_id);
                if !still_current {
                    return;
                }
                match extracted {
                    Ok((decoded, fps)) => {
                        // The screenshot demo extracts + decodes synchronously
                        // and starts playback itself; don't restart it when the
                        // background extraction lands.
                        let already_playing = this.viewer.video == Some(message_id)
                            && !this.viewer.video_frames.is_empty();
                        if !already_playing {
                            this.viewer.video_frames = decoded;
                            this.viewer.video_fps = fps;
                            this.play_viewer_video(&item, &path, cx);
                        }
                    }
                    Err(err) => {
                        this.viewer.video_frames.clear();
                        // MED1: TGX distinguishes unsupported formats
                        // (`VideoPlaybackUnsupported`) from generic
                        // playback failures (`VideoPlaybackError`).
                        let noun = if item.kind.loops() {
                            "animation"
                        } else {
                            "video"
                        };
                        let message = if err.starts_with("unsupported") {
                            format!("{noun} format not supported")
                        } else {
                            format!("couldn't play this {noun}")
                        };
                        this.connection.status_note = message.clone();
                        this.playback.error = Some(message);
                    }
                }
                cx.notify();
            })
            .ok();
        });
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

    /// Begin (or restart) viewer playback of `item`'s clip from offset 0.
    /// Stops every other player first — one thing plays at a time.
    /// Frames must already be in `viewer_video_frames` (extracted async by
    /// `extract_viewer_frames`, or synchronously by the screenshot demo).
    /// State only: the caller starts the audio (the screenshot demo skips the
    /// subprocess, like the audio slice's demo).
    pub(in crate::ui) fn begin_viewer_video(
        &mut self,
        item: &MediaViewerItem,
        path: &std::path::Path,
        cx: &mut Context<Self>,
    ) {
        self.stop_voice_playback();
        self.stop_audio_playback();
        self.stop_video_playback();
        self.stop_animation_playback();
        let looping = item.kind.loops();
        let mut duration = item.duration_secs.unwrap_or(0).max(0) as f64;
        // A loop wraps at the end of its frames, not at TDLib's rounded
        // duration.
        if looping && !self.viewer.video_frames.is_empty() && self.viewer.video_fps > 0.0 {
            duration = self.viewer.video_frames.len() as f64 / self.viewer.video_fps;
        }
        let mut clock = PlaybackClock::new(duration);
        if !looping {
            clock.set_rate(self.playback.speed);
        }
        // A media-timestamp link opened this clip at a given second.
        let start = self
            .viewer
            .pending_seek
            .take_if(|(id, _)| *id == item.message_id)
            .map_or(0.0, |(_, secs)| secs.clamp(0.0, duration));
        clock.seek(start);
        clock.resume();
        self.viewer.clock = Some(clock);
        self.viewer.video = Some(item.message_id);
        self.viewer.video_path = Some(path.to_path_buf());
        self.playback.error = None;
        if looping {
            // No transport, no sound: nothing to scrub or mute.
            self.spawn_viewer_tick(cx);
            cx.notify();
            return;
        }
        // MED1: viewer seek slider (the history-row seek bar pattern).
        let slider = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(duration.max(1.0) as f32)
                .default_value(0.0)
        });
        cx.subscribe(&slider, |this, _, event, cx| {
            this.on_viewer_seek_event(event, cx);
        })
        .detach();
        self.viewer.seek_slider = Some(slider);
        // MED1: volume slider (0–100%); applies on release so a drag
        // doesn't restart the sound per tick.
        let volume = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(100.0)
                .default_value((self.playback.volume * 100.0).round())
        });
        cx.subscribe(&volume, |this, _, event, cx| {
            this.on_viewer_volume_event(event, cx);
        })
        .detach();
        self.viewer.volume_slider = Some(volume);
        self.spawn_viewer_tick(cx);
        cx.notify();
    }

    /// Begin viewer playback *and* start the audio — unless this
    /// is a screenshot demo, which skips the subprocess (same posture as
    /// `request_media_download`'s demo branch).
    pub(in crate::ui) fn play_viewer_video(
        &mut self,
        item: &MediaViewerItem,
        path: &std::path::Path,
        cx: &mut Context<Self>,
    ) {
        self.begin_viewer_video(item, path, cx);
        // GIFs are silent: no audio player.
        if self.demo_session.is_none() && !item.kind.loops() {
            let start = self.viewer.clock.as_ref().map_or(0.0, |c| c.elapsed_secs());
            self.start_viewer_audio(path, start);
        }
    }

    /// Play the viewer clip with the native player: real-time hardware
    /// decode with its own audio, drawn frame by frame (no extraction).
    pub(in crate::ui) fn play_native_viewer_video(
        &mut self,
        item: &MediaViewerItem,
        path: &std::path::Path,
        cx: &mut Context<Self>,
    ) {
        self.begin_viewer_video(item, path, cx);
        match crate::ui::native_video::NativeVideo::open(
            path,
            crate::ui::native_video::Purpose::Viewer,
        ) {
            Ok(mut video) => {
                if item.kind.loops() {
                    video.set_volume(0.0);
                    video.play();
                } else {
                    video.set_volume(self.playback.volume);
                    video.play();
                    video.set_rate(self.playback.speed as f32);
                }
                let start = self.viewer.clock.as_ref().map_or(0.0, |c| c.elapsed_secs());
                if start > 0.0 {
                    video.seek(start);
                }
                self.viewer.native = Some(video);
            }
            Err(err) => {
                self.stop_viewer_video();
                self.playback.error = Some(err);
            }
        }
        cx.notify();
    }

    /// Play the viewer clip's soundtrack in-process (MP4/AAC and friends
    /// decode in the same engine as voice notes). The video frames render
    /// in-viewer from `viewer_video_frames`; this only supplies the sound.
    /// A clip with no decodable audio just plays silently, with the error
    /// shown on the transport.
    pub(in crate::ui) fn start_viewer_audio(
        &mut self,
        path: &std::path::Path,
        offset_secs: f64,
    ) -> bool {
        match self
            .viewer
            .audio
            .start(path, offset_secs, self.playback.volume, self.playback.speed)
        {
            Ok(()) => {
                self.playback.error = None;
                true
            }
            Err(err) => {
                self.playback.error = Some(err.to_string());
                false
            }
        }
    }

    pub(in crate::ui) fn kill_viewer_player(&mut self) {
        self.viewer.audio.stop();
    }

    /// Pause: freeze the clock, stop the sound, keep the item active so the
    /// controls stay and Play resumes from the frozen offset.
    pub(in crate::ui) fn pause_viewer_video(&mut self, cx: &mut Context<Self>) {
        if let Some(clock) = self.viewer.clock.as_mut() {
            clock.pause();
        }
        if let Some(video) = self.viewer.native.as_mut() {
            video.pause();
        }
        self.kill_viewer_player();
        cx.notify();
    }

    /// Resume from the frozen clock position.
    pub(in crate::ui) fn resume_viewer_video(&mut self, cx: &mut Context<Self>) {
        let loops = self.viewer_loops();
        if let Some(video) = self.viewer.native.as_mut() {
            video.play();
            if !loops {
                video.set_rate(self.playback.speed as f32);
            }
            if let Some(clock) = self.viewer.clock.as_mut() {
                clock.seek(video.position_secs());
                clock.resume();
            }
            cx.notify();
            return;
        }
        let offset = self.viewer.clock.as_ref().map(|c| c.elapsed_secs());
        let path = self.viewer.video_path.clone();
        match (offset, path) {
            (Some(offset), Some(path)) => {
                if !loops {
                    self.start_viewer_audio(&path, offset);
                }
                if let Some(clock) = self.viewer.clock.as_mut() {
                    clock.resume();
                }
            }
            _ => self.stop_viewer_video(),
        }
        cx.notify();
    }

    /// The viewer Play/Pause button: playing → pause, paused → resume,
    /// never-started → begin from 0 (the clip is local here). The
    /// never-started path routes through `maybe_autoplay_viewer_video` so a
    /// local clip without extracted frames goes through async extraction
    /// instead of playing with an empty frame cache.
    pub(in crate::ui) fn toggle_viewer_video(&mut self, cx: &mut Context<Self>) {
        let playing = self
            .viewer
            .clock
            .as_ref()
            .is_some_and(|clock| clock.is_playing());
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
        let (playing, duration) = self
            .viewer
            .clock
            .as_ref()
            .map(|clock| (clock.is_playing(), clock.duration_secs()))
            .unwrap_or((false, 0.0));
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
        let Some(clock) = self.viewer.clock.as_ref() else {
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
            clock.duration_secs(),
            clock.elapsed_secs(),
            clock.is_playing(),
            self.playback.speed,
        );
        quill::media_session::publish(Some(&info));
    }

    /// Full stop: stop the sound and any running frame extraction, and clear
    /// all viewer-video state. Called on viewer close/step and when any
    /// other player starts.
    pub(in crate::ui) fn stop_viewer_video(&mut self) {
        self.viewer.pip_window = None;
        self.viewer.native = None;
        self.kill_viewer_player();
        self.kill_viewer_extraction();
        self.viewer.video = None;
        self.viewer.video_path = None;
        self.viewer.clock = None;
        self.viewer.pending_play = None;
        crate::ui::image_budget::retire_all(self.viewer.video_frames.drain(..));
        self.viewer.extracting = false;
        self.viewer.seek_slider = None;
        self.viewer.seek_scrubbing = false;
        self.viewer.seek_preview_secs = None;
        self.viewer.volume_slider = None;
        self.viewer.volume_scrubbing = false;
        if let Some(cached) = self.viewer.frame_cache_file.take() {
            quill::video::discard_viewer_frame_cache(cached);
        }
    }
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
    use super::take_dead_call_tiles;
    use std::collections::HashMap;

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
