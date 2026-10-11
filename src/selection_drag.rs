//! Drag selection over message rows, after Telegram Desktop's
//! `HistoryInner` (`mouseActionUpdate`, `applyDragSelection`) and
//! `Ui::DraggingScrollManager`: the pure rules, with no UI types.
//!
//! A press on a row that is not on text may become a drag. Once the pointer
//! has moved far enough, the rows between the press row and the row under
//! the pointer form a contiguous range. Whether the drag selects or
//! deselects is decided once, by the press row's state, and the range is
//! shown that way while the button is down and applied on release.

use crate::ids::MessageId;
use crate::selection_pin::MAX_SELECTED;

/// `QApplication::startDragDistance`: a press moving less than this (in
/// Manhattan distance) is still a click.
pub const START_DRAG_DISTANCE: f32 = 10.;

/// The edge autoscroll tick (`DraggingScrollManager::checkDeltaScroll`).
pub const AUTOSCROLL_TICK_MS: u64 = 15;

/// Most pixels one autoscroll tick moves (`kMaxScrollSpeed`).
pub const AUTOSCROLL_MAX_STEP: i32 = 37;

/// The rows a drag covers and what it does to them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragRange {
    /// The row the press started on.
    pub anchor: MessageId,
    /// The row under the pointer.
    pub to: MessageId,
    /// Selecting (the press row was not selected) or deselecting.
    pub selecting: bool,
}

impl DragRange {
    /// Whether `id` lies between the anchor and the pointer row. Message
    /// ids grow with time within a chat, so the loaded order is id order.
    pub fn covers(&self, id: MessageId) -> bool {
        let (lo, hi) = self.bounds();
        lo.0 <= id.0 && id.0 <= hi.0
    }

    /// The range as `(oldest, newest)`.
    pub fn bounds(&self) -> (MessageId, MessageId) {
        if self.anchor.0 <= self.to.0 {
            (self.anchor, self.to)
        } else {
            (self.to, self.anchor)
        }
    }

    /// How a row shows while the button is down: covered rows take the
    /// drag's state, the others keep theirs (`itemRenderSelection`).
    pub fn shown(&self, id: MessageId, selected: bool) -> bool {
        if self.covers(id) {
            self.selecting
        } else {
            selected
        }
    }
}

/// The selection after a drag is released (`applyDragSelection`), as
/// ascending ids. `ids` are the loaded selectable messages in history
/// order and `selected` the current selection.
///
/// Selecting adds the covered rows that are not selected yet, nearest to
/// the anchor first, until `limit` is reached. Deselecting removes every
/// covered row. tdesktop fills top to bottom and stops at the limit; adding
/// from the anchor outward keeps the part next to where the drag started.
pub fn apply_drag(
    ids: &[MessageId],
    selected: &[MessageId],
    range: &DragRange,
    limit: usize,
) -> Vec<MessageId> {
    let mut result: Vec<MessageId> = selected.to_vec();
    if range.selecting {
        let anchor_at = ids.iter().position(|id| *id == range.anchor);
        let mut covered: Vec<MessageId> = ids
            .iter()
            .copied()
            .filter(|id| range.covers(*id) && !selected.contains(id))
            .collect();
        if let Some(anchor_at) = anchor_at {
            covered.sort_by_key(|id| {
                ids.iter()
                    .position(|i| i == id)
                    .map_or(usize::MAX, |at| at.abs_diff(anchor_at))
            });
        }
        let room = limit.saturating_sub(result.len());
        result.extend(covered.into_iter().take(room));
    } else {
        result.retain(|id| !range.covers(*id));
    }
    result.sort_by_key(|id| id.0);
    result.dedup();
    result
}

/// Whether the pointer has moved far enough from the press for a drag.
pub fn moved_enough(press: (f32, f32), pointer: (f32, f32)) -> bool {
    (pointer.0 - press.0).abs() + (pointer.1 - press.1).abs() >= START_DRAG_DISTANCE
}

