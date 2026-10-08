//! OS media keys and the "now playing" widget.
//!
//! Telegram Desktop hooks the system transport controls on every platform
//! (`platform/*/specific_*`: `MPRemoteCommandCenter` on macOS, MPRIS on
//! Linux, `SystemMediaTransportControls` on Windows) so the keyboard
//! play/pause key, headphone buttons and the lock-screen widget drive the
//! in-app player. This module is the cross-platform seam: the UI calls
//! [`publish`] with what is playing and drains [`take_commands`] on its
//! playback tick; each backend turns that into the native API.
//!
//! Backends only push [`Command`]s into a queue (their callbacks arrive on
//! foreign threads), so the app state is only ever touched from the UI
//! thread.

use std::sync::Mutex;
use std::time::{Duration, Instant};

#[cfg(all(feature = "ui", target_os = "linux"))]
mod linux;
#[cfg(all(feature = "ui", target_os = "macos"))]
mod mac;
#[cfg(all(feature = "ui", windows))]
mod windows_smtc;

#[cfg(all(feature = "ui", target_os = "linux"))]
use linux as backend;
#[cfg(all(feature = "ui", target_os = "macos"))]
use mac as backend;
#[cfg(all(feature = "ui", windows))]
use windows_smtc as backend;

/// A transport command from the OS.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    Play,
    Pause,
    Toggle,
    Stop,
    Next,
    Previous,
    /// Absolute position in seconds.
    SeekTo(f64),
}

/// What the OS widget shows.
#[derive(Clone, Debug, PartialEq)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub duration_secs: f64,
    pub position_secs: f64,
    pub playing: bool,
    /// Playback speed, so the OS can extrapolate the position.
    pub rate: f64,
    pub can_next: bool,
    pub can_previous: bool,
}

static COMMANDS: Mutex<Vec<Command>> = Mutex::new(Vec::new());
static LAST: Mutex<Option<(NowPlaying, Instant)>> = Mutex::new(None);

/// Queue a command (called by backends from any thread).
#[cfg(all(feature = "ui", any(target_os = "macos", target_os = "linux", windows)))]
pub(crate) fn push_command(command: Command) {
    COMMANDS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .push(command);
}

/// Drain the commands received since the last call.
pub fn take_commands() -> Vec<Command> {
    std::mem::take(
        &mut *COMMANDS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
    )
}

/// Position error beyond which the OS widget is told about a seek.
const DRIFT_SECS: f64 = 1.5;

/// Whether `new` differs from what the OS already shows. The position is
/// compared against where the widget extrapolates it to (`age` after
/// `old`), so a steady play costs no calls and a seek does.
pub fn needs_update(old: &NowPlaying, age: Duration, new: &NowPlaying) -> bool {
    old.title != new.title
        || old.artist != new.artist
        || old.playing != new.playing
        || old.can_next != new.can_next
        || old.can_previous != new.can_previous
        || (old.duration_secs - new.duration_secs).abs() > 0.5
        || (old.rate - new.rate).abs() > 0.01
        || position_jumped(old, age, new)
}

/// Whether `new.position_secs` is not where `old` extrapolates to `age`
/// later: the listener seeked (or the track restarted).
pub fn position_jumped(old: &NowPlaying, age: Duration, new: &NowPlaying) -> bool {
    let expected = old.position_secs
        + if old.playing {
            age.as_secs_f64() * old.rate
        } else {
            0.0
        };
    (expected - new.position_secs).abs() > DRIFT_SECS
}

/// Show what is playing (`None` clears the widget). Cheap to call every
/// tick: the backend is only touched when something changed.
pub fn publish(info: Option<&NowPlaying>) {
    let mut last = LAST.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    match (info, last.as_ref()) {
        (None, None) => return,
        (Some(new), Some((old, at))) if !needs_update(old, at.elapsed(), new) => return,
        _ => {}
    }
    *last = info.map(|info| (info.clone(), Instant::now()));
    drop(last);
    set_backend(info);
}

#[cfg(all(feature = "ui", any(target_os = "macos", target_os = "linux", windows)))]
fn set_backend(info: Option<&NowPlaying>) {
    backend::set(info);
}

#[cfg(not(all(feature = "ui", any(target_os = "macos", target_os = "linux", windows))))]
fn set_backend(_info: Option<&NowPlaying>) {}

#[cfg(test)]
mod tests {
    use super::{NowPlaying, needs_update};
    use std::time::Duration;

    fn playing(position: f64) -> NowPlaying {
        NowPlaying {
            title: "Night Drive".into(),
            artist: "Ada".into(),
            duration_secs: 200.0,
            position_secs: position,
            playing: true,
            rate: 1.0,
            can_next: true,
            can_previous: false,
        }
    }

    #[test]
    fn steady_playback_is_not_republished() {
        let old = playing(10.0);
        assert!(!needs_update(&old, Duration::from_secs(5), &playing(15.2)));
    }

    #[test]
    fn a_seek_is_republished() {
        let old = playing(10.0);
        assert!(needs_update(&old, Duration::from_secs(1), &playing(60.0)));
    }

    #[test]
    fn a_pause_or_new_track_is_republished() {
        let old = playing(10.0);
        let mut paused = playing(10.0);
        paused.playing = false;
        assert!(needs_update(&old, Duration::ZERO, &paused));
        let mut other = playing(10.0);
        other.title = "Other".into();
        assert!(needs_update(&old, Duration::ZERO, &other));
    }

    #[test]
    fn a_paused_widget_does_not_extrapolate() {
        let mut old = playing(10.0);
        old.playing = false;
        let mut new = playing(10.0);
        new.playing = false;
        assert!(!needs_update(&old, Duration::from_secs(30), &new));
    }

    #[test]
    fn faster_playback_extrapolates_faster() {
        let mut old = playing(10.0);
        old.rate = 2.0;
        let mut new = playing(30.0);
        new.rate = 2.0;
        assert!(!needs_update(&old, Duration::from_secs(10), &new));
    }
}
