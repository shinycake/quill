//! Where the viewer's media fits: a pure function of the window and the
//! chrome kept around the picture.
#![deny(clippy::indexing_slicing)]

use super::{VIEWER_SIDE_LANE, VIEWER_TOP_BAR};

/// Room under a video's picture for its 72 px controller and the bar's
/// 16 px bottom padding.
const VIDEO_CONTROLS: f32 = 96.0;
/// Room under a picture for the zoom row.
const ZOOM_CONTROLS: f32 = 72.0;

/// What decides where the media fits, in window points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ViewerFrame {
    pub(super) viewport: (f32, f32),
    /// A video: the controller sits under the picture, not the zoom row.
    pub(super) video: bool,
    /// The caption plate's height; 0 without a caption.
    pub(super) caption: f32,
    /// Video full screen (tdesktop `_fullScreenVideo`).
    pub(super) fullscreen: bool,
}

/// The area the media fits into: its top-left and size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct FitArea {
    pub(super) origin: (f32, f32),
    pub(super) size: (f32, f32),
}

pub(super) fn fit_area(frame: ViewerFrame) -> FitArea {
    let (width, height) = frame.viewport;
    let controls = if frame.video {
        VIDEO_CONTROLS
    } else {
        ZOOM_CONTROLS
    };
    let bottom = if frame.caption > 0.0 {
        controls + 32.0
    } else {
        controls
    };
    FitArea {
        origin: (VIEWER_SIDE_LANE, VIEWER_TOP_BAR),
        size: (
            (width - 2.0 * VIEWER_SIDE_LANE).max(240.0),
            (height - VIEWER_TOP_BAR - bottom).max(200.0),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{FitArea, VIDEO_CONTROLS, ViewerFrame, fit_area};
    use crate::ui::media_viewer::{VIEWER_SIDE_LANE, VIEWER_TOP_BAR};

    fn frame(caption: f32, fullscreen: bool) -> ViewerFrame {
        ViewerFrame {
            viewport: (1600.0, 1000.0),
            video: true,
            caption,
            fullscreen,
        }
    }

    #[test]
    fn a_wrapped_caption_keeps_its_measured_height_under_the_picture() {
        // Two 22 px lines in the plate's 8 px padding, 8 px above the
        // controller.
        let caption = 2.0 * 22.0 + 16.0;
        let area = fit_area(frame(caption, false));
        assert_eq!(
            area.size.1,
            1000.0 - VIEWER_TOP_BAR - VIDEO_CONTROLS - caption - 8.0
        );
        assert_eq!(area.size.0, 1600.0 - 2.0 * VIEWER_SIDE_LANE);
    }

    #[test]
    fn no_caption_keeps_no_room_for_one() {
        let area = fit_area(frame(0.0, false));
        assert_eq!(area.size.1, 1000.0 - VIEWER_TOP_BAR - VIDEO_CONTROLS);
    }

    #[test]
    fn video_full_screen_fits_the_whole_window() {
        let full = FitArea {
            origin: (0.0, 0.0),
            size: (1600.0, 1000.0),
        };
        assert_eq!(fit_area(frame(60.0, true)), full);
        assert_eq!(fit_area(frame(0.0, true)), full);
    }
}
