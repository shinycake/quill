//! Where the open history is brought: the "Unread messages" bar at the top
//! of the view (tdesktop `HistoryWidget::unreadBarTop`), the bottom without
//! a bar when the unread run fits on the last screen
//! (`countAutomaticScrollTop`), and a jumped-to message in the middle of the
//! view (`itemTopForHighlight`).
//!
//! The first scroll puts the target's row at the top. Each paint measures
//! how far the target is from its place and scrolls by the difference.
//! While the bar can't reach the top because the loaded window stops short
//! of the chat's latest message, the landing waits for the next page
//! instead of deciding on part of the run. The reader's own scrolling,
//! clicking or keys end it.

use super::*;
use crate::ui::history_state::ScrollAim;

/// What the open history is brought to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) enum LandingGoal {
    /// The "Unread messages" bar at the view's top.
    Bar,
    /// The row holding this message in the middle of the view, or at its
    /// top when the row is taller than the view.
    Message(MessageId),
}

/// A landing in progress.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) struct Landing {
    pub(in crate::ui) goal: LandingGoal,
    /// Corrections scrolled so far.
    corrections: u8,
    /// Paints that did not draw the target.
    misses: u8,
}

/// What a paint measured: the painted target's distance from its place
/// (positive: below it), `None` when the target was not drawn.
pub(in crate::ui) type LandingError = Option<f32>;

/// Where the paint finds the target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) enum LandingProbe {
    Bar,
    Row(usize),
    /// The target's row is not in the rows.
    Gone,
}

/// The bar's offset into its row last found, without and with a day
/// separator above it, so the next open lands right away.
#[derive(Clone, Copy, Debug, Default)]
pub(in crate::ui) struct BarOffsets {
    plain: Option<f32>,
    under_day: Option<f32>,
}

