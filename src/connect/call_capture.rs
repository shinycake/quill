//! The call microphone captured by Quill (`calls::capture`): while a 1:1
//! call has a transport or you are joined to a group call whose
//! transport is up, the chosen microphone runs through cpal, noise
//! suppression and the feeder into the call's external source
//! (`CallEngine::external_frame_sender`). Muting keeps the stream open
//! and sends silence. 1:1 calls always denoise; group calls follow the
//! "Enable Noise Suppression" switch, which applies to the running
//! stream at once. The same stream feeds your level
//! (`call_audio`), so the microphone is opened once.
//!
//! Shared system audio (`call_screen`, Windows) is handed to the running
//! microphone's controls and mixed into its frames
//! (`calls::capture::system_audio`).
//!
//! Builds without the UI feature have no capture backend
//! (`calls::engine::external_capture_enabled`).

use super::*;
use crate::calls::capture::feeder::AUDIO_SINK_FRAMES;
use crate::calls::capture::system_audio::SystemAudioMix;
use crate::calls::capture::{
    AudioFrame, CaptureError, CaptureEvent, CaptureFeeder, FrameSink, LevelCell, MicCapture,
    MicControls,
    device_thread::{DeviceRun, DeviceThread},
};
use crate::calls::engine::{
    CaptureTarget, ExternalFrameSender, MediaDeviceKind, TransportState, external_capture_enabled,
};
use crate::wake::Signal;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Opens a microphone backend on its own thread: `(device name, controls,
/// sink)`. The driver uses cpal; tests inject a fake.
pub type MicOpener = Arc<
    dyn Fn(
            Option<&str>,
            Arc<MicControls>,
            FrameSink<AudioFrame>,
        ) -> Result<Box<dyn MicCapture>, CaptureError>
        + Send
        + Sync,
>;

/// After a failed open or a lost device, open the microphone again after
/// this long (the design's 2 s).
const REOPEN_AFTER: Duration = Duration::from_secs(2);

/// Where the call microphone stands.
#[derive(Default)]
enum Mic {
    /// Nothing is wanted, or nothing runs yet.
    #[default]
    Off,
    /// Opening or running.
    Active(ActiveCapture),
    /// The microphone could not be opened or stopped at `at`; the error is
    /// the call window's note. It opens again after [`REOPEN_AFTER`].
    Failed { at: Instant, error: String },
    /// The call refused a run of frames while this was wanted (call,
    /// microphone, muted): reopening the same would be refused again, so
    /// it waits until the call, the microphone or the mute changes, or the
    /// call's transport goes down and comes back.
    Refused {
        wanted: (CaptureTarget, Option<String>, bool),
        error: String,
    },
}

/// Driver-side state of the call microphone.
#[derive(Default)]
pub struct CallCapture {
    mic: Mic,
    /// Your level, written by whichever capture runs; the level reader
    /// (`call_audio`) keeps this across restarts.
    level: Arc<LevelCell>,
    opener: Option<MicOpener>,
    /// System audio to mix into the microphone frames, while shared.
    system_audio: Option<Arc<SystemAudioMix>>,
    /// Poked when the open microphone has news; the poll loop's by default.
    sink_wake: Option<&'static Signal>,
}

struct ActiveCapture {
    target: CaptureTarget,
    device: Option<String>,
    controls: Arc<MicControls>,
    sink: FrameSink<AudioFrame>,
    sender: Arc<dyn ExternalFrameSender>,
    mic: DeviceThread,
    run: DeviceRun<CaptureFeeder>,
}

impl CallCapture {
    fn active(&self) -> Option<&ActiveCapture> {
        match &self.mic {
            Mic::Active(active) => Some(active),
            _ => None,
        }
    }

    fn active_mut(&mut self) -> Option<&mut ActiveCapture> {
        match &mut self.mic {
            Mic::Active(active) => Some(active),
            _ => None,
        }
    }

    /// Why the microphone could not be opened or stopped, for the call
    /// window's note.
    pub fn error(&self) -> Option<&str> {
        match &self.mic {
            Mic::Failed { error, .. } | Mic::Refused { error, .. } => Some(error),
            Mic::Off | Mic::Active(_) => None,
        }
    }

