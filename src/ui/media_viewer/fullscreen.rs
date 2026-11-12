//! Video full screen (tdesktop `_fullScreenVideo`): the picture takes the
//! whole window, the arrows seek, Escape leaves. The mode follows the
//! window: when the window leaves OS full screen some other way (the
//! title-bar button, a system shortcut), the viewer leaves the mode too.
#![deny(clippy::indexing_slicing)]

use std::time::{Duration, Instant};

/// How long a cancelled entry waits for the OS to finish going full
/// screen before it gives up on giving the window back.
const CANCEL_WAIT: Duration = Duration::from_secs(1);

/// Where video full screen is, as seen against the window's own state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::ui) enum VideoFullscreen {
    #[default]
    Off,
    /// On; the window was asked to go full screen and is not there yet
    /// (the OS animates it, or the platform can't).
    Entering,
    /// On; the window went full screen for it and goes back when it ends.
    Active,
    /// On, over a window that was already full screen: it stays so.
    InFullscreenWindow,
    /// Off; the window goes back to windowed on the next frame (nothing is
    /// in flight: the frame toggles it).
    GoingBack,
    /// Off; the window was asked to leave full screen and the OS is still
    /// animating it out. Held until the window reports windowed or `until`
    /// passes.
    Exiting { until: Instant },
    /// On again while the window is still on its way out: once it reports
    /// windowed, it is asked to go full screen again (or, past `until`, it
    /// is taken to have stayed there).
    Reentering { until: Instant },
    /// Off before the window got there: once it reports full screen (until
    /// `until`), it goes back.
    Cancelling { until: Instant },
}

impl VideoFullscreen {
    /// The picture takes the whole window.
    pub(in crate::ui) fn is_on(self) -> bool {
        matches!(
            self,
            Self::Entering | Self::Active | Self::InFullscreenWindow | Self::Reentering { .. }
        )
    }

    /// Turn the mode on over a window that is (or is not) full screen.
    /// Returns whether to ask the window to go full screen. A window Quill
    /// made full screen and has not given back yet stays Quill's.
    pub(in crate::ui) fn enter(self, window_fullscreen: bool) -> (Self, bool) {
        match self {
            Self::Entering | Self::Active | Self::InFullscreenWindow | Self::Reentering { .. } => {
                (self, false)
            }
            Self::GoingBack | Self::Cancelling { .. } if window_fullscreen => (Self::Active, false),
            // The window is still leaving: ask for it again once it has.
            Self::Exiting { until } if window_fullscreen => (Self::Reentering { until }, false),
            Self::Exiting { .. } => (Self::Entering, true),
            // The OS may still be on its way there.
            Self::Cancelling { .. } => (Self::Entering, false),
            Self::Off | Self::GoingBack if window_fullscreen => (Self::InFullscreenWindow, false),
            Self::Off | Self::GoingBack => (Self::Entering, true),
        }
    }

    /// Turn the mode off; a window it made full screen goes back.
    pub(in crate::ui) fn leave(self, now: Instant) -> Self {
        match self {
            Self::Entering => Self::Cancelling {
                until: now + CANCEL_WAIT,
            },
            Self::Active => Self::GoingBack,
            Self::InFullscreenWindow => Self::Off,
            Self::Reentering { until } => Self::Exiting { until },
            Self::Off | Self::GoingBack | Self::Exiting { .. } | Self::Cancelling { .. } => self,
        }
    }

