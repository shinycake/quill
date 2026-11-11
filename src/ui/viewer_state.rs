//! Media viewer: zoom, video, controls and the photo editor.
#![deny(clippy::indexing_slicing)]

use gpui_kit::*;
use quill::ids::ChatMessage;
use quill::media_viewer::MediaViewer;
use quill::media_viewer::SwipeNav;
use quill::media_viewer::ViewerZoom;
use quill::media_viewer::WheelNav;

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
    /// The photo editor over a pending photo attachment, when open, or the
    /// open asked for while the photo still renders (so its pending edit
    /// shows, not the original).
    pub(super) photo_editor: super::viewer_parts::PhotoEditorSlot,
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
    /// Parity slice 5: the clip of the shown item, playing (sound playing)
    /// or paused (clock frozen, sound paused), a GIF animating as an image,
    /// or the line saying why it could not play. The controls read the
    /// player's transport at render time.
    pub(super) clip: super::viewer_parts::ViewerClip,
    /// The Picture-in-Picture window playing the viewer's clip, while
    /// one is open (the viewer itself is closed then).
    pub(super) pip: Option<super::media_viewer::PipLink>,
    /// Draws the viewer's picture above the cached viewer slice, so a new
    /// video frame redraws only the picture (`media_viewer::layer`).
    pub(super) video_layer: Option<Entity<super::media_viewer::ViewerVideoLayer>>,
    /// MED1: photo rotation and mirror flips (photos only; reset on
    /// open/step). Rendered from `viewer_rotated`.
    pub(super) orientation: quill::media_viewer::ViewerOrientation,
    /// MED1: re-oriented render of the viewer photo, keyed
    /// `(path, orientation code)`; decoded eagerly by Rotate/Flip so the
    /// overlay render stays allocation-free. A decode of a new orientation
    /// replaces `orientation` when it lands; a late one is dropped.
    pub(super) reorient: super::viewer_parts::Reorient,
    /// HDR rendition of the current picture file (macOS).
    pub(super) hdr: super::hdr_image::HdrViewer,
    /// The header's album Pin or Unpin, kept while the history is unchanged.
    pub(super) album_pin: quill::album_pin::AlbumPins,
    /// Counts viewer opens; keys the 200 ms fade-in so each open animates.
    pub(super) open_gen: u64,
    /// A media-timestamp link opened this message's video; the viewer
    /// starts it at the given second once the clip is ready.
    /// Keyed by chat and message: a bare id matched a later clip of
    /// another chat with the same id.
    pub(super) pending_seek: Option<(ChatMessage, f64)>,
    /// Last mouse movement over the viewer (controls auto-hide clock).
    pub(super) last_activity: std::time::Instant,
    /// Toolbar, arrows and caption: up, or faded out after the idle wait.
    pub(super) visibility: super::viewer_parts::ControlsVisibility,
    /// The pointer rests on a control, so they must not auto-hide.
    pub(super) over_controls: bool,
    /// The video controller's sliders, drag previews, scrubber hover and
    /// the Space-hold boost (`video_player::controls`).
    pub(super) controls: super::video_player::controls::VideoControlsState,
}

/// The PiP window plays the old account's video: it closes. A window
/// the viewer made full screen for a video goes back: the next screen's
/// viewer starts `Off` and would never give it back. The OS Now Playing
/// widget lets go of the old account's clip, and media keys pressed for
/// it are dropped: the viewer's media-session tick is the old screen's,
/// so it never runs again to clear them.
impl super::on_screen::Farewell for ViewerUi {
    fn farewell(
        app: &mut super::app::QuillApp,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<super::app::QuillApp>,
    ) {
        app.close_video_pip(cx);
        app.viewer_leave_video_fullscreen();
        app.viewer_follow_window_fullscreen(window);
        app.leave_viewer_edr(window);
        quill::media_session::clear();
    }
}

impl ViewerUi {
    pub(super) fn new() -> Self {
        Self {
            state: MediaViewer::closed(),
            extra: Default::default(),
            photo_editor: Default::default(),
            zoom: ViewerZoom::new(),
            frame: (720.0, 480.0),
            drag: None,
            wheel: WheelNav::default(),
            swipe: SwipeNav::default(),
            clip: Default::default(),
            pip: None,
            video_layer: None,
            orientation: Default::default(),
            reorient: Default::default(),
            hdr: Default::default(),
            open_gen: 0,
            pending_seek: None,
            last_activity: std::time::Instant::now(),
            visibility: Default::default(),
            over_controls: false,
            album_pin: Default::default(),
            controls: Default::default(),
        }
    }
}
