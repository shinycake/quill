//! Telegram Desktop motion for the history view: new-message reveal,
//! selection-mode fades and the selection top bar slide.
//!
//! Pure timing and curve math lives here (unit-tested); the views read
//! [`MotionState`] while rendering and ask for frames through the shared
//! frame clock (`QuillApp::request_animation_tick`) only while something
//! is moving. Nothing here uses GPUI's `with_animation`.
//!
//! References (tdesktop, read-only):
//! - `history/history_widget.cpp:8607` `startItemRevealAnimations`: a new
//!   message at the bottom reveals over `st::itemRevealDuration` (150 ms,
//!   `ui/chat/chat.style:58`) with `anim::easeOutCirc`.
//! - `history/view/history_view_list_widget.cpp:1713` `inSelectionMode`:
//!   the selection checkboxes fade over `st::universalDuration` (120 ms,
//!   `lib_ui/ui/basic.style:135`), linear.
//! - `history/view/history_view_top_bar_widget.cpp:1750`
//!   `toggleSelectedControls`: the selection buttons slide over
//!   `st::slideWrapDuration` (150 ms, `lib_ui/ui/basic.style:100`) with
//!   `anim::easeOutCirc`.
//! - `ui/effects/round_checkbox.cpp:294` `RoundCheckbox::setChecked` and
//!   `:195` `paintFrame`: a 160 ms linear check; the fill closes in over the
//!   first 75 % (`bgDuration`) and the tick wipes in across the whole run.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// `st::itemRevealDuration`.
pub(crate) const REVEAL_DURATION: Duration = Duration::from_millis(150);
/// `st::universalDuration`.
pub(crate) const SELECTION_MODE_DURATION: Duration = Duration::from_millis(120);
/// `st::slideWrapDuration`.
pub(crate) const SELECTION_BAR_DURATION: Duration = Duration::from_millis(150);
/// `st::msgSelectionCheck.duration` (`defaultRoundCheckbox`).
pub(crate) const CHECK_DURATION: Duration = Duration::from_millis(160);
/// `defaultRoundCheckbox.bgDuration`.
pub(crate) const CHECK_BG_FRACTION: f32 = 0.75;
/// `st::msgSelectionOffset`: how far an outgoing bubble slides left to make
/// room for the check.
pub(crate) const SELECTION_OFFSET: f32 = 30.;
/// `st::msgSelectionCheck.size`.
pub(crate) const CHECK_SIZE: f32 = 20.;
/// `st::msgSelectionBottomSkip`.
pub(crate) const CHECK_BOTTOM_SKIP: f32 = 5.;
/// Height guessed for a revealed row before its real height was painted.
pub(crate) const REVEAL_HEIGHT_GUESS: f32 = 56.;

/// `elapsed / total`, clamped to `0..=1`.
pub(crate) fn progress(elapsed: Duration, total: Duration) -> f32 {
    if total.is_zero() {
        return 1.;
    }
    (elapsed.as_secs_f32() / total.as_secs_f32()).clamp(0., 1.)
}

/// `anim::easeOutCirc`.
pub(crate) fn ease_out_circ(t: f32) -> f32 {
    let t = t.clamp(0., 1.) - 1.;
    (1. - t * t).sqrt()
}

pub(crate) fn linear(t: f32) -> f32 {
    t.clamp(0., 1.)
}

/// How far a bottom-pinned history is still displaced downwards, `elapsed`
/// after `height` px of new rows arrived: the rows start fully hidden below
/// the bottom edge and rise into place (`revealItemsCallback`).
pub(crate) fn reveal_shift(elapsed: Duration, height: f32) -> f32 {
    height * (1. - ease_out_circ(progress(elapsed, REVEAL_DURATION)))
}

/// Whether a reveal started `elapsed` ago still moves.
pub(crate) fn reveal_active(elapsed: Duration) -> bool {
    elapsed < REVEAL_DURATION
}