    /// The next state once the window reports whether it is full screen,
    /// and whether to toggle the window.
    pub(in crate::ui) fn follow_window(
        self,
        window_fullscreen: bool,
        now: Instant,
    ) -> (Self, bool) {
        match self {
            Self::Off => (Self::Off, false),
            Self::GoingBack if window_fullscreen => (
                Self::Exiting {
                    until: now + CANCEL_WAIT,
                },
                true,
            ),
            Self::GoingBack => (Self::Off, false),
            Self::Exiting { until } if window_fullscreen && now < until => (self, false),
            Self::Exiting { .. } => (Self::Off, false),
            Self::Reentering { until } if window_fullscreen && now >= until => {
                (Self::Active, false)
            }
            Self::Reentering { .. } if window_fullscreen => (self, false),
            Self::Reentering { .. } => (Self::Entering, true),
            Self::Cancelling { until } if now >= until => (Self::Off, false),
            Self::Cancelling { .. } if window_fullscreen => (Self::Off, true),
            Self::Cancelling { .. } => (self, false),
            Self::Entering if window_fullscreen => (Self::Active, false),
            Self::Entering => (Self::Entering, false),
            Self::Active | Self::InFullscreenWindow if window_fullscreen => (self, false),
            Self::Active | Self::InFullscreenWindow => (Self::Off, false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CANCEL_WAIT, VideoFullscreen};
    use std::time::{Duration, Instant};

    #[test]
    fn a_windowed_window_goes_full_screen_and_back() {
        let now = Instant::now();
        let (mode, toggle) = VideoFullscreen::Off.enter(false);
        assert!(toggle);
        assert!(mode.is_on());
        let (mode, toggle) = mode.follow_window(false, now);
        assert_eq!((mode, toggle), (VideoFullscreen::Entering, false));
        let (mode, _) = mode.follow_window(true, now);
        assert_eq!(mode, VideoFullscreen::Active);
        let mode = mode.leave(now);
        assert!(!mode.is_on());
        let (mode, toggle) = mode.follow_window(true, now);
        assert!(toggle, "the frame asks the window to leave");
        assert_eq!(
            mode,
            VideoFullscreen::Exiting {
                until: now + CANCEL_WAIT
            }
        );
        // The OS animates out; the window then reports windowed.
        assert_eq!(mode.follow_window(true, now), (mode, false));
        assert_eq!(
            mode.follow_window(false, now),
            (VideoFullscreen::Off, false)
        );
    }

    #[test]
    fn entering_during_the_exit_animation_asks_again_once_windowed() {
        let now = Instant::now();
        let (leaving, toggle) = VideoFullscreen::Active.leave(now).follow_window(true, now);
        assert!(toggle);
        // F pressed while the window still reports full screen.
        let (mode, ask) = leaving.enter(true);
        assert!(!ask, "not yet: the window is still leaving");
        assert!(mode.is_on());
        assert_eq!(
            mode.follow_window(true, now),
            (mode, false),
            "still leaving"
        );
        let (mode, toggle) = mode.follow_window(false, now);
        assert_eq!((mode, toggle), (VideoFullscreen::Entering, true));
        // A window that never left within the wait stays Quill's.
        let (stayed, _) = leaving.enter(true);
        assert_eq!(
            stayed.follow_window(true, now + CANCEL_WAIT),
            (VideoFullscreen::Active, false)
        );
        // Windowed already when F comes: ask at once.
        assert_eq!(leaving.enter(false), (VideoFullscreen::Entering, true));
        // Leaving again during the wait goes back to waiting out the exit.
        let (again, _) = leaving.enter(true);
        assert!(matches!(again.leave(now), VideoFullscreen::Exiting { .. }));
    }

    #[test]
    fn leaving_os_full_screen_another_way_ends_the_mode() {
        let now = Instant::now();
        let (mode, _) = VideoFullscreen::Active.follow_window(false, now);
        assert_eq!(mode, VideoFullscreen::Off);
        let (mode, _) = VideoFullscreen::InFullscreenWindow.follow_window(false, now);
        assert_eq!(mode, VideoFullscreen::Off);
    }

    #[test]
    fn a_full_screen_window_stays_full_screen() {
        let now = Instant::now();
        let (mode, toggle) = VideoFullscreen::Off.enter(true);
        assert!(!toggle);
        assert_eq!(mode.follow_window(true, now), (mode, false));
        let mode = mode.leave(now);
        assert_eq!(mode, VideoFullscreen::Off);
        assert_eq!(mode.follow_window(true, now), (VideoFullscreen::Off, false));
    }

    #[test]
    fn leaving_before_the_window_got_there_gives_it_back_when_it_arrives() {
        let now = Instant::now();
        let mode = VideoFullscreen::Entering.leave(now);
        assert!(!mode.is_on());
        let (mode, toggle) = mode.follow_window(false, now);
        assert!(!toggle);
        let later = now + Duration::from_millis(400);
        assert_eq!(
            mode.follow_window(true, later),
            (VideoFullscreen::Off, true)
        );
    }

    #[test]
    fn a_window_that_never_gets_there_is_left_alone() {
        let now = Instant::now();
        let mode = VideoFullscreen::Entering.leave(now);
        let late = now + CANCEL_WAIT;
        assert_eq!(
            mode.follow_window(false, late),
            (VideoFullscreen::Off, false)
        );
        assert_eq!(
            mode.follow_window(true, late),
            (VideoFullscreen::Off, false)
        );
    }

    #[test]
    fn entering_again_keeps_the_window_quill_made_full_screen() {
        let now = Instant::now();
        let leaving = VideoFullscreen::Active.leave(now);
        assert_eq!(leaving.enter(true), (VideoFullscreen::Active, false));
        let (mode, _) = leaving.enter(true);
        assert!(mode.leave(now).follow_window(true, now).1);
        let cancelling = VideoFullscreen::Entering.leave(now);
        assert_eq!(cancelling.enter(true), (VideoFullscreen::Active, false));
        assert_eq!(cancelling.enter(false), (VideoFullscreen::Entering, false));
    }
}
