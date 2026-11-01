// Added by the Quill project (2026) to gpui-pre-macos 0.3.10 (Apache-2.0). See
// third_party/gpui-pre-macos/QUILL-CHANGES.md. Standard library only, so
// scripts/test-frame-idle.sh runs its tests on any host.

//! Each tick of a window's display link wakes a CoreVideo thread and the
//! main thread, so it runs only while the display should pace frames: GPUI
//! asked for the next frame, or draws come close together. A lone draw comes
//! from the frame waker, draws at once and leaves the link stopped. A wake
//! draws within about 2 ms, so such a frame gains nothing from the link.

use std::time::{Duration, Instant};

/// How long the display link keeps running after the last frame that drew,
/// presented or asked for another frame. Animations at 4 fps and up keep it
/// running; after high-rate input GPUI keeps presenting for a second, which
/// counts as activity.
const FRAME_IDLE_AFTER: Duration = Duration::from_millis(250);

/// Two woken draws closer than this are a burst, and the display paces what
/// follows. Under the 500 ms of a caret blink, and over the 83 ms between
/// frames of a 12 fps animation on a timer.
const BURST_WITHIN: Duration = Duration::from_millis(100);

/// How long a window must stay idle before its spare drawables are freed.
/// Long enough that typing, reading a message or a pause in scrolling keeps
/// them; the first frame after a release allocates one window-sized surface.
const DRAWABLE_RELEASE_AFTER: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Step {
    pub(crate) drew: bool,
    /// GPUI asked for another frame after the draw, if there was one.
    pub(crate) demand: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LinkChange {
    Keep,
    Start,
    Stop,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct LinkPace {
    last_activity: Instant,
    last_woken_draw: Option<Instant>,
}

impl LinkPace {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            last_activity: now,
            last_woken_draw: None,
        }
    }

    pub(crate) fn link_started(&mut self, now: Instant) {
        self.last_activity = now;
    }

    /// `running`: the link was running when the step began, so the link asked
    /// for it; otherwise the frame waker did.
    pub(crate) fn step(&mut self, running: bool, step: Step, now: Instant) -> LinkChange {
        let active = step.drew || step.demand;
        if active {
            self.last_activity = now;
        }
        if running {
            let idle = now.saturating_duration_since(self.last_activity) >= FRAME_IDLE_AFTER;
            return if idle && !active {
                LinkChange::Stop
            } else {
                LinkChange::Keep
            };
        }
        let burst = step.drew
            && self
                .last_woken_draw
                .is_some_and(|at| now.saturating_duration_since(at) < BURST_WITHIN);
        if step.drew {
            self.last_woken_draw = Some(now);
        }
        if step.demand || burst {
            LinkChange::Start
        } else {
            LinkChange::Keep
        }
    }

    pub(crate) fn drawable_release_in(&self, now: Instant) -> Duration {
        DRAWABLE_RELEASE_AFTER.saturating_sub(now.saturating_duration_since(self.last_activity))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    const DRAW: Step = Step {
        drew: true,
        demand: false,
    };
    const DEMAND: Step = Step {
        drew: true,
        demand: true,
    };
    const NOTHING: Step = Step {
        drew: false,
        demand: false,
    };

    /// `count` woken draws at `period`, following the link's changes: how
    /// many times the link started and stopped, and how many steps ran.
    fn timed_draws(period: Duration, count: u32, refresh: Duration) -> (u32, u32, u32) {
        let t0 = Instant::now();
        let mut pace = LinkPace::new(t0);
        let mut running = false;
        let (mut starts, mut stops, mut steps) = (0, 0, 0);
        let mut change = |running: &mut bool, step, now, pace: &mut LinkPace| {
            steps += 1;
            match pace.step(*running, step, now) {
                LinkChange::Keep => {}
                LinkChange::Start => {
                    *running = true;
                    starts += 1;
                    pace.link_started(now);
                }
                LinkChange::Stop => {
                    *running = false;
                    stops += 1;
                }
            }
        };
        let mut tick = t0;
        for n in 1..=count {
            let draw_at = t0 + period * n;
            while running && tick + refresh < draw_at {
                tick += refresh;
                change(&mut running, NOTHING, tick, &mut pace);
            }
            change(&mut running, DRAW, draw_at, &mut pace);
            tick = draw_at;
        }
        (starts, stops, steps)
    }

    #[test]
    fn a_caret_blink_never_starts_the_link() {
        let (starts, stops, steps) = timed_draws(ms(500), 120, ms(16));
        assert_eq!((starts, stops, steps), (0, 0, 120));
    }

    #[test]
    fn a_lone_draw_leaves_the_link_stopped() {
        let t0 = Instant::now();
        let mut pace = LinkPace::new(t0);
        assert_eq!(pace.step(false, DRAW, t0 + ms(1000)), LinkChange::Keep);
        assert_eq!(pace.step(false, NOTHING, t0 + ms(1010)), LinkChange::Keep);
    }

    #[test]
    fn two_draws_close_together_start_the_link() {
        let t0 = Instant::now();
        let mut pace = LinkPace::new(t0);
        assert_eq!(pace.step(false, DRAW, t0), LinkChange::Keep);
        assert_eq!(pace.step(false, DRAW, t0 + ms(99)), LinkChange::Start);
    }

    #[test]
    fn two_draws_further_apart_do_not() {
        let t0 = Instant::now();
        let mut pace = LinkPace::new(t0);
        assert_eq!(pace.step(false, DRAW, t0), LinkChange::Keep);
        assert_eq!(pace.step(false, DRAW, t0 + ms(100)), LinkChange::Keep);
        assert_eq!(pace.step(false, DRAW, t0 + ms(200)), LinkChange::Keep);
    }

    #[test]
    fn a_frame_asked_for_after_the_draw_starts_the_link_at_once() {
        let t0 = Instant::now();
        let mut pace = LinkPace::new(t0);
        assert_eq!(pace.step(false, DEMAND, t0), LinkChange::Start);
        let demand_alone = Step {
            drew: false,
            demand: true,
        };
        assert_eq!(pace.step(false, demand_alone, t0), LinkChange::Start);
    }

    #[test]
    fn a_timed_animation_keeps_one_link_running() {
        let (starts, stops, steps) = timed_draws(ms(83), 100, ms(16));
        assert_eq!((starts, stops), (1, 0));
        assert!(steps > 400, "{steps}");
    }

    #[test]
    fn the_link_stops_after_the_grace_period() {
        let t0 = Instant::now();
        let mut pace = LinkPace::new(t0);
        assert_eq!(pace.step(true, DRAW, t0), LinkChange::Keep);
        assert_eq!(pace.step(true, NOTHING, t0 + ms(249)), LinkChange::Keep);
        assert_eq!(pace.step(true, NOTHING, t0 + ms(250)), LinkChange::Stop);
    }

    #[test]
    fn a_started_link_gets_a_fresh_grace_period() {
        let t0 = Instant::now();
        let mut pace = LinkPace::new(t0);
        pace.link_started(t0 + ms(5000));
        assert_eq!(pace.step(true, NOTHING, t0 + ms(5100)), LinkChange::Keep);
        assert_eq!(pace.step(true, NOTHING, t0 + ms(5250)), LinkChange::Stop);
    }

    #[test]
    fn a_step_that_was_not_asked_for_changes_nothing() {
        let t0 = Instant::now();
        let mut pace = LinkPace::new(t0);
        assert_eq!(pace.step(false, NOTHING, t0 + ms(9000)), LinkChange::Keep);
        assert_eq!(pace.drawable_release_in(t0 + ms(9000)), Duration::ZERO);
    }

    #[test]
    fn drawables_go_three_seconds_after_the_last_draw_with_or_without_the_link() {
        let t0 = Instant::now();
        let mut pace = LinkPace::new(t0);
        assert_eq!(pace.step(false, DRAW, t0 + ms(1000)), LinkChange::Keep);
        assert_eq!(pace.drawable_release_in(t0 + ms(1000)), ms(3000));
        assert_eq!(pace.drawable_release_in(t0 + ms(3500)), ms(500));
        assert_eq!(pace.step(false, DRAW, t0 + ms(3500)), LinkChange::Keep);
        assert_eq!(pace.drawable_release_in(t0 + ms(4000)), ms(2500));
        assert_eq!(pace.drawable_release_in(t0 + ms(6500)), Duration::ZERO);
    }

    #[test]
    fn a_clock_that_goes_backwards_does_not_stop_the_link() {
        let t0 = Instant::now() + Duration::from_secs(10);
        let mut pace = LinkPace::new(t0);
        assert_eq!(pace.step(true, NOTHING, t0 - ms(500)), LinkChange::Keep);
    }
}
