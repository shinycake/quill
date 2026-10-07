//! Call sounds (tdesktop `Calls::Call` plays `call_incoming`,
//! `call_outgoing`, `call_connect`, `call_end` and `call_busy`). Telegram's
//! own tones can't ship in an MIT app; on macOS Quill plays the system's
//! FaceTime sounds instead: the ringtone, the ringback, the connect and
//! end chimes, the busy tone and the mute clicks. Nothing plays where
//! they don't exist.

use std::path::{Path, PathBuf};
use std::process::Child;
use std::time::{Duration, Instant};

const TELEPHONY: &str =
    "/System/Library/PrivateFrameworks/TelephonyUtilities.framework/Versions/A/Resources";
const RINGTONES: &str =
    "/System/Library/PrivateFrameworks/ToneLibrary.framework/Versions/A/Resources/Ringtones";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CallSound {
    /// Someone is calling (loops).
    Incoming,
    /// The peer's phone is ringing (loops).
    Ringback,
    Connect,
    End,
    Busy,
    Mute,
    Unmute,
}

impl CallSound {
    fn path(self) -> PathBuf {
        let telephony = |name: &str| Path::new(TELEPHONY).join(name);
        match self {
            CallSound::Incoming => Path::new(RINGTONES).join("Opening.m4r"),
            CallSound::Ringback => telephony("vc~ringing.aif"),
            CallSound::Connect => telephony("vc~invitation-accepted.caf"),
            CallSound::End => telephony("vc~ended.caf"),
            CallSound::Busy => telephony("busy_tone_cept.caf"),
            CallSound::Mute => telephony("mute.caf"),
            CallSound::Unmute => telephony("unmute.caf"),
        }
    }

    /// The pause before a looping sound starts again.
    fn gap(self) -> Duration {
        match self {
            CallSound::Ringback => Duration::from_millis(1200),
            _ => Duration::from_millis(400),
        }
    }
}

/// The one call sound playing (a new one cuts the last), and the loop to
/// keep going.
#[derive(Default)]
pub(super) struct CallSounds {
    child: Option<Child>,
    looping: Option<CallSound>,
    /// When the current loop's sound may start again.
    again_at: Option<Instant>,
}

impl CallSounds {
    fn spawn(&mut self, sound: CallSound) {
        self.stop_child();
        let path = sound.path();
        if !cfg!(target_os = "macos") || !path.is_file() {
            return;
        }
        self.child = std::process::Command::new("afplay")
            .arg("--")
            .arg(path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .ok();
    }

    fn stop_child(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Play once, cutting whatever plays (and any loop).
    pub(super) fn play(&mut self, sound: CallSound) {
        self.looping = None;
        self.again_at = None;
        self.spawn(sound);
    }

    /// Keep `sound` looping, or stop looping with `None`. Call it often
    /// (every call tick): it restarts the loop after its gap.
    pub(super) fn keep_looping(&mut self, sound: Option<CallSound>) {
        if self.looping != sound {
            if self.looping.is_some() || sound.is_some() {
                self.stop_child();
            }
            self.looping = sound;
            self.again_at = Some(Instant::now());
        }
        let Some(sound) = self.looping else {
            return;
        };
        let finished = self
            .child
            .as_mut()
            .is_none_or(|child| child.try_wait().ok().flatten().is_some());
        if !finished {
            return;
        }
        match self.again_at {
            Some(at) if Instant::now() >= at => {
                self.again_at = None;
                self.spawn(sound);
            }
            Some(_) => {}
            None => self.again_at = Some(Instant::now() + sound.gap()),
        }
    }
}

impl Drop for CallSounds {
    fn drop(&mut self) {
        self.stop_child();
    }
}

/// What has played for the current call, so each cue plays once.
#[derive(Default)]
pub(super) struct SoundMarks {
    call_id: Option<i32>,
    connected: bool,
    ended: bool,
    muted: bool,
}

impl super::app::QuillApp {
    /// Ring, ring back, chime on connect and on hang-up, click on mute.
    pub(super) fn sync_call_sounds(&mut self) {
        use quill::calls::engine::TransportState;
        use quill::telegram::envelope::CallState;
        let Some(session) = self.session() else {
            return;
        };
        let (call, summary) = (session.active_call.clone(), session.call_summary.clone());
        let marks = &mut self.call_sound_marks;
        let sounds = &mut self.call_sounds;
        match (call, summary) {
            (Some(call), _) => {
                if marks.call_id != Some(call.id) {
                    *marks = SoundMarks {
                        call_id: Some(call.id),
                        muted: call.muted,
                        ..SoundMarks::default()
                    };
                }
                let ringing = match call.state {
                    CallState::Pending { .. } if !call.is_outgoing => Some(CallSound::Incoming),
                    CallState::Pending {
                        is_received: true, ..
                    } => Some(CallSound::Ringback),
                    _ => None,
                };
                sounds.keep_looping(ringing);
                let connected = matches!(call.state, CallState::Ready)
                    && call.transport == Some(TransportState::Connected);
                if connected && !marks.connected {
                    marks.connected = true;
                    sounds.play(CallSound::Connect);
                }
                if marks.muted != call.muted {
                    marks.muted = call.muted;
                    sounds.play(if call.muted {
                        CallSound::Mute
                    } else {
                        CallSound::Unmute
                    });
                }
            }
            (None, Some(summary)) => {
                sounds.keep_looping(None);
                if marks.call_id == Some(summary.call_id) && !marks.ended {
                    marks.ended = true;
                    sounds.play(if summary.busy {
                        CallSound::Busy
                    } else {
                        CallSound::End
                    });
                }
            }
            (None, None) => sounds.keep_looping(None),
        }
    }
}
