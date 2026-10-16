//! What a live picture shows over itself, by state and time
//! (`docs/decisions/video-player-live-ui.md`):
//!
//! - Opening (no frame yet): the peer's blurred photo, or the initials
//!   avatar, with a spinner after 500 ms (`kWaitingShowDelay`, as the
//!   file viewer).
//! - Stalled (Buffering: no frame for 1.5 s while the sender is active):
//!   the last picture dimmed to 60 % with the spinner, and "Connecting…"
//!   once the picture has stood still for 5 s.
//! - Paused (the sender paused): the blurred 90 px picture (the element's
//!   `paused_overlay`) under tdesktop's shadow and pause icon, with
//!   "Video is paused" when the view is wide (`kPausedVideoSize`,
//!   `calls_group_viewport_raster.cpp`).
//! - Idle (nothing is expected: not joined, nobody broadcasting, the
//!   build cannot play it): the placeholder alone; the stream window
//!   puts its label on the controls backdrop ([`stream_label_box`]).
//! - Ended: the last picture dimmed, "Live stream ended" and Leave.
//! - Failed: the file viewer's failed card with Retry.
//!
//! [`LiveOverlayClock`] decides which, and when to look again; it reads
//! the time it is given, so tests inject the clock.
#![deny(clippy::indexing_slicing)]

use super::super::PlayState;
use std::time::{Duration, Instant};

/// tdesktop `kWaitingShowDelay`: the opening spinner's delay.
pub(crate) const OPENING_SPINNER: Duration = Duration::from_millis(500);
/// "Connecting…" once the picture stood still this long.
pub(crate) const CONNECTING_AFTER: Duration = Duration::from_secs(5);

/// What the overlay is decided from, read once per draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LiveInput {
    /// The live player's state; `None` before there is a player (no feed
    /// yet, which reads as opening).
    pub(crate) state: Option<PlayState>,
    /// Nothing is expected: not joined, nobody broadcasting, or not
    /// playable here. The placeholder shows without a spinner.
    pub(crate) idle: bool,
    /// The host knows the picture failed (the call's connect reported an
    /// error) although the player has not.
    pub(crate) failed: bool,
}

/// What shows over the picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LiveOverlay {
    /// The picture plays (or the sender has no video): nothing.
    None,
    /// No picture yet: the placeholder, and the spinner past its delay.
    Opening {
        spinner: bool,
    },
    /// Stalled: the picture dimmed with the spinner, and "Connecting…"
    /// past [`CONNECTING_AFTER`].
    Stalled {
        connecting: bool,
    },
    Paused,
    /// Nothing expected: the placeholder alone.
    Idle,
    Ended,
    Failed,
}

/// The overlay's phase, whose start the clock remembers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    None,
    Opening,
    Stalled,
    Paused,
    Idle,
    Ended,
    Failed,
}

fn phase(input: LiveInput) -> Phase {
    if input.failed || input.state == Some(PlayState::Failed) {
        return Phase::Failed;
    }
    if input.state == Some(PlayState::Ended) {
        return Phase::Ended;
    }
    if input.idle {
        return Phase::Idle;
    }
    match input.state {
        None | Some(PlayState::Opening) => Phase::Opening,
        Some(PlayState::Buffering) => Phase::Stalled,
        Some(PlayState::Paused) => Phase::Paused,
        Some(PlayState::Ready | PlayState::Playing) => Phase::None,
        // Handled above.
        Some(PlayState::Ended | PlayState::Failed) => Phase::None,
    }
}

/// The overlay for `phase` after `elapsed` in it, and how long until it
/// changes by itself.
fn overlay_at(phase: Phase, elapsed: Duration) -> (LiveOverlay, Option<Duration>) {
    match phase {
        Phase::None => (LiveOverlay::None, None),
        Phase::Opening => {
            let left = OPENING_SPINNER
                .checked_sub(elapsed)
                .filter(|d| !d.is_zero());
            (
                LiveOverlay::Opening {
                    spinner: left.is_none(),
                },
                left,
            )
        }
        Phase::Stalled => {
            // Buffering begins once no frame came for the stall time, so
            // the picture has stood still that long already.
            let still = elapsed.saturating_add(super::super::STALL_AFTER);
            let left = CONNECTING_AFTER.checked_sub(still).filter(|d| !d.is_zero());
            (
                LiveOverlay::Stalled {
                    connecting: left.is_none(),
                },
                left,
            )
        }
        Phase::Paused => (LiveOverlay::Paused, None),
        Phase::Idle => (LiveOverlay::Idle, None),
        Phase::Ended => (LiveOverlay::Ended, None),
        Phase::Failed => (LiveOverlay::Failed, None),
    }
}

/// When each phase began, and the redraw armed for its next change.
#[derive(Default)]
pub(crate) struct LiveOverlayClock {
    phase: Option<(Phase, Instant)>,
    armed: Option<Instant>,
    timer: Option<gpui_kit::Task<()>>,
}

impl LiveOverlayClock {
    /// The overlay at `now`, and when to redraw for its next change if no
    /// redraw is armed for that time yet (arm it with [`Self::arm`]).
    pub(crate) fn observe(
        &mut self,
        input: LiveInput,
        now: Instant,
    ) -> (LiveOverlay, Option<Instant>) {
        let current = phase(input);
        let since = match self.phase {
            Some((phase, since)) if phase == current => since,
            _ => {
                self.phase = Some((current, now));
                now
            }
        };
        let (overlay, left) = overlay_at(current, now.saturating_duration_since(since));
        let due = left.and_then(|left| now.checked_add(left));
        if due.is_none() {
            self.armed = None;
            self.timer = None;
            return (overlay, None);
        }
        if due == self.armed {
            return (overlay, None);
        }
        (overlay, due)
    }

