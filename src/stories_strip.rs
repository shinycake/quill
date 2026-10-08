//! Stories strip metrics and the scroll-driven collapse (tdesktop
//! `Dialogs::Stories::List`, `dialogs/ui/dialogs_stories_list.cpp`).
//!
//! tdesktop draws the strip either expanded (a 77 px row of 42 px avatars
//! with names) or collapsed (up to three overlapping 21 px avatars next to
//! the search field), morphing between them by a layout ratio. Quill's
//! strip is the first row of the chat list, so scrolling the list down by
//! the strip's height collapses it: the row scrolls away and the compact
//! stack takes its place in the search row, and scrolling back to the top
//! expands it again. Nothing here owns time: the ratio is a pure function
//! of the list's scroll offset.
//!
//! Pure core, no GPUI.

/// `dialogsStoriesFull.height` (dialogs.style:693).
pub const FULL_HEIGHT: f32 = 77.0;
/// `dialogsStoriesFull.photo`.
pub const FULL_PHOTO: f32 = 42.0;
/// `dialogsStoriesFull.photoLeft`: space on each side of an avatar.
pub const FULL_PHOTO_LEFT: f32 = 10.0;
/// `dialogsStoriesFull.photoTop`.
pub const FULL_PHOTO_TOP: f32 = 9.0;
/// `dialogsStoriesFull.nameTop`.
pub const FULL_NAME_TOP: f32 = 56.0;
/// `dialogsStoriesFull.nameStyle` font size.
pub const FULL_NAME_FONT: f32 = 11.0;
/// `dialogsStoriesFull.left`: leading skip before the first cell.
pub const FULL_LEFT: f32 = 4.0;
/// `dialogsStories.photo`: collapsed avatar size.
pub const SMALL_PHOTO: f32 = 21.0;
/// `dialogsStories.shift`: distance between collapsed avatars.
pub const SMALL_SHIFT: f32 = 16.0;
/// `dialogsStories.height`: the collapsed stack's row height.
pub const SMALL_HEIGHT: f32 = 35.0;
/// `kSmallThumbsShown` (dialogs_stories_list.cpp:34).
pub const SMALL_THUMBS: usize = 3;
/// `dialogsStoriesList.readOpacity` (dialogs.style:790).
pub const READ_OPACITY: f32 = 0.6;
/// `kExpandAfterRatio` / `kCollapseAfterRatio` (dialogs_stories_list.cpp:36).
const EXPAND_AFTER: f32 = 0.72;
const COLLAPSE_AFTER: f32 = 0.68;
/// `kFrictionRatio`: how far the collapsed state follows a pull.
const FRICTION: f32 = 0.15;

/// One expanded cell: avatar plus its side skips
/// (`full.photoLeft * 2 + full.photo`).
pub const CELL_WIDTH: f32 = FULL_PHOTO_LEFT * 2.0 + FULL_PHOTO;

/// How far the strip is collapsed, 0 (fully expanded, list at the top) to
/// 1 (the strip scrolled out). `scroll_top` is how far the chat list is
/// scrolled down in px (negative values, from rubber-banding, count as 0).
pub fn collapse_progress(scroll_top: f32) -> f32 {
    (scroll_top / FULL_HEIGHT).clamp(0.0, 1.0)
}

/// tdesktop's `expandRatio` (`List::computeLayout`): how "expanded" the
/// strip reads for a layout ratio, 0 at or below `kExpandAfterRatio *
/// kFrictionRatio` and 1 at or above `kCollapseAfterRatio`. Drives the
/// ring thickness and the title/stack cross-fade there.
pub fn expand_ratio(layout_ratio: f32) -> f32 {
    let low = EXPAND_AFTER * FRICTION;
    if layout_ratio >= COLLAPSE_AFTER {
        1.0
    } else if layout_ratio <= low {
        0.0
    } else {
        (layout_ratio - low) / (COLLAPSE_AFTER - low)
    }
}

