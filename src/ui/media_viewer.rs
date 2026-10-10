//! media_viewer.

use super::app::QuillApp;

/// Height of the viewer's top bar (title + actions).
const VIEWER_TOP_BAR: f32 = 56.0;
/// Width kept free on each side of the media for the prev/next arrows.
const VIEWER_SIDE_LANE: f32 = 80.0;
use super::message_media::{file_is_downloading, viewer_display_path};
use super::message_text::{custom_emoji_paths, rich_text_line};
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState, SliderValue};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::{ChatId, FileId, MessageId};
use quill::local_path::sandboxed_display_path;
use quill::media_viewer::{
    MediaViewer, MediaViewerItem, MediaViewerKind, VIEWER_FADE_MS, VIEWER_SHOW_MS,
    VIEWER_WAIT_HIDE_MS, VIEWER_WHEEL_NOTCH_PX, ViewerKeyAction, ViewerKeyMods, ViewerOrientation,
    ViewerSource, ViewerVideoStart, collect_media_items, controls_hide_wait_ms,
    controls_should_hide, decide_viewer_video_start, orient_rgba, save_media_to_downloads,
    seek_target_secs, viewer_delete_gate, viewer_key_action, wheel_zoom_factor,
};
use quill::playback::PlaybackClock;
use quill::settings::MediaPrefs;
use quill::state::HistoryMessage;
use quill::state::SharedMediaTab;
use quill::telegram::envelope::ParsedFile;
use quill::voice::format_voice_duration;
use smallvec::SmallVec;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Child;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
/// Viewer state that is not part of the item list: Shared Media paging,
/// video full screen, and the inactive-window pause of looping clips.
#[derive(Default)]
pub(super) struct ViewerExtra {
    /// The Shared Media tab the viewer pages over.
    pub shared_tab: SharedMediaTab,
    /// Items of that tab the viewer has already merged in.
    pub shared_seen: usize,
    /// Video full screen (tdesktop `_fullScreenVideo`): arrows seek, digits
    /// jump, Escape leaves it.
    pub video_fullscreen: bool,
    /// The window was already full screen when the mode was entered.
    pub window_was_fullscreen: bool,
    /// Give the window back to windowed mode on the next frame.
    pub restore_fullscreen: bool,
    /// A looping clip paused because the window lost focus.
    pub inactive_paused: bool,
    /// B10: whose profile photos the viewer shows (`ViewerSource::Profile`).
    pub profile_user: Option<i64>,
    /// The "saved to your Downloads folder" toast and its generation (a
    /// newer save restarts the hide timer).
    pub saved_toast: Option<quill::viewer_extras::SavedToast>,
    pub saved_toast_gen: u64,
    /// The speed dial's slider and the speed shown while it is dragged.
    pub speed_slider: Option<Entity<SliderState>>,
    pub speed_preview: Option<f64>,
    /// Screenshot demo: open the speed dial on the first frame.
    pub demo_speed_dial_open: bool,
}

/// Screenshot-capture runs render a single frame: skip fades there so the
/// shot shows the settled viewer, not frame zero of an animation.
fn still_frame() -> bool {
    std::env::var_os("QUILL_DEMO_CAPTURE").is_some()
}

