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
    MicControls, device_thread::DeviceThread,
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

/// Driver-side state of the call microphone.
#[derive(Default)]
pub struct CallCapture {
    active: Option<ActiveCapture>,
    /// Your level, written by whichever capture runs; the level reader
    /// (`call_audio`) keeps this across restarts.
    level: Arc<LevelCell>,
    opener: Option<MicOpener>,
    failed_at: Option<Instant>,
    /// System audio to mix into the microphone frames, while shared.
    system_audio: Option<Arc<SystemAudioMix>>,
    /// Why the microphone could not be opened or stopped, for the call
    /// window's note.
    pub error: Option<String>,
    /// Poked when the microphone has news; the poll loop's by default.
    wake: Option<&'static Signal>,
}

struct ActiveCapture {
    target: CaptureTarget,
    device: Option<String>,
    controls: Arc<MicControls>,
    sink: FrameSink<AudioFrame>,
    sender: Arc<dyn ExternalFrameSender>,
    mic: DeviceThread,
    feeder: Option<CaptureFeeder>,
}

impl CallCapture {
    /// Use `opener` instead of cpal (tests, demos). Also turns capture
    /// on in builds and modes where it would be off.
    pub fn inject_opener(&mut self, opener: MicOpener) {
        self.opener = Some(opener);
    }

    /// Poke `wake` instead of the poll loop's signal (tests).
    pub fn inject_wake(&mut self, wake: &'static Signal) {
        self.wake = Some(wake);
    }

    fn wake(&self) -> &'static Signal {
        self.wake.unwrap_or(&crate::wake::UPDATES)
    }

    /// Whether Quill captures the call microphone at all. Unit tests
    /// never open the real microphone: there only an injected capture
    /// turns it on.
    pub fn enabled(&self) -> bool {
        self.opener.is_some() || (!cfg!(test) && external_capture_enabled())
    }

    /// The call the microphone feeds right now.
    pub fn target(&self) -> Option<CaptureTarget> {
        self.active
            .as_ref()
            .filter(|active| active.feeder.is_some())
            .map(|active| active.target)
    }

    pub fn opening(&self) -> bool {
        self.active
            .as_ref()
            .is_some_and(|active| active.feeder.is_none())
    }

    /// The level written by the running capture.
    pub fn level(&self) -> Arc<LevelCell> {
        self.level.clone()
    }

    /// Frames the engine accepted from the running capture.
    pub fn frames_sent(&self) -> u64 {
        self.active
            .as_ref()
            .and_then(|active| active.feeder.as_ref())
            .map_or(0, |feeder| feeder.stats().sent())
    }

    /// The raw microphone's highest level since the last take while the
    /// running capture is muted (tgcalls' scale).
    pub fn take_muted_level(&self) -> Option<f32> {
        self.active
            .as_ref()
            .and_then(|active| active.controls.muted_level().take())
    }

    /// Whether the running capture sends silence.
    pub fn muted(&self) -> Option<bool> {
        self.active.as_ref().map(|active| active.controls.muted())
    }

    /// Whether the running capture denoises.
    pub fn noise_suppression(&self) -> Option<bool> {
        self.active
            .as_ref()
            .map(|active| active.controls.noise_suppression())
    }

    fn stop(&mut self) {
        self.active = None;
    }

    /// Mix `mix` into the microphone frames (or stop mixing), now and in
    /// every microphone opened later.
    pub fn set_system_audio(&mut self, mix: Option<Arc<SystemAudioMix>>) {
        if let Some(active) = self.active.as_ref() {
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
        self.active
            .as_ref()
            .is_some_and(|active| active.controls.system_audio().is_some())
    }

    pub(crate) fn until_reopen(&self, now: Instant) -> Option<Duration> {
        let waited = now.saturating_duration_since(self.failed_at?);
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
        let sink = FrameSink::waking(AUDIO_SINK_FRAMES, self.wake());
        let opener = self.opener.clone();
        let mic = DeviceThread::open(
            "quill-call-mic",
            self.wake(),
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
        self.active = Some(ActiveCapture {
            target,
            device,
            controls,
            sink,
            sender,
            mic,
            feeder: None,
        });
    }

    fn finish_open(&mut self, now: Instant) {
        let Some(active) = self
            .active
            .as_mut()
            .filter(|active| active.feeder.is_none())
        else {
            return;
        };
        let Some(answer) = active.mic.opened() else {
            return;
        };
        let failure = match answer {
            Ok(()) => match CaptureFeeder::start(active.sink.clone(), active.sender.clone()) {
                Ok(feeder) => {
                    active.feeder = Some(feeder);
                    self.failed_at = None;
                    self.error = None;
                    return;
                }
                Err(err) => format!("Couldn't start the call microphone: {err}"),
            },
            Err(err) => err.to_string(),
        };
        self.active = None;
        self.failed_at = Some(now);
        self.error = Some(failure);
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
                call.transport,
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
        if !call.is_joined || !call.transport_ready {
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

    pub(crate) fn pump_call_media(&mut self, now: Instant) -> bool {
        let shown = |driver: &Self| {
            (
                driver.call_capture.target(),
                driver.call_capture.error.clone(),
                driver.call_screen.target(),
                driver.call_screen.paused(),
                driver.call_screen.error.clone(),
            )
        };
        let before = shown(self);
        self.warm_call_devices();
        self.pump_call_capture(now);
        self.pump_call_screen();
        before != shown(self)
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
        if self.call_devices_warmed || !coming {
            return;
        }
        if let Some(engine) = self.call_engine.as_deref() {
            engine.warm_devices();
            self.call_devices_warmed = true;
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
        if capture.active.as_ref().is_some_and(|active| {
            !wanted.is_some_and(|(target, _)| target == active.target) || active.device != device
        }) {
            capture.stop();
        }
        // A device that failed or went away: report it and retry later.
        let failure = capture.active.as_ref().and_then(|active| {
            active
                .sink
                .take_events()
                .into_iter()
                .find_map(|event| match event {
                    CaptureEvent::Failed(err) => Some(err.to_string()),
                    CaptureEvent::Ended => Some("The microphone was disconnected.".to_string()),
                    CaptureEvent::Paused | CaptureEvent::Resumed => None,
                })
        });
        if let Some(err) = failure {
            capture.stop();
            capture.failed_at = Some(now);
            capture.error = Some(err);
        }
        let Some((target, muted)) = wanted else {
            capture.failed_at = None;
            capture.error = None;
            return;
        };
        let noise_suppression = denoises(target, group_pref);
        capture.finish_open(now);
        if let Some(active) = capture.active.as_ref() {
            active.controls.set_muted(muted);
            active.controls.set_noise_suppression(noise_suppression);
            return;
        }
        if !capture.enabled()
            || capture
                .failed_at
                .is_some_and(|at| now.saturating_duration_since(at) < REOPEN_AFTER)
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
        if let Some(active) = self.call_capture.active.as_ref() {
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
