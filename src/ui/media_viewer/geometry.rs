//! Where the viewer's media fits: a pure function of the window, the part
//! of it the screen leaves visible, and the chrome kept around the picture.
#![deny(clippy::indexing_slicing)]

use super::{VIEWER_SIDE_LANE, VIEWER_TOP_BAR};
use crate::ui::video_player::controls::{PANEL_HEIGHT, PANEL_WIDTH};
use quill::media_viewer::{GROUP_STRIP_PADDING, GROUP_THUMB_HEIGHT, ScreenCover, ViewRect};

/// The bottom bar's gap under its last row. tdesktop keeps its controller
/// 6 px off the bottom of a window that never reaches under the taskbar
/// (`mediaviewCaptionPadding.bottom`); Quill's viewer fills the window the
/// user sized, so it sits higher, off the screen's edge where an auto-hidden
/// Dock or taskbar slides in.
pub(super) const BAR_BOTTOM_GAP: f32 = 32.0;
/// The bottom bar's inset from the sides (`px_6`).
pub(super) const BAR_SIDE_GAP: f32 = 24.0;
/// Room under a video's picture for its 72 px controller, the bar's bottom
/// gap and 8 px above the controller.
const VIDEO_CONTROLS: f32 = 72.0 + BAR_BOTTOM_GAP + 8.0;
/// Room under a picture for the zoom row.
const ZOOM_CONTROLS: f32 = 56.0 + BAR_BOTTOM_GAP;
/// The bottom bar's gap between the caption and the controls.
pub(super) const CAPTION_GAP: f32 = 8.0;
/// Room under the controls for an album's thumbnail strip: the thumbs and
/// the space above them (the bar's gap included).
pub(super) const GROUP_STRIP_ROOM: f32 = GROUP_STRIP_PADDING + GROUP_THUMB_HEIGHT;

/// What decides where the media fits, in window points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ViewerFrame {
    pub(super) viewport: (f32, f32),
    /// The window's edges the Dock, taskbar or a panel hide
    /// (`screen_cover.rs`); the chrome keeps clear of them.
    pub(super) cover: ScreenCover,
    /// A video: the controller sits under the picture, not the zoom row.
    pub(super) video: bool,
    /// The caption plate's height; 0 without a caption.
    pub(super) caption: f32,
    /// Video full screen (tdesktop `_fullScreenVideo`).
    pub(super) fullscreen: bool,
    /// An album's thumbnail strip sits at the bottom (hidden in full
    /// screen).
    pub(super) strip: bool,
}

impl ViewerFrame {
    /// The part of the window the chrome keeps to.
    pub(super) fn safe_area(&self) -> ViewRect {
        self.cover.safe_area(self.viewport)
    }
}

/// The area the media fits into: its top-left and size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct FitArea {
    pub(super) origin: (f32, f32),
    pub(super) size: (f32, f32),
}

/// The width between the arrow lanes, which a caption wraps at too.
pub(super) fn lane_width(viewport_w: f32) -> f32 {
    (viewport_w - 2.0 * VIEWER_SIDE_LANE).max(240.0)
}

/// Normal: between the top bar and the caption and controls, outside the
/// arrow lanes, inside the safe area. Video full screen: the whole window,
/// the controls over the picture (tdesktop `resizeContentByScreenSize`).
pub(super) fn fit_area(frame: ViewerFrame) -> FitArea {
    let (width, height) = frame.viewport;
    if frame.fullscreen {
        return FitArea {
            origin: (0.0, 0.0),
            size: (width.max(0.0), height.max(0.0)),
        };
    }
    let safe = frame.safe_area();
    FitArea {
        origin: (safe.x + VIEWER_SIDE_LANE, safe.y + VIEWER_TOP_BAR),
        size: (
            lane_width(safe.w),
            (safe.h - VIEWER_TOP_BAR - bottom_band(frame)).max(200.0),
        ),
    }
}

/// The height of the bottom bar itself: the controls plus the caption
/// plate above them (video full screen hides the caption and floats the
/// controls over the picture, so they still count).
fn bottom_band(frame: ViewerFrame) -> f32 {
    let controls = if frame.video {
        VIDEO_CONTROLS
    } else {
        ZOOM_CONTROLS
    };
    let caption = if frame.caption > 0.0 && !frame.fullscreen {
        frame.caption + CAPTION_GAP
    } else {
        0.0
    };
    controls + caption + strip_room(frame)
}

fn strip_room(frame: ViewerFrame) -> f32 {
    if frame.strip && !frame.fullscreen {
        GROUP_STRIP_ROOM
    } else {
        0.0
    }
}

/// The height the bottom bar takes from the window's bottom edge: the bar
/// and the cover under it.
pub(super) fn bottom_chrome(frame: ViewerFrame) -> f32 {
    bottom_band(frame) + frame.cover.bottom
}