    /// Use `opener` instead of cpal (tests, demos). Also turns capture
    /// on in builds and modes where it would be off.
    pub fn inject_opener(&mut self, opener: MicOpener) {
        self.opener = Some(opener);
    }

    /// Have the microphone's sink poke `wake` instead of the poll loop's
    /// signal (tests). The device thread keeps its own, so a test sees the
    /// sink's poke alone.
    pub fn inject_sink_wake(&mut self, wake: &'static Signal) {
        self.sink_wake = Some(wake);
    }

    /// Whether Quill captures the call microphone at all. Unit tests
    /// never open the real microphone: there only an injected capture
    /// turns it on.
    pub fn enabled(&self) -> bool {
        self.opener.is_some() || (!cfg!(test) && external_capture_enabled())
    }

    /// The call the microphone feeds right now.
    pub fn target(&self) -> Option<CaptureTarget> {
        self.active()
            .filter(|active| !active.run.is_opening())
            .map(|active| active.target)
    }

    pub fn opening(&self) -> bool {
        self.active().is_some_and(|active| active.run.is_opening())
    }

    /// The level written by the running capture.
    pub fn level(&self) -> Arc<LevelCell> {
        self.level.clone()
    }

    /// Frames the engine accepted from the running capture.
    pub fn frames_sent(&self) -> u64 {
        self.active()
            .and_then(|active| active.run.running())
            .map_or(0, |feeder| feeder.stats().sent())
    }

    /// The raw microphone's highest level since the last take while the
    /// running capture is muted (tgcalls' scale).
    pub fn take_muted_level(&self) -> Option<f32> {
        self.active()
            .and_then(|active| active.controls.muted_level().take())
    }

    /// Whether the running capture sends silence.
    pub fn muted(&self) -> Option<bool> {
        self.active().map(|active| active.controls.muted())
    }

    /// Whether the running capture denoises.
    pub fn noise_suppression(&self) -> Option<bool> {
        self.active()
            .map(|active| active.controls.noise_suppression())
    }

    /// Close the running stream; a failure on record stays.
    fn stop(&mut self) {
        if matches!(self.mic, Mic::Active(_)) {
            self.mic = Mic::Off;
        }
    }

    /// Mix `mix` into the microphone frames (or stop mixing), now and in
    /// every microphone opened later.
    pub fn set_system_audio(&mut self, mix: Option<Arc<SystemAudioMix>>) {
        if let Some(active) = self.active() {
            active.controls.set_system_audio(mix.clone());
        }
        self.system_audio = mix;
    }

    /// The system audio handed to the microphone.
    pub fn system_audio(&self) -> Option<&Arc<SystemAudioMix>> {
        self.system_audio.as_ref()
    }

    /// Whether system audio is mixed into the running microphone.
    pub fn mixes_system_audio(&self) -> bool {
        self.active()
            .is_some_and(|active| active.controls.system_audio().is_some())
    }

    pub(crate) fn until_reopen(&self, now: Instant) -> Option<Duration> {
        let Mic::Failed { at, .. } = &self.mic else {
            return None;
        };
        let waited = now.saturating_duration_since(*at);
        REOPEN_AFTER
            .checked_sub(waited)
            .filter(|left| !left.is_zero())
    }

    fn open(
        &mut self,
        target: CaptureTarget,
        device: Option<String>,
        noise_suppression: bool,
        muted: bool,
        sender: Arc<dyn ExternalFrameSender>,
    ) {
        let controls = Arc::new(MicControls::new(noise_suppression, self.level.clone()));
        controls.set_muted(muted);
        controls.set_system_audio(self.system_audio.clone());
        let sink_wake = self.sink_wake.unwrap_or(&crate::wake::UPDATES);
        let sink = FrameSink::waking(AUDIO_SINK_FRAMES, sink_wake);
        let opener = self.opener.clone();
        let mic = DeviceThread::open(
            "quill-call-mic",
            &crate::wake::UPDATES,
            {
                let device = device.clone();
                let controls = controls.clone();
                let sink = sink.clone();
                move || match opener {
                    Some(opener) => opener(device.as_deref(), controls, sink),
                    None => open_cpal(device.as_deref(), controls, sink),
                }
            },
            |mut mic| mic.stop(),
        );
        self.mic = Mic::Active(ActiveCapture {
            target,
            device,
            controls,
            sink,
            sender,
            mic,
            run: DeviceRun::Opening,
        });
    }

