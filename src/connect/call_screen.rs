//! The shared screen captured by Quill (`calls::capture::screen`): once
//! the chooser picked a source and the call's screen stream is set up (a
//! group call's presentation is connected, or a 1:1 call with a
//! transport shares its screen), the source is captured into the call's
//! external screen source (`CallEngine::external_frame_sender`).
//!
//! Pause follows tdesktop (`calls_group_call.cpp`, `setOnPause` and
//! `SendUpdateType::ScreenPaused`): when the shared window is hidden the
//! feeder stops forwarding and a group call sends
//! `toggleGroupCallScreenSharingIsPaused(true)`, once per edge; when it
//! comes back, frames and `false`. 1:1 calls have no paused state in
//! tdesktop ("Paused not supported right now", `calls_call.cpp`), so
//! there only the frames stop. A closed window or a failed capture ends
//! the share.
//!
//! Your own picture for your tile comes from the capture's preview slot,
//! a few times a second, into the group frame slots.
//!
//! "Share System Audio" (Windows, tdesktop's chooser checkbox): while a
//! share picked with it runs, a loopback capture records what the
//! computer plays and the call microphone (`call_capture`) mixes it into
//! its frames. It stops with the share, or when the box is cleared.
//!
//! Builds without a screen backend never start this, and calls there
//! cannot share a screen.

use super::*;
use crate::calls::capture::feeder::VIDEO_SINK_FRAMES;
use crate::calls::capture::screen::{
    PreviewSlot, ScreenCapture, ScreenOptions, SourceId, native_backend,
};
use crate::calls::capture::system_audio::{
    SystemAudioCapture, SystemAudioMix, system_audio_supported,
};
use crate::calls::capture::{
    CaptureError, CaptureEvent, CaptureFeeder, FrameSink, VideoFrame, device_thread::DeviceThread,
};
use crate::calls::engine::{
    CaptureTarget, ExternalFrameSender, TransportState, external_screen_capture_enabled,
};
use crate::state::{CallsPurpose, RequestPurpose};
use crate::telegram::envelope::MessageSender;
use crate::telegram::requests::toggle_group_call_screen_sharing_is_paused;
use std::sync::Arc;

/// Starts a screen capture on its own thread: `(source, options, sink,
/// preview)`. The driver uses the OS backend; tests inject a fake.
pub type ScreenOpener = Arc<
    dyn Fn(
            &SourceId,
            ScreenOptions,
            FrameSink<VideoFrame>,
            Arc<PreviewSlot>,
        ) -> Result<Box<dyn ScreenCapture>, CaptureError>
        + Send
        + Sync,
>;

/// Starts recording system audio into the mix on its own thread. The
/// driver uses WASAPI loopback; tests inject a fake.
pub type SystemAudioOpener = Arc<
    dyn Fn(Arc<SystemAudioMix>) -> Result<Box<dyn SystemAudioCapture>, CaptureError> + Send + Sync,
>;

/// Driver-side state of the shared screen.
#[derive(Default)]
pub struct CallScreen {
    /// The source the chooser picked for the next or running share.
    chosen: Option<SourceId>,
    /// Whether the chooser's "Share System Audio" was ticked with it.
    with_audio: bool,
    active: Option<ActiveScreen>,
    opener: Option<ScreenOpener>,
    audio_opener: Option<SystemAudioOpener>,
    /// Bumped for every preview handed to the UI, so its image cache
    /// never mistakes a new capture's picture for an old one.
    preview_seq: u64,
    /// Why the share stopped or could not start, for the call window.
    pub error: Option<String>,
}

struct ActiveScreen {
    target: CaptureTarget,
    source: SourceId,
    sink: FrameSink<VideoFrame>,
    sender: Arc<dyn ExternalFrameSender>,
    preview: Arc<PreviewSlot>,
    last_preview: u64,
    paused: bool,
    /// The system audio shared with it, while running.
    audio: Option<SharedAudio>,
    capture: DeviceThread,
    feeder: Option<CaptureFeeder>,
}

struct SharedAudio {
    mix: Arc<SystemAudioMix>,
    capture: DeviceThread,
    opening: bool,
}

impl SharedAudio {
    fn open(opener: Option<SystemAudioOpener>) -> Self {
        let mix = Arc::new(SystemAudioMix::default());
        let capture = DeviceThread::open(
            "quill-system-audio",
            {
                let mix = mix.clone();
                move || match opener {
                    Some(opener) => opener(mix),
                    None => open_loopback(mix),
                }
            },
            |mut capture| capture.stop(),
        );
        Self {
            mix,
            capture,
            opening: true,
        }
    }
}

impl CallScreen {
    /// Use `opener` instead of the OS backend (tests, demos). Also turns
    /// Quill's screen capture on in builds without a backend.
    pub fn inject_opener(&mut self, opener: ScreenOpener) {
        self.opener = Some(opener);
    }

