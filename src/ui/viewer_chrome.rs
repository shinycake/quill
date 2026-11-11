//! Whether the viewer's toolbar, arrows and caption are shown, and the idle
//! clock that fades them out (tdesktop `mediaviewWaitHide`). One value:
//! `Shown` holds the task that waits for the idle time, `Hidden` when the
//! fade-out began.
#![deny(clippy::indexing_slicing)]

use gpui_kit::Task;
use std::time::Instant;

/// The task that hides the controls after the idle wait.
pub(super) enum Hider {
    /// No task waits: the next activity starts one.
    Off,
    /// One task waits; dropping it cancels the wait.
    Watching { _task: Task<()> },
}

pub(super) enum Visibility {
    Shown {
        hider: Hider,
    },
    /// Faded out at this instant.
    Hidden {
        at: Instant,
    },
}

pub(super) struct ViewerChrome {
    pub(super) visibility: Visibility,
    /// Last mouse movement over the viewer (controls auto-hide clock).
    pub(super) last_activity: Instant,
    /// The pointer rests on a control, so they must not auto-hide.
    pub(super) over_controls: bool,
    /// Bumped on every show/hide flip; keys the 150 ms controls fade.
    pub(super) fade_gen: u64,
}

impl ViewerChrome {
    pub(super) fn new() -> Self {
        Self {
            visibility: Visibility::Shown { hider: Hider::Off },
            last_activity: Instant::now(),
            over_controls: false,
            fade_gen: 0,
        }
    }

    pub(super) fn is_hidden(&self) -> bool {
        matches!(self.visibility, Visibility::Hidden { .. })
    }

    /// When they faded out, while hidden.
    pub(super) fn hidden_at(&self) -> Option<Instant> {
        match self.visibility {
            Visibility::Hidden { at } => Some(at),
            Visibility::Shown { .. } => None,
        }
    }

    /// Whether a task already waits to hide them.
    pub(super) fn is_watching(&self) -> bool {
        matches!(
            self.visibility,
            Visibility::Shown {
                hider: Hider::Watching { .. }
            }
        )
    }

    /// Show them (no wait task) and restart the idle clock. `true` when
    /// they were hidden.
    pub(super) fn show(&mut self) -> bool {
        self.last_activity = Instant::now();
        if self.is_hidden() {
            self.visibility = Visibility::Shown { hider: Hider::Off };
            self.fade_gen += 1;
            true
        } else {
            false
        }
    }

    /// Start fading them out; drops the wait task.
    pub(super) fn hide(&mut self) {
        self.visibility = Visibility::Hidden { at: Instant::now() };
        self.fade_gen += 1;
    }

    /// Hold the task that waits for the idle time.
    pub(super) fn watch(&mut self, task: Task<()>) {
        if let Visibility::Shown { hider } = &mut self.visibility {
            *hider = Hider::Watching { _task: task };
        }
    }

    /// The wait task ended without hiding them.
    pub(super) fn stop_watching(&mut self) {
        if let Visibility::Shown { hider } = &mut self.visibility {
            *hider = Hider::Off;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn showing_after_hidden_flips_the_fade_key_once() {
        let mut chrome = ViewerChrome::new();
        assert!(!chrome.show(), "already shown");
        assert_eq!(chrome.fade_gen, 0);
        chrome.hide();
        assert!(chrome.is_hidden() && chrome.hidden_at().is_some());
        assert_eq!(chrome.fade_gen, 1);
        assert!(chrome.show());
        assert!(!chrome.is_hidden() && !chrome.is_watching());
        assert_eq!(chrome.fade_gen, 2);
    }
}
