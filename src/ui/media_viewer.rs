//! media_viewer.

use super::app::QuillApp;

/// Height of the viewer's top bar (title + actions).
const VIEWER_TOP_BAR: f32 = 56.0;
/// Width kept free on each side of the media for the prev/next arrows.
const VIEWER_SIDE_LANE: f32 = 80.0;
use super::message_media::{file_is_downloading, viewer_display_path};
use super::message_text::rich_text_line;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState, SliderValue};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::{ChatId, FileId, MessageId};
use quill::local_path::sandboxed_display_path;
use quill::media_viewer::{
    MediaViewer, MediaViewerItem, MediaViewerKind, ViewerVideoStart, collect_media_items,
    decide_viewer_video_start, rotate_rgba_quarter_turns, save_media_to_downloads,
};
use quill::playback::PlaybackClock;
use quill::settings::MediaPrefs;
use quill::state::HistoryMessage;
use quill::telegram::envelope::{MessageSender, ParsedFile};
use quill::voice::format_voice_duration;
use smallvec::SmallVec;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::{Child, Stdio};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
impl QuillApp {
    /// Phase 4.5: open the fullscreen media viewer on the clicked message.
    /// Items are the chat's photo/video messages (oldest first); the clicked
    /// message becomes the current item. When nothing viewable is local yet
    /// the viewer shows a loading state and `downloadFile` is triggered —
    /// the 40ms poll loop re-renders when `updateFile` lands.
    pub(super) fn open_media_viewer(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        let items = self
            .session()
            .and_then(|session| session.histories.get(&chat_id.0))
            .map(|history| {
                collect_media_items(&history.ordered().into_iter().cloned().collect::<Vec<_>>())
            })
            .unwrap_or_default();
        if items.is_empty() {
            return;
        }
        let index = items.iter().position(|item| item.message_id == message_id);
        // The clicked message may not be viewer-openable (e.g. an album
        // tile for a non-photo/video part) — then stay closed instead of
        // opening on an unrelated item.
        let Some(index) = index else {
            return;
        };
        self.media_viewer = MediaViewer::open(items, index);
        self.viewer_zoom.reset();
        self.viewer_drag = None;
        // MED1: rotation and playback error are per-item state.
        self.viewer_rotation = 0;
        self.viewer_rotated = None;
        self.playback_error = None;
        self.stop_viewer_video();
        self.ensure_viewer_download(cx);
        self.maybe_autoplay_viewer_video(cx);
        cx.notify();
    }

    pub(super) fn close_media_viewer(&mut self, cx: &mut Context<Self>) {
        self.stop_viewer_video();
        self.media_viewer.close();
        cx.notify();
    }

    pub(super) fn step_media_viewer(&mut self, delta: i32, cx: &mut Context<Self>) {
        if delta < 0 {
            self.media_viewer.prev();
        } else {
            self.media_viewer.next();
        }
        self.viewer_zoom.reset();
        self.viewer_drag = None;
        // MED1: rotation is per-item.
        self.viewer_rotation = 0;
        self.viewer_rotated = None;
        self.stop_viewer_video();
        self.ensure_viewer_download(cx);
        self.maybe_autoplay_viewer_video(cx);
        cx.notify();
    }

    /// Trigger `downloadFile` for the current viewer item when no display
    /// candidate is local yet (photo: largest size; video: thumbnail, else
    /// the clip itself). Reuses `request_media_download`; no live request
    /// happens in demo mode (it only sets a status note).
    pub(super) fn ensure_viewer_download(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.media_viewer.current().cloned() else {
            return;
        };
        let roots = self.media_display_roots();
        let local = self.session().is_some_and(|session| {
            item.display_file_ids.iter().any(|id| {
                session
                    .files
                    .get(&id.0)
                    .and_then(|file| file.usable_path())
                    .and_then(|path| sandboxed_display_path(path, &roots))
                    .is_some()
            })
        });
        if !local {
            self.request_media_download(item.download_file_id, None, cx);
        }
    }

    /// Parity slice 5: in-viewer video playback. The clip's frames are
    /// extracted with ffmpeg, pre-decoded into GPUI image handles, and
    /// rendered in-place in the viewer overlay (no GPUI video element in
    /// this stack — same frame-cycling approach as the row video preview,
    /// but full-clip); ffplay runs `-nodisp` for the audio track only. The
    /// overlay keeps Play/Pause and elapsed/total; the thumbnail shows
    /// until frames are ready. Closing or stepping the viewer stops
    /// playback and drops the frame cache.

    /// Sandbox-checked local path of the current item's full video clip.
    pub(super) fn viewer_clip_path(&self, item: &MediaViewerItem) -> Option<PathBuf> {
        let play_id = item.play_file_id?;
        let roots = self.media_display_roots();
        self.session()?
            .files
            .get(&play_id.0)?
            .usable_path()
            .and_then(|path| sandboxed_display_path(path, &roots).map(|p| p.to_path_buf()))
    }

