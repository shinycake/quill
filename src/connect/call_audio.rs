//! Your own microphone level in a group call: a level tap runs while you
//! are joined and unmuted, tdesktop's speaking rules decide when you
//! count as speaking (`calls::audio_level`), and
//! `setGroupCallParticipantIsSpeaking` tells TDLib (which marks your
//! participant and sends the speaking action to the server).
//!
//! ntgcalls hands Quill no capture samples, so the tap opens the default
//! input device beside the call's own capture. tdesktop's remote levels
//! come from tgcalls per ssrc; Quill has no such signal, so other
//! participants' speaking state stays TDLib's `is_speaking`.

use super::*;
use crate::calls::audio_level::SpeakingTracker;
use crate::state::{ActiveGroupCall, CallsPurpose, RequestPurpose};
use crate::telegram::requests::set_group_call_participant_is_speaking;
use std::time::{Duration, Instant};

#[cfg(feature = "ui")]
pub use crate::calls::level_tap::LevelSource;

/// The level-source trait when the UI feature (and cpal) is off: the
/// driver still compiles, with no source to open.
#[cfg(not(feature = "ui"))]
pub trait LevelSource {
    fn take_level(&self) -> Option<f32>;
    fn error(&self) -> Option<String>;
}

/// After a failed open, try the microphone again after this long.
const REOPEN_AFTER: Duration = Duration::from_secs(5);

/// Driver-side state of the level tap and the speaking decision.
pub struct CallAudio {
    source: Option<Box<dyn LevelSource>>,
    /// A source injected by tests or demos; used instead of opening
    /// the microphone, and kept across close/open.
    injected: Option<Box<dyn LevelSource>>,
    /// The open source is the injected one (so closing keeps it).
    injected_open: bool,
    tracker: SpeakingTracker,
    clock: Instant,
    failed_at: Option<Instant>,
    /// The last open error, for the call window's note.
    pub error: Option<String>,
    speaking: bool,
}

impl Default for CallAudio {
    fn default() -> Self {
        Self {
            source: None,
            injected: None,
            injected_open: false,
            tracker: SpeakingTracker::default(),
            clock: Instant::now(),
            failed_at: None,
            error: None,
            speaking: false,
        }
    }
}

impl CallAudio {
    /// Use `source` instead of the microphone (tests, screenshot demos).
    pub fn inject(&mut self, source: Box<dyn LevelSource>) {
        self.injected = Some(source);
    }

    /// Milliseconds since the driver started, for the tracker.
    fn now_ms(&self) -> u64 {
        u64::try_from(self.clock.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// The latest level and whether you count as speaking.
    pub fn self_level(&self) -> (f32, bool) {
        (self.tracker.level(), self.speaking)
    }

    pub fn is_open(&self) -> bool {
        self.source.is_some()
    }

    fn open(&mut self) {
        if self.source.is_some() {
            return;
        }
        if let Some(injected) = self.injected.take() {
            self.source = Some(injected);
            self.injected_open = true;
            return;
        }
        if self.failed_at.is_some_and(|at| at.elapsed() < REOPEN_AFTER) {
            return;
        }
        match open_microphone() {
            Ok(source) => {
                self.source = Some(source);
                self.failed_at = None;
                self.error = None;
            }
            Err(err) => {
                self.failed_at = Some(Instant::now());
                self.error = Some(err);
            }
        }
    }

    /// Close the microphone; an injected source goes back to waiting.
    fn close(&mut self) {
        let source = self.source.take();
        if self.injected_open {
            self.injected = source;
            self.injected_open = false;
        }
    }
}

#[cfg(feature = "ui")]
fn open_microphone() -> Result<Box<dyn LevelSource>, String> {
    crate::calls::level_tap::LevelTap::open_default()
        .map(|tap| Box::new(tap) as Box<dyn LevelSource>)
}

#[cfg(not(feature = "ui"))]
fn open_microphone() -> Result<Box<dyn LevelSource>, String> {
    Err("No microphone support in this build.".into())
}

/// What the tap needs from the tracked group call: the call id and your
/// audio source (ssrc) while you are joined and allowed to speak.
/// `None` closes the tap (not joined, muted, or muted by an admin).
pub fn tap_wanted(call: &ActiveGroupCall) -> Option<(i32, i32)> {
    if !call.is_joined || call.is_muted_self {
        return None;
    }
    let me = call.participants.iter().find(|p| p.is_current_user)?;
    if me.is_muted_for_all_users && !me.can_unmute_self {
        return None;
    }
    Some((call.id, me.audio_source_id))
}

impl<S: JsonSender> ConnectDriver<S> {
    /// Run the level tap for the tracked group call and tell TDLib when
    /// your speaking state changes. Called from the driver pump.
    pub(crate) fn pump_call_audio(&mut self) -> Result<(), ConnectSendError> {
        let wanted = self.session.active_group_call.as_ref().and_then(tap_wanted);
        let Some((group_call_id, audio_source)) = wanted else {
            let closing_speaking = self.call_audio.is_open() || self.call_audio.speaking;
            self.call_audio.close();
            self.call_audio.speaking = false;
            if self.call_audio.tracker.reset() && closing_speaking {
                // You stopped being able to speak while marked speaking.
                let group_call_id = self.session.active_group_call.as_ref().map(|c| c.id);
                let audio_source = self.session.active_group_call.as_ref().and_then(|c| {
                    c.participants
                        .iter()
                        .find(|p| p.is_current_user)
                        .map(|p| p.audio_source_id)
                });
                if let (Some(group_call_id), Some(audio_source)) = (group_call_id, audio_source) {
                    self.send_speaking(group_call_id, audio_source, false)?;
                }
            }
            return Ok(());
        };
        self.call_audio.open();
        let Some(level) = self
            .call_audio
            .source
            .as_ref()
            .and_then(|source| source.take_level())
        else {
            return Ok(());
        };
        if let Some(err) = self
            .call_audio
            .source
            .as_ref()
            .and_then(|source| source.error())
        {
            // The device went away: drop the tap and try again later.
            self.call_audio.close();
            self.call_audio.failed_at = Some(Instant::now());
            self.call_audio.error = Some(err);
            return Ok(());
        }
        let now = self.call_audio.now_ms();
        let update = self.call_audio.tracker.update(level, false, now);
        self.call_audio.speaking = update.speaking;
        if let Some(is_speaking) = update.send
            && audio_source != 0
        {
            self.send_speaking(group_call_id, audio_source, is_speaking)?;
        }
        Ok(())
    }

    /// Your own level and speaking state, for the call window.
    pub fn group_call_self_level(&self) -> (f32, bool) {
        self.call_audio.self_level()
    }

    /// The level tap's error, if it could not open the microphone.
    pub fn group_call_level_error(&self) -> Option<&str> {
        self.call_audio.error.as_deref()
    }

    fn send_speaking(
        &mut self,
        group_call_id: i32,
        audio_source: i32,
        is_speaking: bool,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::SetGroupCallParticipantIsSpeaking {
                group_call_id,
            }),
            None,
        );
        if let Err(err) = self
            .sender
            .send_json(&set_group_call_participant_is_speaking(
                extra,
                group_call_id,
                audio_source,
                is_speaking,
            ))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(())
    }
}
