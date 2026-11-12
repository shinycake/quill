//! The controller's stall delay, one value with its timer. The timer lives
//! inside the stage that needs it, so a stage that ends takes its timer
//! with it and nothing clears the timer by hand. (The Space hold is
//! `quill::media_viewer::BoostState`, which holds its timer the same way.)

use gpui_kit::{Context, Task};
use quill::media_viewer::waiting_wait;
use std::time::{Duration, Instant};

/// The delay before a stall shows its spinner (tdesktop
/// `kWaitingShowDelay`): the stage remembers when waiting began and holds
/// the timer that redraws when the delay is up.
#[derive(Default)]
pub(crate) enum WaitingGate {
    /// Not waiting.
    #[default]
    Idle,
    /// Waiting since `since`, the delay not up; `timer` redraws when it is.
    Delayed { since: Instant, _timer: Task<()> },
    /// Waiting past the delay: the spinner shows.
    Shown,
}

impl WaitingGate {
    /// [`Self::observe`] for a view: the timer redraws `cx`'s view when the
    /// delay is up.
    pub(crate) fn shown_in<V: 'static>(&mut self, waiting: bool, cx: &mut Context<V>) -> bool {
        let now = cx.background_executor().now();
        self.observe(waiting, now, |wait| {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(wait).await;
                let _ = this.update(cx, |_, cx| cx.notify());
            })
        })
    }

    /// Feed whether the clip waits now; `true` while the spinner shows. A
    /// stall that begins asks `arm` for the timer that redraws when the
    /// delay is up (given the time left) and holds it, built whole with the
    /// stage; one that ends takes it away.
    pub(crate) fn observe(
        &mut self,
        waiting: bool,
        now: Instant,
        arm: impl FnOnce(Duration) -> Task<()>,
    ) -> bool {
        if !waiting {
            *self = Self::Idle;
            return false;
        }
        let since = match self {
            Self::Delayed { since, .. } => *since,
            Self::Idle => now,
            Self::Shown => return true,
        };
        match waiting_wait(since, now) {
            None => {
                *self = Self::Shown;
                true
            }
            Some(wait) => {
                if !matches!(self, Self::Delayed { .. }) {
                    *self = Self::Delayed {
                        since,
                        _timer: arm(wait),
                    };
                }
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WaitingGate;
    use gpui_kit::Task;
    use std::cell::Cell;
    use std::time::{Duration, Instant};

    fn ms(base: Instant, n: u64) -> Instant {
        base + Duration::from_millis(n)
    }

    #[test]
    fn a_stall_arms_one_timer_and_a_wait_that_ends_takes_it() {
        let t0 = Instant::now();
        let armed = Cell::new(Vec::new());
        let arm = |wait| {
            let mut all = armed.take();
            all.push(wait);
            armed.set(all);
            Task::ready(())
        };
        let mut gate = WaitingGate::default();
        assert!(!gate.observe(false, ms(t0, 0), arm));
        assert!(matches!(gate, WaitingGate::Idle));
        assert!(!gate.observe(true, ms(t0, 0), arm));
        assert!(!gate.observe(true, ms(t0, 100), arm), "the timer is set");
        assert_eq!(armed.take(), vec![Duration::from_millis(500)]);
        assert!(!gate.observe(false, ms(t0, 200), arm));
        assert!(matches!(gate, WaitingGate::Idle));
        gate.observe(true, ms(t0, 300), arm);
        assert!(gate.observe(true, ms(t0, 900), arm));
        assert!(matches!(gate, WaitingGate::Shown));
        assert_eq!(armed.take(), vec![Duration::from_millis(500)]);
    }
}
