//! Settings > Calls, "Other" block (tdesktop `settings_calls.cpp`,
//! `BuildOtherSection`): "Accept calls on this device" and "Open system
//! sound preferences". The rules live here so the UI file stays thin.
//!
//! tdesktop has a microphone level meter (Quill's `ui::call_audio_ui`) but
//! no output "Test" playback, so Quill has none either.
#![deny(clippy::indexing_slicing)]

use crate::telegram::envelope::ParsedSession;

pub const ACCEPT_CALLS_TITLE: &str = "Accept calls on this device";
pub const SYSTEM_PREFS_TITLE: &str = "Open system sound preferences";

/// The current session's id and whether it accepts calls, from the
/// `getActiveSessions` list. `None` until the list arrives (or when no
/// session is flagged current).
pub fn current_session_accepts_calls(sessions: &[ParsedSession]) -> Option<(i64, bool)> {
    sessions
        .iter()
        .find(|session| session.is_current)
        .map(|session| (session.id, session.can_accept_calls))
}

/// What to hand the OS to show its sound settings, on platforms where
/// that works reliably. Linux has no single place (GNOME, KDE and the
/// PipeWire / PulseAudio tools differ and `xdg-open` has no sound
/// scheme), so the row is hidden there.
pub fn system_sound_target() -> Option<&'static str> {
    sound_target_for(cfg!(target_os = "macos"), cfg!(windows))
}

fn sound_target_for(mac: bool, windows: bool) -> Option<&'static str> {
    if mac {
        Some("x-apple.systempreferences:com.apple.Sound-Settings.extension")
    } else if windows {
        Some("ms-settings:sound")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{current_session_accepts_calls, sound_target_for};
    use crate::telegram::envelope::ParsedSession;

    fn session(id: i64, current: bool, calls: bool) -> ParsedSession {
        ParsedSession {
            id,
            is_current: current,
            can_accept_calls: calls,
            ..Default::default()
        }
    }

    #[test]
    fn picks_the_current_session() {
        let list = [session(1, false, true), session(2, true, false)];
        assert_eq!(current_session_accepts_calls(&list), Some((2, false)));
        assert_eq!(
            current_session_accepts_calls(&[session(3, false, true)]),
            None
        );
        assert_eq!(current_session_accepts_calls(&[]), None);
    }

    #[test]
    fn sound_settings_route_per_platform() {
        assert!(
            sound_target_for(true, false)
                .unwrap()
                .starts_with("x-apple")
        );
        assert_eq!(sound_target_for(false, true), Some("ms-settings:sound"));
        assert_eq!(sound_target_for(false, false), None);
    }
}