/// Opacity of the compact avatar stack for a collapse progress: it fades
/// in as the strip leaves, exactly where tdesktop's expand ratio fades
/// out (`1 - expand_ratio(1 - progress)`).
pub fn compact_opacity(progress: f32) -> f32 {
    1.0 - expand_ratio(1.0 - progress.clamp(0.0, 1.0))
}

/// How many avatars the collapsed stack shows.
pub fn compact_count(total: usize) -> usize {
    total.min(SMALL_THUMBS)
}

/// Width of the collapsed stack of `count` avatars (overlapping by
/// `SMALL_PHOTO - SMALL_SHIFT`).
pub fn compact_width(count: usize) -> f32 {
    if count == 0 {
        return 0.0;
    }
    SMALL_PHOTO + (count - 1) as f32 * SMALL_SHIFT
}

/// Horizontal layout of the expanded cells inside a strip `width` wide:
/// (left edge of the first cell's slot, distance between cells). Few cells
/// spread evenly across the width like tdesktop's `skipSide`/`skipBetween`;
/// many cells pack at the natural pitch and scroll sideways.
pub fn expanded_layout(width: f32, count: usize) -> (f32, f32) {
    if count == 0 {
        return (FULL_LEFT, CELL_WIDTH);
    }
    let total = FULL_LEFT + CELL_WIDTH * count as f32;
    if total >= width {
        return (FULL_LEFT, CELL_WIDTH);
    }
    let skip_side = (width - total) / (count as f32 + 1.0);
    let skip_between = if count > 1 {
        (width - total - 2.0 * skip_side) / (count as f32 - 1.0)
    } else {
        skip_side
    };
    (FULL_LEFT + skip_side, CELL_WIDTH + skip_between)
}

/// Largest sideways scroll offset of the expanded cells
/// (`updateScrollMax`).
pub fn max_scroll(width: f32, count: usize) -> f32 {
    (FULL_LEFT + CELL_WIDTH * count as f32 - width).max(0.0)
}

/// Axis of an in-progress wheel gesture over the strip. The strip scrolls
/// sideways itself and hands vertical scrolling to the chat list
/// (`List::verticalScrollEvents`), so one gesture must not do both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Axis {
    #[default]
    Undecided,
    Horizontal,
    Vertical,
}

/// Locks the first dominant axis of a precise scroll until the gesture
/// ends (like GPUI's own `OngoingScroll` filter and tdesktop's swipe
/// orientation lock).
#[derive(Debug, Default)]
pub struct AxisLock {
    axis: Axis,
    sum: (f32, f32),
}

/// Travel (px) before an axis is decided.
const AXIS_DECIDE_PX: f32 = 2.0;

impl AxisLock {
    /// Feed one scroll delta; `restart` is true for a new gesture. A line
    /// (mouse wheel) delta is `precise = false` and decides at once.
    pub fn feed(&mut self, restart: bool, precise: bool, dx: f32, dy: f32) -> Axis {
        if restart {
            *self = Self::default();
        }
        if self.axis != Axis::Undecided {
            return self.axis;
        }
        self.sum.0 += dx;
        self.sum.1 += dy;
        let (ax, ay) = (self.sum.0.abs(), self.sum.1.abs());
        let decide = if precise { AXIS_DECIDE_PX } else { 0.0 };
        if ax.max(ay) > decide || (!precise && (dx != 0.0 || dy != 0.0)) {
            self.axis = if ax > ay {
                Axis::Horizontal
            } else {
                Axis::Vertical
            };
        }
        self.axis
    }

    /// The gesture ended.
    pub fn end(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Axis, AxisLock, CELL_WIDTH, FULL_HEIGHT, FULL_LEFT, SMALL_PHOTO, SMALL_SHIFT,
        collapse_progress, compact_count, compact_opacity, compact_width, expand_ratio,
        expanded_layout, max_scroll,
    };

