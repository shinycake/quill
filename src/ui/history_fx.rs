//! Telegram Desktop history-view polish: the floating date pill that shows
//! while scrolling, the jump-target highlight fade, and the reveal of new
//! messages at the bottom (`motion.rs` holds the timing).
//!
//! Both are driven by stored `Instant`s and the shared frame clock
//! (`QuillApp::request_animation_tick`) only while they are visible — never
//! GPUI `with_animation`, which would redraw the app at display rate.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::motion::MotionState;
use gpui_kit::*;
use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Whether the older and the newer page are due, `above` and `below` being
/// how far the viewport is from the loaded window's ends.
pub(super) fn preload_due(above: Pixels, below: Pixels, viewport: Pixels) -> (bool, bool) {
    let lead = viewport * PRELOAD_SCREENS;
    (above <= lead, below <= lead)
}

/// A history row painted last frame: `(row, bounds, starts its day)`.
pub(crate) type ScrollProbeRow = (usize, Bounds<Pixels>, bool);

/// `historyScrollDateHideTimeout`.
const DATE_HIDE_AFTER: Duration = Duration::from_millis(1000);
/// tdesktop keeps the pill longer right after the day changed.
const DATE_HIDE_AFTER_DAY_CHANGE: Duration = Duration::from_millis(3000);
/// `historyDateFadeDuration`.
const DATE_FADE: Duration = Duration::from_millis(200);
/// tdesktop `kPreloadHeightsCount`: the next page is asked for while this
/// many screens are still left toward that end.
const PRELOAD_SCREENS: f32 = 3.;

/// `activeFadeInDuration` / `activeFadeOutDuration` (lib_ui basic.style).
const JUMP_FADE_IN: Duration = Duration::from_millis(500);
const JUMP_FADE_OUT: Duration = Duration::from_millis(3000);
/// Strongest tint of the jump highlight (alpha of the primary color).
const JUMP_PEAK_ALPHA: f32 = 0.2;

/// Opacity factor (0..=1) of the jump highlight `elapsed` after the jump.
pub(crate) fn jump_fade(elapsed: Duration) -> f32 {
    if elapsed < JUMP_FADE_IN {
        elapsed.as_secs_f32() / JUMP_FADE_IN.as_secs_f32()
    } else {
        let out = elapsed.saturating_sub(JUMP_FADE_IN).as_secs_f32() / JUMP_FADE_OUT.as_secs_f32();
        (1. - out).max(0.)
    }
}

/// Whether the highlight still needs frames.
pub(crate) fn jump_fade_active(elapsed: Duration) -> bool {
    elapsed < JUMP_FADE_IN + JUMP_FADE_OUT
}

/// How long a reaction takes to fly up from its message.
pub(crate) const REACTION_FLY_DURATION: Duration = Duration::from_millis(520);

/// A reaction just added to a message (the fly animation).
#[derive(Clone, Debug)]
pub(crate) struct ReactionFly {
    pub chat_id: i64,
    pub message_id: i64,
    pub glyph: SharedString,
    pub started: Instant,
}

/// One frame of the fly: glyph scale, rise in px and opacity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ReactionFlyFrame {
    pub scale: f32,
    pub rise: f32,
    pub alpha: f32,
}

/// The fly at `elapsed` (tdesktop `ReactionFlyAnimation`: the reaction
/// pops out bigger, drifts up and fades). `None` once finished, so the
/// frame clock stops. Reduced motion skips it entirely.
pub(crate) fn reaction_fly_frame(elapsed: Duration, reduced: bool) -> Option<ReactionFlyFrame> {
    if reduced || elapsed >= REACTION_FLY_DURATION {
        return None;
    }
    let t = elapsed.as_secs_f32() / REACTION_FLY_DURATION.as_secs_f32();
    let pop = super::motion::ease_out_circ((t / 0.35).min(1.));
    let fade = ((1. - t) / 0.45).clamp(0., 1.);
    Some(ReactionFlyFrame {
        scale: 0.6 + 0.9 * pop,
        rise: 56. * super::motion::ease_out_circ(t),
        alpha: fade,
    })
}

