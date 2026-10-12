//! Short value animations for the chart: a tween that restarts from its
//! sampled value when retargeted, and fading sets (old rulers or date
//! labels fading out while the new ones fade in).

use quill::stat_chart::Limits;
use std::time::{Duration, Instant};

/// Easing curves tdesktop uses for these animations.
#[derive(Clone, Copy)]
pub(super) enum Ease {
    OutCubic,
    InOutSine,
    OutCirc,
    Linear,
}

impl Ease {
    fn apply(self, t: f64) -> f64 {
        match self {
            Self::OutCubic => 1.0 - (1.0 - t).powi(3),
            Self::InOutSine => -((std::f64::consts::PI * t).cos() - 1.0) / 2.0,
            Self::OutCirc => (1.0 - (t - 1.0).powi(2)).sqrt(),
            Self::Linear => t,
        }
    }
}

/// Whether animations are off (power saving).
pub(super) fn animations_off() -> bool {
    quill::power_saving::on(quill::power_saving::Flag::Animations)
}

#[derive(Clone, Copy)]
pub(super) struct Tween {
    from: f64,
    pub to: f64,
    start: Option<Instant>,
    duration: Duration,
    ease: Ease,
}

impl Tween {
    pub(super) fn at(value: f64, duration: Duration, ease: Ease) -> Self {
        Self {
            from: value,
            to: value,
            start: None,
            duration,
            ease,
        }
    }

    pub(super) fn value(&self, now: Instant) -> f64 {
        let Some(start) = self.start else {
            return self.to;
        };
        let t = (now.saturating_duration_since(start).as_secs_f64()
            / self.duration.as_secs_f64().max(1e-6))
        .min(1.0);
        self.from + (self.to - self.from) * self.ease.apply(t)
    }

    pub(super) fn running(&self, now: Instant) -> bool {
        self.start
            .is_some_and(|start| now.saturating_duration_since(start) < self.duration)
    }

    /// Head for `to`, from where the value is now; without `animate` it
    /// jumps there.
    pub(super) fn retarget(&mut self, to: f64, now: Instant, animate: bool) {
        if (to - self.to).abs() < f64::EPSILON {
            return;
        }
        if animate && !animations_off() {
            self.from = self.value(now);
            self.start = Some(now);
        } else {
            self.from = to;
            self.start = None;
        }
        self.to = to;
    }

    /// Start over from `from` towards `to`.
    pub(super) fn restart(&mut self, from: f64, to: f64, now: Instant) {
        self.from = from;
        self.to = to;
        self.start = (!animations_off()).then_some(now);
    }
}

#[derive(Clone, Copy)]
pub(super) struct TweenLimits {
    pub min: Tween,
    pub max: Tween,
}

impl TweenLimits {
    pub(super) fn at(limits: Limits, duration: Duration) -> Self {
        Self {
            min: Tween::at(limits.min, duration, Ease::OutCubic),
            max: Tween::at(limits.max, duration, Ease::OutCubic),
        }
    }

    pub(super) fn value(&self, now: Instant) -> Limits {
        Limits::new(self.min.value(now), self.max.value(now))
    }

    pub(super) fn target(&self) -> Limits {
        Limits::new(self.min.to, self.max.to)
    }

    pub(super) fn running(&self, now: Instant) -> bool {
        self.min.running(now) || self.max.running(now)
    }

    pub(super) fn retarget(&mut self, to: Limits, now: Instant, animate: bool) {
        self.min.retarget(to.min, now, animate);
        self.max.retarget(to.max, now, animate);
    }
}

/// Versions of something that cross-fade: the newest fades in, the
/// others fade out and are dropped once invisible.
pub(super) struct Fades<T> {
    sets: Vec<(T, Tween, Instant)>,
    duration: Duration,
}

impl<T: Clone + PartialEq> Fades<T> {
    pub(super) fn new(duration: Duration) -> Self {
        Self {
            sets: Vec::new(),
            duration,
        }
    }

    pub(super) fn current(&self) -> Option<&T> {
        self.sets.last().map(|(value, _, _)| value)
    }

    /// Show `value`: at once the first time, fading in afterwards. A new
    /// value within `merge` of the last one replaces it rather than
    /// stacking another fade (tdesktop adds rulers at most every 320 ms).
    pub(super) fn show(&mut self, value: T, now: Instant, animate: bool, merge: Duration) {
        if self.current() == Some(&value) {
            return;
        }
        let animate = animate && !animations_off() && !self.sets.is_empty();
        if !animate {
            self.sets.clear();
            self.sets
                .push((value, Tween::at(1.0, self.duration, Ease::Linear), now));
            return;
        }
        let several = self.sets.len() > 1;
        if let Some((last, _, added)) = self.sets.last_mut()
            && several
            && now.saturating_duration_since(*added) < merge
        {
            *last = value;
            return;
        }
        for (_, alpha, _) in &mut self.sets {
            alpha.retarget(0.0, now, true);
        }
        let mut alpha = Tween::at(0.0, self.duration, Ease::Linear);
        alpha.retarget(1.0, now, true);
        self.sets.push((value, alpha, now));
    }

    /// Drop the versions that faded out.
    pub(super) fn settle(&mut self, now: Instant) {
        let last = self.sets.len().saturating_sub(1);
        let mut index = 0;
        self.sets.retain(|(_, alpha, _)| {
            let keep = index == last || alpha.running(now) || alpha.to > 0.0;
            index += 1;
            keep
        });
    }

    pub(super) fn running(&self, now: Instant) -> bool {
        self.sets.iter().any(|(_, alpha, _)| alpha.running(now))
    }

    /// Every version with its opacity.
    pub(super) fn visible(&self, now: Instant) -> Vec<(T, f32)> {
        self.sets
            .iter()
            .map(|(value, alpha, _)| (value.clone(), alpha.value(now) as f32))
            .filter(|(_, alpha)| *alpha > 0.0)
            .collect()
    }
}