/// Debugging aid: `QUILL_MOTION_HOLD_MS=40` freezes every motion at that
/// elapsed time, so a capture can show a mid-animation frame.
fn hold() -> Option<Duration> {
    static HOLD: std::sync::OnceLock<Option<Duration>> = std::sync::OnceLock::new();
    *HOLD.get_or_init(|| {
        std::env::var("QUILL_MOTION_HOLD_MS")
            .ok()
            .and_then(|ms| ms.parse().ok())
            .map(Duration::from_millis)
    })
}

/// Whether motion is frozen for a capture (`QUILL_MOTION_HOLD_MS`).
pub(crate) fn held() -> bool {
    hold().is_some()
}

/// Whether the user turned interface animations off (power saving).
pub(crate) fn interface_animations_off() -> bool {
    quill::power_saving::on(quill::power_saving::Flag::Animations)
}

/// Time since `since`, or the frozen time under `QUILL_MOTION_HOLD_MS`.
pub(crate) fn elapsed_since(since: Instant, now: Instant) -> Duration {
    hold().unwrap_or_else(|| now.saturating_duration_since(since))
}

/// A value that glides between `from` and `to`, like tdesktop's
/// `Ui::Animations::Simple`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Glide {
    from: f32,
    to: f32,
    since: Instant,
    duration: Duration,
    ease: fn(f32) -> f32,
}

impl Glide {
    pub(crate) fn settled(at: f32, now: Instant, ease: fn(f32) -> f32) -> Self {
        Self {
            from: at,
            to: at,
            since: now,
            duration: Duration::ZERO,
            ease,
        }
    }

    pub(crate) fn value(&self, now: Instant) -> f32 {
        let t = progress(elapsed_since(self.since, now), self.duration);
        self.from + (self.to - self.from) * (self.ease)(t)
    }

    pub(crate) fn target(&self) -> f32 {
        self.to
    }

    pub(crate) fn animating(&self, now: Instant) -> bool {
        elapsed_since(self.since, now) < self.duration
    }

    /// Head for `to` from wherever the value is now. The run lasts
    /// `full` scaled by the distance left (tdesktop's `change`), or snaps
    /// when `animate` is false.
    pub(crate) fn go(&mut self, to: f32, full: Duration, animate: bool, now: Instant) {
        if self.to == to {
            return;
        }
        let from = self.value(now);
        self.from = from;
        self.to = to;
        self.since = now;
        // "Interface animations" off in Battery and animations: snap.
        let animate = animate && !interface_animations_off();
        self.duration = if animate {
            full.mul_f32((to - from).abs().clamp(0., 1.))
        } else {
            Duration::ZERO
        };
    }
}

/// How the selection check circle paints at checked-progress `p`
/// (`RoundCheckbox::paintFrame`): `(fill, tick)`, both `0..=1`. The fill is
/// the share of the disc that has closed in; the tick is how much of it has
/// wiped in from the left.
pub(crate) fn check_frame(p: f32) -> (f32, f32) {
    let p = p.clamp(0., 1.);
    ((p / CHECK_BG_FRACTION).min(1.), p)
}

/// Horizontal room an outgoing bubble gives the check circle at selection
/// progress `mode` (`AdditionalSpaceForSelectionCheckbox`, the
/// narrow-column case).
pub(crate) fn bubble_slide(mode: f32) -> f32 {
    SELECTION_OFFSET * mode.clamp(0., 1.)
}

/// New rows revealing at the bottom of the history.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Reveal {
    pub(crate) started: Instant,
    /// Index of the first row that arrived.
    pub(crate) first_row: usize,
    /// Displacement still left from earlier arrivals when this one began.
    pub(crate) carry: f32,
}

#[derive(Debug)]
struct CheckAnim {
    checked: bool,
    glide: Glide,
}

/// Selection mode motion: the checks fading in over every row, the top
/// bar swapping to its buttons, and each check filling on selection.
#[derive(Debug)]
pub(crate) struct SelectionFx {
    mode: Glide,
    bar: Glide,
    last_change: Instant,
    checks: HashMap<i64, CheckAnim>,
    /// The count shown on the buttons while the bar slides away.
    pub(crate) last_count: usize,
}