/// Reads the painted height of freshly arrived rows for the reveal.
pub(crate) struct RevealProbe {
    first_row: Option<usize>,
    measure: Rc<Cell<f32>>,
    fills: Rc<Cell<bool>>,
}

impl RevealProbe {
    /// Rows painted this frame: when every row from the first arrival to
    /// the last (`count` rows in all) is among them, their summed height is
    /// what the reveal has to hide (plus the 4 px gap under each).
    pub(crate) fn note(&self, rows: &[ScrollProbeRow], count: usize, viewport_top: Pixels) {
        // Row 0 sitting at the top edge: the list is short (or scrolled to
        // its start), so nothing is pinned to the bottom to reveal against.
        if !rows.is_empty() {
            self.fills.set(
                !rows
                    .iter()
                    .any(|(ix, bounds, _)| *ix == 0 && bounds.top() >= viewport_top),
            );
        }
        let Some(first) = self.first_row else {
            return;
        };
        let (mut seen, mut height) = (0, 0.);
        for (ix, bounds, _) in rows {
            if *ix >= first && *ix < count {
                seen += 1;
                height += f32::from(bounds.size.height) + 4.;
            }
        }
        if seen > 0 && seen == count.saturating_sub(first) {
            self.measure.set(height);
        }
    }
}

impl MotionState {
    pub(crate) fn reveal_probe(&self) -> RevealProbe {
        RevealProbe {
            first_row: self.reveal.borrow().map(|r| r.first_row),
            measure: self.reveal_measure.clone(),
            fills: self.list_fills.clone(),
        }
    }
}

/// The history viewport during a reveal. `reveal` is `(shift, extra)` from
/// `MotionState::reveal_now`: the list is laid out `extra` px taller than
/// the window and slid down by `shift`, so the new rows rise out of the
/// bottom edge while the older ones glide up. At rest it is a plain filler.
pub(crate) fn reveal_viewport(
    reveal: Option<(f32, f32)>,
    scroller: impl IntoElement,
) -> impl IntoElement {
    let (shift, extra) = reveal.unwrap_or_default();
    div().relative().flex_1().min_h_0().overflow_hidden().child(
        div()
            .absolute()
            .left_0()
            .right_0()
            .top(px(shift - extra))
            .bottom(px(-shift))
            .child(scroller),
    )
}

/// What the floating date pill shows, and until when.
#[derive(Default)]
pub(crate) struct ScrollDate {
    shown_at: Option<Instant>,
    last_scroll: Option<Instant>,
    hide_after: Duration,
    /// Day of the row at the top of the viewport (None: unknown).
    pub(crate) top_label: Option<String>,
    /// The top row starts its day, so an inline separator is on screen.
    pub(crate) separator_at_top: bool,
}

impl ScrollDate {
    /// The list scrolled at `now`.
    pub(crate) fn scrolled(&mut self, now: Instant, day_changed: bool) {
        if self.opacity(now) <= 0. {
            self.shown_at = Some(now);
            self.hide_after = DATE_HIDE_AFTER;
        }
        if day_changed {
            self.hide_after = DATE_HIDE_AFTER_DAY_CHANGE;
        }
        self.last_scroll = Some(now);
    }

    /// The day under the top edge changed while the pill is up.
    pub(crate) fn hold_for_day_change(&mut self) {
        if self.last_scroll.is_some() {
            self.hide_after = DATE_HIDE_AFTER_DAY_CHANGE;
        }
    }

    /// 0 while hidden, fading in and out around the visible stretch.
    pub(crate) fn opacity(&self, now: Instant) -> f32 {
        let (Some(shown), Some(last)) = (self.shown_at, self.last_scroll) else {
            return 0.;
        };
        let remaining = (last + self.hide_after).saturating_duration_since(now);
        let fade_in =
            (now.saturating_duration_since(shown).as_secs_f32() / DATE_FADE.as_secs_f32()).min(1.);
        let fade_out = (remaining.as_secs_f32() / DATE_FADE.as_secs_f32()).min(1.);
        fade_in.min(fade_out)
    }
}

