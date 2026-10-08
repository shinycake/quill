//! Chat-row micro-animations: the avatar's online dot and the unread
//! badge. Pure timing state; the UI passes `now` so tests need no clock.
//!
//! Telegram Desktop animates the online dot over 150 ms
//! (`st::dialogsOnlineBadgeDuration`, `Row::CornerLayersManager`): the dot
//! grows from its centre while its ring widens. It does not animate the
//! unread counter; Quill gives a newly appearing badge the same 150 ms
//! scale and fade so the list reads consistently.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// `st::dialogsOnlineBadgeDuration`.
pub const DURATION: Duration = Duration::from_millis(150);

/// One 0..1 value that eases toward a boolean target.
#[derive(Debug, Clone, Copy)]
struct Toggle {
    target: bool,
    from: f32,
    /// When the current ease started; `None` once settled.
    since: Option<Instant>,
}

impl Toggle {
    fn snapped(target: bool) -> Self {
        Self {
            target,
            from: if target { 1. } else { 0. },
            since: None,
        }
    }

    fn goal(&self) -> f32 {
        if self.target { 1. } else { 0. }
    }

    fn value(&self, now: Instant) -> f32 {
        let Some(since) = self.since else {
            return self.goal();
        };
        let t = (now.saturating_duration_since(since).as_secs_f32() / DURATION.as_secs_f32())
            .clamp(0., 1.);
        // Ease-out cubic.
        let eased = 1. - (1. - t).powi(3);
        self.from + (self.goal() - self.from) * eased
    }

    fn settled(&self, now: Instant) -> bool {
        self.since
            .is_none_or(|since| now.saturating_duration_since(since) >= DURATION)
    }

    /// Move to `target`. `animate` false snaps. Returns the new value.
    fn set(&mut self, target: bool, animate: bool, now: Instant) -> f32 {
        if target != self.target {
            *self = if animate {
                Self {
                    target,
                    from: self.value(now),
                    since: Some(now),
                }
            } else {
                Self::snapped(target)
            };
        }
        self.value(now)
    }
}

/// What a row should draw this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RowFx {
    /// Online dot scale, 0 (hidden) to 1.
    pub online: f32,
    /// Unread badge scale/opacity, 0 to 1. Only rises animate: a badge
    /// that disappears is simply gone.
    pub badge: f32,
    /// Another frame is needed.
    pub animating: bool,
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    online: Toggle,
    badge: Toggle,
}

/// Per-chat animation state for the visible rows.
#[derive(Debug, Default)]
pub struct RowFxMap {
    rows: HashMap<i64, Entry>,
}

impl RowFxMap {
    /// Record this frame's targets for `chat` and read the animated
    /// values. The first sight of a chat, or an inactive window
    /// (`animate` false), snaps instead of animating.
    pub fn observe(
        &mut self,
        chat: i64,
        online: bool,
        badge: bool,
        animate: bool,
        now: Instant,
    ) -> RowFx {
        let entry = self.rows.entry(chat).or_insert_with(|| Entry {
            online: Toggle::snapped(online),
            badge: Toggle::snapped(badge),
        });
        let online_value = entry.online.set(online, animate, now);
        // Falling edge: snap, the badge row has no exit animation.
        let badge_animate = animate && badge;
        let badge_value = entry.badge.set(badge, badge_animate, now);
        RowFx {
            online: online_value,
            badge: badge_value,
            animating: !entry.online.settled(now) || !entry.badge.settled(now),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DURATION, RowFxMap};
    use std::time::{Duration, Instant};

    #[test]
    fn first_sight_snaps() {
        let mut map = RowFxMap::default();
        let now = Instant::now();
        let fx = map.observe(1, true, true, true, now);
        assert_eq!(fx.online, 1.);
        assert_eq!(fx.badge, 1.);
        assert!(!fx.animating);
        let fx = map.observe(2, false, false, true, now);
        assert_eq!((fx.online, fx.badge), (0., 0.));
    }

    #[test]
    fn online_dot_grows_over_150ms_then_settles() {
        let mut map = RowFxMap::default();
        let t0 = Instant::now();
        map.observe(1, false, false, true, t0);
        let t1 = t0 + Duration::from_millis(50);
        let started = map.observe(1, true, false, true, t1);
        assert_eq!(started.online, 0.);
        assert!(started.animating);
        let mid = map.observe(1, true, false, true, t1 + Duration::from_millis(75));
        assert!(mid.online > 0.5 && mid.online < 1., "{}", mid.online);
        assert!(mid.animating);
        let done = map.observe(1, true, false, true, t1 + DURATION);
        assert_eq!(done.online, 1.);
        assert!(!done.animating);
    }

    #[test]
    fn online_dot_fades_out_from_where_it_is() {
        let mut map = RowFxMap::default();
        let t0 = Instant::now();
        map.observe(1, true, false, true, t0);
        let t1 = t0 + Duration::from_millis(100);
        let start = map.observe(1, false, false, true, t1);
        assert_eq!(start.online, 1.);
        let mid = map.observe(1, false, false, true, t1 + Duration::from_millis(75));
        assert!(mid.online > 0. && mid.online < 0.5, "{}", mid.online);
        let end = map.observe(1, false, false, true, t1 + DURATION);
        assert_eq!(end.online, 0.);
        assert!(!end.animating);
    }

    #[test]
    fn badge_scales_in_but_never_out() {
        let mut map = RowFxMap::default();
        let t0 = Instant::now();
        map.observe(1, false, false, true, t0);
        let t1 = t0 + Duration::from_millis(40);
        assert_eq!(map.observe(1, false, true, true, t1).badge, 0.);
        let mid = map.observe(1, false, true, true, t1 + Duration::from_millis(60));
        assert!(mid.badge > 0. && mid.badge < 1.);
        let gone = map.observe(1, false, false, true, t1 + Duration::from_millis(80));
        assert_eq!(gone.badge, 0.);
        assert!(!gone.animating);
    }

    #[test]
    fn inactive_window_snaps() {
        let mut map = RowFxMap::default();
        let t0 = Instant::now();
        map.observe(1, false, false, true, t0);
        // Window inactive: no animation, no ticks.
        let fx = map.observe(1, true, true, false, t0 + Duration::from_millis(10));
        assert_eq!((fx.online, fx.badge, fx.animating), (1., 1., false));
    }
}
