//! Screen sharing on Linux, split the way tdesktop's WebRTC splits it
//! (`EnvironmentLinux::uniqueDesktopCaptureSource`,
//! `DesktopCapturer::IsRunningUnderWayland`):
//! - **Wayland** (`XDG_SESSION_TYPE=wayland` with `WAYLAND_DISPLAY` set):
//!   one "unique" source. The share button skips Quill's chooser and the
//!   xdg-desktop-portal ScreenCast dialog picks the screen or window
//!   (`screen_portal.rs`), whose PipeWire stream is read in `pipewire.rs`.
//! - **X11**: Quill's chooser over the screens and windows `screen_x11.rs`
//!   lists, captured through the X server.
//!
//! [`PauseTracker`] turns "a picture came" / "no picture" into tgcalls'
//! pause edges (`DesktopCaptureSourceHelper`: a failed or 1 x 1 frame
//! pauses, the next good one resumes).
#![deny(clippy::indexing_slicing)]

use super::screen::{
    PreviewSlot, ScreenAccess, ScreenBackend, ScreenCapture, ScreenOptions, ScreenSource, SourceId,
    SourceKind, Thumbnail,
};
use super::{CaptureError, CaptureEvent, FrameSink, VideoFrame};
use std::sync::Arc;

/// The portal's one source (tdesktop `desktop_capturer_pipewire`).
pub const PORTAL_SOURCE: &str = "portal";

/// webrtc `DesktopCapturer::IsRunningUnderWayland`, for given values of
/// `XDG_SESSION_TYPE` and `WAYLAND_DISPLAY`.
pub fn wayland_session(session_type: Option<&str>, wayland_display: Option<&str>) -> bool {
    session_type.is_some_and(|kind| kind.starts_with("wayland")) && wayland_display.is_some()
}

/// Whether this process runs in a Wayland session (whatever GPUI itself
/// uses: under XWayland, X11 capture would only see XWayland windows).
pub fn running_under_wayland() -> bool {
    wayland_session(
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
    )
}

/// Pause edges from what a capture got, once per edge.
#[derive(Debug, Default)]
pub struct PauseTracker {
    paused: bool,
}

impl PauseTracker {
    /// A picture was captured: resume if paused.
    pub fn picture(&mut self) -> Option<CaptureEvent> {
        std::mem::take(&mut self.paused).then_some(CaptureEvent::Resumed)
    }

    /// No picture this time (hidden, minimised, not started yet).
    pub fn no_picture(&mut self) -> Option<CaptureEvent> {
        (!std::mem::replace(&mut self.paused, true)).then_some(CaptureEvent::Paused)
    }

    pub fn paused(&self) -> bool {
        self.paused
    }
}

/// The Linux backend. Stateless: the session type and the X server are
/// asked on every call.
pub struct LinuxScreen;

impl ScreenBackend for LinuxScreen {
    fn access(&self) -> ScreenAccess {
        // Wayland asks in the portal dialog; X11 has no permission.
        ScreenAccess::Granted
    }

    fn open_privacy_settings(&self) {}

    fn unique_source(&self) -> Option<SourceId> {
        running_under_wayland().then(|| SourceId(PORTAL_SOURCE.into()))
    }

    fn sources(&self) -> Result<Vec<ScreenSource>, CaptureError> {
        if running_under_wayland() {
            return Ok(vec![ScreenSource {
                id: SourceId(PORTAL_SOURCE.into()),
                kind: SourceKind::Screen,
                title: String::new(),
                size: (0, 0),
            }]);
        }
        super::screen_x11::sources()
    }

    fn thumbnail(&self, source: &ScreenSource, max: (u32, u32)) -> Option<Thumbnail> {
        if source.id.0 == PORTAL_SOURCE {
            return None;
        }
        super::screen_x11::thumbnail(source, max)
    }

    fn start(
        &self,
        source: &SourceId,
        options: ScreenOptions,
        sink: FrameSink<VideoFrame>,
        preview: Arc<PreviewSlot>,
    ) -> Result<Box<dyn ScreenCapture>, CaptureError> {
        if source.0 == PORTAL_SOURCE {
            return super::screen_portal::start(options, sink, preview);
        }
        super::screen_x11::start(source, options, sink, preview)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wayland_needs_the_session_type_and_a_display() {
        assert!(wayland_session(Some("wayland"), Some("wayland-0")));
        assert!(!wayland_session(Some("wayland"), None));
        assert!(!wayland_session(Some("x11"), Some("wayland-0")));
        assert!(!wayland_session(None, Some("wayland-0")));
        assert!(!wayland_session(Some("tty"), None));
    }

    #[test]
    fn pause_edges_come_once() {
        let mut tracker = PauseTracker::default();
        assert_eq!(tracker.picture(), None, "a running capture starts unpaused");
        assert_eq!(tracker.no_picture(), Some(CaptureEvent::Paused));
        assert_eq!(tracker.no_picture(), None);
        assert!(tracker.paused());
        assert_eq!(tracker.picture(), Some(CaptureEvent::Resumed));
        assert_eq!(tracker.picture(), None);
        assert!(!tracker.paused());
    }

    #[test]
    fn the_portal_source_has_no_picture() {
        let source = ScreenSource {
            id: SourceId(PORTAL_SOURCE.into()),
            kind: SourceKind::Screen,
            title: String::new(),
            size: (0, 0),
        };
        assert_eq!(LinuxScreen.thumbnail(&source, (10, 10)), None);
        assert_eq!(LinuxScreen.access(), ScreenAccess::Granted);
    }
}