impl QuillApp {
    /// Highlight tint for a jump target at render time: starts the fade on
    /// a new jump, asks for frames while it runs. `None` once finished.
    pub(super) fn jump_highlight_alpha(&self, cx: &mut Context<Self>) -> Option<f32> {
        let (_, started) = self.history.highlight_fade?;
        let elapsed = started.elapsed();
        if !jump_fade_active(elapsed) {
            return None;
        }
        self.request_animation_tick(30, cx);
        Some(jump_fade(elapsed) * JUMP_PEAK_ALPHA)
    }

    /// The fly of a reaction just added to this message, if it runs
    /// (asks for frames only while it does; none for reduced motion).
    pub(super) fn reaction_fly_for(
        &self,
        chat_id: i64,
        message_id: i64,
        cx: &mut Context<Self>,
    ) -> Option<(SharedString, ReactionFlyFrame)> {
        let fly = self
            .history
            .reaction_fly
            .as_ref()
            .filter(|fly| fly.chat_id == chat_id && fly.message_id == message_id)?;
        let elapsed = super::motion::elapsed_since(fly.started, Instant::now());
        let frame = reaction_fly_frame(elapsed, cx.reduce_motion())?;
        self.request_animation_tick(60, cx);
        Some((fly.glyph.clone(), frame))
    }

    /// The reveal of new bottom rows at render time: `(shift, extra)` while
    /// it runs (asking for frames), `None` once settled, when the user
    /// scrolled away, or when the window is inactive.
    pub(super) fn history_reveal(&self, cx: &mut Context<Self>) -> Option<(f32, f32)> {
        let list = self.history.scroller.read(cx);
        // (A list with no rows yet has not anchored: not "scrolled away".)
        let scrolled_away = list.item_count() > 0 && !list.is_following_tail();
        if (!self.frame.window_active.get() && !super::motion::held()) || scrolled_away {
            self.frame.motion.cancel_reveal();
            return None;
        }
        let reveal = self.frame.motion.reveal_now(Instant::now())?;
        self.request_animation_tick(60, cx);
        Some(reveal)
    }

    /// The main window's history list scrolled (or its rows moved): refresh
    /// the pill. Observed on that list's scroller, so it runs as the main
    /// window whatever state is in place.
    pub(super) fn note_history_scroll(&mut self, cx: &mut Context<Self>) {
        self.in_window_scope(None, cx, |this, cx| {
            // Record where the reader is now: the saved position follows
            // every scroll, not only the next full render.
            if let Some(key) = this.history.key {
                this.remember_scroll(key, cx);
            }
            this.preload_history_by_scroll(cx);
            // Pinned to the live edge, rows only move because messages arrive.
            if this.history.scroller.read(cx).is_following_tail() {
                return;
            }
            this.history.scroll_date.scrolled(Instant::now(), false);
            // The pill lives in the conversation: the chat list can replay.
            this.notify_conversation(cx);
        });
    }

    /// tdesktop `HistoryWidget::preloadHistoryByScroll`: ask for the older
    /// or newer page while three screens are still left toward that end, so
    /// a scroll rarely reaches the edge of the loaded window and waits there.
    fn preload_history_by_scroll(&mut self, cx: &mut Context<Self>) {
        let Some((above, below, viewport)) = self.history.scroller.read(cx).distance_to_edges()
        else {
            return;
        };
        let (older, newer) = preload_due(above, below, viewport);
        if older {
            self.maybe_auto_load_older(cx);
        }
        if newer {
            self.maybe_auto_load_newer(cx);
        }
    }

    /// Resolve the probed top row to its day; a changed day while the pill
    /// is up holds it longer.
    fn refresh_scroll_date_top(&mut self) {
        let Some((ix, separator)) = self.history.scroll_top_probe.get() else {
            return;
        };
        let Some(last) = self.history.rows.len().checked_sub(1) else {
            return;
        };
        let Some(rows) = self.history.rows.get(..=ix.min(last)) else {
            return;
        };
        let label = rows
            .iter()
            .rev()
            .find_map(|row| row.day_label().map(str::to_string));
        if self.history.scroll_date.top_label.is_some()
            && self.history.scroll_date.top_label != label
        {
            self.history.scroll_date.hold_for_day_change();
        }
        self.history.scroll_date.top_label = label;
        self.history.scroll_date.separator_at_top = separator;
    }

