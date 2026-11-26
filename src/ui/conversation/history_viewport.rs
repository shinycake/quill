//! What of the history a reader sees: the list's bounds less what floats
//! over its rows (docs/decisions/floating-layout.md). In Classic nothing
//! floats, so the two rectangles are equal and every reader behaves as
//! it did.

use super::*;
use crate::ui::chrome::frame::HistoryInsets;

/// The history list at its last paint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) struct HistoryViewport {
    /// The list's bounds: pointer geometry, edge auto-scroll, the reveal.
    pub full: Bounds<Pixels>,
    /// The list's bounds less the pills over it: what a reader can see,
    /// for reads, landings, the date pill and paging.
    pub readable: Bounds<Pixels>,
}

impl HistoryViewport {
    pub(in crate::ui) fn new(full: Bounds<Pixels>, insets: HistoryInsets) -> Self {
        Self {
            full,
            readable: insets.inside(full),
        }
    }
}

impl QuillApp {
    /// The history's first and last rows carry the insets as padding:
    /// when the insets change (a bar appears, the composer grows), those
    /// two are measured again and the rest keep their heights.
    pub(super) fn note_history_insets(&mut self, insets: HistoryInsets, cx: &mut Context<Self>) {
        if self.history.insets_laid.replace(insets) == insets {
            return;
        }
        self.history.scroller.update(cx, |state, cx| {
            let count = state.item_count();
            let _ = state.remeasure_items(0..count.min(1), cx);
            let _ = state.remeasure_items(count.saturating_sub(1)..count, cx);
        });
    }

    /// Scroll so row `ix` starts where the reader's view starts: at the
    /// list's top, or under the header pill when one floats over it.
    pub(in crate::ui) fn scroll_row_to_view_top(&mut self, ix: usize, cx: &mut Context<Self>) {
        let top = self.history_insets().top;
        self.history.scroller.update(cx, |state, cx| {
            state.scroll_to_item(ix, cx);
            if top > px(0.) {
                state.scroll_by(-top, cx);
            }
        });
    }
}
