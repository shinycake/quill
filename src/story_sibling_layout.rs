//! Where the neighbour posters sit beside the story in a viewer opened from
//! the story tray, ported from tdesktop's layout
//! (`Controller::initLayout`, `media_stories_controller.cpp:482-546`): the
//! sibling is the story scaled down, as large as the space beside the
//! story column allows, a third of the way in from the window edge and
//! never closer than one control's width to the column, poking out of the
//! window by at most a quarter of itself. The viewer centres each sibling
//! vertically on the story frame.

/// The story frame.
pub const STORY_WIDTH: f32 = 360.;
pub const STORY_HEIGHT: f32 = 640.;
/// The viewer's column: the frame inside the panel's 16 px padding. The
/// header, caption and controls stay inside it, like tdesktop's controls
/// stay inside the story width (`storiesControlsExtend`).
pub const STORY_COLUMN_WIDTH: f32 = STORY_WIDTH + 32.;

/// `kSiblingMultiplierDefault`, `kSiblingMultiplierMax`.
const MULTIPLIER_DEFAULT: f32 = 0.448;
const MULTIPLIER_MAX: f32 = 0.72;
/// `kSiblingOutsidePart`: how much of a sibling may leave the window.
const OUTSIDE_PART: f32 = 0.24;
/// `storiesSiblingWidthMin`: a sibling tries not to be narrower than this.
const WIDTH_MIN: f32 = 200.;
/// `storiesFieldMargin.bottom()`: the margin kept beside the column.
const FIELD_MARGIN: f32 = 12.;
/// `storiesControlSize`: the least gap between a sibling and the column.
const CONTROL_SIZE: f32 = 64.;
/// A sibling narrower than this is too small to read or hit, so a very
/// narrow window shows none.
const WIDTH_SHOWN_MIN: f32 = 48.;

/// Both siblings' size and their left edges in window coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SiblingLayout {
    pub width: f32,
    pub height: f32,
    pub left_x: f32,
    pub right_x: f32,
}

impl SiblingLayout {
    /// The left sibling's left edge relative to the story frame, which is
    /// centred in the window.
    pub fn left_from_frame(&self, window_width: f32) -> f32 {
        self.left_x - frame_x(window_width)
    }

    pub fn right_from_frame(&self, window_width: f32) -> f32 {
        self.right_x - frame_x(window_width)
    }
}

fn frame_x(window_width: f32) -> f32 {
    (window_width - STORY_WIDTH) / 2.
}

/// The siblings for a window `window_width` wide, or `None` when there is
/// no room beside the column for a sibling at least 48 px wide.
pub fn sibling_layout(window_width: f32) -> Option<SiblingLayout> {
    let column_x = (window_width - STORY_COLUMN_WIDTH) / 2.;
    let width_for_siblings = window_width - STORY_COLUMN_WIDTH - 2. * FIELD_MARGIN;
    let width_max = width_for_siblings / (2. * (1. - OUTSIDE_PART));
    let preferred = MULTIPLIER_DEFAULT.max(WIDTH_MIN / STORY_WIDTH);
    let multiplier = preferred.min(MULTIPLIER_MAX).min(width_max / STORY_WIDTH);
    let width = STORY_WIDTH * multiplier;
    if !width.is_finite() || width < WIDTH_SHOWN_MIN {
        return None;
    }
    let height = STORY_HEIGHT * multiplier;
    let left_available = column_x - width;
    let left_x = (left_available / 3.)
        .min(left_available - CONTROL_SIZE)
        .max(-(width * OUTSIDE_PART).round());
    Some(SiblingLayout {
        width,
        height,
        left_x,
        right_x: window_width - width - left_x,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn a_wide_window_keeps_the_200_px_sibling_a_third_of_the_way_in() {
        let layout = sibling_layout(1200.).expect("room");
        assert!(close(layout.width, 200.), "{layout:?}");
        assert!(close(layout.height, 355.56), "{layout:?}");
        // The column starts at 404: 204 px are left, a third goes outside.
        assert!(close(layout.left_x, 68.), "{layout:?}");
        assert!(close(layout.right_x, 932.), "{layout:?}");
    }

    #[test]
    fn a_narrow_window_lets_the_sibling_leave_the_window_by_a_quarter() {
        let layout = sibling_layout(800.).expect("room");
        assert!(close(layout.width, 200.), "{layout:?}");
        assert!(close(layout.left_x, -48.), "{layout:?}");
        let layout = sibling_layout(600.).expect("room");
        assert!(layout.width < 200., "shrinks to fit: {layout:?}");
        assert!(close(layout.left_x, -(layout.width * 0.24).round()));
    }

    #[test]
    fn a_window_no_wider_than_the_column_has_no_siblings() {
        assert_eq!(sibling_layout(STORY_COLUMN_WIDTH), None);
        assert_eq!(sibling_layout(300.), None);
    }

    #[test]
    fn a_window_with_room_for_only_a_sliver_has_no_siblings() {
        assert_eq!(sibling_layout(480.), None, "a 42 px sibling");
        let layout = sibling_layout(500.).expect("room");
        assert!(layout.width >= WIDTH_SHOWN_MIN, "{layout:?}");
        for width in (400..3000).step_by(3) {
            if let Some(layout) = sibling_layout(width as f32) {
                assert!(layout.width >= WIDTH_SHOWN_MIN, "{width}: {layout:?}");
            }
        }
    }

    #[test]
    fn siblings_never_overlap_the_column() {
        for width in (420..3000).step_by(7) {
            let window = width as f32;
            let Some(layout) = sibling_layout(window) else {
                continue;
            };
            let column_x = (window - STORY_COLUMN_WIDTH) / 2.;
            assert!(
                layout.left_x + layout.width <= column_x + 0.01,
                "{window}: {layout:?}"
            );
            assert!(
                layout.right_x >= column_x + STORY_COLUMN_WIDTH - 0.01,
                "{window}: {layout:?}"
            );
            assert!(layout.width <= STORY_WIDTH * MULTIPLIER_MAX + 0.01);
        }
    }

    #[test]
    fn siblings_are_placed_against_the_centred_frame() {
        let layout = sibling_layout(1200.).expect("room");
        assert!(close(layout.left_from_frame(1200.), 68. - 420.));
        assert!(close(layout.right_from_frame(1200.), 932. - 420.));
    }
}