    /// The floating pill, absolutely positioned at the top center.
    pub(super) fn scroll_date_pill(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.refresh_scroll_date_top();
        let alpha = self.history.scroll_date.opacity(Instant::now());
        if alpha <= 0. {
            return None;
        }
        self.request_animation_tick(30, cx);
        let label = self.history.scroll_date.top_label.clone()?;
        if self.history.scroll_date.separator_at_top {
            return None;
        }
        // The wide layout centres it in its column (`PaintDate` with
        // `chatWide`).
        let column = div().absolute().top_2().left_0();
        let column = if self.chat_wide() {
            column.w(px(quill::chat_wide::WIDE_CHAT_WIDTH))
        } else {
            column.right_0()
        };
        Some(
            column
                .flex()
                .justify_center()
                .opacity(alpha)
                .child(super::anim_layer::occluder(
                    super::conversation::date_pill_button(&label, cx),
                ))
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::ui::history_fx::{ScrollDate, jump_fade, jump_fade_active};
    use std::time::{Duration, Instant};

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn jump_fade_curve() {
        assert_eq!(jump_fade(ms(0)), 0.);
        assert!((jump_fade(ms(250)) - 0.5).abs() < 1e-4);
        assert!((jump_fade(ms(500)) - 1.).abs() < 1e-4);
        assert!((jump_fade(ms(2000)) - 0.5).abs() < 1e-4);
        assert_eq!(jump_fade(ms(3500)), 0.);
        assert_eq!(jump_fade(ms(9000)), 0.);
        assert!(jump_fade_active(ms(3499)));
        assert!(!jump_fade_active(ms(3500)));
    }

    #[test]
    fn date_pill_shows_then_hides_after_idle() {
        let t0 = Instant::now();
        let mut date = ScrollDate::default();
        assert_eq!(date.opacity(t0), 0.);
        date.scrolled(t0, false);
        assert!(date.opacity(t0) < 0.01);
        assert_eq!(date.opacity(t0 + ms(300)), 1.);
        assert_eq!(date.opacity(t0 + ms(700)), 1.);
        assert!(date.opacity(t0 + ms(900)) < 1.);
        assert_eq!(date.opacity(t0 + ms(1000)), 0.);
        // Continued scrolling keeps it up.
        date.scrolled(t0 + ms(800), false);
        assert_eq!(date.opacity(t0 + ms(1500)), 1.);
    }

    #[test]
    fn day_change_extends_the_timeout() {
        let t0 = Instant::now();
        let mut date = ScrollDate::default();
        date.scrolled(t0, true);
        assert_eq!(date.opacity(t0 + ms(2500)), 1.);
        assert_eq!(date.opacity(t0 + ms(3000)), 0.);
    }
}

#[cfg(test)]
mod fly_tests {
    use crate::ui::history_fx::{REACTION_FLY_DURATION, reaction_fly_frame};
    use std::time::Duration;

    #[test]
    fn reaction_fly_rises_fades_and_ends() {
        let start = reaction_fly_frame(Duration::ZERO, false).unwrap();
        assert!(start.scale < 1. && start.alpha >= 1. - 1e-4 && start.rise == 0.);
        let mid = reaction_fly_frame(Duration::from_millis(200), false).unwrap();
        assert!(mid.scale > 1. && mid.rise > start.rise);
        let late = reaction_fly_frame(Duration::from_millis(500), false).unwrap();
        assert!(late.alpha < mid.alpha);
        assert_eq!(reaction_fly_frame(REACTION_FLY_DURATION, false), None);
    }

    #[test]
    fn reaction_fly_is_skipped_for_reduced_motion() {
        assert_eq!(reaction_fly_frame(Duration::ZERO, true), None);
    }
}