    /// Whether Quill captures the shared screen (and so shows its own
    /// chooser). Unit tests
    /// never reach the real screen: there only an injected capture
    /// turns it on.
    pub fn enabled(&self) -> bool {
        self.opener.is_some() || (!cfg!(test) && external_screen_capture_enabled())
    }

    /// Use `opener` instead of WASAPI loopback (tests, demos). Also
    /// turns system audio on in builds without it.
    pub fn inject_audio_opener(&mut self, opener: SystemAudioOpener) {
        self.audio_opener = Some(opener);
    }

    /// Whether this build can share system audio (the chooser shows its
    /// checkbox).
    pub fn system_audio_enabled(&self) -> bool {
        self.audio_opener.is_some() || (!cfg!(test) && system_audio_supported())
    }

    /// Whether the picked share includes system audio.
    pub fn with_audio(&self) -> bool {
        self.with_audio
    }

    /// Whether system audio is being recorded for the running share.
    pub fn sharing_audio(&self) -> bool {
        self.active
            .as_ref()
            .and_then(|active| active.audio.as_ref())
            .is_some_and(|audio| !audio.opening)
    }

    /// The source being shared right now.
    pub fn source(&self) -> Option<&SourceId> {
        self.active.as_ref().map(|active| &active.source)
    }

    /// The source picked for the next share.
    pub fn chosen(&self) -> Option<&SourceId> {
        self.chosen.as_ref()
    }

    /// The call the screen feeds right now.
    pub fn target(&self) -> Option<CaptureTarget> {
        self.active
            .as_ref()
            .filter(|active| active.feeder.is_some())
            .map(|active| active.target)
    }

    pub fn opening(&self) -> bool {
        self.active.as_ref().is_some_and(|active| {
            active.feeder.is_none() || active.audio.as_ref().is_some_and(|audio| audio.opening)
        })
    }

    /// Whether the shared window is hidden.
    pub fn paused(&self) -> bool {
        self.active.as_ref().is_some_and(|active| active.paused)
    }

    /// Frames the engine accepted from the running capture.
    pub fn frames_sent(&self) -> u64 {
        self.active
            .as_ref()
            .and_then(|active| active.feeder.as_ref())
            .map_or(0, |feeder| feeder.stats().sent())
    }

    fn open(
        &mut self,
        target: CaptureTarget,
        source: SourceId,
        sender: Arc<dyn ExternalFrameSender>,
    ) {
        let sink = FrameSink::new(VIDEO_SINK_FRAMES);
        let preview = Arc::new(PreviewSlot::default());
        let options = ScreenOptions::default();
        let opener = self.opener.clone();
        let capture = DeviceThread::open(
            "quill-screen-open",
            {
                let source = source.clone();
                let sink = sink.clone();
                let preview = preview.clone();
                move || match (opener, native_backend()) {
                    (Some(opener), _) => opener(&source, options, sink, preview),
                    (None, Some(backend)) => backend.start(&source, options, sink, preview),
                    (None, None) => Err(CaptureError::Unsupported(
                        "Screen capture isn't available in this build.".into(),
                    )),
                }
            },
            |mut capture| capture.stop(),
        );
        self.active = Some(ActiveScreen {
            target,
            source,
            sink,
            sender,
            preview,
            last_preview: 0,
            paused: false,
            audio: None,
            capture,
            feeder: None,
        });
    }

    /// How the open went; `Ok` says whether events read while it ran
    /// paused the share.
    fn finish_open(&mut self) -> Option<Result<bool, String>> {
        let active = self
            .active
            .as_mut()
            .filter(|active| active.feeder.is_none())?;
        let started = active
            .capture
            .opened()?
            .map_err(|err| err.to_string())
            .and_then(|()| {
                CaptureFeeder::start(active.sink.clone(), active.sender.clone())
                    .map_err(|err| format!("Couldn't start screen sharing: {err}"))
            });
        Some(match started {
            Ok(feeder) => {
                active.feeder = Some(feeder);
                let paused = active.paused;
                self.error = None;
                Ok(paused)
            }
            Err(err) => {
                self.active = None;
                Err(err)
            }
        })
    }
}

#[cfg(all(feature = "ui", windows))]
fn open_loopback(mix: Arc<SystemAudioMix>) -> Result<Box<dyn SystemAudioCapture>, CaptureError> {
    let capture = crate::calls::capture::system_audio_cpal::start_loopback(mix)?;
    Ok(Box::new(capture))
}

#[cfg(not(all(feature = "ui", windows)))]
fn open_loopback(_mix: Arc<SystemAudioMix>) -> Result<Box<dyn SystemAudioCapture>, CaptureError> {
    Err(CaptureError::Unsupported(
        "System audio can't be shared in this build.".into(),
    ))
}