    /// Keep the redraw due at `due`.
    pub(crate) fn arm(&mut self, due: Instant, timer: gpui_kit::Task<()>) {
        self.armed = Some(due);
        self.timer = Some(timer);
    }
}

#[cfg(test)]
mod tests {
    use super::{CONNECTING_AFTER, LiveInput, LiveOverlay, LiveOverlayClock, OPENING_SPINNER};
    use crate::ui::video_player::{PlayState, STALL_AFTER};
    use std::time::{Duration, Instant};

    fn input(state: Option<PlayState>) -> LiveInput {
        LiveInput {
            state,
            idle: false,
            failed: false,
        }
    }

    #[test]
    fn opening_shows_the_spinner_after_half_a_second() {
        let t0 = Instant::now();
        let mut clock = LiveOverlayClock::default();
        let (overlay, due) = clock.observe(input(None), t0);
        assert_eq!(overlay, LiveOverlay::Opening { spinner: false });
        assert_eq!(due, Some(t0 + OPENING_SPINNER));
        // The player arrives, still opening: the same phase, same clock.
        let (overlay, _) = clock.observe(
            input(Some(PlayState::Opening)),
            t0 + Duration::from_millis(499),
        );
        assert_eq!(overlay, LiveOverlay::Opening { spinner: false });
        let (overlay, due) = clock.observe(input(Some(PlayState::Opening)), t0 + OPENING_SPINNER);
        assert_eq!(overlay, LiveOverlay::Opening { spinner: true });
        assert_eq!(due, None);
        // The first frame: nothing over the picture.
        let (overlay, _) = clock.observe(input(Some(PlayState::Playing)), t0 + OPENING_SPINNER);
        assert_eq!(overlay, LiveOverlay::None);
    }

    #[test]
    fn a_stall_dims_at_once_and_says_connecting_after_five_still_seconds() {
        let t0 = Instant::now();
        let mut clock = LiveOverlayClock::default();
        clock.observe(input(Some(PlayState::Playing)), t0);
        // Buffering is reported 1.5 s after the last frame.
        let stalled = t0 + STALL_AFTER;
        let (overlay, due) = clock.observe(input(Some(PlayState::Buffering)), stalled);
        assert_eq!(overlay, LiveOverlay::Stalled { connecting: false });
        let connecting_at = t0 + CONNECTING_AFTER;
        assert_eq!(due, Some(connecting_at));
        let (overlay, _) = clock.observe(
            input(Some(PlayState::Buffering)),
            t0 + CONNECTING_AFTER.saturating_sub(Duration::from_millis(1)),
        );
        assert_eq!(overlay, LiveOverlay::Stalled { connecting: false });
        let (overlay, due) = clock.observe(input(Some(PlayState::Buffering)), connecting_at);
        assert_eq!(overlay, LiveOverlay::Stalled { connecting: true });
        assert_eq!(due, None);
        // Frames again, then another stall: its clock starts over.
        clock.observe(input(Some(PlayState::Playing)), connecting_at);
        let again = connecting_at + Duration::from_secs(10);
        let (overlay, _) = clock.observe(input(Some(PlayState::Buffering)), again);
        assert_eq!(overlay, LiveOverlay::Stalled { connecting: false });
    }

    #[test]
    fn an_armed_redraw_is_asked_for_once() {
        let t0 = Instant::now();
        let mut clock = LiveOverlayClock::default();
        let (_, due) = clock.observe(input(None), t0);
        let due = due.expect("a redraw for the spinner");
        clock.arm(due, gpui_kit::Task::ready(()));
        let (_, again) = clock.observe(input(None), t0 + Duration::from_millis(100));
        assert_eq!(again, None, "armed already");
    }

    #[test]
    fn failure_and_end_win_over_everything_and_idle_has_no_spinner() {
        let t0 = Instant::now();
        let mut clock = LiveOverlayClock::default();
        let idle = LiveInput {
            state: None,
            idle: true,
            failed: false,
        };
        let (overlay, due) = clock.observe(idle, t0);
        assert_eq!(overlay, LiveOverlay::Idle);
        assert_eq!(due, None);
        let (overlay, _) = clock.observe(input(Some(PlayState::Paused)), t0);
        assert_eq!(overlay, LiveOverlay::Paused);
        let (overlay, _) = clock.observe(input(Some(PlayState::Ended)), t0);
        assert_eq!(overlay, LiveOverlay::Ended);
        let failed = LiveInput {
            state: Some(PlayState::Playing),
            idle: true,
            failed: true,
        };
        let (overlay, _) = clock.observe(failed, t0);
        assert_eq!(overlay, LiveOverlay::Failed);
        let (overlay, _) = clock.observe(input(Some(PlayState::Failed)), t0);
        assert_eq!(overlay, LiveOverlay::Failed);
        // A sender with no video (Ready) draws nothing over the picture.
        let (overlay, _) = clock.observe(input(Some(PlayState::Ready)), t0);
        assert_eq!(overlay, LiveOverlay::None);
    }
}
