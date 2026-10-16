//! Video full screen (tdesktop `_fullScreenVideo`): the picture takes the
//! whole window, the arrows seek, Escape leaves. The mode follows the
//! window: when the window leaves OS full screen some other way (the
//! title-bar button, a system shortcut), the viewer leaves the mode too.
#![deny(clippy::indexing_slicing)]

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
    /// Off; the window goes back to windowed on the next frame.
    Leaving,
}

impl VideoFullscreen {
    /// The picture takes the whole window.
    pub(in crate::ui) fn is_on(self) -> bool {
        matches!(
            self,
            Self::Entering | Self::Active | Self::InFullscreenWindow
        )
    }

    /// Turn the mode on over a window that is (or is not) full screen
    /// already. Returns whether to ask the window to go full screen.
    pub(in crate::ui) fn enter(window_fullscreen: bool) -> (Self, bool) {
        if window_fullscreen {
            (Self::InFullscreenWindow, false)
        } else {
            (Self::Entering, true)
        }
    }

    /// Turn the mode off; a window it made full screen goes back.
    pub(in crate::ui) fn leave(self) -> Self {
        match self {
            Self::Entering | Self::Active => Self::Leaving,
            Self::InFullscreenWindow => Self::Off,
            Self::Off | Self::Leaving => self,
        }
    }

    /// The next state once the window reports whether it is full screen,
    /// and whether to toggle the window.
    pub(in crate::ui) fn follow_window(self, window_fullscreen: bool) -> (Self, bool) {
        match self {
            Self::Off => (Self::Off, false),
            Self::Leaving => (Self::Off, window_fullscreen),
            Self::Entering if window_fullscreen => (Self::Active, false),
            Self::Entering => (Self::Entering, false),
            Self::Active | Self::InFullscreenWindow if window_fullscreen => (self, false),
            Self::Active | Self::InFullscreenWindow => (Self::Off, false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::VideoFullscreen;

    #[test]
    fn a_windowed_window_goes_full_screen_and_back() {
        let (mode, toggle) = VideoFullscreen::enter(false);
        assert!(toggle);
        assert!(mode.is_on());
        let (mode, toggle) = mode.follow_window(false);
        assert_eq!((mode, toggle), (VideoFullscreen::Entering, false));
        let (mode, _) = mode.follow_window(true);
        assert_eq!(mode, VideoFullscreen::Active);
        let mode = mode.leave();
        assert!(!mode.is_on());
        assert_eq!(mode.follow_window(true), (VideoFullscreen::Off, true));
    }

    #[test]
    fn leaving_os_full_screen_another_way_ends_the_mode() {
        let (mode, _) = VideoFullscreen::Active.follow_window(false);
        assert_eq!(mode, VideoFullscreen::Off);
        let (mode, _) = VideoFullscreen::InFullscreenWindow.follow_window(false);
        assert_eq!(mode, VideoFullscreen::Off);
    }

    #[test]
    fn a_full_screen_window_stays_full_screen() {
        let (mode, toggle) = VideoFullscreen::enter(true);
        assert!(!toggle);
        assert_eq!(mode.follow_window(true), (mode, false));
        let mode = mode.leave();
        assert_eq!(mode, VideoFullscreen::Off);
        assert_eq!(mode.follow_window(true), (VideoFullscreen::Off, false));
    }

    #[test]
    fn a_window_already_windowed_is_not_toggled_back() {
        let mode = VideoFullscreen::Entering.leave();
        assert_eq!(mode.follow_window(false), (VideoFullscreen::Off, false));
    }
}
