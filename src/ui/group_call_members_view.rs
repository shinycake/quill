//! The voice chat's member rows as a view of their own, as tdesktop keeps
//! them in a `PeerListContent` apart from the video `Viewport`. A new video
//! frame redraws the panel and its tiles while the rows replay, and only
//! the rows in the list's scrolled window are built, however many people
//! are in the call.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::frame_clock::{follow_app, trace_slice_render};
use gpui_kit::*;
use std::ops::Range;
use std::time::Instant;

/// One member row.
pub(super) const ROW_HEIGHT: f32 = 56.;

/// Rows built past each end of the window, so a wheel notch shows no gap.
const OVERSCAN_ROWS: usize = 2;

/// Rows built while the list has not been laid out yet.
const UNMEASURED_ROWS: usize = 16;

/// The rows to build of a list of `count` that starts `top` pixels down a
/// scrolled area, seen through a `viewport` tall window with `scrolled`
/// pixels of the area above it.
fn visible_rows(scrolled: f32, viewport: f32, top: f32, count: usize) -> Range<usize> {
    let first = ((scrolled - top) / ROW_HEIGHT).floor().max(0.) as usize;
    let end = ((scrolled + viewport - top) / ROW_HEIGHT).ceil().max(0.) as usize;
    let first = first.saturating_sub(OVERSCAN_ROWS).min(count);
    first..end.saturating_add(OVERSCAN_ROWS).min(count)
}

/// Where the rows sit in the scrolled area: the child of the area that
/// holds them, and how far below that child's top they start.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct RowsPlace {
    pub(super) card: usize,
    pub(super) head: f32,
}

/// The cached view of the member rows.
pub(super) struct GroupCallMembers {
    owner: WeakEntity<QuillApp>,
    _observe: Subscription,
}

impl GroupCallMembers {
    pub(super) fn new(owner: &Entity<QuillApp>, cx: &mut Context<Self>) -> Self {
        Self {
            owner: owner.downgrade(),
            _observe: follow_app(owner, cx),
        }
    }
}

impl Render for GroupCallMembers {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        trace_slice_render("group-call-members");
        let view = cx.entity_id();
        let Some(owner) = self.owner.upgrade() else {
            return Empty.into_any_element();
        };
        owner.update(cx, |app, cx| app.group_call_member_rows(view, window, cx))
    }
}

impl QuillApp {
    /// Which of `count` rows the scrolled list shows now. A cached view
    /// renders during prepaint, after the area's own layout, so the
    /// numbers are this frame's.
    fn shown_member_rows(&self, count: usize) -> Range<usize> {
        let scroll = &self.group_call.members_scroll;
        let place = self.group_call.rows_place;
        let Some(card) = scroll.bounds_for_item(place.card) else {
            return 0..count.min(UNMEASURED_ROWS);
        };
        let viewport = scroll.bounds();
        visible_rows(
            -f32::from(scroll.offset().y),
            f32::from(viewport.size.height),
            f32::from(card.top() - viewport.top()) + place.head,
            count,
        )
    }

    fn group_call_member_rows(
        &mut self,
        view: EntityId,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(call) = self
            .session()
            .and_then(|s| s.calls.active_group_call.clone())
        else {
            return Empty.into_any_element();
        };
        let (_, self_speaking) = self.group_call_self_level();
        let now = Instant::now();
        let shown = self.shown_member_rows(call.participants.len());
        let rows: Vec<AnyElement> = call
            .participants
            .iter()
            .skip(shown.start)
            .take(shown.len())
            .map(|participant| self.group_member_row(&call, participant, self_speaking, now, cx))
            .collect();
        self.tick_group_member_speakers(view, now, window, cx);
        div()
            .size_full()
            .pt(px(shown.start as f32 * ROW_HEIGHT))
            .children(rows)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{OVERSCAN_ROWS, ROW_HEIGHT, visible_rows};

    #[test]
    fn the_window_at_the_top_builds_what_fits_and_the_overscan() {
        let rows = visible_rows(0., ROW_HEIGHT * 5., 0., 100);
        assert_eq!(rows, 0..5 + OVERSCAN_ROWS);
    }

    #[test]
    fn scrolling_moves_the_window_with_the_overscan_at_both_ends() {
        let rows = visible_rows(ROW_HEIGHT * 20., ROW_HEIGHT * 5., 0., 100);
        assert_eq!(rows, 20 - OVERSCAN_ROWS..25 + OVERSCAN_ROWS);
    }

    #[test]
    fn a_row_scrolled_part_way_still_counts() {
        let rows = visible_rows(ROW_HEIGHT * 20.5, ROW_HEIGHT * 5., 0., 100);
        assert_eq!(rows, 20 - OVERSCAN_ROWS..26 + OVERSCAN_ROWS);
    }

    #[test]
    fn tiles_above_the_list_push_the_window_down_the_list() {
        let top = ROW_HEIGHT * 3.;
        assert_eq!(visible_rows(0., ROW_HEIGHT * 5., top, 100), 0..2 + OVERSCAN_ROWS);
        let scrolled = visible_rows(ROW_HEIGHT * 6., ROW_HEIGHT * 5., top, 100);
        assert_eq!(scrolled, 3 - OVERSCAN_ROWS..8 + OVERSCAN_ROWS);
    }

    #[test]
    fn the_list_above_the_window_builds_only_the_overscan_end() {
        let rows = visible_rows(ROW_HEIGHT * 200., ROW_HEIGHT * 5., 0., 100);
        assert!(rows.is_empty());
    }

    #[test]
    fn a_short_list_builds_all_of_it() {
        assert_eq!(visible_rows(0., 1_000., 0., 3), 0..3);
        assert_eq!(visible_rows(0., 1_000., 0., 0), 0..0);
    }
}