    fn finish_open(&mut self, now: Instant) {
        let Some(active) = self.active_mut().filter(|active| active.run.is_opening()) else {
            return;
        };
        let Some(answer) = active.mic.opened() else {
            return;
        };
        let failure = match answer {
            Ok(()) => match CaptureFeeder::start(active.sink.clone(), active.sender.clone()) {
                Ok(feeder) => {
                    active.run = DeviceRun::Running(feeder);
                    return;
                }
                Err(err) => format!("Couldn't start the call microphone: {err}"),
            },
            Err(err) => err.to_string(),
        };
        self.mic = Mic::Failed {
            at: now,
            error: failure,
        };
    }
}

#[cfg(feature = "ui")]
fn open_cpal(
    device: Option<&str>,
    controls: Arc<MicControls>,
    sink: FrameSink<AudioFrame>,
) -> Result<Box<dyn MicCapture>, CaptureError> {
    let mut mic = crate::calls::capture::mic_cpal::CpalMic::new(controls);
    mic.start(device, sink)?;
    Ok(Box::new(mic))
}

#[cfg(not(feature = "ui"))]
fn open_cpal(
    _device: Option<&str>,
    _controls: Arc<MicControls>,
    _sink: FrameSink<AudioFrame>,
) -> Result<Box<dyn MicCapture>, CaptureError> {
    Err(CaptureError::Unsupported(
        "No microphone support in this build.".into(),
    ))
}

impl<S: JsonSender> ConnectDriver<S> {
    /// Which call the microphone should feed, and whether it is muted:
    /// a 1:1 call with a transport, else a joined group call whose
    /// transport is up.
    fn capture_wanted(&self) -> Option<(CaptureTarget, bool)> {
        if let Some(call) = self.session.calls.active_call.as_ref()
            && matches!(
                call.transport.state(),
                Some(
                    TransportState::Connecting
                        | TransportState::Connected
                        | TransportState::Reconnecting
                )
            )
        {
            return Some((CaptureTarget::Private(call.id), call.muted));
        }
        let call = self.session.calls.active_group_call.as_ref()?;
        if !call.is_joined || !call.transport.is_connected() {
            return None;
        }
        Some((
            CaptureTarget::Group(call.id),
            call.is_muted_self || call.is_force_muted(),
        ))
    }

    /// The chosen microphone's name as the engine lists it (`None`: the
    /// system default).
    fn capture_device_name(&self) -> Option<String> {
        let id = self.selected_devices.0.as_deref()?;
        self.call_devices_cache
            .iter()
            .find(|device| device.kind == MediaDeviceKind::Microphone && device.id == id)
            .map(|device| device.name.clone())
    }

    /// Per poll: the call's devices and what they reported. Returns whether
    /// the UI should redraw.
    pub(crate) fn pump_call_media(&mut self, now: Instant) -> bool {
        let shown = |driver: &Self| {
            (
                driver.call_capture.target(),
                driver.call_capture.error().map(str::to_string),
                driver.call_screen.target(),
                driver.call_screen.paused(),
                driver.call_screen.error().map(str::to_string),
            )
        };
        let before = shown(self);
        self.warm_call_devices();
        let listed = self.take_listed_call_devices();
        let landed = self.land_group_offers();
        self.key_group_call_by_chat();
        self.pump_call_capture(now);
        self.pump_call_screen(now);
        listed || landed || before != shown(self)
    }