/// Where a history row was painted last: its index and vertical extent in
/// window pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RowBand {
    pub index: usize,
    pub top: f32,
    pub bottom: f32,
}

/// The row under `y`: the painted row containing it, else the nearest
/// painted row when `y` is above or below them all (the pointer left the
/// list while dragging), else the row whose gap `y` falls in (the one
/// above it). `None` without painted rows.
pub fn row_under(rows: &[RowBand], y: f32) -> Option<usize> {
    if rows.is_empty() {
        return None;
    }
    if let Some(row) = rows.iter().find(|row| row.top <= y && y < row.bottom) {
        return Some(row.index);
    }
    let first = rows.iter().min_by(|a, b| a.top.total_cmp(&b.top))?;
    let last = rows.iter().max_by(|a, b| a.bottom.total_cmp(&b.bottom))?;
    if y < first.top {
        return Some(first.index);
    }
    if y >= last.bottom {
        return Some(last.index);
    }
    // In the gap between two rows: the row just above it.
    rows.iter()
        .filter(|row| row.bottom <= y)
        .max_by(|a, b| a.bottom.total_cmp(&b.bottom))
        .map(|row| row.index)
}

/// How far the pointer is beyond the viewport: negative above `top`,
/// positive at or below `bottom`, zero inside
/// (`DraggingScrollManager::checkDeltaScroll(point, top, bottom)`).
pub fn edge_delta(y: f32, top: f32, bottom: f32) -> i32 {
    let diff = y - top;
    if diff < 0. {
        diff.floor() as i32
    } else if y >= bottom {
        (y - bottom).floor() as i32 + 1
    } else {
        0
    }
}

/// Pixels to scroll on one tick for `delta` (`scrollByTimer`): grows with
/// the distance, at least one pixel, at most [`AUTOSCROLL_MAX_STEP`].
pub fn autoscroll_step(delta: i32) -> i32 {
    if delta > 0 {
        (delta * 3 / 20 + 1).min(AUTOSCROLL_MAX_STEP)
    } else if delta < 0 {
        (delta * 3 / 20 - 1).max(-AUTOSCROLL_MAX_STEP)
    } else {
        0
    }
}

/// The selection limit every path shares (`Data::MaxSelectedItems`).
pub const LIMIT: usize = MAX_SELECTED;

#[cfg(test)]
mod tests {
    use super::{
        AUTOSCROLL_MAX_STEP, DragRange, LIMIT, RowBand, apply_drag, autoscroll_step, edge_delta,
        moved_enough, row_under,
    };
    use crate::ids::MessageId;

    fn ids() -> Vec<MessageId> {
        [10, 20, 30, 40, 50, 60].map(MessageId).to_vec()
    }

    #[test]
    fn range_covers_both_directions() {
        let down = DragRange {
            anchor: MessageId(20),
            to: MessageId(40),
            selecting: true,
        };
        let up = DragRange {
            anchor: MessageId(40),
            to: MessageId(20),
            selecting: true,
        };
        for range in [down, up] {
            assert_eq!(range.bounds(), (MessageId(20), MessageId(40)));
            assert!(range.covers(MessageId(20)));
            assert!(range.covers(MessageId(30)));
            assert!(range.covers(MessageId(40)));
            assert!(!range.covers(MessageId(10)));
            assert!(!range.covers(MessageId(50)));
        }
    }

    #[test]
    fn preview_shows_the_drag_state_only_inside_the_range() {
        let range = DragRange {
            anchor: MessageId(20),
            to: MessageId(30),
            selecting: false,
        };
        // Covered rows read as deselected, the rest keep their state.
        assert!(!range.shown(MessageId(20), true));
        assert!(range.shown(MessageId(50), true));
        assert!(!range.shown(MessageId(50), false));
        let selecting = DragRange {
            selecting: true,
            ..range
        };
        assert!(selecting.shown(MessageId(30), false));
    }