impl SelectionFx {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            mode: Glide::settled(0., now, linear),
            bar: Glide::settled(0., now, ease_out_circ),
            last_change: now,
            checks: HashMap::new(),
            last_count: 0,
        }
    }

    /// Follow whether selection mode is on. Idempotent within a frame.
    pub(crate) fn sync_mode(&mut self, on: bool, animate: bool, now: Instant) {
        let to = if on { 1. } else { 0. };
        if self.mode.target() != to {
            self.last_change = now;
        }
        self.mode.go(to, SELECTION_MODE_DURATION, animate, now);
        self.bar.go(to, SELECTION_BAR_DURATION, animate, now);
        if !on && !self.moving(now) {
            self.checks.clear();
        }
    }

    /// Check circles' opacity (and bubble slide), `0..=1`.
    pub(crate) fn mode(&self, now: Instant) -> f32 {
        self.mode.value(now)
    }

    /// Top bar slide, `0` (normal header) to `1` (selection buttons).
    pub(crate) fn bar(&self, now: Instant) -> f32 {
        self.bar.value(now)
    }

    /// Checked-progress of message `id`, `0..=1`, starting a fill or an
    /// un-fill when its state changed since the last frame. A row seen for
    /// the first time animates only right after selection mode began.
    pub(crate) fn check(&mut self, id: i64, selected: bool, animate: bool, now: Instant) -> f32 {
        let target = if selected { 1. } else { 0. };
        let fresh_mode = elapsed_since(self.last_change, now) < Duration::from_millis(400);
        let entry = self.checks.entry(id).or_insert_with(|| CheckAnim {
            checked: false,
            glide: Glide::settled(
                if selected && !(animate && fresh_mode) {
                    1.
                } else {
                    0.
                },
                now,
                linear,
            ),
        });
        if entry.checked != selected {
            entry.checked = selected;
            entry.glide.go(target, CHECK_DURATION, animate, now);
        }
        entry.glide.value(now)
    }

    /// Anything still moving, so the frame clock keeps ticking.
    pub(crate) fn moving(&self, now: Instant) -> bool {
        self.mode.animating(now)
            || self.bar.animating(now)
            || self.checks.values().any(|c| c.glide.animating(now))
    }
}

/// Everything the history motion needs, held by the app in one field.
pub(crate) struct MotionState {
    pub(crate) reveal: RefCell<Option<Reveal>>,
    /// Painted height of the rows from `Reveal::first_row` on; written by a
    /// canvas after the rows paint, `0` while unknown.
    pub(crate) reveal_measure: Rc<Cell<f32>>,
    /// Whether the history's rows fill the viewport (set while painting).
    pub(crate) list_fills: Rc<Cell<bool>>,
    pub(crate) selection: RefCell<SelectionFx>,
}

impl Default for MotionState {
    fn default() -> Self {
        Self {
            reveal: RefCell::new(None),
            reveal_measure: Rc::new(Cell::new(0.)),
            list_fills: Rc::new(Cell::new(true)),
            selection: RefCell::new(SelectionFx::new(Instant::now())),
        }
    }
}

impl MotionState {
    /// Start (or extend) a reveal of rows from `first_row`.
    pub(crate) fn start_reveal(&self, first_row: usize, now: Instant) {
        if interface_animations_off() {
            return;
        }
        let mut slot = self.reveal.borrow_mut();
        let carry = slot.as_ref().map_or(0., |r| {
            reveal_shift(elapsed_since(r.started, now), self.reveal_total(r))
        });
        *slot = Some(Reveal {
            started: now,
            first_row,
            carry,
        });
        self.reveal_measure.set(0.);
    }

    fn reveal_total(&self, reveal: &Reveal) -> f32 {
        let measured = self.reveal_measure.get();
        reveal.carry
            + if measured > 0. {
                measured
            } else {
                REVEAL_HEIGHT_GUESS
            }
    }

