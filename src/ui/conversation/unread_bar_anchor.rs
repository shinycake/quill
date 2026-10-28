//! An opened history lands with the "Unread messages" bar at the top of the
//! view, or at the bottom without a bar when the unread run fits on the
//! last screen (tdesktop `HistoryWidget::countInitialScrollTop` /
//! `countAutomaticScrollTop`, `unreadBarTop`).
//!
//! The bar sits inside its message's row, under the row's day separator,
//! so the first scroll puts the row at the top with the offset found last
//! time; the next paint measures how far the bar is from the top and
//! corrects the offset, or drops the bar when the list cannot scroll that
//! far.

use super::*;

/// The bar the opened history is landing on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) struct BarAnchor {
    /// The row the bar is drawn in.
    pub(in crate::ui) ix: usize,
    /// The offset into the row the last scroll used.
    applied: f32,
    /// How far below the view's top the bar was before the last
    /// correction.
    last_from_top: Option<f32>,
    /// Paints checked so far.
    paints: u8,
}

/// The bar's offset into its row last found, without and with a day
/// separator above it, so the next open lands right away.
#[derive(Clone, Copy, Debug, Default)]
pub(in crate::ui) struct BarOffsets {
    plain: Option<f32>,
    under_day: Option<f32>,
}

impl BarOffsets {
    pub(in crate::ui) fn get(self, has_day: bool) -> Option<f32> {
        if has_day { self.under_day } else { self.plain }
    }

    fn set(&mut self, has_day: bool, offset: f32) {
        *(if has_day {
            &mut self.under_day
        } else {
            &mut self.plain
        }) = Some(offset);
    }
}

/// What a paint found the history must do for its bar anchor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) enum BarFix {
    /// Scroll the bar's row to this offset.
    Offset(usize, f32),
    /// The bar stayed below the top: when the list stopped at its end
    /// there, open at the bottom without the bar.
    Lower,
}

/// A bar is checked for this many paints before the anchor gives up (its
/// row scrolled out of the overdraw, the reader scrolled away).
const MAX_PAINTS: u8 = 3;

impl QuillApp {
    /// Scroll the opened history to the bar in row `ix` and check it on the
    /// next paint.
    pub(in crate::ui) fn anchor_at_unread_bar(&mut self, ix: usize, cx: &mut Context<Self>) {
        let has_day = self
            .history
            .rows
            .get(ix)
            .is_some_and(|row| row.day_label().is_some());
        let applied = self.history.bar_offsets.get(has_day).unwrap_or(0.);
        self.history.bar_anchor.set(Some(BarAnchor {
            ix,
            applied,
            last_from_top: None,
            paints: 0,
        }));
        self.history.scroller.update(cx, |state, cx| {
            state.scroll_to_item_offset(ix, applied, cx);
        });
    }

    /// Apply what the paint found (`check_bar_anchor`).
    pub(in crate::ui) fn fix_bar_anchor(&mut self, fix: BarFix, cx: &mut Context<Self>) {
        match fix {
            BarFix::Offset(ix, offset) => {
                if let Some(has_day) = self.history.rows.get(ix).map(|r| r.day_label().is_some()) {
                    self.history.bar_offsets.set(has_day, offset);
                }
                self.history.scroller.update(cx, |state, cx| {
                    state.scroll_to_item_offset(ix, offset, cx);
                });
            }
            BarFix::Lower => {
                // The list's own top padding also keeps a first row low.
                let at_end = self
                    .history
                    .scroller
                    .read(cx)
                    .distance_to_edges()
                    .is_some_and(|(_, below, _)| below < px(1.));
                if !at_end {
                    return;
                }
                if let Some(chat) = self.viewed_chat() {
                    if let Some(live) = self.live.as_mut() {
                        live.driver.session.hide_unread_bar(chat);
                    } else if let Some(session) = self.demo_session.as_mut() {
                        session.hide_unread_bar(chat);
                    }
                }
                self.history
                    .scroller
                    .update(cx, |state, cx| state.scroll_to_end(cx));
            }
        }
        cx.notify();
    }
}

/// After a paint: compare where the anchored bar was painted (`bar`, its
/// bounds) with the viewport's top. Updates `anchor` and returns the fix to
/// apply, if any.
pub(in crate::ui) fn check_bar_anchor(
    anchor: &std::cell::Cell<Option<BarAnchor>>,
    bar: Option<Bounds<Pixels>>,
    viewport: Bounds<Pixels>,
) -> Option<BarFix> {
    let mut current = anchor.get()?;
    current.paints = current.paints.saturating_add(1);
    let Some(bar) = bar else {
        anchor.set((current.paints < MAX_PAINTS).then_some(current));
        return None;
    };
    let from_top = f32::from(bar.top() - viewport.top());
    if from_top.abs() < 0.5 {
        anchor.set(None);
        return None;
    }
    // The last correction moved the bar; one that didn't hit an end of
    // the list.
    let moved = current
        .last_from_top
        .is_none_or(|last| (last - from_top).abs() >= 0.5);
    let offset = (current.applied + from_top).max(0.);
    if moved && current.paints < MAX_PAINTS && offset != current.applied {
        current.applied = offset;
        current.last_from_top = Some(from_top);
        anchor.set(Some(current));
        return Some(BarFix::Offset(current.ix, offset));
    }
    anchor.set(None);
    (from_top > 0.).then_some(BarFix::Lower)
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "unread_bar_anchor_tests.rs"]
mod tests;
