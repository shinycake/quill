//! Your own microphone level in a group call: a level tap runs while you
//! are joined and unmuted, tdesktop's speaking rules decide when you
//! count as speaking (`calls::audio_level`), and
//! `setGroupCallParticipantIsSpeaking` tells TDLib (which marks your
//! participant and sends the speaking action to the server).
//!
//! The level comes from the call's own microphone stream
//! (`call_capture`), so nothing opens the device twice. Without that
//! stream the tap opens the default input device itself. tdesktop's remote levels
//! come from tgcalls per ssrc; Quill has no such signal, so other
//! participants' speaking state stays TDLib's `is_speaking`.

use super::*;
use crate::calls::audio_level::SpeakingTracker;
use crate::calls::capture::LevelCell;
use crate::calls::engine::CallEngine;
use crate::locks::LockExt;
use crate::state::{ActiveGroupCall, CallsPurpose, RequestPurpose};
use crate::telegram::requests::set_group_call_participant_is_speaking;
use crate::thread_guard::spawn_guarded;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
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

/// How often the poll reads your level while the tap is open: one
/// tgcalls window ([`crate::calls::audio_level::LEVEL_WINDOW_SAMPLES`]
/// at 48 kHz, 91.7 ms). The capture writes the level without waking the
/// poll loop, so the loop wakes for it.
const READ_EVERY: Duration = Duration::from_millis(92);

type Opened = Result<Box<dyn LevelSource + Send>, String>;

/// Driver-side state of the level tap and the speaking decision.
pub struct CallAudio {
    source: Option<Box<dyn LevelSource>>,
    pub(crate) opening: Option<Receiver<Opened>>,
    pub(crate) open_microphone: Arc<dyn Fn() -> Opened + Send + Sync>,
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
    /// When the poll last read the open source.
    read_at: Option<Instant>,
}

impl Default for CallAudio {
    fn default() -> Self {
        Self {
            source: None,
            opening: None,
            open_microphone: Arc::new(open_microphone),
            injected: None,
            injected_open: false,
            tracker: SpeakingTracker::default(),
            clock: Instant::now(),
            failed_at: None,
            error: None,
            speaking: false,
            read_at: None,
        }
    }
}

impl CallAudio {
    /// Use `source` instead of the microphone (tests, screenshot demos).
    pub fn inject(&mut self, source: Box<dyn LevelSource>) {
        self.injected = Some(source);
    }

    /// `at` in milliseconds on the tracker's clock.
    fn ms(&self, at: Instant) -> u64 {
        u64::try_from(at.saturating_duration_since(self.clock).as_millis()).unwrap_or(u64::MAX)
    }

    /// When the poll loop should wake to read the level: one level window
    /// after the last read while the tap is open and `measuring`, or while
    /// you are marked speaking (so the mark runs out on time).
    pub(crate) fn deadline(&self, now: Instant, measuring: bool) -> Option<Duration> {
        self.source.as_ref()?;
        if !measuring && !self.speaking {
            return None;
        }
        let since = self
            .read_at
            .map_or(READ_EVERY, |at| now.saturating_duration_since(at));
        Some(READ_EVERY.saturating_sub(since))
    }

    /// The latest level and whether you count as speaking.
    pub fn self_level(&self) -> (f32, bool) {
        (self.tracker.level(), self.speaking)
    }

    pub fn is_open(&self) -> bool {
        self.source.is_some()
    }