    fn warm_call_devices(&mut self) {
        let calls = &self.session.calls;
        // A tracked video chat may be one you only look at or one that is
        // scheduled; only a ringing, placed or joined call needs devices.
        let coming = calls.active_call.is_some()
            || calls
                .active_group_call
                .as_ref()
                .is_some_and(|call| call.is_joined);
        if !matches!(self.call_devices_listing, DeviceListing::Cold) || !coming {
            return;
        }
        if self.call_engine.is_some() {
            self.refresh_call_devices();
        }
    }

    /// Start, follow and stop the call microphone. Called from the
    /// driver pump.
    pub(crate) fn pump_call_capture(&mut self, now: Instant) {
        let wanted = self.capture_wanted();
        let device = self.capture_device_name();
        let group_pref = self.session.calls.prefs.call_noise_suppression;
        let capture = &mut self.call_capture;
        // A different call or microphone: close the old stream first.
        if capture.active().is_some_and(|active| {
            !wanted.is_some_and(|(target, _)| target == active.target) || active.device != device
        }) {
            capture.stop();
        }
        // A device that failed or went away: report it and retry later.
        let failure = capture.active().and_then(|active| {
            active
                .sink
                .take_events()
                .into_iter()
                .find_map(|event| match event {
                    CaptureEvent::Failed(err) => {
                        Some((err.to_string(), matches!(err, CaptureError::Refused(_))))
                    }
                    CaptureEvent::Ended => {
                        Some(("The microphone was disconnected.".to_string(), false))
                    }
                    CaptureEvent::Paused | CaptureEvent::Resumed => None,
                })
        });
        if let Some((err, refused)) = failure {
            capture.mic = match (refused, wanted) {
                (true, Some((target, muted))) => Mic::Refused {
                    wanted: (target, device.clone(), muted),
                    error: err,
                },
                (true, None) => Mic::Off,
                (false, _) => Mic::Failed {
                    at: now,
                    error: err,
                },
            };
        }
        let Some((target, muted)) = wanted else {
            capture.mic = Mic::Off;
            return;
        };
        let noise_suppression = denoises(target, group_pref);
        capture.finish_open(now);
        if let Some(active) = capture.active() {
            active.controls.set_muted(muted);
            active.controls.set_noise_suppression(noise_suppression);
            return;
        }
        if let Mic::Refused { wanted, .. } = &capture.mic {
            if wanted.0 == target && wanted.1 == device && wanted.2 == muted {
                return;
            }
            capture.mic = Mic::Off;
        }
        if !capture.enabled()
            || matches!(&capture.mic, Mic::Failed { at, .. } if now.saturating_duration_since(*at) < REOPEN_AFTER)
        {
            return;
        }
        // The engine hands out a sender once the call's sources exist.
        let Some(sender) = self
            .call_engine
            .as_deref_mut()
            .filter(|engine| engine.is_available())
            .and_then(|engine| engine.external_frame_sender(target))
        else {
            return;
        };
        capture.open(target, device, noise_suppression, muted, sender);
    }

    /// Apply the noise suppression switch (the voice chat window's
    /// Settings box) to the running group call microphone now.
    pub fn apply_call_noise_suppression(&mut self) {
        let pref = self.session.calls.prefs.call_noise_suppression;
        if let Some(active) = self.call_capture.active() {
            active
                .controls
                .set_noise_suppression(denoises(active.target, pref));
        }
    }

    /// The call microphone's state, for tests and diagnostics.
    pub fn call_capture(&self) -> &CallCapture {
        &self.call_capture
    }

    /// Use a fake microphone (tests, demos).
    pub fn inject_call_microphone(&mut self, opener: MicOpener) {
        self.call_capture.inject_opener(opener);
    }
}

/// Whether a call's microphone runs through noise suppression. As in
/// tdesktop, 1:1 calls always do (`calls_call.cpp` sets `enableNS = true`
/// on its config), and group calls follow the switch in the voice chat
/// window's Settings box (`calls_group_settings.cpp`).
fn denoises(target: CaptureTarget, group_pref: bool) -> bool {
    match target {
        CaptureTarget::Private(_) => true,
        CaptureTarget::Group(_) => group_pref,
    }
}