    #[test]
    fn progress_follows_the_scroll_offset() {
        assert_eq!(collapse_progress(0.0), 0.0);
        assert_eq!(collapse_progress(-30.0), 0.0, "rubber-band counts as top");
        assert!((collapse_progress(FULL_HEIGHT / 2.0) - 0.5).abs() < 1e-6);
        assert_eq!(collapse_progress(FULL_HEIGHT), 1.0);
        assert_eq!(collapse_progress(5000.0), 1.0);
        // Pure function of the offset: going back up retraces exactly.
        let down = collapse_progress(40.0);
        let _ = collapse_progress(300.0);
        assert_eq!(collapse_progress(40.0), down);
    }

    #[test]
    fn expand_ratio_matches_tdesktop_thresholds() {
        let low = 0.72 * 0.15;
        assert_eq!(expand_ratio(0.0), 0.0);
        assert_eq!(expand_ratio(low), 0.0);
        assert_eq!(expand_ratio(0.68), 1.0);
        assert_eq!(expand_ratio(1.0), 1.0);
        let mid = (low + 0.68) / 2.0;
        assert!((expand_ratio(mid) - 0.5).abs() < 1e-5);
    }

    #[test]
    fn compact_stack_fades_in_as_the_strip_leaves() {
        // Fully expanded and mostly expanded: no stack.
        assert_eq!(compact_opacity(0.0), 0.0);
        assert_eq!(compact_opacity(0.3), 0.0);
        // Fully collapsed: the stack is opaque.
        assert!((compact_opacity(1.0) - 1.0).abs() < 1e-6);
        // Monotonic in between.
        let mut last = 0.0;
        for step in 0..=100 {
            let o = compact_opacity(step as f32 / 100.0);
            assert!(o >= last - 1e-6);
            last = o;
        }
    }

    #[test]
    fn compact_stack_geometry() {
        assert_eq!(compact_count(0), 0);
        assert_eq!(compact_count(2), 2);
        assert_eq!(compact_count(40), 3);
        assert_eq!(compact_width(0), 0.0);
        assert_eq!(compact_width(1), SMALL_PHOTO);
        assert_eq!(compact_width(3), SMALL_PHOTO + 2.0 * SMALL_SHIFT);
    }

    #[test]
    fn few_cells_spread_many_scroll() {
        // 2 cells in 280 px spread evenly, like tdesktop's skipSide.
        let (first, pitch) = expanded_layout(280.0, 2);
        assert!(first > FULL_LEFT && pitch > CELL_WIDTH);
        let right_gap = 280.0 - (first + pitch + CELL_WIDTH);
        assert!((right_gap - (first - FULL_LEFT)).abs() < 1e-3, "balanced");
        assert_eq!(max_scroll(280.0, 2), 0.0);
        // 10 cells overflow: natural pitch, scrollable by the overflow.
        let (first, pitch) = expanded_layout(280.0, 10);
        assert_eq!((first, pitch), (FULL_LEFT, CELL_WIDTH));
        assert!((max_scroll(280.0, 10) - (FULL_LEFT + 620.0 - 280.0)).abs() < 1e-3);
        assert_eq!(expanded_layout(280.0, 0), (FULL_LEFT, CELL_WIDTH));
    }

    #[test]
    fn axis_lock_holds_until_the_gesture_ends() {
        let mut lock = AxisLock::default();
        assert_eq!(lock.feed(true, true, 0.5, 0.5), Axis::Undecided);
        assert_eq!(lock.feed(false, true, 6.0, 1.0), Axis::Horizontal);
        // A later vertical burst does not flip a locked gesture.
        assert_eq!(lock.feed(false, true, 0.0, 90.0), Axis::Horizontal);
        lock.end();
        assert_eq!(lock.feed(true, true, 0.0, 9.0), Axis::Vertical);
        // A mouse wheel decides immediately.
        let mut lock = AxisLock::default();
        assert_eq!(lock.feed(true, false, 0.0, 3.0), Axis::Vertical);
        let mut lock = AxisLock::default();
        assert_eq!(lock.feed(true, false, 3.0, 0.0), Axis::Horizontal);
    }
}
