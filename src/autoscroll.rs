//! Middle-click autoscroll, after `Ui::MiddleClickAutoscroll`
//! (`ui/widgets/middle_click_autoscroll.cpp`): pressing the middle button
//! anchors a point; while the pointer stays below or above it the list
//! scrolls in that direction, faster the farther away it is.

use std::time::Duration;

/// `st::middleClickAutoscrollSpeedScale`: pixels per second for each pixel
/// of distance beyond the dead zone.
pub const SPEED_SCALE: f32 = 30.;
/// `st::middleClickAutoscrollMaxSpeed`, pixels per second.
pub const MAX_SPEED: f32 = 7200.;
/// A press held this long scrolls only while held (`kHoldToToggleThreshold`).
pub const HOLD_TO_TOGGLE: Duration = Duration::from_millis(220);
/// Tdesktop's 6 px dead zone around the anchor.
pub const DEADZONE: f32 = 6.;

/// Which way the pointer is pulling, for the anchor mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pull {
    Neutral,
    Up,
    Down,
}

pub fn pull(delta_y: f32) -> Pull {
    if delta_y.abs() <= DEADZONE {
        Pull::Neutral
    } else if delta_y < 0. {
        Pull::Up
    } else {
        Pull::Down
    }
}

/// Pixels to scroll for one tick: positive moves the pointer's side of the
/// list into view (down, towards newer messages). At least one pixel once
/// outside the dead zone.
pub fn scroll_step(delta_y: f32, elapsed: Duration) -> f32 {
    let beyond = delta_y.abs() - DEADZONE;
    if beyond <= 0. {
        return 0.;
    }
    let speed = (beyond * SPEED_SCALE).min(MAX_SPEED);
    let step = (speed * elapsed.as_secs_f32()).round().max(1.);
    if delta_y < 0. { -step } else { step }
}

/// Whether releasing the middle button ends the scroll: a long press is a
/// hold, a short one toggles and keeps scrolling until the next click.
pub fn release_stops(held: Duration) -> bool {
    held >= HOLD_TO_TOGGLE
}

#[cfg(test)]
mod tests {
    use super::*;

    const TICK: Duration = Duration::from_millis(15);

    #[test]
    fn dead_zone_does_not_scroll() {
        assert_eq!(scroll_step(0., TICK), 0.);
        assert_eq!(scroll_step(6., TICK), 0.);
        assert_eq!(scroll_step(-6., TICK), 0.);
        assert_eq!(pull(5.), Pull::Neutral);
    }

    #[test]
    fn direction_follows_the_pointer() {
        assert!(scroll_step(40., TICK) > 0.);
        assert!(scroll_step(-40., TICK) < 0.);
        assert_eq!(pull(40.), Pull::Down);
        assert_eq!(pull(-40.), Pull::Up);
    }

    #[test]
    fn speed_grows_with_distance_and_caps() {
        assert!(scroll_step(200., TICK) > scroll_step(50., TICK));
        // 30 px beyond the dead zone is 24 * 30 = 720 px/s; 15 ms is 11 px.
        assert_eq!(scroll_step(30., TICK), 11.);
        let far = scroll_step(100_000., Duration::from_secs(1));
        assert_eq!(far, MAX_SPEED);
    }

    #[test]
    fn a_slow_tick_still_moves_one_pixel() {
        assert_eq!(scroll_step(7., Duration::from_millis(1)), 1.);
        assert_eq!(scroll_step(-7., Duration::from_millis(1)), -1.);
    }

    #[test]
    fn short_press_toggles_long_press_holds() {
        assert!(!release_stops(Duration::from_millis(100)));
        assert!(release_stops(HOLD_TO_TOGGLE));
        assert!(release_stops(Duration::from_secs(2)));
    }
}
