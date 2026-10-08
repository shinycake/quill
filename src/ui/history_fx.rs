//! Telegram Desktop history-view polish: the floating date pill that shows
//! while scrolling, and the jump-target highlight fade.
//!
//! Both are driven by stored `Instant`s and the shared frame clock
//! (`QuillApp::request_animation_tick`) only while they are visible — never
//! GPUI `with_animation`, which would redraw the app at display rate.

use super::app::QuillApp;
use gpui_kit::*;
use std::time::{Duration, Instant};

/// A history row painted last frame: `(row, bounds, starts its day)`.
pub(crate) type ScrollProbeRow = (usize, Bounds<Pixels>, bool);

/// `historyScrollDateHideTimeout`.
const DATE_HIDE_AFTER: Duration = Duration::from_millis(1000);
/// tdesktop keeps the pill longer right after the day changed.
const DATE_HIDE_AFTER_DAY_CHANGE: Duration = Duration::from_millis(3000);
/// `historyDateFadeDuration`.
const DATE_FADE: Duration = Duration::from_millis(200);

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
        let out = (elapsed - JUMP_FADE_IN).as_secs_f32() / JUMP_FADE_OUT.as_secs_f32();
        (1. - out).max(0.)
    }
}

/// Whether the highlight still needs frames.
pub(crate) fn jump_fade_active(elapsed: Duration) -> bool {
    elapsed < JUMP_FADE_IN + JUMP_FADE_OUT
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
        let (_, started) = self.highlight_fade?;
        let elapsed = started.elapsed();
        if !jump_fade_active(elapsed) {
            return None;
        }
        self.request_animation_tick(30, cx);
        Some(jump_fade(elapsed) * JUMP_PEAK_ALPHA)
    }

    /// The history list scrolled (or its rows moved): refresh the pill.
    pub(super) fn note_history_scroll(&mut self, cx: &mut Context<Self>) {
        // Pinned to the live edge, rows only move because messages arrive.
        if self.history_scroller.read(cx).is_following_tail() {
            return;
        }
        self.scroll_date.scrolled(Instant::now(), false);
        // The pill lives in the conversation: the chat list can replay.
        self.notify_conversation(cx);
    }

    /// Resolve the probed top row to its day; a changed day while the pill
    /// is up holds it longer.
    fn refresh_scroll_date_top(&mut self) {
        let Some((ix, separator)) = self.scroll_top_probe.get() else {
            return;
        };
        let Some(last) = self.history_rows.len().checked_sub(1) else {
            return;
        };
        let label = self.history_rows[..=ix.min(last)]
            .iter()
            .rev()
            .find_map(|row| row.day_label().map(str::to_string));
        if self.scroll_date.top_label.is_some() && self.scroll_date.top_label != label {
            self.scroll_date.hold_for_day_change();
        }
        self.scroll_date.top_label = label;
        self.scroll_date.separator_at_top = separator;
    }

    /// The floating pill, absolutely positioned at the top center.
    pub(super) fn scroll_date_pill(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.refresh_scroll_date_top();
        let alpha = self.scroll_date.opacity(Instant::now());
        if alpha <= 0. {
            return None;
        }
        self.request_animation_tick(30, cx);
        let label = self.scroll_date.top_label.clone()?;
        if self.scroll_date.separator_at_top {
            return None;
        }
        Some(
            div()
                .absolute()
                .top_2()
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .opacity(alpha)
                .child(super::conversation::pill_label(&label, cx))
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