    /// Start viewer playback when the current item is a video whose clip is
    /// local; otherwise trigger `downloadFile` for the clip and park the
    /// request in `viewer_pending_play` (resumed from the poll loop).
    ///
    /// Parity slice 5: the clip's frames are extracted (async — ffmpeg takes
    /// ~2 s for a 12 s clip), decoded into pre-loaded image handles, and
    /// rendered in-viewer; ffplay runs `-nodisp` for audio only. If frames
    /// are already cached for this file (e.g. the screenshot demo decoded
    /// them synchronously), playback starts at once. Every start path
    /// routes through `decide_viewer_video_start` so a local clip without
    /// cached frames always goes through extraction — never straight to
    /// playback with an empty frame cache.
    pub(super) fn maybe_autoplay_viewer_video(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.media_viewer.current().cloned() else {
            return;
        };
        let file_id = item.play_file_id.map(|id| id.0).unwrap_or(0);
        let path = self.viewer_clip_path(&item);
        let frames_ready =
            self.viewer_frame_cache_file == Some(file_id) && !self.viewer_video_frames.is_empty();
        match decide_viewer_video_start(&item, path.is_some(), frames_ready) {
            ViewerVideoStart::Nothing => {}
            ViewerVideoStart::ParkDownload => {
                if let Some(play_id) = item.play_file_id {
                    self.viewer_pending_play = Some((item.message_id, play_id));
                    self.request_media_download(play_id, None, cx);
                }
            }
            ViewerVideoStart::PlayNow => {
                self.viewer_pending_play = None;
                let path = path.expect("clip checked local by decide_viewer_video_start");
                self.play_viewer_video(&item, &path, cx);
            }
            ViewerVideoStart::ExtractFrames => {
                self.viewer_pending_play = None;
                // The screenshot demo extracts + decodes frames synchronously
                // itself; don't start a redundant background extraction.
                if self.viewer_demo_sync_frames {
                    return;
                }
                let path = path.expect("clip checked local by decide_viewer_video_start");
                self.extract_viewer_frames(&item, &path, cx);
            }
        }
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
    pub(super) fn decode_viewer_frames(paths: &[PathBuf]) -> Result<Vec<Arc<RenderImage>>, String> {
        paths
            .iter()
            .map(|path| {
                let mut rgba = image::open(path)
                    .map_err(|err| format!("{}: {err}", path.display()))?
                    .into_rgba8();
                // RenderImage stores BGRA, as does the shared call-frame decoder below.
                for pixel in rgba.chunks_exact_mut(4) {
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
    pub(super) fn video_render_image(
        frame: &quill::calls::engine::VideoFrame,
    ) -> Option<Arc<RenderImage>> {
        let mut rgba = image::RgbaImage::from_raw(
            u32::from(frame.width),
            u32::from(frame.height),
            frame.rgba.clone(),
        )?;
        // GPUI holds RenderImage pixels in BGRA (its own decoder swaps
        // R<->B after into_rgba8); the engine delivers RGBA, so swap here.
        for pixel in rgba.chunks_exact_mut(4) {
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
    pub(super) fn cached_video_image(
        &mut self,
        local: bool,
        frame: &quill::calls::engine::VideoFrame,
    ) -> Option<Arc<RenderImage>> {
        let key = (frame.seq, frame.is_screen);
        let slot = if local {
            &mut self.call_local_image
        } else {
            &mut self.call_remote_image
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
    pub(super) fn cached_group_video_image(
        &mut self,
        call_id: i32,
        user_id: i64,
        screen: bool,
        frame: &quill::calls::engine::VideoFrame,
    ) -> Option<Arc<RenderImage>> {
        let key = (call_id, user_id, screen);
        if self
            .group_video_images
            .get(&key)
            .is_some_and(|(seq, _)| *seq == frame.seq)
        {
            return self
                .group_video_images
                .get(&key)
                .map(|(_, image)| image.clone());
        }
        let image = Self::video_render_image(frame)?;
        self.group_video_images
            .insert(key, (frame.seq, image.clone()));
        Some(image)
    }

    /// Phase C2g: newest frame for one group participant slot. Live
    /// mode reads the driver's retained slot; demo mode reads the
    /// fixture's synthetic frames. Prefers the screen-sharing slot when
    /// the participant is sharing and a screen frame exists, otherwise
    /// the camera slot. `None` when the participant isn't sending
    /// video or no frame arrived yet.
    pub(super) fn group_participant_frame(
        &self,
        call_id: i32,
        participant: &quill::telegram::envelope::ParsedGroupCallParticipant,
    ) -> Option<(i64, bool, quill::calls::engine::VideoFrame)> {
        let MessageSender::User { user_id } = participant.participant_id else {
            return None;
        };
        let slot = |screen: bool| {
            if let Some(live) = self.live.as_ref() {
                live.driver
                    .latest_group_video_frame(call_id, user_id, screen)
            } else {
                self.demo_group_frames.get(&(user_id, screen)).cloned()
            }
        };
        if participant.screen_sharing_enabled
            && let Some(frame) = slot(true)
        {
            return Some((user_id, true, frame));
        }
        if participant.video_enabled
            && let Some(frame) = slot(false)
        {
            return Some((user_id, false, frame));
        }
        None
    }

    /// Kill a running viewer frame extraction (ffmpeg child), if any, and
    /// invalidate its completion. Called on viewer close/step and before a
    /// fresh extraction starts, so an abandoned extraction can't run to
    /// completion on a discarded cache dir.
    pub(super) fn kill_viewer_extraction(&mut self) {
        // Signal cancellation first: the worker checks this before spawning
        // and right after publishing the child, so a kill that lands before
        // ffmpeg publishes still aborts the run instead of orphaning it.
        if let Some(cancel) = self.viewer_extract_cancel.take() {
            cancel.store(true, Ordering::SeqCst);
        }
        if let Some(slot) = self.viewer_extract_child.take() {
            // Take the child out of the lock before kill/wait: the worker
            // only holds the lock briefly around `try_wait`.
            let child = slot.lock().ok().and_then(|mut guard| guard.take());
            drop(slot);
            if let Some(mut child) = child {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        self.viewer_extract_epoch = self.viewer_extract_epoch.wrapping_add(1);
    }

    /// Extract the clip's frames on a background thread, decode them into
    /// pre-loaded image handles, then start playback if the viewer is still
    /// on the same item. The thumbnail stays visible with a loading hint
    /// meanwhile. The ffmpeg child is published so close/step can kill it;
    /// a completion from a killed or superseded run is dropped by epoch.
    pub(super) fn extract_viewer_frames(
        &mut self,
        item: &MediaViewerItem,
        path: &std::path::Path,
        cx: &mut Context<Self>,
    ) {
        let file_id = item.play_file_id.map(|id| id.0).unwrap_or(0);
        if self
            .viewer_frame_cache_file
            .is_some_and(|cached| cached != file_id)
            && let Some(old) = self.viewer_frame_cache_file.take()
        {
            quill::video::discard_viewer_frame_cache(old);
        }
        self.viewer_frame_cache_file = Some(file_id);
        self.viewer_video_frames.clear();
        self.viewer_extracting = true;
        // A step between two videos goes through `stop_viewer_video` first,
        // but cancel explicitly anyway: a fresh run must not share the
        // previous run's slot or epoch.
        self.kill_viewer_extraction();
        let epoch = self.viewer_extract_epoch;
        let slot: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
        self.viewer_extract_child = Some(slot.clone());
        let cancel: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
        self.viewer_extract_cancel = Some(cancel.clone());
        cx.notify();

        let cache = quill::video::viewer_frame_cache_dir(file_id);
        let mime = item.mime_type.clone().unwrap_or_default();
        let duration = item.duration_secs.unwrap_or(0);
        let start_timestamp = item.start_timestamp.unwrap_or(0);
        let message_id = item.message_id;
        let path = path.to_path_buf();
        let item = item.clone();
        let extract_path = path.clone();
        let task_slot = slot.clone();
        let task_cancel = cancel.clone();
        let _ = cx.spawn(async move |this, cx| {
            let extracted = cx
                .background_executor()
                .spawn(async move {
                    let frames = quill::video::viewer_playback_frames_cancelable(
                        &extract_path,
                        &mime,
                        &cache,
                        start_timestamp,
                        duration,
                        &task_slot,
                        &task_cancel,
                    )?;
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
                if this.viewer_extract_epoch != epoch {
                    return;
                }
                this.viewer_extracting = false;
                // Drop the slot only if it's still ours (`stop_viewer_video`
                // may have taken it to kill the child).
                if this
                    .viewer_extract_child
                    .as_ref()
                    .is_some_and(|current| Arc::ptr_eq(current, &slot))
                {
                    this.viewer_extract_child = None;
                }
                let still_current = this
                    .media_viewer
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
                        let already_playing = this.viewer_video == Some(message_id)
                            && !this.viewer_video_frames.is_empty();
                        if !already_playing {
                            this.viewer_video_frames = decoded;
                            this.viewer_video_fps = fps;
                            this.play_viewer_video(&item, &path, cx);
                        }
                    }
                    Err(err) => {
                        this.viewer_video_frames.clear();
                        // MED1: TGX distinguishes unsupported formats
                        // (`VideoPlaybackUnsupported`) from generic
                        // playback failures (`VideoPlaybackError`).
                        let message = if err == "unsupported video" {
                            "video format not supported"
                        } else {
                            "couldn't play this video"
                        };
                        this.playback_error = Some(message.into());
                        this.status_note = message.into();
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
    pub(super) fn resume_pending_viewer_video(&mut self, cx: &mut Context<Self>) {
        let Some((message_id, file_id)) = self.viewer_pending_play else {
            return;
        };
        let ready = self.session().is_some_and(|session| {
            session
                .files
                .get(&file_id.0)
                .and_then(|file| file.usable_path())
                .is_some()
        });
        if !ready {
            return;
        }
        let matches = self.media_viewer.current().is_some_and(|item| {
            item.message_id == message_id && item.kind == MediaViewerKind::Video
        });
        if !matches {
            self.viewer_pending_play = None;
            return;
        }
        self.maybe_autoplay_viewer_video(cx);
    }

    /// Begin (or restart) viewer playback of `item`'s clip from offset 0.
    /// Stops every other player first — one thing plays at a time.
    /// Frames must already be in `viewer_video_frames` (extracted async by
    /// `extract_viewer_frames`, or synchronously by the screenshot demo).
    /// State only: the caller spawns ffplay (the screenshot demo skips the
    /// subprocess, like the audio slice's demo).
    pub(super) fn begin_viewer_video(
        &mut self,
        item: &MediaViewerItem,
        path: &std::path::Path,
        cx: &mut Context<Self>,
    ) {
        self.stop_voice_playback();
        self.stop_audio_playback();
        self.stop_video_playback();
        self.stop_animation_playback();
        let duration = item.duration_secs.unwrap_or(0).max(0) as f64;
        let mut clock = PlaybackClock::new(duration);
        clock.set_rate(self.playback_speed);
        clock.seek(0.0);
        clock.resume();
        self.viewer_clock = Some(clock);
        self.viewer_video = Some(item.message_id);
        self.viewer_video_path = Some(path.to_path_buf());
        self.playback_error = None;
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
        self.viewer_seek_slider = Some(slider);
        // MED1: volume slider (0–100%); applies on release so a drag
        // doesn't restart ffplay per tick.
        let volume = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(100.0)
                .default_value((self.playback_volume * 100.0).round())
        });
        cx.subscribe(&volume, |this, _, event, cx| {
            this.on_viewer_volume_event(event, cx);
        })
        .detach();
        self.viewer_volume_slider = Some(volume);
        self.spawn_viewer_tick(cx);
        cx.notify();
    }

    /// Begin viewer playback *and* spawn ffplay for audio — unless this
    /// is a screenshot demo, which skips the subprocess (same posture as
    /// `request_media_download`'s demo branch).
    pub(super) fn play_viewer_video(
        &mut self,
        item: &MediaViewerItem,
        path: &std::path::Path,
        cx: &mut Context<Self>,
    ) {
        self.begin_viewer_video(item, path, cx);
        if self.demo_session.is_none() {
            self.spawn_viewer_ffplay(path, 0.0);
        }
    }

    /// Spawn ffplay `-nodisp` (audio only) for the viewer clip. The video
    /// frames render in-viewer from `viewer_video_frames`; ffplay only
    /// supplies the soundtrack. `-autoexit` ends the child at the clip's
    /// end; our tick clears state to match. A missing ffplay (or a clip
    /// with no audio) just means silent playback — the frames still show.
    pub(super) fn spawn_viewer_ffplay(&mut self, path: &std::path::Path, offset_secs: f64) -> bool {
        self.kill_viewer_player();
        let mut command = self.ffplay_command(offset_secs);
        match command
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => {
                self.viewer_player = Some(child);
                self.playback_error = None;
                true
            }
            // MED1: honest error instead of the old silent failure.
            Err(_) => {
                self.playback_error = Some("audio player (ffplay) couldn't start".into());
                false
            }
        }
    }

    pub(super) fn kill_viewer_player(&mut self) {
        if let Some(mut child) = self.viewer_player.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Pause: freeze the clock, kill ffplay, keep the item active so the
    /// controls stay and Play resumes from the frozen offset.
    pub(super) fn pause_viewer_video(&mut self, cx: &mut Context<Self>) {
        if let Some(clock) = self.viewer_clock.as_mut() {
            clock.pause();
        }
        self.kill_viewer_player();
        cx.notify();
    }

    /// Resume from the frozen clock position.
    pub(super) fn resume_viewer_video(&mut self, cx: &mut Context<Self>) {
        let offset = self.viewer_clock.as_ref().map(|c| c.elapsed_secs());
        let path = self.viewer_video_path.clone();
        match (offset, path) {
            (Some(offset), Some(path)) => {
                self.spawn_viewer_ffplay(&path, offset);
                if let Some(clock) = self.viewer_clock.as_mut() {
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
    pub(super) fn toggle_viewer_video(&mut self, cx: &mut Context<Self>) {
        let playing = self
            .viewer_clock
            .as_ref()
            .is_some_and(|clock| clock.is_playing());
        if self.viewer_video.is_none() {
            self.maybe_autoplay_viewer_video(cx);
            return;
        }
        if playing {
            self.pause_viewer_video(cx);
        } else {
            self.resume_viewer_video(cx);
        }
    }

    /// Full stop: kill ffplay and any running frame extraction, and clear
    /// all viewer-video state. Called on viewer close/step and when any
    /// other player starts.
    pub(super) fn stop_viewer_video(&mut self) {
        self.pip_window = None;
        self.kill_viewer_player();
        self.kill_viewer_extraction();
        self.viewer_video = None;
        self.viewer_video_path = None;
        self.viewer_clock = None;
        self.viewer_pending_play = None;
        self.viewer_video_frames.clear();
        self.viewer_extracting = false;
        self.viewer_seek_slider = None;
        self.viewer_seek_scrubbing = false;
        self.viewer_seek_preview_secs = None;
        self.viewer_volume_slider = None;
        self.viewer_volume_scrubbing = false;
        if let Some(cached) = self.viewer_frame_cache_file.take() {
            quill::video::discard_viewer_frame_cache(cached);
        }
    }

    // ===================== MED1: viewer actions =====================

    /// MED1: rotate the viewer photo 90° clockwise (photos only). The
    /// rotated pixels are decoded eagerly and cached in `viewer_rotated`
    /// so the overlay render stays allocation-free; reset on open/step.
    pub(super) fn rotate_viewer_photo(&mut self, cx: &mut Context<Self>) {
        let item = self.media_viewer.current().cloned();
        let Some(item) = item else { return };
        if item.kind != MediaViewerKind::Photo {
            return;
        }
        self.viewer_rotation = (self.viewer_rotation + 1) % 4;
        self.viewer_rotated = None;
        if self.viewer_rotation == 0 {
            cx.notify();
            return;
        }
        let files: HashMap<i32, ParsedFile> =
            self.session().map(|s| s.files.clone()).unwrap_or_default();
        let roots = self.media_display_roots();
        let path = viewer_display_path(&item, &files, &roots);
        match path {
            Some(path) => match Self::rotated_render_image(&path, self.viewer_rotation) {
                Some(image) => {
                    self.viewer_rotated = Some((path, self.viewer_rotation, image));
                }
                None => {
                    self.status_note = "couldn't rotate this photo".into();
                    self.viewer_rotation = 0;
                }
            },
            None => {
                self.status_note = "download the photo first to rotate it".into();
                self.viewer_rotation = 0;
            }
        }
        cx.notify();
    }

    /// MED1: decode `path` and rotate it by `turns` quarter-turns into a
    /// `RenderImage` (the `decode_viewer_frames` construction pattern).
    pub(super) fn rotated_render_image(path: &PathBuf, turns: u8) -> Option<Arc<RenderImage>> {
        let rgba = image::open(path).ok()?.to_rgba8();
        let (pixels, width, height) =
            rotate_rgba_quarter_turns(rgba.as_raw(), rgba.width(), rgba.height(), turns);
        let rotated = image::RgbaImage::from_raw(width, height, pixels)?;
        Some(Arc::new(RenderImage::new(SmallVec::from_buf([
            image::Frame::new(rotated),
        ]))))
    }

    /// MED1: share the viewer media — opens the forward picker with the
    /// current message selected (the message-menu forward flow, reused).
    pub(super) fn share_viewer_media(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.media_viewer.current() else {
            return;
        };
        let (chat_id, message_id) = (item.chat_id, item.message_id);
        // Close the overlay first so the forward picker is visible.
        self.close_media_viewer(cx);
        self.begin_forward_one(chat_id, message_id, false, window, cx);
    }

    /// MED1: save the viewer media to the downloads folder. Photos save
    /// the largest local size; videos save the full clip when local,
    /// else the thumbnail.
    pub(super) fn save_viewer_media(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.media_viewer.current().cloned() else {
            return;
        };
        let files: HashMap<i32, ParsedFile> =
            self.session().map(|s| s.files.clone()).unwrap_or_default();
        let path = match item.kind {
            MediaViewerKind::Photo => {
                let roots = self.media_display_roots();
                viewer_display_path(&item, &files, &roots)
            }
            // Video: save the full clip only — falling back to the
            // thumbnail would write a JPEG as the "video". If the clip
            // isn't local the user gets the honest download-first note.
            MediaViewerKind::Video => item
                .play_file_id
                .and_then(|id| files.get(&id.0))
                .and_then(|file| file.usable_path())
                .map(PathBuf::from),
        };
        match path {
            Some(path) => match save_media_to_downloads(&path) {
                Ok(dest) => {
                    self.status_note = format!(
                        "saved to {}",
                        dest.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("downloads")
                    );
                }
                Err(err) => {
                    self.status_note = format!("couldn't save: {err}");
                }
            },
            None => {
                self.status_note = "download the media first to save it".into();
            }
        }
        cx.notify();
    }

    /// MED1: "Show in chat" — close the viewer and jump to the source
    /// message (the reply-jump machinery, reused).
    pub(super) fn show_viewer_in_chat(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.media_viewer.current() else {
            return;
        };
        let message_id = item.message_id;
        self.close_media_viewer(cx);
        self.jump_to_replied_message(message_id, cx);
    }

    /// MED1: pin/unpin the album the viewer item belongs to. TGX
    /// `MessagePinAlbum` pins each member (`pinChatMessage` per message —
    /// TDLib 1.8.67 has no album-level pin, schema line 13559). When any
    /// member is pinned the action unpins the pinned members instead.
    /// Rights-gated on `ChatSummary::can_pin_messages`.
    pub(super) fn toggle_viewer_album_pin(&mut self, cx: &mut Context<Self>) {
        let (chat_id, album_id) = match self.media_viewer.current() {
            Some(item) => (item.chat_id, self.viewer_album_id(item.message_id)),
            None => return,
        };
        let Some(album_id) = album_id else { return };
        let can_pin = self
            .session()
            .and_then(|s| s.chats.get(&chat_id.0))
            .is_some_and(|chat| chat.can_pin_messages());
        if !can_pin {
            self.status_note = "you can't pin messages in this chat".into();
            cx.notify();
            return;
        }
        let history: Vec<HistoryMessage> = self
            .session()
            .and_then(|s| s.histories.get(&chat_id.0))
            .map(|h| h.ordered().into_iter().cloned().collect())
            .unwrap_or_default();
        let ids = quill::album::album_message_ids(&history, album_id);
        if ids.is_empty() {
            return;
        }
        let pinned: Vec<MessageId> = ids
            .iter()
            .filter(|id| history.iter().any(|m| m.id == **id && m.is_pinned))
            .copied()
            .collect();
        if self.demo_session.is_some() {
            // Demo: toggle only the messages whose pin state must change.
            let pin_target = pinned.is_empty();
            for id in &ids {
                let currently = history.iter().any(|m| m.id == *id && m.is_pinned);
                if currently != pin_target {
                    self.apply_demo_pin_toggle(chat_id, *id);
                }
            }
            self.status_note = "demo: album pin updated".into();
            cx.notify();
            return;
        }
        let Some(live) = self.live.as_mut() else {
            self.status_note = "no live connection".into();
            cx.notify();
            return;
        };
        let mut failed = 0;
        if pinned.is_empty() {
            for id in &ids {
                if live.driver.pin_chat_message(chat_id, *id, false).is_err() {
                    failed += 1;
                }
            }
            self.status_note = if failed == 0 {
                "pinning album…".into()
            } else {
                format!("couldn't pin {failed} album item(s)")
            };
        } else {
            for id in &pinned {
                if live.driver.unpin_chat_message(chat_id, *id).is_err() {
                    failed += 1;
                }
            }
            self.status_note = if failed == 0 {
                "unpinning album…".into()
            } else {
                format!("couldn't unpin {failed} album item(s)")
            };
        }
        cx.notify();
    }

    /// MED1: the `media_album_id` of the history message behind the viewer
    /// item (`None` when the message is not in an album).
    pub(super) fn viewer_album_id(&self, message_id: MessageId) -> Option<i64> {
        let item = self.media_viewer.current()?;
        self.session()?
            .histories
            .get(&item.chat_id.0)?
            .messages
            .get(&message_id.0)
            .and_then(|m| (m.media_album_id != 0).then_some(m.media_album_id))
    }

    // ===================== MED1: viewer seek =====================

    /// MED1: `SliderEvent` sink for the viewer video seek slider. Drag
    /// previews the position; release seeks the clock and restarts ffplay
    /// at the new offset (the history-row `on_seek_event` pattern).
    pub(super) fn on_viewer_seek_event(&mut self, event: &SliderEvent, cx: &mut Context<Self>) {
        match event {
            SliderEvent::Change(value) => {
                self.viewer_seek_scrubbing = true;
                self.viewer_seek_preview_secs = Some(f64::from(value.end()));
                cx.notify();
            }
            SliderEvent::Release(value) => {
                self.viewer_seek_scrubbing = false;
                self.viewer_seek_preview_secs = None;
                self.seek_viewer_to(f64::from(value.end()), cx);
            }
        }
    }

    /// MED1: apply a finished viewer seek. Seeking while paused just moves
    /// the frozen clock; while playing, ffplay restarts at the offset.
    pub(super) fn seek_viewer_to(&mut self, secs: f64, cx: &mut Context<Self>) {
        let Some(clock) = self.viewer_clock.as_mut() else {
            return;
        };
        clock.seek(secs);
        let offset = clock.elapsed_secs();
        if clock.is_playing()
            && let Some(path) = self.viewer_video_path.clone()
        {
            self.spawn_viewer_ffplay(&path, offset);
        }
        cx.notify();
    }

    /// MED1: push the viewer clock into the seek slider so the thumb
    /// follows elapsed time. Called from the overlay render (the tick has
    /// no `&mut Window`, which `SliderState::set_value` needs). Skipped
    /// while scrubbing so the drag is never fought.
    pub(super) fn sync_viewer_seek_slider(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.viewer_seek_scrubbing {
            return;
        }
        if let (Some(slider), Some(clock)) =
            (self.viewer_seek_slider.as_ref(), self.viewer_clock.as_ref())
        {
            let value = clock.elapsed_secs().clamp(0.0, clock.duration_secs()) as f32;
            let changed = slider.read(cx).value() != SliderValue::Single(value);
            if changed {
                slider.update(cx, |state, cx| {
                    state.set_value(value, window, cx);
                });
            }
        }
    }

    /// MED1: `SliderEvent` sink for the viewer volume slider. The volume
    /// applies on release so a drag doesn't restart ffplay per tick.
    pub(super) fn on_viewer_volume_event(&mut self, event: &SliderEvent, cx: &mut Context<Self>) {
        match event {
            SliderEvent::Change(_) => {
                self.viewer_volume_scrubbing = true;
                cx.notify();
            }
            SliderEvent::Release(value) => {
                self.viewer_volume_scrubbing = false;
                self.set_playback_volume(f64::from(value.end()) as f32 / 100.0, cx);
            }
        }
    }

    /// MED1: push `playback_volume` into the volume slider (e.g. after
    /// the mute toggle moved it). Skipped while dragging.
    pub(super) fn sync_viewer_volume_slider(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.viewer_volume_scrubbing {
            return;
        }
        if let Some(slider) = self.viewer_volume_slider.as_ref() {
            let value = (self.playback_volume * 100.0).round();
            let changed = slider.read(cx).value() != SliderValue::Single(value);
            if changed {
                slider.update(cx, |state, cx| {
                    state.set_value(value, window, cx);
                });
            }
        }
    }

    // ===================== MED1: speed & volume =====================

    /// MED1: cycle playback speed through the TGX `PlaybackSpeedLayout`
    /// set (0.5x, 0.7x, 1x, 1.2x, 1.5x, 2x). Applies to the active
    /// voice/audio track and the viewer video: the clock rate moves the
    /// playhead and ffplay restarts with `-af atempo=` so audio stays in
    /// sync.
    pub(super) fn cycle_playback_speed(&mut self, cx: &mut Context<Self>) {
        const SPEEDS: [f64; 6] = [0.5, 0.7, 1.0, 1.2, 1.5, 2.0];
        let next = SPEEDS
            .iter()
            .position(|s| (*s - self.playback_speed).abs() < 0.01)
            .map(|i| SPEEDS[(i + 1) % SPEEDS.len()])
            .unwrap_or(1.0);
        self.playback_speed = next;
        let mut restarted = false;
        if let Some(clock) = self.playback_clock.as_mut() {
            let offset = clock.elapsed_secs();
            let was_playing = clock.is_playing();
            clock.set_rate(next);
            if was_playing {
                self.restart_player_at(offset);
                restarted = true;
            }
        }
        if let Some(clock) = self.viewer_clock.as_mut() {
            let offset = clock.elapsed_secs();
            let was_playing = clock.is_playing();
            clock.set_rate(next);
            if was_playing && let Some(path) = self.viewer_video_path.clone() {
                self.spawn_viewer_ffplay(&path, offset);
                restarted = true;
            }
        }
        if !restarted {
            // Nothing playing: the speed applies to the next play.
            self.status_note = format!("playback speed {}×", Self::speed_label(next));
        }
        cx.notify();
    }

    /// MED1: short "1.5×" style label for the speed button.
    pub(super) fn speed_label(speed: f64) -> String {
        if (speed - speed.round()).abs() < 0.01 {
            format!("{}×", speed as i32)
        } else {
            format!("{speed}×")
        }
    }

    /// MED1: mute toggle — 0 volume remembers the previous level and
    /// restores it on unmute; ffplay restarts with `-volume`.
    pub(super) fn toggle_playback_mute(&mut self, cx: &mut Context<Self>) {
        if self.playback_volume > 0.01 {
            self.playback_unmuted_volume = self.playback_volume;
            self.set_playback_volume(0.0, cx);
        } else {
            let restore = self.playback_unmuted_volume.max(0.01);
            self.set_playback_volume(restore, cx);
        }
    }

    /// MED1: set playback volume 0.0–1.0 and restart any active player so
    /// ffplay picks up the new `-volume`.
    pub(super) fn set_playback_volume(&mut self, volume: f32, cx: &mut Context<Self>) {
        self.playback_volume = volume.clamp(0.0, 1.0);
        if self.playback_clock.as_ref().is_some_and(|c| c.is_playing()) {
            let offset = self
                .playback_clock
                .as_ref()
                .map(|c| c.elapsed_secs())
                .unwrap_or(0.0);
            self.restart_player_at(offset);
        }
        if self.viewer_clock.as_ref().is_some_and(|c| c.is_playing())
            && let Some(path) = self.viewer_video_path.clone()
        {
            let offset = self
                .viewer_clock
                .as_ref()
                .map(|c| c.elapsed_secs())
                .unwrap_or(0.0);
            self.spawn_viewer_ffplay(&path, offset);
        }
        cx.notify();
    }

    /// MED1: update one media pref in the session and persist it
    /// (the `set_call_pref` pattern).
    pub(super) fn set_media_pref(
        &mut self,
        update: impl FnOnce(&mut MediaPrefs),
        cx: &mut Context<Self>,
    ) {
        let mut prefs = self
            .session()
            .map(|session| session.media_prefs.clone())
            .unwrap_or_default();
        update(&mut prefs);
        if let Some(live) = self.live.as_mut() {
            live.driver.session.media_prefs = prefs;
            if let Err(err) = live.driver.save_media_prefs() {
                self.status_note = format!("couldn't save media settings: {err}");
            }
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.media_prefs = prefs;
        }
        cx.notify();
    }

    /// MED1: effective composer grouping — the user's toggle when set,
    /// else the remembered pref (grouped when remembering is off).
    pub(super) fn composer_group_media_effective(&self) -> bool {
        self.composer_group_media.unwrap_or_else(|| {
            self.session()
                .map(|s| s.media_prefs.default_grouping())
                .unwrap_or(true)
        })
    }

    /// MED1: flip the composer "group media" choice for 2+ attachments;
    /// persisted when "remember grouping" is on (TGX `RememberAlbumSetting`).
    pub(super) fn toggle_composer_group_media(&mut self, cx: &mut Context<Self>) {
        let next = !self.composer_group_media_effective();
        self.composer_group_media = Some(next);
        if self
            .session()
            .is_some_and(|s| s.media_prefs.remember_media_grouping)
        {
            self.set_media_pref(|prefs| prefs.group_media = next, cx);
        } else {
            cx.notify();
        }
    }

    /// MED1: flip the "remember media grouping" setting itself.
    pub(super) fn toggle_remember_media_grouping(&mut self, cx: &mut Context<Self>) {
        let next = !self
            .session()
            .map(|s| s.media_prefs.remember_media_grouping)
            .unwrap_or(false);
        // Turning it on snapshots the current grouping choice.
        let current = self.composer_group_media_effective();
        self.set_media_pref(
            |prefs| {
                prefs.remember_media_grouping = next;
                if next {
                    prefs.group_media = current;
                }
            },
            cx,
        );
    }

    /// 125 ms tick while a viewer clip is active: re-renders so the
    /// elapsed/total label advances and the in-viewer frame animates
    /// (8 fps frames need a sub-250 ms refresh); auto-stops when the clock
    /// reaches the duration (ffplay `-autoexit` exits on its own).
    pub(super) fn spawn_viewer_tick(&mut self, cx: &mut Context<Self>) {
        if self.viewer_tick {
            return;
        }
        self.viewer_tick = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(125))
                    .await;
                let cont = this
                    .update(cx, |this, cx| {
                        let active = this.viewer_video.is_some();
                        if !active {
                            return false;
                        }
                        let finished = this
                            .viewer_clock
                            .as_ref()
                            .is_some_and(|clock| clock.is_playing() && clock.finished());
                        if finished {
                            this.stop_viewer_video();
                            cx.notify();
                            return false;
                        }
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !cont {
                    break;
                }
            }
            let _ = this.update(cx, |this, _| {
                this.viewer_tick = false;
            });
        })
        .detach();
    }

    /// Parity slice 5: scroll-zoom the viewer visual (scroll up = zoom in,
    /// matching the platform's positive-y convention).
    pub(super) fn viewer_zoom_scroll(&mut self, delta_y: f32, cx: &mut Context<Self>) {
        if delta_y == 0.0 {
            return;
        }
        self.viewer_zoom.step(delta_y > 0.0, self.viewer_frame);
        cx.notify();
    }

    /// Parity slice 5: drag-pan the zoomed visual by a mouse delta in px.
    pub(super) fn viewer_pan_drag(&mut self, dx: f32, dy: f32, cx: &mut Context<Self>) {
        if !self.viewer_zoom.is_zoomed() {
            return;
        }
        self.viewer_zoom.pan_by(dx, dy, self.viewer_frame);
        cx.notify();
    }

    /// Parity slice 5: step the viewer zoom in or out one notch.
    pub(super) fn viewer_zoom_step(&mut self, zoom_in: bool, cx: &mut Context<Self>) {
        self.viewer_zoom.step(zoom_in, self.viewer_frame);
        cx.notify();
    }

    /// Parity slice 5: reset zoom/pan to fit (double-click / `0`).
    pub(super) fn viewer_reset_zoom(&mut self, cx: &mut Context<Self>) {
        self.viewer_zoom.reset();
        self.viewer_drag = None;
        cx.notify();
    }

    /// Phase 4.5: fullscreen media viewer overlay. The backdrop is a
    /// separate sibling painted behind the panel (not an ancestor), so a
    /// click on the panel never bubbles into the backdrop's close handler —
    /// "click outside" works regardless of click-bubbling semantics. Esc
    /// closes through `cancel_search`; the header close button is the third
    /// path. Videos show their thumbnail (no in-viewer playback — the
    /// history row's Play path is unchanged); secret/spoiler media never
    /// reach the viewer (filtered in `collect_media_items`).
    /// MED1: fullscreen photo/video viewer overlay. Takes `&mut self` +
    /// `window` so the viewer seek/volume sliders can sync to the clocks
    /// during render.
    pub(super) fn media_viewer_overlay(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // MED1: keep the viewer seek/volume thumbs on the clocks (the
        // tick has no `&mut Window`, which `SliderState::set_value`
        // needs).
        self.sync_viewer_seek_slider(window, cx);
        self.sync_viewer_volume_slider(window, cx);
        let item = self
            .media_viewer
            .current()
            .cloned()
            .unwrap_or_else(|| MediaViewerItem {
                chat_id: ChatId(0),
                message_id: MessageId(0),
                kind: MediaViewerKind::Photo,
                display_file_ids: Vec::new(),
                download_file_id: FileId(0),
                play_file_id: None,
                duration_secs: None,
                mime_type: None,
                start_timestamp: None,
                caption: String::new(),
                caption_entities: Vec::new(),
                duration_label: None,
                natural_size: None,
            });
        let (position, total) = self.media_viewer.position().unwrap_or((0, 0));
        let files: HashMap<i32, ParsedFile> =
            self.session().map(|s| s.files.clone()).unwrap_or_default();
        let downloading: HashSet<i32> = self
            .session()
            .map(|s| s.downloading.clone())
            .unwrap_or_default();
        let roots = self.media_display_roots();
        let thumb_path = viewer_display_path(&item, &files, &roots);
        // Parity slice 5: when the clip's frames are extracted and decoded,
        // the viewer shows the pre-loaded frame matching the playback clock
        // — real in-viewer video (`ImageSource::Render` resolves
        // synchronously, so the 125 ms tick animates without a per-frame
        // async load). Otherwise it falls back to the thumbnail (or the
        // loading status).
        let frame: Option<Arc<RenderImage>> =
            if item.kind == MediaViewerKind::Video && !self.viewer_video_frames.is_empty() {
                self.viewer_render_frame()
            } else {
                None
            };
        let row_id = item.message_id.0 as u64;
        let downloading_now = item
            .display_file_ids
            .iter()
            .chain(std::iter::once(&item.download_file_id))
            .any(|id| file_is_downloading(*id, &files, &downloading));
        let kind_label = item.kind.label();
        let header_label = if total > 1 {
            format!("{kind_label} {position} of {total}")
        } else {
            kind_label.to_string()
        };
        // MED1: album pin action for the header — only when the item is in
        // an album and the user may pin in this chat (rights-gated, TGX
        // `MessagePinAlbum` semantics: unpins when any member is pinned).
        let album_pin_label: Option<String> =
            self.viewer_album_id(item.message_id).and_then(|album_id| {
                let can_pin = self
                    .session()
                    .and_then(|s| s.chats.get(&item.chat_id.0))
                    .is_some_and(|chat| chat.can_pin_messages());
                if !can_pin {
                    return None;
                }
                let history: Vec<HistoryMessage> = self
                    .session()
                    .and_then(|s| s.histories.get(&item.chat_id.0))
                    .map(|h| h.ordered().into_iter().cloned().collect())
                    .unwrap_or_default();
                let ids = quill::album::album_message_ids(&history, album_id);
                if ids.is_empty() {
                    return None;
                }
                let any_pinned = ids
                    .iter()
                    .any(|id| history.iter().any(|m| m.id == *id && m.is_pinned));
                Some(if any_pinned {
                    "Unpin album".to_string()
                } else {
                    "Pin album".to_string()
                })
            });
        // The visual fills the window between the top bar and the bottom
        // controls, leaving lanes for the prev/next arrows; the media fits
        // inside it (object-fit contain). Scroll zooms (1×–8×, frame-center
        // kept) and drag pans when zoomed.
        let viewport = window.viewport_size();
        let bottom_space = if item.caption.is_empty() { 72.0 } else { 104.0 };
        let fit = (
            (f32::from(viewport.width) - 2.0 * VIEWER_SIDE_LANE).max(240.0),
            (f32::from(viewport.height) - VIEWER_TOP_BAR - bottom_space).max(200.0),
        );
        let frame_h = fit.1;
        // The media's own box inside the frame: sized from its natural
        // dimensions (axes swapped for a quarter turn) rather than trusting
        // object-fit, so it can never spill out of the frame.
        let natural = item.natural_size.map(|(w, h)| {
            if self.viewer_rotation % 2 == 1 {
                (h as f32, w as f32)
            } else {
                (w as f32, h as f32)
            }
        });
        let (media_w, media_h) = natural
            .map(|natural| quill::media_viewer::fit_within(natural, fit))
            .unwrap_or(fit);
        // Zoom and pan work in the media's own box.
        if (media_w, media_h) != self.viewer_frame {
            // A resized window (or another item) refits the media.
            self.viewer_frame = (media_w, media_h);
            self.viewer_zoom.reset();
        }
        let zoom = self.viewer_zoom;
        let (zoom_w, zoom_h) = (media_w * zoom.zoom, media_h * zoom.zoom);
        let (pan_x, pan_y) = zoom.pan;
        let content: AnyElement = {
            // Pre-decoded video frame and thumbnail both render through
            // `img`; the frame is an `ImageSource::Render` (synchronous),
            // the thumbnail a path (async-loaded once, then cached). MED1:
            // a rotated photo renders from the eagerly-decoded
            // `viewer_rotated` cache (90°/180°/270° clockwise).
            let rotated: Option<ImageSource> = (item.kind == MediaViewerKind::Photo)
                .then(|| self.viewer_rotated.as_ref())
                .flatten()
                .filter(|(path, turns, _)| {
                    *turns == self.viewer_rotation && Some(path.as_path()) == thumb_path.as_deref()
                })
                .map(|(_, _, image)| ImageSource::from(image.clone()));
            let source: Option<ImageSource> = frame
                .map(ImageSource::from)
                .or(rotated)
                .or_else(|| thumb_path.clone().map(ImageSource::from));
            if let Some(source) = source {
                img(source)
                    .id(("media-viewer-img", row_id))
                    .w(px(zoom_w))
                    .h(px(zoom_h))
                    .aspect_ratio(px(zoom_w) / px(zoom_h))
                    .object_fit(ObjectFit::Contain)
                    .with_fallback(move || {
                        div()
                            .w(px(zoom_w))
                            .h(px(zoom_h))
                            .bg(bg_deep())
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(gpui_kit::white())
                            .child(format!("{kind_label} — could not render"))
                            .into_any_element()
                    })
                    .into_any_element()
            } else {
                // MED3: show the real download percent when known
                // (`updateFile` → `downloaded_size`).
                let progress_pct: Option<i32> = item
                    .display_file_ids
                    .iter()
                    .chain(std::iter::once(&item.download_file_id))
                    .filter_map(|id| files.get(&id.0))
                    .filter_map(|f| f.download_progress())
                    .map(|p| (p * 100.0).round() as i32)
                    .next();
                let downloading_label = match progress_pct {
                    Some(pct) => format!("downloading… {pct}%"),
                    None => "downloading…".to_string(),
                };
                let status = match (&item.duration_label, downloading_now) {
                    (Some(duration), true) => format!("Video · {duration} — {downloading_label}"),
                    (Some(duration), false) => format!("Video · {duration} — not downloaded"),
                    (None, true) => format!("{kind_label} — {downloading_label}"),
                    (None, false) => format!("{kind_label} — not downloaded"),
                };
                div()
                    .id(("media-viewer-loading", row_id))
                    .w(px(zoom_w))
                    .h(px(zoom_h))
                    .bg(bg_deep())
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(div().text_sm().text_color(gpui_kit::white()).child(status))
                    .into_any_element()
            }
        };
        let visual = {
            let view = cx.entity().downgrade();
            let scroll_view = view.clone();
            let down_view = view.clone();
            let move_view = view.clone();
            let up_view = view;
            div()
                .id(("media-viewer-visual", row_id))
                .relative()
                .w(px(media_w))
                .h(px(media_h))
                .overflow_hidden()
                .child(
                    div()
                        .absolute()
                        .left(px(pan_x))
                        .top(px(pan_y))
                        .w(px(zoom_w))
                        .h(px(zoom_h))
                        .child(content),
                )
                .on_scroll_wheel(move |event, _window, cx| {
                    let dy = match event.delta {
                        ScrollDelta::Pixels(p) => f32::from(p.y),
                        ScrollDelta::Lines(l) => l.y,
                    };
                    if let Some(view) = scroll_view.upgrade() {
                        view.update(cx, |this, cx| this.viewer_zoom_scroll(dy, cx));
                    }
                })
                .on_mouse_down(MouseButton::Left, move |event, _window, cx| {
                    let pos = (f32::from(event.position.x), f32::from(event.position.y));
                    if let Some(view) = down_view.upgrade() {
                        view.update(cx, |this, cx| {
                            this.viewer_drag = Some(pos);
                            cx.notify();
                        });
                    }
                })
                .on_mouse_move(move |event, _window, cx| {
                    let pos = (f32::from(event.position.x), f32::from(event.position.y));
                    if let Some(view) = move_view.upgrade() {
                        view.update(cx, |this, cx| {
                            if let Some((lx, ly)) = this.viewer_drag {
                                this.viewer_pan_drag(pos.0 - lx, pos.1 - ly, cx);
                                this.viewer_drag = Some(pos);
                            }
                        });
                    }
                })
                .on_mouse_up(MouseButton::Left, move |_event, _window, cx| {
                    if let Some(view) = up_view.upgrade() {
                        view.update(cx, |this, cx| {
                            this.viewer_drag = None;
                            cx.notify();
                        });
                    }
                })
                // Double-click resets zoom/pan to fit.
                .on_click(cx.listener(|this, event: &ClickEvent, _, cx| {
                    if event.click_count() >= 2 {
                        this.viewer_reset_zoom(cx);
                    }
                }))
                // The media itself never closes the viewer.
                .occlude()
        };
        // Parity slice 5: video transport under the visual. ffplay runs
        // `-nodisp` for audio only (no GPUI video element in this stack);
        // the decoded video frames render in-viewer above. The overlay
        // shows Play/Pause plus elapsed/total, or a download CTA while the
        // clip is not local.
        let video_controls: Option<AnyElement> = (item.kind == MediaViewerKind::Video).then(|| {
            let clip_path = self.viewer_clip_path(&item);
            if let Some(_clip) = clip_path {
                let playing = self
                    .viewer_clock
                    .as_ref()
                    .is_some_and(|clock| clock.is_playing())
                    && self.viewer_video == Some(item.message_id);
                // MED1: while scrubbing, the label previews the drag
                // position (the history-row seek pattern).
                let elapsed = self
                    .viewer_seek_preview_secs
                    .or_else(|| self.viewer_clock.as_ref().map(|clock| clock.elapsed_secs()))
                    .unwrap_or(0.0);
                let total = item.duration_secs.unwrap_or(0) as f64;
                let label = format!(
                    "{} / {}",
                    format_voice_duration(elapsed as i32),
                    format_voice_duration(total as i32)
                );
                // While ffmpeg extracts frames the thumbnail stays up;
                // the Play button appears once frames are ready.
                let extracting = self.viewer_extracting;
                let speed_label = Self::speed_label(self.playback_speed);
                let muted = self.playback_volume < 0.01;
                let volume_pct = (self.playback_volume * 100.0).round() as i32;
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(if extracting {
                        div()
                            .text_sm()
                            .text_color(gpui_kit::white())
                            .child("Loading video…")
                            .into_any_element()
                    } else {
                        Button::new(("media-viewer-play", row_id))
                            .label(if playing { "❚❚ Pause" } else { "▶ Play" })
                            .custom(ButtonCustomVariant::new(cx).foreground(text_on_fill().into()))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.toggle_viewer_video(cx);
                            }))
                            .into_any_element()
                    })
                    .child(div().text_sm().text_color(gpui_kit::white()).child(label))
                    .when(
                        cfg!(target_os = "macos") && !self.viewer_video_frames.is_empty(),
                        |this| {
                            this.child(
                                Button::new("media-viewer-pip")
                                    .label("Picture-in-Picture")
                                    .accessibility_label("Picture-in-Picture")
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.open_video_pip(cx)),
                                    ),
                            )
                        },
                    )
                    // MED1: seek slider (created in `begin_viewer_video`).
                    .when_some(self.viewer_seek_slider.clone(), |this, slider| {
                        this.child(
                            div().w(px(180.)).child(
                                Slider::new(&slider).bg(accent()).text_color(text_on_fill()),
                            ),
                        )
                    })
                    // MED1: playback speed (TGX 0.5x–2x).
                    .child(
                        Button::new(("media-viewer-speed", row_id))
                            .label(speed_label)
                            .custom(ButtonCustomVariant::new(cx).foreground(text_on_fill().into()))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cycle_playback_speed(cx);
                            })),
                    )
                    // MED1: volume slider + mute toggle.
                    .when_some(self.viewer_volume_slider.clone(), |this, slider| {
                        this.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(div().w(px(80.)).child(
                                    Slider::new(&slider).bg(accent()).text_color(text_on_fill()),
                                ))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(text_on_fill())
                                        .child(format!("{volume_pct}%")),
                                ),
                        )
                    })
                    .child(
                        Button::new(("media-viewer-mute", row_id))
                            .label(if muted { "Unmute" } else { "Mute" })
                            .custom(ButtonCustomVariant::new(cx).foreground(text_on_fill().into()))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.toggle_playback_mute(cx);
                            })),
                    )
                    .into_any_element()
            } else {
                let clip_downloading = item
                    .play_file_id
                    .is_some_and(|id| file_is_downloading(id, &files, &downloading));
                let play_id = item.play_file_id;
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().text_sm().text_color(gpui_kit::white()).child(
                        if clip_downloading {
                            "Video — downloading clip…"
                        } else {
                            "Video — clip not downloaded"
                        },
                    ))
                    .when_some(play_id.filter(|_| !clip_downloading), |this, id| {
                        this.child(
                            Button::new(("media-viewer-download", row_id))
                                .label("Download")
                                .ghost()
                                .text_color(gpui_kit::white())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.viewer_pending_play = Some((item.message_id, id));
                                    this.request_media_download(id, None, cx);
                                })),
                        )
                    })
                    .into_any_element()
            }
        });
        // Parity slice 5: zoom controls share a row with the video
        // transport — − / % / + / Reset, then Play/Pause + elapsed/total.
        let zoom_pct = format!("{}%", (zoom.zoom * 100.0).round() as i32);
        let mut transport = div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                Button::new(("media-viewer-zoom-out", row_id))
                    .label("−")
                    .ghost()
                    .text_color(gpui_kit::white())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.viewer_zoom_step(false, cx);
                    })),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(gpui_kit::white())
                    .child(zoom_pct),
            )
            .child(
                Button::new(("media-viewer-zoom-in", row_id))
                    .label("+")
                    .ghost()
                    .text_color(gpui_kit::white())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.viewer_zoom_step(true, cx);
                    })),
            )
            .child(
                Button::new(("media-viewer-zoom-reset", row_id))
                    .label("Reset")
                    .ghost()
                    .text_color(gpui_kit::white())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.viewer_reset_zoom(cx);
                    })),
            );
        if let Some(controls) = video_controls {
            transport = transport.child(controls);
        }
        let transport = transport.into_any_element();
        let caption: Option<AnyElement> = (!item.caption.is_empty()).then(|| {
            rich_text_line(
                &item.caption,
                &item.caption_entities,
                (item.chat_id.0, row_id),
                true,
                &self.spoiler_revealed,
                // Settings → Appearance: captions follow the message font size.
                self.msg_font(),
                // Captions don't resolve custom emoji in this slice (text fallback).
                &HashMap::new(),
                cx,
            )
        });
        let icon_action =
            |id: (&'static str, u64), icon: gpui_kit::assets::IconName, label: &'static str| {
                Button::new(id)
                    .icon(icon)
                    .ghost()
                    .text_color(gpui_kit::white())
                    .tooltip(label)
                    .accessibility_label(label)
            };
        // Every control surface occludes: GPUI delivers a click to all
        // hitboxes under the cursor down to the first occluding one, so
        // without it the backdrop below also gets the click and closes.
        let top_bar = div()
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(VIEWER_TOP_BAR))
            .px_4()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .font_semibold()
                    .text_color(gpui_kit::white())
                    .child(header_label),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .when(item.kind == MediaViewerKind::Photo, |this| {
                        this.child(
                            icon_action(
                                ("media-viewer-rotate", row_id),
                                gpui_kit::assets::IconName::RotateCw,
                                "Rotate",
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.rotate_viewer_photo(cx);
                            })),
                        )
                    })
                    .child(
                        icon_action(
                            ("media-viewer-share", row_id),
                            gpui_kit::assets::IconName::Forward,
                            "Share",
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.share_viewer_media(window, cx);
                        })),
                    )
                    .child(
                        icon_action(
                            ("media-viewer-save", row_id),
                            gpui_kit::assets::IconName::Download,
                            "Save",
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.save_viewer_media(cx);
                        })),
                    )
                    .child(
                        icon_action(
                            ("media-viewer-show-in-chat", row_id),
                            gpui_kit::assets::IconName::MessageSquare,
                            "Show in chat",
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.show_viewer_in_chat(cx);
                        })),
                    )
                    .when_some(album_pin_label, |this, label| {
                        this.child(
                            Button::new(("media-viewer-pin-album", row_id))
                                .icon(gpui_kit::assets::IconName::Pin)
                                .ghost()
                                .text_color(gpui_kit::white())
                                .tooltip(label.clone())
                                .accessibility_label(label)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.toggle_viewer_album_pin(cx);
                                })),
                        )
                    })
                    .child(
                        icon_action(
                            ("media-viewer-close", row_id),
                            gpui_kit::assets::IconName::X,
                            "Close media viewer",
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.close_media_viewer(cx);
                        })),
                    ),
            );
        let nav_arrow = |id: &'static str,
                         icon: gpui_kit::assets::IconName,
                         label: &'static str,
                         step: i32,
                         cx: &mut Context<Self>| {
            Button::new(id)
                .icon(icon)
                .large()
                .rounded_full()
                .custom(
                    ButtonCustomVariant::new(cx)
                        .color(gpui_kit::black().opacity(0.4))
                        .foreground(gpui_kit::white())
                        .hover(gpui_kit::black().opacity(0.6)),
                )
                .tooltip(label)
                .accessibility_label(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.step_media_viewer(step, cx);
                }))
        };
        let prev = (position > 1).then(|| {
            nav_arrow(
                "media-viewer-prev",
                gpui_kit::assets::IconName::ChevronLeft,
                "Previous",
                -1,
                cx,
            )
        });
        let next = (position < total).then(|| {
            nav_arrow(
                "media-viewer-next",
                gpui_kit::assets::IconName::ChevronRight,
                "Next",
                1,
                cx,
            )
        });
        div()
            .id("media-viewer-overlay")
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            // Clicking anywhere outside the media and controls closes.
            .child(
                div()
                    .id("media-viewer-backdrop")
                    .occlude()
                    .absolute()
                    .inset_0()
                    .bg(gpui_kit::black().opacity(0.92))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.close_media_viewer(cx);
                    })),
            )
            .child(top_bar)
            .child(
                div()
                    .absolute()
                    .top(px(VIEWER_TOP_BAR))
                    .left_0()
                    .right_0()
                    .h(px(frame_h))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(visual),
            )
            .when_some(prev, |this, prev| {
                this.child(
                    div()
                        .occlude()
                        .absolute()
                        .left(px(16.))
                        .top(px(VIEWER_TOP_BAR + frame_h / 2.0 - 24.0))
                        .child(prev),
                )
            })
            .when_some(next, |this, next| {
                this.child(
                    div()
                        .occlude()
                        .absolute()
                        .right(px(16.))
                        .top(px(VIEWER_TOP_BAR + frame_h / 2.0 - 24.0))
                        .child(next),
                )
            })
            .child(
                div()
                    .occlude()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .px_6()
                    .pb_4()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .when_some(caption, |this, caption| {
                        this.child(
                            div()
                                .max_w(px(720.))
                                .text_color(gpui_kit::white())
                                .child(caption),
                        )
                    })
                    // MED1: honest playback error (unsupported format /
                    // player failure) instead of a silent stall.
                    .when_some(self.playback_error.clone(), |this, err| {
                        this.child(div().text_sm().text_color(danger_bright()).child(err))
                    })
                    .child(transport),
            )
    }
}