    /// `(shift, extra)` for the history viewport right now: the content
    /// sits `shift` px below its settled place inside a viewport made
    /// `extra` px taller. `None` when nothing is revealing.
    pub(crate) fn reveal_now(&self, now: Instant) -> Option<(f32, f32)> {
        let mut slot = self.reveal.borrow_mut();
        let reveal = (*slot)?;
        if !reveal_active(elapsed_since(reveal.started, now)) {
            *slot = None;
            return None;
        }
        let total = self.reveal_total(&reveal);
        Some((
            reveal_shift(elapsed_since(reveal.started, now), total),
            total,
        ))
    }

    pub(crate) fn cancel_reveal(&self) {
        *self.reveal.borrow_mut() = None;
    }
}

#[cfg(test)]
mod tests {
    use crate::ui::motion::{
        CHECK_DURATION, Glide, MotionState, REVEAL_DURATION, SELECTION_MODE_DURATION, SelectionFx,
        bubble_slide, check_frame, ease_out_circ, linear, progress, reveal_active, reveal_shift,
    };
    use std::time::{Duration, Instant};

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn curves_match_tdesktop_endpoints() {
        for f in [ease_out_circ, linear] {
            assert!(f(0.).abs() < 1e-6);
            assert!((f(1.) - 1.).abs() < 1e-6);
            assert!(f(-1.).abs() < 1e-6);
            assert!((f(2.) - 1.).abs() < 1e-6);
        }
        // sqrt(1 - (t-1)^2) at t = .5 is sqrt(.75).
        assert!((ease_out_circ(0.5) - 0.75_f32.sqrt()).abs() < 1e-6);
    }

    #[test]
    fn progress_clamps_and_handles_zero() {
        assert_eq!(progress(ms(75), ms(150)), 0.5);
        assert_eq!(progress(ms(500), ms(150)), 1.);
        assert_eq!(progress(ms(5), Duration::ZERO), 1.);
    }

    #[test]
    fn reveal_rises_into_place_in_150ms() {
        assert_eq!(reveal_shift(ms(0), 80.), 80.);
        assert_eq!(reveal_shift(REVEAL_DURATION, 80.), 0.);
        // easeOutCirc front-loads the motion: past halfway at 1/3 of the time.
        assert!(reveal_shift(ms(50), 80.) < 40.);
        let mut last = f32::MAX;
        for t in (0..=150).step_by(10) {
            let s = reveal_shift(ms(t), 80.);
            assert!(s <= last);
            last = s;
        }
        assert!(reveal_active(ms(149)));
        assert!(!reveal_active(ms(150)));
    }

    #[test]
    fn glide_reverses_from_where_it_is() {
        let t0 = Instant::now();
        let mut g = Glide::settled(0., t0, linear);
        g.go(1., SELECTION_MODE_DURATION, true, t0);
        assert!(g.animating(t0 + ms(60)));
        assert!((g.value(t0 + ms(60)) - 0.5).abs() < 1e-3);
        // Reversing halfway takes half the time.
        g.go(0., SELECTION_MODE_DURATION, true, t0 + ms(60));
        assert!((g.value(t0 + ms(60)) - 0.5).abs() < 1e-3);
        assert!(g.animating(t0 + ms(119)));
        assert!(!g.animating(t0 + ms(121)));
        assert_eq!(g.value(t0 + ms(200)), 0.);
        // Asking for the same target again is a no-op.
        g.go(0., SELECTION_MODE_DURATION, true, t0 + ms(300));
        assert!(!g.animating(t0 + ms(300)));
    }

    #[test]
    fn glide_snaps_when_not_animating() {
        let t0 = Instant::now();
        let mut g = Glide::settled(0., t0, linear);
        g.go(1., SELECTION_MODE_DURATION, false, t0);
        assert_eq!(g.value(t0), 1.);
        assert!(!g.animating(t0));
    }