impl BarOffsets {
    fn get(self, has_day: bool) -> Option<f32> {
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

/// Corrections that move the view before a landing gives up (rows that
/// keep changing height under it).
const MAX_CORRECTIONS: u8 = 4;
/// Paints without the target before a landing gives up (its row went).
const MAX_MISSES: u8 = 3;

/// The painted target's distance from its place.
pub(in crate::ui) fn landing_error(
    probe: LandingProbe,
    bar: Option<Bounds<Pixels>>,
    rows: &[super::history_fx::ScrollProbeRow],
    viewport: Bounds<Pixels>,
) -> LandingError {
    match probe {
        LandingProbe::Bar => bar.map(|bar| f32::from(bar.top() - viewport.top())),
        LandingProbe::Row(target) => {
            rows.iter()
                .find(|(ix, _, _)| *ix == target)
                .map(|(_, row, _)| {
                    let room = (viewport.size.height - row.size.height).max(px(0.));
                    f32::from(row.top() - (viewport.top() + room / 2.))
                })
        }
        LandingProbe::Gone => None,
    }
}

impl QuillApp {
    /// Bring the open history to `goal`: scroll its row to the top now,
    /// and correct after each paint. `false` when the rows don't hold it.
    pub(in crate::ui) fn start_landing(
        &mut self,
        goal: LandingGoal,
        cx: &mut Context<Self>,
    ) -> bool {
        let rows = &self.history.rows;
        let Some(ix) = (match goal {
            LandingGoal::Bar => rows.iter().position(HistoryRow::unread_divider),
            LandingGoal::Message(id) => rows.iter().position(|row| row.contains(id)),
        }) else {
            return false;
        };
        let offset = match goal {
            LandingGoal::Bar => {
                let has_day = rows.get(ix).is_some_and(|row| row.day_label().is_some());
                self.history.bar_offsets.get(has_day).unwrap_or(0.)
            }
            LandingGoal::Message(_) => 0.,
        };
        self.history.aim = ScrollAim::Landing(Landing {
            goal,
            corrections: 0,
            misses: 0,
        });
        self.history.scroller.update(cx, |state, cx| {
            state.scroll_to_item_offset(ix, offset, cx);
        });
        true
    }

    /// The reader took over the view.
    pub(in crate::ui) fn end_landing(&mut self) {
        self.history.aim.end_landing();
    }

    /// Ends a landing when the reader scrolls over the history with a wheel
    /// or trackpad. Painted before the list: the list's bounce takes scroll
    /// events in the capture phase and stops them there.
    pub(super) fn landing_scroll_watch(&self, cx: &mut Context<Self>) -> AnyElement {
        let app = cx.weak_entity();
        canvas(
            |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
            move |_, hitbox, window, _| {
                let app = app.clone();
                window.on_mouse_event(move |_: &ScrollEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture && hitbox.should_handle_scroll(window) {
                        let _ = app.update(cx, |this, _| this.end_landing());
                    }
                });
            },
        )
        .absolute()
        .inset_0()
        .size_full()
        .into_any_element()
    }

    /// What the next paint measures for the landing, if one is running.
    pub(in crate::ui) fn landing_probe(&self) -> Option<LandingProbe> {
        match self.history.aim.landing()?.goal {
            LandingGoal::Bar => Some(LandingProbe::Bar),
            LandingGoal::Message(id) => Some(
                self.history
                    .rows
                    .iter()
                    .position(|row| row.contains(id))
                    .map_or(LandingProbe::Gone, LandingProbe::Row),
            ),
        }
    }

    /// Act on what the last paint measured: there (done), short of an end
    /// of the list (correct), or held by the end. The bar held by the end
    /// waits while newer messages are still to load, and otherwise gives
    /// way to the bottom: the unread run fits on the last screen.
    pub(in crate::ui) fn advance_landing(&mut self, error: LandingError, cx: &mut Context<Self>) {
        let Some(mut landing) = self.history.aim.landing() else {
            return;
        };
        let Some(error) = error else {
            landing.misses = landing.misses.saturating_add(1);
            self.history.aim = if landing.misses < MAX_MISSES {
                ScrollAim::Landing(landing)
            } else {
                ScrollAim::Free
            };
            return;
        };
        if error.abs() < 0.5 {
            self.end_landing();
            if landing.goal == LandingGoal::Bar {
                self.learn_bar_offset(cx);
            }
            return;
        }
        let (above, below) = self
            .history
            .scroller
            .read(cx)
            .distance_to_edges()
            .map_or((px(0.), px(0.)), |(above, below, _)| (above, below));
        if error > 0. && below < px(1.) {
            match landing.goal {
                // Rows still to come may bring the bar up.
                LandingGoal::Bar if self.history.had_newer => {}
                LandingGoal::Bar => {
                    self.end_landing();
                    self.drop_unread_bar_for_bottom(cx);
                }
                LandingGoal::Message(_) => self.end_landing(),
            }
            return;
        }
        if (error < 0. && above < px(1.)) || landing.corrections >= MAX_CORRECTIONS {
            self.end_landing();
            return;
        }
        landing.corrections += 1;
        self.history.aim = ScrollAim::Landing(landing);
        self.history
            .scroller
            .update(cx, |state, cx| state.scroll_by(px(error), cx));
        cx.notify();
    }

    /// Keep where the bar sat in its row, so the next open lands there at
    /// once.
    fn learn_bar_offset(&mut self, cx: &App) {
        let top = self.history.scroller.read(cx).logical_scroll_top();
        if let Some(row) = self.history.rows.get(top.item_ix)
            && row.unread_divider()
        {
            let has_day = row.day_label().is_some();
            self.history
                .bar_offsets
                .set(has_day, f32::from(top.offset_in_item));
        }
    }

    /// tdesktop creates no bar for an unread run that fits on the last
    /// screen: the history opens at the bottom. Only the bar goes; reads
    /// stay held as before (`opened_with_unread`).
    fn drop_unread_bar_for_bottom(&mut self, cx: &mut Context<Self>) {
        if let Some(chat) = self.viewed_chat()
            && let Some(session) = self.session_mut()
        {
            session.hide_unread_bar(chat);
        }
        self.history
            .scroller
            .update(cx, |state, cx| state.scroll_to_end(cx));
        cx.notify();
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "landing_tests.rs"]
mod tests;