    #[test]
    fn selecting_drag_adds_the_covered_rows() {
        let range = DragRange {
            anchor: MessageId(50),
            to: MessageId(20),
            selecting: true,
        };
        let got = apply_drag(&ids(), &[MessageId(10)], &range, LIMIT);
        assert_eq!(
            got,
            [10, 20, 30, 40, 50].map(MessageId).to_vec(),
            "the earlier selection stays and the range joins it"
        );
    }

    #[test]
    fn deselecting_drag_removes_only_the_covered_rows() {
        let range = DragRange {
            anchor: MessageId(30),
            to: MessageId(40),
            selecting: false,
        };
        let selected = [10, 30, 40, 60].map(MessageId).to_vec();
        let got = apply_drag(&ids(), &selected, &range, LIMIT);
        assert_eq!(got, vec![MessageId(10), MessageId(60)]);
    }

    #[test]
    fn selecting_drag_stops_at_the_limit_nearest_the_anchor() {
        let range = DragRange {
            anchor: MessageId(60),
            to: MessageId(10),
            selecting: true,
        };
        // Room for two more: the two rows next to the press row.
        let got = apply_drag(&ids(), &[MessageId(60)], &range, 3);
        assert_eq!(got, [40, 50, 60].map(MessageId).to_vec());
        // Already full: nothing changes.
        let full: Vec<MessageId> = (0..LIMIT as i64).map(|n| MessageId(1000 + n)).collect();
        let got = apply_drag(&ids(), &full, &range, LIMIT);
        assert_eq!(got.len(), LIMIT);
    }

    #[test]
    fn drag_needs_the_start_distance() {
        assert!(!moved_enough((0., 0.), (4., 4.)));
        assert!(moved_enough((0., 0.), (5., 5.)));
        assert!(moved_enough((100., 100.), (100., 88.)));
    }

    fn bands() -> Vec<RowBand> {
        vec![
            RowBand {
                index: 3,
                top: 100.,
                bottom: 140.,
            },
            RowBand {
                index: 4,
                top: 144.,
                bottom: 200.,
            },
            RowBand {
                index: 5,
                top: 204.,
                bottom: 260.,
            },
        ]
    }

    #[test]
    fn hit_testing_finds_rows_gaps_and_edges() {
        let rows = bands();
        assert_eq!(row_under(&rows, 120.), Some(3));
        assert_eq!(row_under(&rows, 199.), Some(4));
        // The gap below row 4 belongs to row 4.
        assert_eq!(row_under(&rows, 202.), Some(4));
        // Above or below every painted row: the nearest edge row.
        assert_eq!(row_under(&rows, 10.), Some(3));
        assert_eq!(row_under(&rows, 900.), Some(5));
        assert_eq!(row_under(&[], 50.), None);
    }

    #[test]
    fn edge_delta_is_zero_inside_the_viewport() {
        assert_eq!(edge_delta(50., 0., 400.), 0);
        assert_eq!(edge_delta(0., 0., 400.), 0);
        assert_eq!(edge_delta(-12.5, 0., 400.), -13);
        assert_eq!(edge_delta(400., 0., 400.), 1);
        assert_eq!(edge_delta(430., 0., 400.), 31);
    }

    #[test]
    fn autoscroll_speed_matches_tdesktop() {
        assert_eq!(autoscroll_step(0), 0);
        // 37px per 15ms at most, at least a pixel once outside.
        assert_eq!(autoscroll_step(1), 1);
        assert_eq!(autoscroll_step(-1), -1);
        assert_eq!(autoscroll_step(100), 16);
        assert_eq!(autoscroll_step(-100), -16);
        assert_eq!(autoscroll_step(1000), AUTOSCROLL_MAX_STEP);
        assert_eq!(autoscroll_step(-1000), -AUTOSCROLL_MAX_STEP);
    }
}