    /// Open the level source: an injected one, the call capture's level
    /// (`capture`), or the default microphone (no call capture).
    fn open(&mut self, capture: Option<Arc<LevelCell>>) {
        if self.source.is_some() {
            return;
        }
        if let Some(injected) = self.injected.take() {
            self.source = Some(injected);
            self.injected_open = true;
            return;
        }
        if let Some(level) = capture {
            // Drop what the stream measured before you could speak.
            let _ = level.take();
            self.source = Some(Box::new(CaptureLevel(level)));
            self.opening = None;
            self.failed_at = None;
            self.error = None;
            return;
        }
        if self.failed_at.is_some_and(|at| at.elapsed() < REOPEN_AFTER) {
            return;
        }
        let Some(opening) = self.opening.as_ref() else {
            self.opening = Some(self.open_on_worker());
            return;
        };
        let opened = match opening.try_recv() {
            Ok(opened) => opened,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("The microphone thread stopped.".to_string()),
        };
        self.opening = None;
        match opened {
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

    fn open_on_worker(&self) -> Receiver<Opened> {
        let (answer, opened) = mpsc::channel();
        let open = self.open_microphone.clone();
        let _ = spawn_guarded("quill-level-tap-open", move || {
            let _ = answer.send(open());
            crate::wake::UPDATES.poke();
        });
        opened
    }

    /// Close the microphone; an injected source goes back to waiting.
    fn close(&mut self) {
        self.opening = None;
        self.read_at = None;
        let source = self.source.take();
        if self.injected_open {
            self.injected = source;
            self.injected_open = false;
        }
    }
}

/// Your level as the call's own microphone stream measures it.
struct CaptureLevel(Arc<LevelCell>);

impl LevelSource for CaptureLevel {
    fn take_level(&self) -> Option<f32> {
        self.0.take()
    }

    fn error(&self) -> Option<String> {
        // The capture reports its own failures (`CallCapture::error`).
        None
    }
}

#[cfg(feature = "ui")]
fn open_microphone() -> Opened {
    // No call capture: the tap measures the raw microphone.
    crate::calls::level_tap::LevelTap::open_default()
        .map(|tap| Box::new(tap) as Box<dyn LevelSource + Send>)
}

#[cfg(not(feature = "ui"))]
fn open_microphone() -> Opened {
    Err("No microphone support in this build.".into())
}

/// What the tap needs from the tracked group call: the call id and your
/// audio source (ssrc) while you are joined and allowed to speak.
/// `None` closes the tap (not joined, muted, or muted by an admin).
pub fn tap_wanted(call: &ActiveGroupCall) -> Option<(i32, i32)> {
    if !call.is_joined || call.is_muted_self {
        return None;
    }
    if call.is_force_muted() {
        return None;
    }
    let me = call.participants.iter().find(|p| p.is_current_user)?;
    Some((call.id, me.audio_source_id))
}

impl<S: JsonSender> ConnectDriver<S> {
    /// Route the peer's camera, screen-share and microphone states from the
    /// engine's worker threads into the driver queues (Phase C2e / C2j),
    /// where the pump applies them behind the active-call gate.
    pub(crate) fn install_remote_state_hooks(&self, engine: &mut dyn CallEngine) {
        let outbox = self.video_state_outbox.clone();
        engine.set_remote_video_state_callback(Arc::new(move |call_id, state| {
            crate::advanced::debug_mode::debug_log(&format!(
                "[call] {call_id} peer video {state:?}"
            ));
            outbox.lock_or_recover().push_back((call_id, state));
            crate::wake::UPDATES.poke();
        }));
        let outbox = self.screen_state_outbox.clone();
        engine.set_remote_screen_state_callback(Arc::new(move |call_id, state| {
            crate::advanced::debug_mode::debug_log(&format!(
                "[call] {call_id} peer screen {state:?}"
            ));
            outbox.lock_or_recover().push_back((call_id, state));
            crate::wake::UPDATES.poke();
        }));
        let outbox = self.audio_state_outbox.clone();
        engine.set_remote_audio_state_callback(Arc::new(move |call_id, muted| {
            crate::advanced::debug_mode::debug_log(&format!("[call] {call_id} peer muted {muted}"));
            outbox.lock_or_recover().push_back((call_id, muted));
            crate::wake::UPDATES.poke();
        }));
    }

    /// Apply the peer's microphone state to the active 1:1 call.
    pub(crate) fn drain_remote_audio_state(&mut self) {
        loop {
            let update = self.audio_state_outbox.lock_or_recover().pop_front();
            let Some((call_id, muted)) = update else {
                break;
            };
            if let Some(call) = self
                .session
                .calls
                .active_call
                .as_mut()
                .filter(|call| call.id == call_id)
            {
                call.remote_audio_muted = muted;
            }
        }
    }

    /// The peer's battery state, from the engine, onto the active 1:1
    /// call; a report for another call is dropped.
    pub(crate) fn apply_peer_battery(&mut self, call_id: i32, is_low: bool) {
        crate::advanced::debug_mode::debug_log(&format!(
            "[call] {call_id} peer battery low {is_low}"
        ));
        if let Some(call) = self
            .session
            .calls
            .active_call
            .as_mut()
            .filter(|call| call.id == call_id)
        {
            call.remote_battery_low = is_low;
        }
    }

    /// The connection quality, from the engine, onto the active 1:1 call;
    /// a report for another call is dropped.
    pub(crate) fn apply_signal_bars(&mut self, call_id: i32, bars: u8) {
        if let Some(call) = self
            .session
            .calls
            .active_call
            .as_mut()
            .filter(|call| call.id == call_id)
        {
            call.signal_bars = Some(bars);
        }
    }

    /// Per poll: run the level tap for the tracked group call and tell
    /// TDLib when your speaking state changes. The level comes from the
    /// microphone, not from Telegram, so this must not wait for an
    /// update; a poll with no new level window still lets the speaking
    /// state run out. Returns whether your speaking state changed.
    pub(crate) fn pump_call_audio(&mut self, now: Instant) -> Result<bool, ConnectSendError> {
        self.drain_remote_audio_state();
        let was_speaking = self.call_audio.speaking;
        let wanted = self
            .session
            .calls
            .active_group_call
            .as_ref()
            .and_then(tap_wanted);
        let Some((group_call_id, audio_source)) = wanted else {
            let closing_speaking = self.call_audio.is_open() || self.call_audio.speaking;
            self.call_audio.close();
            self.call_audio.speaking = false;
            if self.call_audio.tracker.reset() && closing_speaking {
                // You stopped being able to speak while marked speaking.
                let group_call_id = self.session.calls.active_group_call.as_ref().map(|c| c.id);
                let audio_source = self.session.calls.active_group_call.as_ref().and_then(|c| {
                    c.participants
                        .iter()
                        .find(|p| p.is_current_user)
                        .map(|p| p.audio_source_id)
                });
                if let (Some(group_call_id), Some(audio_source)) = (group_call_id, audio_source) {
                    self.send_speaking(group_call_id, audio_source, false)?;
                }
            }
            return Ok(was_speaking);
        };
        let capture_level = self
            .call_capture
            .enabled()
            .then(|| self.call_capture.level());
        self.call_audio.open(capture_level);
        let Some(source) = self.call_audio.source.as_ref() else {
            return Ok(false);
        };
        let (level, error) = (source.take_level(), source.error());
        if let Some(err) = error {
            // The device went away: drop the tap and try again later.
            self.call_audio.close();
            self.call_audio.failed_at = Some(Instant::now());
            self.call_audio.error = Some(err);
            return Ok(false);
        }
        self.call_audio.read_at = Some(now);
        let now_ms = self.call_audio.ms(now);
        let update = match level {
            Some(level) => self.call_audio.tracker.update(level, false, now_ms),
            // No window since the last read (the poll ran early, or the
            // capture stopped): the kept-for time still runs out.
            None => self.call_audio.tracker.idle(now_ms),
        };
        self.call_audio.speaking = update.speaking;
        if let Some(is_speaking) = update.send
            && audio_source != 0
        {
            self.send_speaking(group_call_id, audio_source, is_speaking)?;
        }
        Ok(was_speaking != update.speaking)
    }

    /// [`CallAudio::deadline`]: the call microphone measures only while it
    /// runs; the raw tap measures whenever it is open.
    pub(crate) fn call_audio_deadline(&self, now: Instant) -> Option<Duration> {
        let measuring = !self.call_capture.enabled() || self.call_capture.target().is_some();
        self.call_audio.deadline(now, measuring)
    }

    /// Your own level and speaking state, for the call window.
    pub fn group_call_self_level(&self) -> (f32, bool) {
        self.call_audio.self_level()
    }

    /// The level tap's error, if it could not open the microphone.
    pub fn group_call_level_error(&self) -> Option<&str> {
        self.call_capture
            .error
            .as_deref()
            .or(self.call_audio.error.as_deref())
    }

    pub(crate) fn send_speaking(
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
