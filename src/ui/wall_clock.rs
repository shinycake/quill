//! The time and zone the date pickers read. The app runs on the OS clock
//! and zone; a test installs a fixed instant in a zone of its choosing.
#![deny(clippy::indexing_slicing)]

use gpui_kit::{App, Global};
use quill::local_time::{CivilTime, civil_at, now_unix, utc_offset_at};
use quill::schedule::civil_to_unix_with;
use std::rc::Rc;

#[derive(Clone)]
pub(crate) struct WallClock {
    now: Rc<dyn Fn() -> i64>,
    offset_at: Rc<dyn Fn(i64) -> i32>,
}

impl Global for WallClock {}

impl WallClock {
    /// The installed clock, or the OS one.
    pub(crate) fn of(cx: &App) -> Self {
        cx.try_global::<Self>().cloned().unwrap_or_else(|| Self {
            now: Rc::new(now_unix),
            offset_at: Rc::new(utc_offset_at),
        })
    }

    pub(crate) fn now(&self) -> i64 {
        (self.now)()
    }

    /// Local wall-clock time of `unix`.
    pub(crate) fn civil(&self, unix: i64) -> CivilTime {
        civil_at(unix, (self.offset_at)(unix))
    }

    /// The instant a picked local time means (see [`civil_to_unix_with`]).
    pub(crate) fn resolve(&self, civil: &CivilTime) -> i64 {
        civil_to_unix_with(civil, &*self.offset_at, self.now())
    }
}

#[cfg(test)]
impl WallClock {
    /// 2026-11-01 07:00 UTC: Chicago's clocks go from 02:00 CDT back to
    /// 01:00 CST, so 01:00 to 02:00 shows twice.
    pub(crate) const CHICAGO_FALL_BACK: i64 = 1_793_516_400;

    /// 01:05 CST in Chicago on the night clocks go back: five minutes into
    /// the second pass of the repeated hour.
    pub(crate) fn chicago_repeated_hour() -> Self {
        let switch = Self::CHICAGO_FALL_BACK;
        Self {
            now: Rc::new(move || switch + 300),
            offset_at: Rc::new(move |t| if t < switch { -5 * 3600 } else { -6 * 3600 }),
        }
    }
}