/// Where the video controller rests before it is dragged: centered at the
/// bottom of the safe area, over the album strip.
pub(super) fn controller_home(frame: ViewerFrame) -> ViewRect {
    let safe = frame.safe_area();
    let width = PANEL_WIDTH
        .as_f32()
        .min(safe.w - 2.0 * BAR_SIDE_GAP)
        .max(0.0);
    let height = PANEL_HEIGHT.as_f32();
    ViewRect::new(
        safe.x + (safe.w - width) / 2.0,
        safe.bottom() - BAR_BOTTOM_GAP - strip_room(frame) - height,
        width,
        height,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        BAR_BOTTOM_GAP, CAPTION_GAP, FitArea, GROUP_STRIP_ROOM, VIDEO_CONTROLS, ViewerFrame,
        bottom_chrome, controller_home, fit_area,
    };
    use crate::ui::media_viewer::{VIEWER_SIDE_LANE, VIEWER_TOP_BAR};
    use quill::media_viewer::{ScreenCover, ViewRect};

    fn frame(caption: f32, fullscreen: bool) -> ViewerFrame {
        ViewerFrame {
            viewport: (1600.0, 1000.0),
            cover: ScreenCover::default(),
            video: true,
            caption,
            fullscreen,
            strip: false,
        }
    }

    #[test]
    fn a_wrapped_caption_keeps_its_measured_height_under_the_picture() {
        let (line_h, plate_padding) = (22.0, 16.0);
        let caption = 2.0 * line_h + plate_padding;
        let area = fit_area(frame(caption, false));
        assert_eq!(
            area.size.1,
            1000.0 - VIEWER_TOP_BAR - VIDEO_CONTROLS - caption - CAPTION_GAP
        );
        assert_eq!(area.size.0, 1600.0 - 2.0 * VIEWER_SIDE_LANE);
    }

    #[test]
    fn no_caption_keeps_no_room_for_one() {
        let area = fit_area(frame(0.0, false));
        assert_eq!(area.size.1, 1000.0 - VIEWER_TOP_BAR - VIDEO_CONTROLS);
    }

    #[test]
    fn an_album_strip_takes_room_under_the_controls_but_not_in_full_screen() {
        let strip = |fullscreen| ViewerFrame {
            strip: true,
            ..frame(0.0, fullscreen)
        };
        let area = fit_area(strip(false));
        assert_eq!(
            area.size.1,
            1000.0 - VIEWER_TOP_BAR - VIDEO_CONTROLS - GROUP_STRIP_ROOM
        );
        assert_eq!(fit_area(strip(true)).size, (1600.0, 1000.0));
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

    #[test]
    fn the_bottom_chrome_is_the_controls_plus_the_caption_plate() {
        assert_eq!(bottom_chrome(frame(0.0, false)), VIDEO_CONTROLS);
        assert_eq!(
            bottom_chrome(frame(40.0, false)),
            VIDEO_CONTROLS + 40.0 + CAPTION_GAP
        );
        assert_eq!(bottom_chrome(frame(40.0, true)), VIDEO_CONTROLS);
    }

    fn docked(cover: ScreenCover) -> ViewerFrame {
        ViewerFrame {
            cover,
            ..frame(0.0, false)
        }
    }

    #[test]
    fn a_bottom_dock_lifts_the_picture_and_the_controller_above_it() {
        let dock = ScreenCover {
            bottom: 70.0,
            ..ScreenCover::default()
        };
        let area = fit_area(docked(dock));
        assert_eq!(area.origin, (VIEWER_SIDE_LANE, VIEWER_TOP_BAR));
        assert_eq!(area.size.1, 1000.0 - 70.0 - VIEWER_TOP_BAR - VIDEO_CONTROLS);
        assert_eq!(bottom_chrome(docked(dock)), VIDEO_CONTROLS + 70.0);
        let home = controller_home(docked(dock));
        assert_eq!(
            home,
            ViewRect::new(560.0, 1000.0 - 70.0 - BAR_BOTTOM_GAP - 72.0, 480.0, 72.0)
        );
    }

    #[test]
    fn a_side_dock_moves_the_lanes_and_centers_the_controller_in_what_is_left() {
        let dock = ScreenCover {
            left: 80.0,
            ..ScreenCover::default()
        };
        let area = fit_area(docked(dock));
        assert_eq!(area.origin.0, 80.0 + VIEWER_SIDE_LANE);
        assert_eq!(area.size.0, 1600.0 - 80.0 - 2.0 * VIEWER_SIDE_LANE);
        assert_eq!(
            controller_home(docked(dock)).x,
            80.0 + (1520.0 - 480.0) / 2.0
        );
    }

    #[test]
    fn the_controller_rests_above_the_album_strip() {
        let with_strip = ViewerFrame {
            strip: true,
            ..frame(0.0, false)
        };
        let home = controller_home(with_strip);
        assert_eq!(home.bottom(), 1000.0 - BAR_BOTTOM_GAP - GROUP_STRIP_ROOM);
        // Full screen hides the strip.
        let full = ViewerFrame {
            strip: true,
            ..frame(0.0, true)
        };
        assert_eq!(controller_home(full).bottom(), 1000.0 - BAR_BOTTOM_GAP);
    }

    #[test]
    fn a_narrow_window_narrows_the_controller() {
        let narrow = ViewerFrame {
            viewport: (400.0, 700.0),
            ..frame(0.0, false)
        };
        let home = controller_home(narrow);
        assert_eq!((home.x, home.w), (24.0, 352.0));
    }
}