/// Fade a group of viewer controls in or out over `VIEWER_FADE_MS`
/// (tdesktop `mediaviewFadeDuration`). The animation id carries the flip
/// generation, so each show/hide restarts it.
fn fade_controls<E: Styled + IntoElement + 'static>(
    element: E,
    id: &'static str,
    generation: u64,
    hidden: bool,
) -> AnimationElement<E> {
    element.with_animation(
        (id, generation as usize),
        Animation::new(Duration::from_millis(VIEWER_FADE_MS)),
        move |element, t| {
            let t = if still_frame() { 1.0 } else { t };
            element.opacity(if hidden { 1.0 - t } else { t })
        },
    )
}

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
        self.viewer_open_gen += 1;
        self.viewer_note_activity(false, cx);
        self.reset_viewer_item_state(cx);
        cx.notify();
    }

    /// Open the viewer on a photo, video or GIF of the Shared Media panel,
    /// paging over that tab's list (tdesktop `SharedMediaWithLastSlice`).
    /// The list loads older pages as the user pages toward its start.
    /// `false` when the message is not viewer-openable (a secret photo):
    /// the caller then jumps to it in the chat.
    pub(super) fn open_shared_media_viewer(
        &mut self,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(session) = self.session() else {
            return false;
        };
        let state = &session.shared_media;
        let Some(chat_id) = state.chat_id else {
            return false;
        };
        let tab = state.active_tab;
        let tab_state = &state.tabs[tab.index()];
        // The panel lists newest first; the viewer pages oldest first.
        let messages: Vec<HistoryMessage> = tab_state
            .items
            .iter()
            .rev()
            .filter_map(|item| item.message.as_deref().cloned())
            .collect();
        let loaded = tab_state.items.len();
        let total = tab_state.total_count.max(0) as usize;
        let items = collect_media_items(&messages);
        let Some(index) = items.iter().position(|item| item.message_id == message_id) else {
            return false;
        };
        debug_assert!(items.iter().all(|item| item.chat_id == chat_id));
        self.media_viewer = MediaViewer::open_shared(items, index, total);
        self.viewer_extra.shared_tab = tab;
        self.viewer_extra.shared_seen = loaded;
        self.viewer_open_gen += 1;
        self.viewer_note_activity(false, cx);
        self.reset_viewer_item_state(cx);
        self.sync_shared_media_viewer(cx);
        cx.notify();
        true
    }

    /// Keep a Shared Media viewer in step with its tab: merge older pages
    /// that landed, and ask for the next one when the viewer nears the
    /// start of the loaded list.
    pub(super) fn sync_shared_media_viewer(&mut self, cx: &mut Context<Self>) {
        if self.media_viewer.source() != ViewerSource::SharedMedia {
            return;
        }
        let tab = self.viewer_extra.shared_tab;
        let seen = self.viewer_extra.shared_seen;
        let grown = self.session().and_then(|session| {
            let state = &session.shared_media.tabs[tab.index()];
            (state.items.len() != seen).then(|| {
                let messages: Vec<HistoryMessage> = state
                    .items
                    .iter()
                    .rev()
                    .filter_map(|item| item.message.as_deref().cloned())
                    .collect();
                (
                    collect_media_items(&messages),
                    state.total_count.max(0) as usize,
                    state.items.len(),
                )
            })
        });
        if let Some((items, total, loaded)) = grown {
            self.viewer_extra.shared_seen = loaded;
            if self.media_viewer.merge_older(items, total) > 0 {
                cx.notify();
            }
        }
        if self.media_viewer.wants_older()
            && let Some(live) = self.live.as_mut()
        {
            let _ = live.driver.fetch_more_shared_media(tab);
        }
    }

    /// Per-item viewer state: zoom, orientation, playback error, video
    /// and the download / delete-permission lookups for the new current
    /// item. Shared by open, step, and "the current item was deleted".
    pub(super) fn reset_viewer_item_state(&mut self, cx: &mut Context<Self>) {
        self.viewer_zoom.reset();
        self.viewer_drag = None;
        // MED1: orientation and playback error are per-item state.
        self.viewer_orientation = Default::default();
        self.viewer_rotated = None;
        self.playback_error = None;
        self.viewer_extra.inactive_paused = false;
        self.stop_viewer_video();
        self.ensure_viewer_download(cx);
        self.maybe_autoplay_viewer_video(cx);
        // What TDLib allows for this message (Delete in the toolbar).
        // Profile photos are not messages.
        if self.media_viewer.source() != ViewerSource::Profile
            && let (Some(item), Some(live)) = (self.media_viewer.current(), self.live.as_mut())
        {
            let _ = live
                .driver
                .fetch_message_menu_actions(item.chat_id, item.message_id);
        }
    }

    pub(super) fn close_media_viewer(&mut self, cx: &mut Context<Self>) {
        self.viewer_leave_video_fullscreen();
        self.stop_viewer_video();
        self.viewer_extra.saved_toast = None;
        self.media_viewer.close();
        cx.notify();
    }

    pub(super) fn step_media_viewer(&mut self, delta: i32, cx: &mut Context<Self>) {
        if delta < 0 {
            self.media_viewer.prev();
        } else {
            self.media_viewer.next();
        }
        self.viewer_note_activity(false, cx);
        self.reset_viewer_item_state(cx);
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
    /// but full-clip); the audio engine plays the audio track. The
    /// overlay keeps Play/Pause and elapsed/total; the thumbnail shows
    /// until frames are ready. Closing or stepping the viewer stops
    /// playback and drops the frame cache.
    ///
    /// Sandbox-checked local path of the current item's full video clip.
    pub(super) fn viewer_clip_path(&self, item: &MediaViewerItem) -> Option<PathBuf> {
        self.playable_clip_path(item.chat_id, item.message_id, item.play_file_id?)
    }

    /// A message's clip as a local path the player may open: inside the
    /// account's media folders, or for a video you sent, the original file
    /// you picked (TDLib's local copy of an upload, which it won't download
    /// again). Like Telegram Desktop, that original plays while it exists.
    pub(super) fn playable_clip_path(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
        play_id: FileId,
    ) -> Option<PathBuf> {
        let roots = self.media_display_roots();
        let session = self.session()?;
        let path = session.files.get(&play_id.0)?.usable_path()?;
        if let Some(path) = sandboxed_display_path(path, &roots) {
            return Some(path.to_path_buf());
        }
        let outgoing = session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .is_some_and(|message| message.is_outgoing);
        outgoing
            .then(|| std::fs::canonicalize(path).ok())
            .flatten()
            .filter(|path| path.is_file())
    }

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
            ViewerVideoStart::PlayNow | ViewerVideoStart::ExtractFrames
                if Self::viewer_uses_native(&item, path.as_deref())
                    && !self.viewer_demo_sync_frames =>
            {
                self.viewer_pending_play = None;
                let path = path.expect("clip checked local by decide_viewer_video_start");
                self.play_native_viewer_video(&item, &path, cx);
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
        super::native_video::supported() && !(item.kind.loops() && is_gif_file)
    }

    /// The current item is a GIF: loops, no sound, no transport.
    pub(super) fn viewer_loops(&self) -> bool {
        self.media_viewer
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
    pub(super) fn decode_viewer_frames(paths: &[PathBuf]) -> Result<Vec<Arc<RenderImage>>, String> {
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
        // Tiles of any other call are dead weight.
        self.prune_group_video_images(Some(call_id));
        self.group_video_images
            .insert(key, (frame.seq, image.clone()));
        Some(image)
    }

    /// Drop cached group-call tiles that don't belong to `live_call`
    /// (`None`: no call, drop all), handing them to the atlas sweeper.
    pub(super) fn prune_group_video_images(&mut self, live_call: Option<i32>) {
        let images = take_dead_call_tiles(&mut self.group_video_images, live_call)
            .into_iter()
            .map(|(_, image)| image);
        super::image_budget::retire_all(images);
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
        super::image_budget::retire_all(self.viewer_video_frames.drain(..));
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
                        this.status_note = message.clone();
                        this.playback_error = Some(message);
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
        let matches = self
            .media_viewer
            .current()
            .is_some_and(|item| item.message_id == message_id && item.kind.is_playable());
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
    /// State only: the caller starts the audio (the screenshot demo skips the
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
        let looping = item.kind.loops();
        let mut duration = item.duration_secs.unwrap_or(0).max(0) as f64;
        // A loop wraps at the end of its frames, not at TDLib's rounded
        // duration.
        if looping && !self.viewer_video_frames.is_empty() && self.viewer_video_fps > 0.0 {
            duration = self.viewer_video_frames.len() as f64 / self.viewer_video_fps;
        }
        let mut clock = PlaybackClock::new(duration);
        if !looping {
            clock.set_rate(self.playback_speed);
        }
        // A media-timestamp link opened this clip at a given second.
        let start = self
            .pending_viewer_seek
            .take_if(|(id, _)| *id == item.message_id)
            .map_or(0.0, |(_, secs)| secs.clamp(0.0, duration));
        clock.seek(start);
        clock.resume();
        self.viewer_clock = Some(clock);
        self.viewer_video = Some(item.message_id);
        self.viewer_video_path = Some(path.to_path_buf());
        self.playback_error = None;
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
        self.viewer_seek_slider = Some(slider);
        // MED1: volume slider (0–100%); applies on release so a drag
        // doesn't restart the sound per tick.
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

    /// Begin viewer playback *and* start the audio — unless this
    /// is a screenshot demo, which skips the subprocess (same posture as
    /// `request_media_download`'s demo branch).
    pub(super) fn play_viewer_video(
        &mut self,
        item: &MediaViewerItem,
        path: &std::path::Path,
        cx: &mut Context<Self>,
    ) {
        self.begin_viewer_video(item, path, cx);
        // GIFs are silent: no audio player.
        if self.demo_session.is_none() && !item.kind.loops() {
            let start = self.viewer_clock.as_ref().map_or(0.0, |c| c.elapsed_secs());
            self.start_viewer_audio(path, start);
        }
    }

    /// Play the viewer clip with the native player: real-time hardware
    /// decode with its own audio, drawn frame by frame (no extraction).
    pub(super) fn play_native_viewer_video(
        &mut self,
        item: &MediaViewerItem,
        path: &std::path::Path,
        cx: &mut Context<Self>,
    ) {
        self.begin_viewer_video(item, path, cx);
        match super::native_video::NativeVideo::open(path, super::native_video::Purpose::Viewer) {
            Ok(mut video) => {
                if item.kind.loops() {
                    video.set_volume(0.0);
                    video.play();
                } else {
                    video.set_volume(self.playback_volume);
                    video.play();
                    video.set_rate(self.playback_speed as f32);
                }
                let start = self.viewer_clock.as_ref().map_or(0.0, |c| c.elapsed_secs());
                if start > 0.0 {
                    video.seek(start);
                }
                self.viewer_native = Some(video);
            }
            Err(err) => {
                self.stop_viewer_video();
                self.playback_error = Some(err);
            }
        }
        cx.notify();
    }

    /// Play the viewer clip's soundtrack in-process (MP4/AAC and friends
    /// decode in the same engine as voice notes). The video frames render
    /// in-viewer from `viewer_video_frames`; this only supplies the sound.
    /// A clip with no decodable audio just plays silently, with the error
    /// shown on the transport.
    pub(super) fn start_viewer_audio(&mut self, path: &std::path::Path, offset_secs: f64) -> bool {
        match self
            .viewer_audio
            .start(path, offset_secs, self.playback_volume, self.playback_speed)
        {
            Ok(()) => {
                self.playback_error = None;
                true
            }
            Err(err) => {
                self.playback_error = Some(err.to_string());
                false
            }
        }
    }

    pub(super) fn kill_viewer_player(&mut self) {
        self.viewer_audio.stop();
    }

    /// Pause: freeze the clock, stop the sound, keep the item active so the
    /// controls stay and Play resumes from the frozen offset.
    pub(super) fn pause_viewer_video(&mut self, cx: &mut Context<Self>) {
        if let Some(clock) = self.viewer_clock.as_mut() {
            clock.pause();
        }
        if let Some(video) = self.viewer_native.as_mut() {
            video.pause();
        }
        self.kill_viewer_player();
        cx.notify();
    }

    /// Resume from the frozen clock position.
    pub(super) fn resume_viewer_video(&mut self, cx: &mut Context<Self>) {
        let loops = self.viewer_loops();
        if let Some(video) = self.viewer_native.as_mut() {
            video.play();
            if !loops {
                video.set_rate(self.playback_speed as f32);
            }
            if let Some(clock) = self.viewer_clock.as_mut() {
                clock.seek(video.position_secs());
                clock.resume();
            }
            cx.notify();
            return;
        }
        let offset = self.viewer_clock.as_ref().map(|c| c.elapsed_secs());
        let path = self.viewer_video_path.clone();
        match (offset, path) {
            (Some(offset), Some(path)) => {
                if !loops {
                    self.start_viewer_audio(&path, offset);
                }
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

    /// Media keys and the OS Now Playing widget for the viewer's video.
    /// The audio player owns the session while a track is active.
    fn drive_viewer_media_session(&mut self, cx: &mut Context<Self>) {
        use quill::viewer_extras::{ViewerMediaAction, viewer_media_action, viewer_now_playing};
        if self.active_playback_id().is_some() {
            return;
        }
        let Some(item) = self.media_viewer.current().cloned() else {
            return;
        };
        if item.kind != MediaViewerKind::Video {
            return;
        }
        let (playing, duration) = self
            .viewer_clock
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
        let Some(clock) = self.viewer_clock.as_ref() else {
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
            self.playback_speed,
        );
        quill::media_session::publish(Some(&info));
    }

    /// Full stop: stop the sound and any running frame extraction, and clear
    /// all viewer-video state. Called on viewer close/step and when any
    /// other player starts.
    pub(super) fn stop_viewer_video(&mut self) {
        self.pip_window = None;
        self.viewer_native = None;
        self.kill_viewer_player();
        self.kill_viewer_extraction();
        self.viewer_video = None;
        self.viewer_video_path = None;
        self.viewer_clock = None;
        self.viewer_pending_play = None;
        super::image_budget::retire_all(self.viewer_video_frames.drain(..));
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

    /// MED1: rotate the viewer photo 90° clockwise (photos only).
    pub(super) fn rotate_viewer_photo(&mut self, cx: &mut Context<Self>) {
        self.reorient_viewer_photo(|o| o.rotate_cw(), cx);
    }

    /// `H`: mirror the viewer photo left-to-right (tdesktop `_flip`).
    pub(super) fn flip_viewer_horizontal(&mut self, cx: &mut Context<Self>) {
        self.reorient_viewer_photo(|o| o.flip_horizontal(), cx);
    }

    /// `V`: mirror the viewer photo top-to-bottom.
    pub(super) fn flip_viewer_vertical(&mut self, cx: &mut Context<Self>) {
        self.reorient_viewer_photo(|o| o.flip_vertical(), cx);
    }

    /// Apply an orientation change and decode the re-oriented pixels
    /// eagerly into `viewer_rotated`, so the overlay render stays
    /// allocation-free; reset on open/step. A photo that is not local or
    /// cannot be decoded keeps its previous orientation.
    fn reorient_viewer_photo(
        &mut self,
        change: impl FnOnce(&mut ViewerOrientation),
        cx: &mut Context<Self>,
    ) {
        let item = self.media_viewer.current().cloned();
        let Some(item) = item else { return };
        if item.kind != MediaViewerKind::Photo {
            return;
        }
        let previous = (self.viewer_orientation, self.viewer_rotated.clone());
        change(&mut self.viewer_orientation);
        self.viewer_rotated = None;
        if self.viewer_orientation.is_identity() {
            cx.notify();
            return;
        }
        let files: HashMap<i32, ParsedFile> =
            self.session().map(|s| s.files.clone()).unwrap_or_default();
        let roots = self.media_display_roots();
        let path = viewer_display_path(&item, &files, &roots);
        let orientation = self.viewer_orientation;
        match path {
            Some(path) => match Self::rotated_render_image(&path, orientation) {
                Some(image) => {
                    self.viewer_rotated = Some((path, orientation.code(), image));
                }
                None => {
                    self.status_note = "couldn't rotate this photo".into();
                    (self.viewer_orientation, self.viewer_rotated) = previous;
                }
            },
            None => {
                self.status_note = "download the photo first to rotate it".into();
                (self.viewer_orientation, self.viewer_rotated) = previous;
            }
        }
        cx.notify();
    }

    /// MED1: decode `path` and orient it (flip, then rotate) into a
    /// `RenderImage` (the `decode_viewer_frames` construction pattern).
    pub(super) fn rotated_render_image(
        path: &PathBuf,
        orientation: ViewerOrientation,
    ) -> Option<Arc<RenderImage>> {
        let rgba = image::open(path).ok()?.to_rgba8();
        let (pixels, width, height) =
            orient_rgba(rgba.as_raw(), rgba.width(), rgba.height(), orientation);
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
        if self.refuse_protected_copy(chat_id, cx) {
            return;
        }
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
        if self.refuse_protected_copy(item.chat_id, cx) {
            return;
        }
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
            MediaViewerKind::Video | MediaViewerKind::Animation => item
                .play_file_id
                .and_then(|id| files.get(&id.0))
                .and_then(|file| file.usable_path())
                .map(PathBuf::from),
        };
        // B13: "Ask where to save each file" opens a save dialog instead of
        // dropping the copy into the download folder.
        if let Some(path) = path.as_ref()
            && quill::file_prefs::current().ask_download_path
        {
            self.save_file_asking(path.clone(), cx);
            return;
        }
        match path {
            Some(path) => match save_media_to_downloads(&path) {
                Ok(dest) => {
                    self.show_saved_toast(dest, item.kind != MediaViewerKind::Photo, cx);
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

    /// The viewer's own "saved to your Downloads folder" toast, with a
    /// link that reveals the file (tdesktop `showSaveMsgToast`). It hides
    /// itself after a few seconds.
    pub(super) fn show_saved_toast(&mut self, dest: PathBuf, video: bool, cx: &mut Context<Self>) {
        let text = quill::viewer_extras::saved_toast_text(
            &dest,
            quill::media_viewer::downloads_dir().as_deref(),
            video,
        );
        self.viewer_extra.saved_toast = Some(quill::viewer_extras::SavedToast { dest, text });
        self.viewer_extra.saved_toast_gen += 1;
        let generation = self.viewer_extra.saved_toast_gen;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(quill::viewer_extras::SAVED_TOAST_MS))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.viewer_extra.saved_toast_gen == generation {
                    this.viewer_extra.saved_toast = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// The toast's folder link: show the saved file in the file manager.
    fn reveal_saved_toast_file(&mut self, cx: &mut Context<Self>) {
        let Some(toast) = self.viewer_extra.saved_toast.take() else {
            return;
        };
        if !quill::platform::reveal_in_file_manager(&toast.dest) {
            self.status_note = "couldn't open the folder".into();
        }
        cx.notify();
    }

    /// The delete confirmation for the current viewer item, when the
    /// message may be deleted (same gate as the message menu).
    pub(super) fn viewer_delete_confirm(&self) -> Option<quill::composer::DeleteConfirm> {
        let item = self.media_viewer.current()?;
        let session = self.session()?;
        let chat_id = item.chat_id;
        let message = session
            .histories
            .get(&chat_id.0)?
            .messages
            .get(&item.message_id.0)?;
        let mut confirm = quill::composer::DeleteConfirm::for_message(
            chat_id,
            item.message_id,
            message.is_outgoing,
            message.pending,
        )?;
        let actions = session
            .message_menu_actions
            .filter(|(c, m, _)| *c == chat_id && *m == item.message_id)
            .map(|(_, _, actions)| actions);
        let is_channel_post = matches!(
            session.chats.get(&chat_id.0).map(|chat| &chat.kind),
            Some(quill::telegram::envelope::ChatKind::Supergroup {
                is_channel: true,
                ..
            })
        );
        let can_revoke = viewer_delete_gate(
            actions,
            message.is_outgoing,
            is_channel_post,
            session.is_saved_messages(chat_id),
        )?;
        confirm.can_revoke = can_revoke;
        confirm.revoke = can_revoke;
        Some(confirm)
    }

    /// Trash: open the message menu's delete confirmation ("Also delete
    /// for {name}" checkbox) for the current item. Once the message is
    /// gone from the history `prune_deleted_viewer_items` moves the
    /// viewer to the next item, or closes it.
    pub(super) fn delete_viewer_media(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(confirm) = self.viewer_delete_confirm() {
            self.open_delete_dialog(confirm, window, cx);
        }
    }

    /// Drop viewer items whose message no longer exists, moving to the
    /// next item (or closing when none remain).
    fn prune_deleted_viewer_items(&mut self, cx: &mut Context<Self>) {
        // Shared Media items reach past the loaded history; absence there
        // does not mean deleted.
        if matches!(
            self.media_viewer.source(),
            ViewerSource::SharedMedia | ViewerSource::Profile
        ) {
            return;
        }
        let mut viewer = std::mem::take(&mut self.media_viewer);
        let changed = match self.session() {
            Some(session) => viewer.retain(|item| {
                session
                    .histories
                    .get(&item.chat_id.0)
                    .is_none_or(|history| history.messages.contains_key(&item.message_id.0))
            }),
            None => false,
        };
        self.media_viewer = viewer;
        if !changed {
            return;
        }
        if self.media_viewer.is_open() {
            self.reset_viewer_item_state(cx);
        } else {
            self.stop_viewer_video();
        }
        cx.notify();
    }

    /// Cmd+C: copy the current photo to the clipboard as an image (with
    /// the viewer's rotation and flips applied).
    pub(super) fn copy_viewer_photo(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.media_viewer.current().cloned() else {
            return;
        };
        if item.kind != MediaViewerKind::Photo {
            self.status_note = "only photos can be copied".into();
            cx.notify();
            return;
        }
        if self.refuse_protected_copy(item.chat_id, cx) {
            return;
        }
        let files: HashMap<i32, ParsedFile> =
            self.session().map(|s| s.files.clone()).unwrap_or_default();
        let roots = self.media_display_roots();
        let orientation = self.viewer_orientation;
        let png = viewer_display_path(&item, &files, &roots)
            .ok_or("download the photo first to copy it")
            .and_then(|path| {
                let rgba = image::open(path)
                    .map_err(|_| "couldn't copy this photo")?
                    .to_rgba8();
                let (pixels, width, height) =
                    orient_rgba(rgba.as_raw(), rgba.width(), rgba.height(), orientation);
                let oriented = image::RgbaImage::from_raw(width, height, pixels)
                    .ok_or("couldn't copy this photo")?;
                let mut bytes = std::io::Cursor::new(Vec::new());
                image::DynamicImage::ImageRgba8(oriented)
                    .write_to(&mut bytes, image::ImageFormat::Png)
                    .map_err(|_| "couldn't copy this photo")?;
                Ok(bytes.into_inner())
            });
        match png {
            Ok(bytes) => {
                cx.write_to_clipboard(ClipboardItem::new_image(&gpui_kit::Image::from_bytes(
                    gpui_kit::ImageFormat::Png,
                    bytes,
                )));
                self.status_note = "photo copied".into();
            }
            Err(note) => self.status_note = note.into(),
        }
        cx.notify();
    }

    /// "Copy Frame": the video frame on screen to the clipboard as an
    /// image (tdesktop's viewer context menu offers it for videos). Needs
    /// the in-viewer frames; a native-surface or still-loading clip says
    /// so instead of copying a thumbnail.
    pub(super) fn copy_viewer_frame(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.media_viewer.current().cloned() else {
            return;
        };
        if self.refuse_protected_copy(item.chat_id, cx) {
            return;
        }
        let png = self
            .viewer_render_frame()
            .ok_or("a frame can only be copied once the video has loaded")
            .and_then(|frame| {
                let size = frame.size(0);
                let (width, height) = (size.width.0 as u32, size.height.0 as u32);
                let mut pixels = frame
                    .as_bytes(0)
                    .ok_or("couldn't copy this frame")?
                    .to_vec();
                // RenderImage pixels are BGRA.
                for pixel in pixels.as_chunks_mut::<4>().0 {
                    pixel.swap(0, 2);
                }
                let rgba = image::RgbaImage::from_raw(width, height, pixels)
                    .ok_or("couldn't copy this frame")?;
                let mut bytes = std::io::Cursor::new(Vec::new());
                image::DynamicImage::ImageRgba8(rgba)
                    .write_to(&mut bytes, image::ImageFormat::Png)
                    .map_err(|_| "couldn't copy this frame")?;
                Ok(bytes.into_inner())
            });
        match png {
            Ok(bytes) => {
                cx.write_to_clipboard(ClipboardItem::new_image(&gpui_kit::Image::from_bytes(
                    gpui_kit::ImageFormat::Png,
                    bytes,
                )));
                self.status_note = "Frame copied".into();
            }
            Err(note) => self.status_note = note.into(),
        }
        cx.notify();
    }

    /// "View all media": close the viewer and open the chat's Shared
    /// Media gallery on its photos and videos (tdesktop opens the same
    /// list as the viewer's paging source).
    pub(super) fn view_all_viewer_media(&mut self, cx: &mut Context<Self>) {
        self.close_media_viewer(cx);
        self.open_shared_media_ui(cx);
        self.select_shared_media_tab_ui(quill::state::SharedMediaTab::Media, cx);
    }

    /// The sender name and date shown under the viewer's title: who sent
    /// the photo and "today at 14:05" (tdesktop's viewer header). Profile
    /// photos carry no message, so they show neither.
    fn viewer_sender_line(&self, item: &MediaViewerItem) -> Option<(String, String)> {
        let session = self.session()?;
        let message = session
            .histories
            .get(&item.chat_id.0)?
            .messages
            .get(&item.message_id.0)?;
        let name = match message.sender {
            Some(quill::telegram::envelope::MessageSender::User { user_id }) => {
                session.user(user_id).map(|user| user.display_name())
            }
            Some(quill::telegram::envelope::MessageSender::Chat { chat_id }) => {
                session.chats.get(&chat_id).map(|chat| chat.title.clone())
            }
            None => None,
        }
        .or_else(|| {
            session
                .chats
                .get(&item.chat_id.0)
                .map(|chat| chat.title.clone())
        })?;
        if message.date <= 0 {
            return Some((name, String::new()));
        }
        let when = quill::local_time::viewer_stamp(
            &quill::local_time::civil_local(i64::from(message.date)),
            &quill::local_time::civil_local(quill::local_time::now_unix()),
        );
        Some((name, when))
    }

    /// Whose profile the sender name opens (tdesktop `Over::Name`).
    fn viewer_sender_profile(
        &self,
        item: &MediaViewerItem,
    ) -> Option<quill::viewer_extras::SenderProfile> {
        let session = self.session()?;
        let message = session
            .histories
            .get(&item.chat_id.0)?
            .messages
            .get(&item.message_id.0)?;
        let sender_chat = match message.sender {
            Some(quill::telegram::envelope::MessageSender::Chat { chat_id }) => {
                session.chats.get(&chat_id).map(|chat| &chat.kind)
            }
            _ => None,
        };
        let chat = session.chats.get(&item.chat_id.0).map(|chat| &chat.kind);
        quill::viewer_extras::sender_profile(message.sender.as_ref(), sender_chat, chat)
    }

    /// The sender name was clicked: close the viewer and show the profile.
    fn open_viewer_sender_profile(
        &mut self,
        profile: quill::viewer_extras::SenderProfile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use quill::state::InfoPanelTarget;
        use quill::viewer_extras::SenderProfile;
        let target = match profile {
            SenderProfile::User(id) => InfoPanelTarget::User(id),
            SenderProfile::Supergroup(id) => InfoPanelTarget::Supergroup(id),
            SenderProfile::BasicGroup(id) => InfoPanelTarget::BasicGroup(id),
        };
        self.close_media_viewer(cx);
        self.open_info_panel_target(target, window, cx);
    }

    /// Mouse-move listener for the control surfaces: keeps them shown.
    fn over_controls_listener(
        cx: &mut Context<Self>,
    ) -> impl Fn(&MouseMoveEvent, &mut Window, &mut App) + 'static {
        cx.listener(|this, _: &MouseMoveEvent, _, cx| {
            this.viewer_note_activity(true, cx);
        })
    }

    /// Mouse moved over the viewer: show the controls and restart the
    /// 1100 ms idle clock (tdesktop `mediaviewWaitHide`). `over_controls`
    /// keeps them up while the pointer rests on one.
    pub(super) fn viewer_note_activity(&mut self, over_controls: bool, cx: &mut Context<Self>) {
        self.viewer_last_activity = std::time::Instant::now();
        self.viewer_over_controls = over_controls;
        if self.viewer_controls_hidden {
            self.viewer_controls_hidden = false;
            self.viewer_controls_gen += 1;
            cx.notify();
        }
        // Captures render once, after the idle wait: keep the controls up.
        if self.viewer_hide_timer || still_frame() {
            return;
        }
        self.viewer_hide_timer = true;
        cx.spawn(async move |this, cx| {
            loop {
                let wait = this
                    .update(cx, |this, cx| {
                        let idle = this.viewer_last_activity.elapsed().as_millis() as u64;
                        let done = !this.media_viewer.is_open() || this.viewer_controls_hidden;
                        if !done && controls_should_hide(idle, this.viewer_over_controls) {
                            this.viewer_controls_hidden = true;
                            this.viewer_controls_gen += 1;
                            cx.notify();
                        } else if !done {
                            return Some(if this.viewer_over_controls {
                                VIEWER_WAIT_HIDE_MS
                            } else {
                                controls_hide_wait_ms(idle).max(16)
                            });
                        }
                        this.viewer_hide_timer = false;
                        None
                    })
                    .ok()
                    .flatten();
                match wait {
                    Some(ms) => {
                        cx.background_executor()
                            .timer(Duration::from_millis(ms))
                            .await;
                    }
                    None => break,
                }
            }
        })
        .detach();
    }

    /// "Attached Stickers": the sticker sets whose stickers were added to
    /// the photo or video (`getAttachedStickerSets`); the first opens in
    /// the sticker set dialog.
    pub(super) fn show_viewer_attached_stickers(&mut self, cx: &mut Context<Self>) {
        let Some(file_id) = self
            .media_viewer
            .current()
            .map(|item| item.download_file_id)
        else {
            return;
        };
        self.message_menu_ui.sticker_set_open = true;
        if let Some(live) = self.live.as_mut()
            && live.driver.fetch_attached_sticker_sets(file_id).is_err()
        {
            self.message_menu_ui.sticker_set_open = false;
            self.status_note = "could not load the attached stickers".into();
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
    /// previews the position; release seeks the clock and restarts the sound
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
    /// the frozen clock; while playing, the sound restarts at the offset.
    pub(super) fn seek_viewer_to(&mut self, secs: f64, cx: &mut Context<Self>) {
        let Some(clock) = self.viewer_clock.as_mut() else {
            return;
        };
        clock.seek(secs);
        let offset = clock.elapsed_secs();
        if let Some(video) = self.viewer_native.as_mut() {
            video.seek(offset);
            cx.notify();
            return;
        }
        if clock.is_playing()
            && !self.viewer_loops()
            && let Some(path) = self.viewer_video_path.clone()
        {
            self.start_viewer_audio(&path, offset);
        }
        cx.notify();
    }

    /// `J` / `L` and the full-screen arrows: seek `delta_secs` from the
    /// playhead, inside the clip.
    pub(super) fn seek_viewer_by(&mut self, delta_secs: f64, cx: &mut Context<Self>) {
        let Some(clock) = self.viewer_clock.as_ref() else {
            return;
        };
        let position = self
            .viewer_native
            .as_ref()
            .map(|video| video.position_secs())
            .unwrap_or_else(|| clock.elapsed_secs());
        let target = seek_target_secs(position, clock.duration_secs(), delta_secs);
        self.seek_viewer_to(target, cx);
    }

    /// Full-screen digits: jump to `fraction` of the clip.
    fn seek_viewer_to_fraction(&mut self, fraction: f64, cx: &mut Context<Self>) {
        if let Some(clock) = self.viewer_clock.as_ref() {
            let target = clock.duration_secs() * fraction.clamp(0.0, 1.0);
            self.seek_viewer_to(target, cx);
        }
    }

    /// A viewer playback key (see `quill::media_viewer::viewer_key_action`).
    /// `true` when the key was consumed.
    pub(super) fn handle_viewer_key(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(kind) = self.media_viewer.current().map(|item| item.kind) else {
            return false;
        };
        let modifiers = &keystroke.modifiers;
        let mods = ViewerKeyMods {
            primary: if cfg!(target_os = "macos") {
                modifiers.platform
            } else {
                modifiers.control
            },
            alt: modifiers.alt,
            shift: modifiers.shift,
        };
        let Some(action) = viewer_key_action(
            &keystroke.key,
            mods,
            kind,
            self.viewer_extra.video_fullscreen,
        ) else {
            return false;
        };
        self.viewer_note_activity(false, cx);
        match action {
            ViewerKeyAction::TogglePlayback => self.toggle_viewer_video(cx),
            ViewerKeyAction::SeekBy(secs) => self.seek_viewer_by(secs, cx),
            ViewerKeyAction::SeekToFraction(fraction) => self.seek_viewer_to_fraction(fraction, cx),
            ViewerKeyAction::ToggleFullscreen => self.viewer_toggle_fullscreen(window, cx),
        }
        true
    }

    /// Video full screen (tdesktop `playbackToggleFullScreen`): the window
    /// goes full screen, the arrows seek instead of paging, Escape leaves.
    pub(super) fn viewer_toggle_fullscreen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.viewer_extra.video_fullscreen {
            self.viewer_leave_video_fullscreen();
        } else {
            self.viewer_extra.window_was_fullscreen = window.is_fullscreen();
            if !self.viewer_extra.window_was_fullscreen {
                window.toggle_fullscreen();
            }
            self.viewer_extra.video_fullscreen = true;
        }
        cx.notify();
    }

    /// Leave video full screen; the window returns to windowed on the next
    /// frame unless it was full screen before.
    pub(super) fn viewer_leave_video_fullscreen(&mut self) {
        if !self.viewer_extra.video_fullscreen {
            return;
        }
        self.viewer_extra.video_fullscreen = false;
        self.viewer_extra.restore_fullscreen = !self.viewer_extra.window_was_fullscreen;
    }

    /// Looping clips play only while the window is active (tdesktop pauses
    /// them in the background): pause on deactivation, resume on return.
    fn sync_viewer_window_activity(&mut self, active: bool, cx: &mut Context<Self>) {
        if !self.viewer_loops() || self.viewer_video.is_none() {
            return;
        }
        let playing = self
            .viewer_clock
            .as_ref()
            .is_some_and(|clock| clock.is_playing());
        if !active && playing {
            self.viewer_extra.inactive_paused = true;
            self.pause_viewer_video(cx);
        } else if active && self.viewer_extra.inactive_paused {
            self.viewer_extra.inactive_paused = false;
            self.resume_viewer_video(cx);
        }
    }

    /// Open the current clip with the system player (the fallback when
    /// in-viewer playback fails, e.g. ffmpeg is missing).
    pub(super) fn open_viewer_clip_externally(&mut self, cx: &mut Context<Self>) {
        let path = self
            .media_viewer
            .current()
            .and_then(|item| self.viewer_clip_path(item));
        match path {
            Some(path) => self.open_file_guarded(path, cx),
            None => {
                self.status_note = "download the media first to open it".into();
                cx.notify();
            }
        }
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
    /// applies on release so a drag doesn't restart the sound per tick.
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
    /// playhead and the sound restarts at the new tempo so audio stays in
    /// sync.
    pub(super) fn cycle_playback_speed(&mut self, cx: &mut Context<Self>) {
        const SPEEDS: [f64; 6] = [0.5, 0.7, 1.0, 1.2, 1.5, 2.0];
        let next = SPEEDS
            .iter()
            .position(|s| (*s - self.playback_speed).abs() < 0.01)
            .map(|i| SPEEDS[(i + 1) % SPEEDS.len()])
            .unwrap_or(1.0);
        self.set_playback_speed(next, cx);
    }

    /// Apply a playback speed from the speed dial (slider or preset) to the
    /// active track and the viewer video.
    pub(super) fn set_playback_speed(&mut self, speed: f64, cx: &mut Context<Self>) {
        let next = quill::viewer_extras::clamp_speed(speed);
        self.playback_speed = next;
        self.viewer_extra.speed_preview = None;
        let mut restarted = false;
        if let Some(clock) = self.playback_clock.as_mut() {
            let was_playing = clock.is_playing();
            clock.set_rate(next);
            // The tempo stretcher follows the new speed on the fly.
            self.audio.set_speed(next);
            restarted = was_playing;
        }
        if let Some(video) = self.viewer_native.as_mut() {
            video.set_rate(next as f32);
            if let Some(clock) = self.viewer_clock.as_mut() {
                clock.set_rate(next);
            }
            restarted = true;
        } else if let Some(clock) = self.viewer_clock.as_mut() {
            let offset = clock.elapsed_secs();
            let was_playing = clock.is_playing();
            clock.set_rate(next);
            if was_playing && let Some(path) = self.viewer_video_path.clone() {
                self.start_viewer_audio(&path, offset);
                restarted = true;
            }
        }
        if !restarted {
            // Nothing playing: the speed applies to the next play.
            self.status_note = format!("playback speed {}×", Self::speed_label(next));
        }
        cx.notify();
    }

    /// The speed slider of the speed dial, created on first use.
    fn ensure_speed_slider(&mut self, cx: &mut Context<Self>) -> Entity<SliderState> {
        if let Some(slider) = self.viewer_extra.speed_slider.clone() {
            return slider;
        }
        let slider = cx.new(|_| {
            SliderState::new()
                .min(quill::viewer_extras::SPEED_MIN as f32)
                .max(quill::viewer_extras::SPEED_MAX as f32)
                .step(0.1)
                .default_value(self.playback_speed as f32)
        });
        cx.subscribe(&slider, |this, _, event, cx| match event {
            SliderEvent::Change(value) => {
                this.viewer_extra.speed_preview =
                    Some(quill::viewer_extras::snap_speed(f64::from(value.end())));
                cx.notify();
            }
            SliderEvent::Release(value) => {
                let speed = quill::viewer_extras::snap_speed(f64::from(value.end()));
                this.set_playback_speed(speed, cx);
            }
        })
        .detach();
        self.viewer_extra.speed_slider = Some(slider.clone());
        slider
    }

    /// Move the speed dial's thumb to the current speed.
    fn sync_speed_slider(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let speed = self.playback_speed as f32;
        if let Some(slider) = self.viewer_extra.speed_slider.as_ref() {
            slider.update(cx, |state, cx| state.set_value(speed, window, cx));
        }
    }

    /// Telegram Desktop's speed dial: the current speed on a button that
    /// opens a slider from 0.5x to 2.5x (sticking to the usual speeds)
    /// above the named presets.
    pub(super) fn speed_dial(&mut self, id: &'static str, cx: &mut Context<Self>) -> AnyElement {
        use quill::viewer_extras::{SPEED_PRESETS, equal_speeds, speed_label};
        let slider = self.ensure_speed_slider(cx);
        let dragging = self.viewer_extra.speed_preview.is_some();
        let current = self.playback_speed;
        let shown = self.viewer_extra.speed_preview.unwrap_or(current);
        let view = cx.entity().downgrade();
        Popover::new((id, 1usize))
            .anchor(Anchor::BottomRight)
            .default_open(self.viewer_extra.demo_speed_dial_open)
            .trigger(
                Button::new((id, 0usize))
                    .label(speed_label(shown))
                    .ghost()
                    .text_color(gpui_kit::white())
                    .tooltip("Playback speed")
                    .accessibility_label("Playback speed"),
            )
            .content(move |_state, window, cx| {
                // Another control (a row's speed link) may have moved the
                // speed since the thumb was last set.
                if !dragging {
                    let want = current as f32;
                    if slider.read(cx).value() != SliderValue::Single(want) {
                        slider.update(cx, |state, cx| state.set_value(want, window, cx));
                    }
                }
                let rows = SPEED_PRESETS.iter().map(|(speed, name)| {
                    let (speed, name) = (*speed, *name);
                    let chosen = equal_speeds(speed, current);
                    let view = view.clone();
                    let popover = cx.entity();
                    Button::new((id, 100 + (speed * 10.0) as usize))
                        .label(format!("{name}  {}", speed_label(speed)))
                        .ghost()
                        .small()
                        .w_full()
                        .when(chosen, |button| {
                            button.icon(gpui_kit::assets::IconName::Check)
                        })
                        .on_click(move |_, window, cx| {
                            let _ = view.update(cx, |this, cx| {
                                this.set_playback_speed(speed, cx);
                                this.sync_speed_slider(window, cx);
                            });
                            popover.update(cx, |state, cx| state.dismiss(window, cx));
                        })
                });
                div()
                    .w(px(220.))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().px_2().py_1().child(Slider::new(&slider)))
                    .children(rows)
            })
            .into_any_element()
    }

    /// MED1: short "1.5×" style label for the speed button.
    pub(super) fn speed_label(speed: f64) -> String {
        quill::viewer_extras::speed_label(speed)
    }

    /// MED1: mute toggle — 0 volume remembers the previous level and
    /// restores it on unmute; the sound restarts at the new volume.
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
    /// the sound picks up the new volume.
    pub(super) fn set_playback_volume(&mut self, volume: f32, cx: &mut Context<Self>) {
        self.playback_volume = volume.clamp(0.0, 1.0);
        self.audio.set_volume(self.playback_volume);
        if let Some(video) = self.viewer_native.as_mut() {
            video.set_volume(self.playback_volume);
        } else if self.viewer_clock.as_ref().is_some_and(|c| c.is_playing())
            && let Some(path) = self.viewer_video_path.clone()
        {
            let offset = self
                .viewer_clock
                .as_ref()
                .map(|c| c.elapsed_secs())
                .unwrap_or(0.0);
            self.start_viewer_audio(&path, offset);
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
    /// reaches the duration (the sound ends on its own).
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
                            if this.active_playback_id().is_none() {
                                quill::media_session::publish(None);
                            }
                            return false;
                        }
                        this.drive_viewer_media_session(cx);
                        if this.viewer_video.is_none() {
                            return false;
                        }
                        // The native player owns time: the clock mirrors
                        // its position, and its end pauses (the last frame
                        // stays; Play starts over).
                        if let Some(video) = this.viewer_native.as_mut() {
                            let playing = video.is_playing();
                            let position = video.position_secs();
                            if let Some(error) = video.error() {
                                this.playback_error = Some(error);
                            }
                            let looping = this.viewer_loops();
                            if let Some(clock) = this.viewer_clock.as_mut() {
                                clock.seek(position);
                                if !playing && clock.is_playing() {
                                    if looping {
                                        // The clip ended: start over.
                                        clock.seek(0.0);
                                    } else {
                                        clock.pause();
                                    }
                                }
                            }
                            if looping
                                && !playing
                                && this.viewer_clock.as_ref().is_some_and(|c| c.is_playing())
                                && let Some(video) = this.viewer_native.as_mut()
                            {
                                video.seek(0.0);
                                video.play();
                            }
                            cx.notify();
                            return true;
                        }
                        let finished = this
                            .viewer_clock
                            .as_ref()
                            .is_some_and(|clock| clock.is_playing() && clock.finished());
                        if finished && this.viewer_loops() {
                            if let Some(clock) = this.viewer_clock.as_mut() {
                                clock.seek(0.0);
                            }
                            cx.notify();
                            return true;
                        }
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

    /// Scroll-zoom the viewer visual around the pointer (`anchor`, px from
    /// the visual's top-left): the point under it stays put and the step
    /// scales with the scroll distance (positive y zooms in).
    pub(super) fn viewer_zoom_scroll(
        &mut self,
        delta_y: f32,
        anchor: (f32, f32),
        cx: &mut Context<Self>,
    ) {
        if delta_y == 0.0 {
            return;
        }
        self.viewer_zoom
            .zoom_at(wheel_zoom_factor(delta_y), anchor, self.viewer_frame);
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
        // A deleted item leaves the viewer (next item, or closed).
        self.prune_deleted_viewer_items(cx);
        self.sync_shared_media_viewer(cx);
        self.sync_viewer_window_activity(window.is_window_active(), cx);
        // MED1: keep the viewer seek/volume thumbs on the clocks (the
        // tick has no `&mut Window`, which `SliderState::set_value`
        // needs).
        self.sync_viewer_seek_slider(window, cx);
        self.sync_viewer_volume_slider(window, cx);
        // Native playback draws a new frame every display refresh; a GIF
        // loop only needs the shared frame clock.
        let loops = self.viewer_loops();
        if self
            .viewer_native
            .as_mut()
            .is_some_and(|video| video.is_playing())
        {
            if loops {
                self.request_animation_tick(30, cx);
            } else {
                window.request_animation_frame();
            }
        } else if loops
            && !self.viewer_video_frames.is_empty()
            && self
                .viewer_clock
                .as_ref()
                .is_some_and(|clock| clock.is_playing())
        {
            let fps = self.viewer_video_fps.round().clamp(1.0, 30.0) as u32;
            self.request_animation_tick(fps, cx);
        }
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
            if item.kind.is_playable() && !self.viewer_video_frames.is_empty() {
                self.viewer_render_frame()
            } else {
                None
            };
        let protected = self
            .media_viewer
            .current()
            .zip(self.session())
            .is_some_and(|(item, session)| session.chat_has_protected_content(item.chat_id));
        let can_delete = self.viewer_delete_confirm().is_some();
        let has_local_clip = self.viewer_clip_path(&item).is_some();
        let hidden = self.viewer_controls_hidden;
        let fade_gen = self.viewer_controls_gen;
        let row_id = item.message_id.0 as u64;
        let downloading_now = item
            .display_file_ids
            .iter()
            .chain(std::iter::once(&item.download_file_id))
            .any(|id| file_is_downloading(*id, &files, &downloading));
        let profile_view = self.media_viewer.source() == ViewerSource::Profile;
        let kind_label = if profile_view {
            "Profile photo"
        } else {
            item.kind.label()
        };
        let sender_line = (!profile_view)
            .then(|| self.viewer_sender_line(&item))
            .flatten();
        let header_label = if total > 1 {
            format!("{kind_label} {position} of {total}")
        } else {
            kind_label.to_string()
        };
        // B10: "Set as main photo" for one of your own earlier photos (the
        // first one already is the main photo).
        let can_set_main = profile_view
            && position > 1
            && self
                .viewer_extra
                .profile_user
                .zip(self.session().and_then(|s| s.my_user_id))
                .is_some_and(|(shown, me)| shown == me);
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
            if self.viewer_orientation.swaps_axes() {
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
        // The player's current frame (an AVPlayer GPU buffer on macOS, a
        // decoded FFmpeg image on Linux and Windows).
        let native_frame = item
            .kind
            .is_playable()
            .then(|| self.viewer_native.as_mut().and_then(|video| video.frame()))
            .flatten();
        let content: AnyElement = if let Some(frame) = native_frame {
            frame.element(
                px(zoom_w),
                px(zoom_h),
                ObjectFit::Contain,
                Corners::default(),
            )
        } else {
            // Pre-decoded video frame and thumbnail both render through
            // `img`; the frame is an `ImageSource::Render` (synchronous),
            // the thumbnail a path (async-loaded once, then cached). MED1:
            // a rotated photo renders from the eagerly-decoded
            // `viewer_rotated` cache (90°/180°/270° clockwise).
            let rotated: Option<ImageSource> = (item.kind == MediaViewerKind::Photo)
                .then_some(self.viewer_rotated.as_ref())
                .flatten()
                .filter(|(path, turns, _)| {
                    *turns == self.viewer_orientation.code()
                        && Some(path.as_path()) == thumb_path.as_deref()
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
                    (Some(duration), true) => {
                        format!("{kind_label} · {duration} — {downloading_label}")
                    }
                    (Some(duration), false) => {
                        format!("{kind_label} · {duration} — not downloaded")
                    }
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
            let up_view = view.clone();
            // Window position of the visual's top-left (it is centered in
            // the frame): wheel zoom keeps the point under the pointer.
            let origin = (
                (f32::from(viewport.width) - media_w) / 2.0,
                VIEWER_TOP_BAR + (frame_h - media_h) / 2.0,
            );
            let menu_view = view;
            let is_photo = item.kind == MediaViewerKind::Photo;
            let menu_protected = protected;
            let menu_can_delete = can_delete;
            let menu_profile = profile_view;
            let menu_set_main = can_set_main;
            let menu_playable = item.kind.is_playable();
            let menu_chat_source = self.media_viewer.source() == ViewerSource::Chat;
            // `photo.has_stickers` / `video.has_stickers`: stickers were
            // added to the media (tdesktop "Attached Stickers").
            let menu_attached = self
                .session()
                .and_then(|s| s.histories.get(&item.chat_id.0))
                .and_then(|h| h.messages.get(&item.message_id.0))
                .is_some_and(|m| match &m.content {
                    quill::telegram::envelope::MessageContent::Photo(p) => p.has_stickers,
                    quill::telegram::envelope::MessageContent::Video(v) => v.has_stickers,
                    _ => false,
                });
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
                        ScrollDelta::Lines(l) => l.y * VIEWER_WHEEL_NOTCH_PX,
                    };
                    let anchor = (
                        f32::from(event.position.x) - origin.0,
                        f32::from(event.position.y) - origin.1,
                    );
                    if let Some(view) = scroll_view.upgrade() {
                        view.update(cx, |this, cx| {
                            this.viewer_note_activity(false, cx);
                            this.viewer_zoom_scroll(dy, anchor, cx)
                        });
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
                            this.viewer_note_activity(false, cx);
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
                // Right-click menu (tdesktop's viewer context menu).
                .context_menu(move |menu, _, _| {
                    let item = |label: &'static str,
                                run: fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>)| {
                        let view = menu_view.clone();
                        PopupMenuItem::new(label).on_click(move |_, window, cx| {
                            let _ = view.update(cx, |this, cx| run(this, window, cx));
                        })
                    };
                    let mut menu = menu;
                    if menu_set_main {
                        menu = menu.item(item("Set as Main Photo", |this, _, cx| {
                            this.set_viewer_photo_as_main(cx)
                        }));
                    }
                    if !menu_protected && !menu_profile {
                        menu = menu.item(item("Forward", |this, window, cx| {
                            this.share_viewer_media(window, cx)
                        }));
                    }
                    if menu_can_delete {
                        menu = menu.item(item("Delete", |this, window, cx| {
                            this.delete_viewer_media(window, cx)
                        }));
                    }
                    if !menu_protected {
                        menu = menu
                            .item(PopupMenuItem::separator())
                            .item(item("Save As…", |this, _, cx| this.save_viewer_media(cx)));
                        if is_photo {
                            menu =
                                menu.item(item("Copy", |this, _, cx| this.copy_viewer_photo(cx)));
                        } else if menu_playable {
                            menu = menu
                                .item(item("Copy Frame", |this, _, cx| this.copy_viewer_frame(cx)));
                        }
                    }
                    if menu_attached {
                        menu = menu.item(item("Attached Stickers", |this, _, cx| {
                            this.show_viewer_attached_stickers(cx)
                        }));
                    }
                    if !menu_profile {
                        menu = menu.item(item("Show in Chat", |this, _, cx| {
                            this.show_viewer_in_chat(cx)
                        }));
                    }
                    if menu_chat_source {
                        menu = menu.item(item(
                            quill::viewer_extras::view_all_label(is_photo),
                            |this, _, cx| this.view_all_viewer_media(cx),
                        ));
                    }
                    if is_photo {
                        menu = menu
                            .item(PopupMenuItem::separator())
                            .item(item("Rotate", |this, _, cx| this.rotate_viewer_photo(cx)))
                            .item(item("Flip Horizontally", |this, _, cx| {
                                this.flip_viewer_horizontal(cx)
                            }))
                            .item(item("Flip Vertically", |this, _, cx| {
                                this.flip_viewer_vertical(cx)
                            }));
                    }
                    menu
                })
        };
        // Parity slice 5: video transport under the visual. the audio engine runs
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
                let speed_dial = self.speed_dial("media-viewer-speed", cx);
                let muted = self.playback_volume < 0.01;
                let volume_pct = (self.playback_volume * 100.0).round() as i32;
                // Telegram Desktop's player panel: the seek bar spans the
                // panel with elapsed / remaining time at its ends; below it
                // play/pause, volume, then speed and picture-in-picture.
                let remaining = format!(
                    "-{}",
                    format_voice_duration((total - elapsed).max(0.0) as i32)
                );
                use gpui_kit::assets::IconName as Lucide;
                let icon_button = |id: &'static str, icon: Lucide, label: &'static str| {
                    Button::new((id, row_id))
                        .icon(icon)
                        .ghost()
                        .text_color(gpui_kit::white())
                        .tooltip(label)
                        .accessibility_label(label)
                };
                let time = |text: String| {
                    div()
                        .flex_none()
                        .min_w(px(44.))
                        .text_xs()
                        .text_color(gpui_kit::white().opacity(0.85))
                        .child(text)
                };
                let seek_row = div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(time(format_voice_duration(elapsed as i32)))
                    .child(
                        // The slider thumb overhangs its track; the padding
                        // keeps it clear of the labels beside it.
                        div().flex_1().px_2().when_some(
                            self.viewer_seek_slider.clone(),
                            |this, slider| {
                                this.child(
                                    Slider::new(&slider).bg(accent()).text_color(text_on_fill()),
                                )
                            },
                        ),
                    )
                    .child(time(remaining).text_right());
                let controls_row = div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(if extracting {
                        div()
                            .text_sm()
                            .text_color(gpui_kit::white())
                            .child("Loading video…")
                            .into_any_element()
                    } else {
                        icon_button(
                            "media-viewer-play",
                            if playing { Lucide::Pause } else { Lucide::Play },
                            if playing {
                                "Pause (Space)"
                            } else {
                                "Play (Space)"
                            },
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.toggle_viewer_video(cx);
                        }))
                        .into_any_element()
                    })
                    .child(
                        icon_button(
                            "media-viewer-mute",
                            if muted {
                                Lucide::VolumeX
                            } else {
                                Lucide::Volume2
                            },
                            if muted { "Unmute" } else { "Mute" },
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.toggle_playback_mute(cx);
                        })),
                    )
                    .when_some(self.viewer_volume_slider.clone(), |this, slider| {
                        this.child(
                            div().w(px(96.)).px_2().child(
                                Slider::new(&slider).bg(accent()).text_color(text_on_fill()),
                            ),
                        )
                    })
                    .child(div().flex_1())
                    .child(speed_dial)
                    .when(
                        cfg!(target_os = "macos") && !self.viewer_video_frames.is_empty(),
                        |this| {
                            this.child(
                                icon_button(
                                    "media-viewer-pip",
                                    Lucide::PictureInPicture2,
                                    "Picture-in-Picture",
                                )
                                .on_click(cx.listener(|this, _, _, cx| this.open_video_pip(cx))),
                            )
                        },
                    )
                    .child({
                        let fullscreen = self.viewer_extra.video_fullscreen;
                        icon_button(
                            "media-viewer-fullscreen",
                            if fullscreen {
                                Lucide::Minimize
                            } else {
                                Lucide::Maximize
                            },
                            if fullscreen {
                                "Exit full screen (Esc)"
                            } else {
                                "Full screen (Alt+Enter)"
                            },
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.viewer_toggle_fullscreen(window, cx);
                        }))
                    });
                let _ = (label, volume_pct);
                div()
                    .id(("media-viewer-player", row_id))
                    .w(px(640.))
                    .max_w_full()
                    .px_4()
                    .py_2()
                    .rounded_xl()
                    .bg(gpui_kit::black().opacity(0.6))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(seek_row)
                    .child(controls_row)
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
        let transport = div()
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
        // Videos get the player panel; zoom controls are for photos.
        let transport = match video_controls {
            Some(controls) => controls,
            None => transport.into_any_element(),
        };
        // Custom emoji in the caption render like they do in message text:
        // the resolved sticker images, the span text until they are.
        let caption_emoji = self
            .session()
            .map(|session| {
                custom_emoji_paths(
                    &item.caption_entities,
                    &session.emoji.custom_emoji_stickers,
                    &files,
                    &roots,
                )
            })
            .unwrap_or_default();
        let caption: Option<AnyElement> = (!item.caption.is_empty()).then(|| {
            rich_text_line(
                &item.caption,
                &item.caption_entities,
                (item.chat_id.0, row_id),
                true,
                &self.spoiler_revealed,
                // Settings → Appearance: captions follow the message font size.
                self.msg_font(),
                &caption_emoji,
                cx,
            )
        });
        let sender_profile = self.viewer_sender_profile(&item);
        // tdesktop `showSaveMsgToast`: where the saved file went, with a
        // link that shows it in the file manager.
        let saved_toast: Option<AnyElement> = self.viewer_extra.saved_toast.clone().map(|toast| {
            div()
                .absolute()
                .top(px(VIEWER_TOP_BAR + 12.))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(
                    div()
                        .id("media-viewer-saved-toast")
                        .occlude()
                        .role(Role::Status)
                        .aria_label(format!(
                            "{}{}{}",
                            toast.text.before, toast.text.folder, toast.text.after
                        ))
                        .max_w(px(520.))
                        .pl_3()
                        .pr_1()
                        .py_1()
                        .rounded_full()
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().popover)
                        .text_color(cx.theme().popover_foreground)
                        .shadow_md()
                        .text_sm()
                        .flex()
                        .items_center()
                        .child(toast.text.before.clone())
                        .child(
                            Button::new("media-viewer-saved-folder")
                                .label(toast.text.folder.clone())
                                .link()
                                .small()
                                .tooltip("Show in folder")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.reveal_saved_toast_file(cx);
                                })),
                        )
                        .child(toast.text.after.clone()),
                )
                .into_any_element()
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
        let over_controls = Self::over_controls_listener;
        let top_bar = div()
            .id("media-viewer-top-bar")
            .on_mouse_move(over_controls(cx))
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
            .child(match sender_line {
                Some((name, when)) => div()
                    .id("media-viewer-sender")
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .text_color(gpui_kit::white())
                    .when_some(sender_profile, |this, profile| {
                        this.role(Role::Button)
                            .aria_label("Open profile")
                            .cursor_pointer()
                            .hover(|style| style.opacity(0.8))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_viewer_sender_profile(profile, window, cx);
                            }))
                    })
                    .child(
                        div()
                            .font_semibold()
                            .truncate()
                            .child(super::bidi_line::one_line_plain(name)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .opacity(0.7)
                            .truncate()
                            .child(if when.is_empty() {
                                header_label
                            } else {
                                format!("{when} \u{b7} {header_label}")
                            }),
                    )
                    .into_any_element(),
                None => div()
                    .font_semibold()
                    .text_color(gpui_kit::white())
                    .child(header_label)
                    .into_any_element(),
            })
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
                    // Protected content can't be shared or saved
                    // (Telegram Desktop hides both).
                    .when(can_set_main, |this| {
                        this.child(
                            icon_action(
                                ("media-viewer-set-main", row_id),
                                gpui_kit::assets::IconName::Images,
                                "Set as main photo",
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_viewer_photo_as_main(cx);
                            })),
                        )
                    })
                    .when(!protected && !profile_view, |this| {
                        this.child(
                            icon_action(
                                ("media-viewer-share", row_id),
                                gpui_kit::assets::IconName::Forward,
                                "Share",
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.share_viewer_media(window, cx);
                                },
                            )),
                        )
                    })
                    .when(!protected, |this| {
                        this.child(
                            icon_action(
                                ("media-viewer-save", row_id),
                                gpui_kit::assets::IconName::Download,
                                "Save",
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.save_viewer_media(cx);
                            })),
                        )
                    })
                    .when(!profile_view, |this| {
                        this.child(
                            icon_action(
                                ("media-viewer-show-in-chat", row_id),
                                gpui_kit::assets::IconName::MessageSquare,
                                "Show in chat",
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_viewer_in_chat(cx);
                            })),
                        )
                    })
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
                    .when(can_delete, |this| {
                        this.child(
                            icon_action(
                                ("media-viewer-delete", row_id),
                                gpui_kit::assets::IconName::Trash,
                                "Delete",
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.delete_viewer_media(window, cx);
                                },
                            )),
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
        let prev = self.media_viewer.has_prev().then(|| {
            nav_arrow(
                "media-viewer-prev",
                gpui_kit::assets::IconName::ChevronLeft,
                "Previous",
                -1,
                cx,
            )
        });
        let next = self.media_viewer.has_next().then(|| {
            nav_arrow(
                "media-viewer-next",
                gpui_kit::assets::IconName::ChevronRight,
                "Next",
                1,
                cx,
            )
        });
        let open_gen = self.viewer_open_gen as usize;
        let arrow_top = px(VIEWER_TOP_BAR + frame_h / 2.0 - 24.0);
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
                    .on_mouse_move(cx.listener(|this, _: &MouseMoveEvent, _, cx| {
                        this.viewer_note_activity(false, cx);
                    }))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.close_media_viewer(cx);
                    })),
            )
            .child(fade_controls(top_bar, "viewer-top-fade", fade_gen, hidden))
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
                this.child(fade_controls(
                    div()
                        .id("media-viewer-prev-lane")
                        .on_mouse_move(over_controls(cx))
                        .occlude()
                        .absolute()
                        .left(px(16.))
                        .top(arrow_top)
                        .child(prev),
                    "viewer-prev-fade",
                    fade_gen,
                    hidden,
                ))
            })
            .when_some(next, |this, next| {
                this.child(fade_controls(
                    div()
                        .id("media-viewer-next-lane")
                        .on_mouse_move(over_controls(cx))
                        .occlude()
                        .absolute()
                        .right(px(16.))
                        .top(arrow_top)
                        .child(next),
                    "viewer-next-fade",
                    fade_gen,
                    hidden,
                ))
            })
            .child(fade_controls(
                div()
                    .id("media-viewer-bottom-bar")
                    .on_mouse_move(over_controls(cx))
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
                        let can_open = item.kind.is_playable() && has_local_clip;
                        this.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(div().text_sm().text_color(danger_bright()).child(err))
                                .when(can_open, |row| {
                                    row.child(
                                        Button::new(("media-viewer-open-externally", row_id))
                                            .label("Open externally")
                                            .ghost()
                                            .text_color(gpui_kit::white())
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.open_viewer_clip_externally(cx);
                                            })),
                                    )
                                }),
                        )
                    })
                    .child(transport),
                "viewer-bottom-fade",
                fade_gen,
                hidden,
            ))
            .children(saved_toast)
            // Fade the whole overlay in over 200 ms when it opens
            // (tdesktop `mediaviewShowDuration`). Closing is instant: a
            // fade-out would have to keep the closed viewer's state alive.
            .with_animation(
                ("media-viewer-fade-in", open_gen),
                Animation::new(Duration::from_millis(VIEWER_SHOW_MS)),
                |overlay, t| overlay.opacity(if still_frame() { 1.0 } else { t }),
            )
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
mod group_tile_tests {
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
