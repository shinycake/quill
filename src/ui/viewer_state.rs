//! Media viewer: zoom, video, controls and the photo editor.
#![deny(clippy::indexing_slicing)]

use gpui_kit::*;
use quill::ids::MessageId;
use quill::media_viewer::MediaViewer;
use quill::media_viewer::SwipeNav;
use quill::media_viewer::ViewerZoom;
use quill::media_viewer::WheelNav;
use std::path::PathBuf;

/// The editor opens on the photo of render `render` once it lands, unless
/// the user has left the chat view `view` it was asked for in.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct DeferredOpen {
    pub(super) render: u64,
    pub(super) view: quill::ids::ViewGeneration,
}

pub(crate) struct ViewerUi {
    /// Phase 4.5: fullscreen media viewer (photo/video overlay).
    pub(super) state: MediaViewer,
    /// Shared Media paging, video full screen and inactive-window state of
    /// the viewer.
    pub(super) extra: super::media_viewer::ViewerExtra,
    /// The photo editor over a pending photo attachment, when open.
    pub(super) photo_editor: Option<super::photo_editor::PhotoEditor>,
    /// Opening a photo mid-render shows its pending edit, not the original.
    pub(super) photo_editor_after: Option<DeferredOpen>,
    /// Parity slice 5: zoom/pan of the viewer visual (reset on open/step).
    pub(super) zoom: ViewerZoom,
    /// The viewer's media frame for the current window size (zoom/pan
    /// math works in it).
    pub(super) frame: (f32, f32),
    /// Parity slice 5: drag-pan anchor — last mouse position in px while the
    /// left button is held over the zoomed visual.
    pub(super) drag: Option<(f32, f32)>,
    /// Wheel and trackpad scrolling toward the next item.
    pub(super) wheel: WheelNav,
    /// A horizontal trackpad swipe pulling the next or previous arrow in.
    pub(super) swipe: SwipeNav,
    /// Parity slice 5: message whose video clip is playing in the viewer
    /// (sound playing) or paused (clock frozen, sound paused).
    pub(super) video: Option<MessageId>,
    /// The Picture-in-Picture window playing the viewer's clip, while
    /// one is open (the viewer itself is closed then).
    pub(super) pip: Option<super::media_viewer::PipLink>,
    /// Sandbox-checked local path of the viewer's clip.
    pub(super) video_path: Option<PathBuf>,
    /// The viewer clip's player (`video_player`): picture, sound and
    /// transport. The controls read its transport at render time.
    pub(super) player: Option<super::video_player::VideoPlayer>,
    /// A `.gif` file the player can't decode (AVPlayer does not read GIF
    /// and the FFmpeg shim is missing): it animates as an image instead.
    pub(super) gif: Option<PathBuf>,
    /// Draws the viewer's picture above the cached viewer slice, so a new
    /// video frame redraws only the picture (`media_viewer::layer`).
    pub(super) video_layer: Option<Entity<super::media_viewer::ViewerVideoLayer>>,
    /// Guard for the viewer's media-session tick (media keys and the OS
    /// Now Playing widget, 250 ms).
    pub(super) session_tick: bool,
    /// MED1: photo rotation and mirror flips (photos only; reset on
    /// open/step). Rendered from `viewer_rotated`.
    pub(super) orientation: quill::media_viewer::ViewerOrientation,
    /// MED1: re-oriented render of the viewer photo, keyed
    /// `(path, orientation code)`; decoded eagerly by Rotate/Flip so the
    /// overlay render stays allocation-free.
    pub(super) rotated: Option<(PathBuf, u8, super::owned_image::OwnedImage)>,
    /// The orientation a background decode is producing; it replaces
    /// `orientation` when the decode lands.
    pub(super) reorient_pending: Option<quill::media_viewer::ViewerOrientation>,
    /// Bumped by every Rotate/Flip press and when the viewer moves on, so
    /// a late decode is dropped.
    pub(super) reorient_gen: u64,
    /// HDR rendition of the current picture file (macOS).
    pub(super) hdr: super::hdr_image::HdrViewer,
    /// Counts viewer opens; keys the 200 ms fade-in so each open animates.
    pub(super) open_gen: u64,
    /// A media-timestamp link opened this message's video; the viewer
    /// starts it at the given second once the clip is ready.
    pub(super) pending_seek: Option<(MessageId, f64)>,
    /// Last mouse movement over the viewer (controls auto-hide clock).
    pub(super) last_activity: std::time::Instant,
    /// Toolbar, arrows and caption are faded out (after the idle wait).
    pub(super) controls_hidden: bool,
    /// When they last faded out.
    pub(super) controls_hidden_at: std::time::Instant,
    /// Bumped on every show/hide flip; keys the 150 ms controls fade.
    pub(super) controls_gen: u64,
    /// The pointer rests on a control, so they must not auto-hide.
    pub(super) over_controls: bool,
    /// A hide-timer task is already waiting.
    pub(super) hide_timer: bool,
    /// The video controller's sliders, drag previews, scrubber hover and
    /// the Space-hold boost (`video_player::controls`).
    pub(super) controls: super::video_player::controls::VideoControlsState,
}

impl ViewerUi {
    /// Drop a rotate/flip decode that is still running.
    pub(super) fn cancel_reorient(&mut self) {
        self.reorient_pending = None;
        self.reorient_gen = self.reorient_gen.wrapping_add(1);
    }

    pub(super) fn new() -> Self {
        Self {
            state: MediaViewer::closed(),
            extra: Default::default(),
            photo_editor: None,
            photo_editor_after: None,
            zoom: ViewerZoom::new(),
            frame: (720.0, 480.0),
            drag: None,
            wheel: WheelNav::default(),
            swipe: SwipeNav::default(),
            video: None,
            pip: None,
            video_path: None,
            player: None,
            gif: None,
            video_layer: None,
            session_tick: false,
            orientation: Default::default(),
            rotated: None,
            reorient_pending: None,
            reorient_gen: 0,
            hdr: Default::default(),
            open_gen: 0,
            pending_seek: None,
            last_activity: std::time::Instant::now(),
            controls_hidden: false,
            controls_hidden_at: std::time::Instant::now(),
            controls_gen: 0,
            over_controls: false,
            hide_timer: false,
            controls: Default::default(),
        }
    }
}