/// What the pump learned from the running capture this time.
enum ScreenChange {
    None,
    Paused(bool),
    Ended(Option<String>),
}

impl<S: JsonSender> ConnectDriver<S> {
    /// Pick the source to share (the chooser's "Share Screen"), with or
    /// without system audio. A running share switches to it on the next
    /// pump.
    pub fn choose_call_screen_source(&mut self, source: SourceId, with_audio: bool) {
        self.call_screen.chosen = Some(source);
        self.call_screen.with_audio = with_audio && self.call_screen.system_audio_enabled();
        self.call_screen.error = None;
    }

    /// Use a fake system audio capture (tests, demos).
    pub fn inject_call_system_audio(&mut self, opener: SystemAudioOpener) {
        self.call_screen.inject_audio_opener(opener);
    }

    /// The shared screen's state, for the UI and tests.
    pub fn call_screen(&self) -> &CallScreen {
        &self.call_screen
    }

    /// Use a fake screen capture (tests, demos).
    pub fn inject_call_screen(&mut self, opener: ScreenOpener) {
        self.call_screen.inject_opener(opener);
    }

    /// Which call's screen stream is set up for Quill's frames.
    fn screen_wanted(&self) -> Option<CaptureTarget> {
        if let Some(call) = self.session.calls.active_call.as_ref() {
            let up = matches!(
                call.transport,
                Some(
                    TransportState::Connecting
                        | TransportState::Connected
                        | TransportState::Reconnecting
                )
            );
            return (call.screen_sharing && up).then_some(CaptureTarget::Private(call.id));
        }
        let call = self.session.calls.active_group_call.as_ref()?;
        (call.is_joined && call.screen_sharing).then_some(CaptureTarget::Group(call.id))
    }

    /// Start, follow and stop the shared screen. Called from the driver
    /// pump.
    pub(crate) fn pump_call_screen(&mut self) {
        if !self.call_screen.enabled() {
            return;
        }
        self.pump_shared_screen();
        self.pump_system_audio();
    }

    /// Start or stop the system audio of the running share, and hand it
    /// to the call microphone.
    fn pump_system_audio(&mut self) {
        let screen = &mut self.call_screen;
        let wanted = screen.with_audio && screen.system_audio_enabled();
        let opener = screen.audio_opener.clone();
        let mut failed = None;
        let running = screen
            .active
            .as_mut()
            .filter(|active| active.feeder.is_some());
        let mix = match running {
            Some(active) if wanted => {
                let audio = active
                    .audio
                    .get_or_insert_with(|| SharedAudio::open(opener));
                if audio.opening {
                    match audio.capture.opened() {
                        Some(Ok(())) => audio.opening = false,
                        Some(Err(err)) => failed = Some(err.to_string()),
                        None => {}
                    }
                }
                let mix = audio.mix.clone();
                if failed.is_some() {
                    active.audio = None;
                }
                failed.is_none().then_some(mix)
            }
            Some(active) => {
                active.audio = None;
                None
            }
            None => None,
        };
        if let Some(err) = failed {
            // The screen keeps going without it; no retry every pump.
            screen.with_audio = false;
            screen.error = Some(err);
        }
        let same = match (self.call_capture.system_audio(), mix.as_ref()) {
            (Some(have), Some(want)) => Arc::ptr_eq(have, want),
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.call_capture.set_system_audio(mix);
        }
    }

    fn pump_shared_screen(&mut self) {
        let wanted = self.screen_wanted();
        let chosen = self.call_screen.chosen.clone();
        // A different call or source: close the old capture first.
        let stale = self.call_screen.active.as_ref().is_some_and(|active| {
            wanted != Some(active.target) || chosen.as_ref() != Some(&active.source)
        });
        if stale {
            self.call_screen.active = None;
            // The share ended: the next one asks for a source again.
            if wanted.is_none() {
                self.call_screen.chosen = None;
                self.call_screen.with_audio = false;
            }
        }
        match self.screen_change() {
            ScreenChange::None => {}
            ScreenChange::Paused(paused) => self.set_screen_paused(paused),
            ScreenChange::Ended(error) => {
                self.call_screen.active = None;
                self.call_screen.chosen = None;
                self.call_screen.with_audio = false;
                self.call_screen.error = error;
                self.stop_sharing_screen();
                return;
            }
        }
        let Some(target) = wanted else {
            if let Some(call) = self.session.calls.active_group_call.as_mut() {
                call.screen_share_paused = false;
            }
            return;
        };
        if self.call_screen.active.is_none() {
            let Some(source) = chosen else {
                return;
            };
            let Some(sender) = self
                .call_engine
                .as_deref_mut()
                .filter(|engine| engine.is_available())
                .and_then(|engine| engine.external_frame_sender(target))
            else {
                return;
            };
            self.call_screen.open(target, source, sender);
        }
        match self.call_screen.finish_open() {
            Some(Ok(paused)) => self.set_screen_paused(paused),
            Some(Err(err)) => {
                self.call_screen.chosen = None;
                self.call_screen.error = Some(err);
                self.stop_sharing_screen();
                return;
            }
            None => {}
        }
        self.publish_screen_preview();
    }

