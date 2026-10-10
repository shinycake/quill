//! Media viewer: zoom, video, controls and the photo editor.

use gpui_kit::component::slider::SliderState;
use gpui_kit::*;
use quill::ids::FileId;
use quill::ids::MessageId;
use quill::media_viewer::MediaViewer;
use quill::media_viewer::ViewerZoom;
use quill::playback::PlaybackClock;
use std::path::PathBuf;
use std::process::Child;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

pub(crate) struct ViewerUi {
    /// Phase 4.5: fullscreen media viewer (photo/video overlay).
    pub(super) state: MediaViewer,
    /// Shared Media paging, video full screen and inactive-window state of
    /// the viewer.
    pub(super) extra: super::media_viewer::ViewerExtra,
    /// The photo editor over a pending photo attachment, when open.
    pub(super) photo_editor: Option<super::photo_editor::PhotoEditor>,
    /// Parity slice 5: zoom/pan of the viewer visual (reset on open/step).
    pub(super) zoom: ViewerZoom,
    /// The viewer's media frame for the current window size (zoom/pan
    /// math works in it).
    pub(super) frame: (f32, f32),
    /// Parity slice 5: drag-pan anchor — last mouse position in px while the
    /// left button is held over the zoomed visual.
    pub(super) drag: Option<(f32, f32)>,
    /// Parity slice 5: message whose video clip is playing in the viewer
    /// (sound playing) or paused (clock frozen, sound paused).
    pub(super) video: Option<MessageId>,
    pub(super) pip_window: Option<WindowHandle<gpui_kit::component::Root>>,
    /// Sandbox-checked local path of the viewer's clip, for pause/resume:
    /// the sound restarts.
    pub(super) video_path: Option<PathBuf>,
    /// The viewer clip's audio engine (sound only; the video frames
    /// render in-viewer). Stopped when the viewer closes, steps, or pauses.
    pub(super) audio: super::audio::AudioEngine,
    /// Playback clock for the viewer's clip (elapsed/total + pause freeze).
    pub(super) clock: Option<PlaybackClock>,
    /// Decoded frames for the viewer's clip, rendered in-place (parity
    /// slice 5). Pre-decoded `RenderImage` handles: `img()` resolves
    /// `ImageSource::Render` synchronously, so the 125 ms tick can cycle
    /// frames without an async load round trip per frame. Empty until
    /// extraction + decode finish; the thumbnail shows meanwhile.
    pub(super) video_frames: Vec<Arc<RenderImage>>,
    /// The native player (AVFoundation on macOS) for the viewer clip; when
    /// set it replaces the soundtrack engine and the extracted frames.
    pub(super) native: Option<super::native_video::NativeVideo>,
    /// Frame rate of `viewer_video_frames`, for clock → frame-index mapping.
    pub(super) video_fps: f64,
    /// File ID whose frames are in `viewer_video_frames` (cache invalidation).
    pub(super) frame_cache_file: Option<i32>,
    /// Frame extraction in progress (async); the viewer shows the thumbnail
    /// with a "loading video" hint until frames land.
    pub(super) extracting: bool,
    /// Running ffmpeg viewer-frame extraction, published by the background
    /// task. Taken and killed when the viewer closes or steps; stale
    /// completions are dropped by `viewer_extract_epoch`.
    pub(super) extract_child: Option<Arc<Mutex<Option<Child>>>>,
    /// Shared cancellation flag for the in-flight extraction: set by
    /// `kill_viewer_extraction` so the worker can abort even if the UI
    /// kills before ffmpeg publishes its child into `viewer_extract_child`.
    pub(super) extract_cancel: Option<Arc<AtomicBool>>,
    /// Generation counter for viewer frame extraction: bumped on every new
    /// extraction and on cancel, so a late completion from an abandoned run
    /// is dropped silently (no error note, no playback).
    pub(super) extract_epoch: u64,
    /// Screenshot demo only: skip the async frame extraction in
    /// `maybe_autoplay_viewer_video` (the demo extracts + decodes
    /// synchronously itself for a deterministic capture).
    pub(super) demo_sync_frames: bool,
    /// Guard for the viewer's 250 ms elapsed tick.
    pub(super) tick: bool,
    /// Play was requested before the clip was local. Resumed from the poll
    /// loop when `downloadFile` finishes.
    pub(super) pending_play: Option<(MessageId, FileId)>,
    /// MED1: photo rotation and mirror flips (photos only; reset on
    /// open/step). Rendered from `viewer_rotated`.
    pub(super) orientation: quill::media_viewer::ViewerOrientation,
    /// MED1: re-oriented render of the viewer photo, keyed
    /// `(path, orientation code)`; decoded eagerly by Rotate/Flip so the
    /// overlay render stays allocation-free.
    pub(super) rotated: Option<(PathBuf, u8, Arc<RenderImage>)>,
    /// Counts viewer opens; keys the 200 ms fade-in so each open animates.
    pub(super) open_gen: u64,
    /// A media-timestamp link opened this message's video; the viewer
    /// starts it at the given second once the clip is ready.
    pub(super) pending_seek: Option<(MessageId, f64)>,
    /// Last mouse movement over the viewer (controls auto-hide clock).
    pub(super) last_activity: std::time::Instant,
    /// Toolbar, arrows and caption are faded out (after the idle wait).
    pub(super) controls_hidden: bool,
    /// Bumped on every show/hide flip; keys the 150 ms controls fade.
    pub(super) controls_gen: u64,
    /// The pointer rests on a control, so they must not auto-hide.
    pub(super) over_controls: bool,
    /// A hide-timer task is already waiting.
    pub(super) hide_timer: bool,
    /// MED1: seek slider for the viewer video transport (created in
    /// `begin_viewer_video`, cleared in `stop_viewer_video`).
    pub(super) seek_slider: Option<Entity<SliderState>>,
    /// MED1: true while the viewer seek thumb is being dragged (the tick
    /// must not fight the drag).
    pub(super) seek_scrubbing: bool,
    /// MED1: drag preview position for the viewer seek slider.
    pub(super) seek_preview_secs: Option<f64>,
    /// MED1: volume slider for the viewer video transport (0–100%).
    pub(super) volume_slider: Option<Entity<SliderState>>,
    /// MED1: true while the viewer volume thumb is being dragged.
    pub(super) volume_scrubbing: bool,
}

impl ViewerUi {
    pub(super) fn new(audio_output: &super::audio::SharedOutput) -> Self {
        Self {
            state: MediaViewer::closed(),
            extra: Default::default(),
            photo_editor: None,
            zoom: ViewerZoom::new(),
            frame: (720.0, 480.0),
            drag: None,
            video: None,
            pip_window: None,
            video_path: None,
            audio: super::audio::AudioEngine::new(audio_output.clone()),
            clock: None,
            video_frames: Vec::new(),
            native: None,
            video_fps: 0.0,
            frame_cache_file: None,
            extracting: false,
            extract_child: None,
            extract_cancel: None,
            extract_epoch: 0,
            demo_sync_frames: false,
            tick: false,
            pending_play: None,
            orientation: Default::default(),
            rotated: None,
            open_gen: 0,
            pending_seek: None,
            last_activity: std::time::Instant::now(),
            controls_hidden: false,
            controls_gen: 0,
            over_controls: false,
            hide_timer: false,
            seek_slider: None,
            seek_scrubbing: false,
            seek_preview_secs: None,
            volume_slider: None,
            volume_scrubbing: false,
        }
    }
}