    #[test]
    fn check_frame_fills_in_75_percent_and_wipes_tick() {
        assert_eq!(check_frame(0.), (0., 0.));
        let (fill, tick) = check_frame(0.375);
        assert!((fill - 0.5).abs() < 1e-6 && (tick - 0.375).abs() < 1e-6);
        assert_eq!(check_frame(0.75).0, 1.);
        assert_eq!(check_frame(1.), (1., 1.));
        assert_eq!(bubble_slide(0.), 0.);
        assert_eq!(bubble_slide(1.), 30.);
    }

    #[test]
    fn selection_fx_follows_mode_and_checks() {
        let t0 = Instant::now();
        let mut fx = SelectionFx::new(t0);
        assert_eq!(fx.mode(t0), 0.);
        fx.sync_mode(true, true, t0);
        assert!(fx.moving(t0 + ms(10)));
        assert!((fx.mode(t0 + ms(60)) - 0.5).abs() < 1e-3);
        // The bar uses easeOutCirc over 150 ms.
        assert!((fx.bar(t0 + ms(75)) - ease_out_circ(0.5)).abs() < 1e-3);
        assert_eq!(fx.mode(t0 + ms(200)), 1.);
        assert!(!fx.moving(t0 + ms(200)));

        // The first message selected right as the mode starts fills in.
        let p = fx.check(7, true, true, t0 + ms(1));
        assert_eq!(p, 0.);
        assert!((fx.check(7, true, true, t0 + ms(81)) - 0.5).abs() < 1e-2);
        assert_eq!(fx.check(7, true, true, t0 + ms(300)), 1.);
        // Unselecting runs the fill backwards.
        let _ = fx.check(7, false, true, t0 + ms(400));
        assert!((fx.check(7, false, true, t0 + ms(480)) - 0.5).abs() < 1e-2);
        assert_eq!(fx.check(7, false, true, t0 + ms(700)), 0.);
        // A row first seen well after the mode began snaps to its state.
        assert_eq!(fx.check(9, true, true, t0 + ms(900)), 1.);
        // An inactive window snaps too.
        assert_eq!(fx.check(11, true, false, t0 + ms(901)), 1.);

        // Leaving the mode fades back and forgets the rows once settled.
        fx.sync_mode(false, true, t0 + ms(1000));
        assert!(fx.moving(t0 + ms(1010)));
        fx.sync_mode(false, true, t0 + ms(1200));
        assert!(fx.checks.is_empty());
        assert_eq!(fx.mode(t0 + ms(1200)), 0.);
    }

    #[test]
    fn inactive_window_snaps_selection_mode() {
        let t0 = Instant::now();
        let mut fx = SelectionFx::new(t0);
        fx.sync_mode(true, false, t0);
        assert_eq!(fx.mode(t0), 1.);
        assert_eq!(fx.bar(t0), 1.);
        assert!(!fx.moving(t0));
    }

    #[test]
    fn reveal_state_measures_extends_and_expires() {
        let t0 = Instant::now();
        let motion = MotionState::default();
        assert_eq!(motion.reveal_now(t0), None);
        motion.start_reveal(10, t0);
        // Unknown height: the guess stands in.
        let (shift, extra) = motion.reveal_now(t0).unwrap();
        assert_eq!(shift, extra);
        assert_eq!(extra, 56.);
        // The real height replaces it once painted.
        motion.reveal_measure.set(80.);
        let (_, extra) = motion.reveal_now(t0 + ms(10)).unwrap();
        assert_eq!(extra, 80.);
        // A second arrival mid-way carries over what was left.
        let left = 80. * (1. - ease_out_circ(0.5));
        motion.start_reveal(11, t0 + ms(75));
        motion.reveal_measure.set(40.);
        let (shift, extra) = motion.reveal_now(t0 + ms(75)).unwrap();
        assert!((extra - (left + 40.)).abs() < 1e-3);
        assert!((shift - extra).abs() < 1e-3);
        assert_eq!(motion.reveal_now(t0 + ms(75) + REVEAL_DURATION), None);
        assert_eq!(motion.reveal_now(t0 + ms(75)), None);
    }

    #[test]
    fn check_duration_is_160ms() {
        assert_eq!(CHECK_DURATION, ms(160));
    }
}