    /// Read the running capture's events: the last pause edge, or the end.
    fn screen_change(&mut self) -> ScreenChange {
        let Some(active) = self.call_screen.active.as_mut() else {
            return ScreenChange::None;
        };
        let mut paused = active.paused;
        for event in active.sink.take_events() {
            match event {
                CaptureEvent::Paused => paused = true,
                CaptureEvent::Resumed => paused = false,
                CaptureEvent::Ended => return ScreenChange::Ended(None),
                CaptureEvent::Failed(err) => return ScreenChange::Ended(Some(err.to_string())),
            }
        }
        if paused == active.paused {
            ScreenChange::None
        } else {
            ScreenChange::Paused(paused)
        }
    }

    /// Apply a pause edge: frames stop or resume, and a group call tells
    /// Telegram (`toggleGroupCallScreenSharingIsPaused`).
    fn set_screen_paused(&mut self, paused: bool) {
        let Some(active) = self.call_screen.active.as_mut() else {
            return;
        };
        let changed = active.paused != paused;
        active.paused = paused;
        if let Some(feeder) = active.feeder.as_ref() {
            feeder.set_forwarding(!paused);
        }
        let CaptureTarget::Group(group_call_id) = active.target else {
            return;
        };
        let Some(call) = self
            .session
            .calls
            .active_group_call
            .as_mut()
            .filter(|call| call.id == group_call_id)
        else {
            return;
        };
        let was = call.screen_share_paused;
        call.screen_share_paused = paused;
        // A fresh share starts unpaused on the server: only edges go out.
        if !changed && was == paused {
            return;
        }
        let extra = self.session.request(
            RequestPurpose::Calls(CallsPurpose::ToggleGroupCallVideo { group_call_id }),
            None,
        );
        if self
            .sender
            .send_json(&toggle_group_call_screen_sharing_is_paused(
                extra,
                group_call_id,
                paused,
            ))
            .is_err()
        {
            self.session.requests.take(extra);
        }
    }

    /// End the share the capture can no longer feed.
    fn stop_sharing_screen(&mut self) {
        if let Some(call) = self.session.calls.active_call.as_ref()
            && call.screen_sharing
        {
            let call_id = call.id;
            let _ = self.set_call_screen_share(call_id, false);
            return;
        }
        let sharing = self
            .session
            .calls
            .active_group_call
            .as_ref()
            .is_some_and(|call| call.screen_sharing || call.screen_share_pending);
        if sharing {
            let _ = self.toggle_group_call_screen_share();
        }
        if let Some(call) = self.session.calls.active_group_call.as_mut() {
            call.screen_share_paused = false;
        }
    }

    /// Your shared screen for your own tile in the group call window.
    fn publish_screen_preview(&mut self) {
        let Some(active) = self.call_screen.active.as_mut() else {
            return;
        };
        let CaptureTarget::Group(group_call_id) = active.target else {
            return;
        };
        let Some((seq, picture)) = active.preview.latest_after(active.last_preview) else {
            return;
        };
        active.last_preview = seq;
        let me = self
            .session
            .calls
            .active_group_call
            .as_ref()
            .and_then(|call| call.participants.iter().find(|p| p.is_current_user))
            .and_then(|me| match me.participant_id {
                MessageSender::User { user_id } => Some(user_id),
                MessageSender::Chat { .. } => None,
            })
            .or(self.session.my_user_id);
        let (Some(user_id), Ok(width), Ok(height)) = (
            me,
            u16::try_from(picture.width),
            u16::try_from(picture.height),
        ) else {
            return;
        };
        self.call_screen.preview_seq = self.call_screen.preview_seq.wrapping_add(1);
        // Your tile plays through the same live player as everyone else's:
        // the small preview (480 px at 4 fps) goes over as I420.
        let Some(planes) =
            crate::calls::live_video::convert::rgba_to_i420(&picture.rgba, width, height)
        else {
            return;
        };
        let Some(frame) = crate::calls::live_video::LiveFrame::new(
            self.call_screen.preview_seq,
            width,
            height,
            crate::calls::live_video::LiveRotation::None,
            planes,
            std::time::Instant::now(),
        ) else {
            return;
        };
        self.push_live_frame(
            crate::calls::live_video::LiveKey::Video {
                call_id: group_call_id,
                user_id,
                screen: true,
            },
            frame,
        );
    }
}
