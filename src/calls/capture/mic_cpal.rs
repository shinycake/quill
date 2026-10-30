//! The microphone through cpal (CoreAudio / WASAPI / ALSA-PulseAudio-
//! PipeWire), the backend voice notes already record with
//! (`voice_input`). The device's own format is down-mixed to mono and,
//! only when the device cannot run at 48 kHz, resampled; the samples then
//! go through [`MicProcessor`] on the audio callback and into the sink as
//! 10 ms frames.
#![deny(clippy::indexing_slicing)]

use super::{
    AudioFrame, CaptureError, CaptureEvent, FrameSink, MicCapture, MicControls, MicProcessor,
};
use crate::calls::engine::EXTERNAL_AUDIO_RATE;
use crate::cpal_stream::{
    RequestedInput, StreamErrors, device_name_matches, downmix_into, requested_input,
};
use crate::locks::LockExt;
use crate::resample::Resampler;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    FromSample, I24, SampleFormat, SizedSample, Stream, StreamConfig, SupportedStreamConfig,
};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// A cpal microphone feeding a [`FrameSink`].
pub struct CpalMic {
    controls: Arc<MicControls>,
    stream: Option<Stream>,
    /// Set by the stream's error callback when the stream ends (device
    /// unplugged, …); dropouts and reroutes only go to the debug log.
    error: Arc<Mutex<Option<String>>>,
}

impl CpalMic {
    pub fn new(controls: Arc<MicControls>) -> Self {
        Self {
            controls,
            stream: None,
            error: Arc::new(Mutex::new(None)),
        }
    }

    /// The error that stopped the stream, if any.
    pub fn error(&self) -> Option<String> {
        self.error.lock_or_recover().clone()
    }
}

impl MicCapture for CpalMic {
    fn start(
        &mut self,
        device: Option<&str>,
        sink: FrameSink<AudioFrame>,
    ) -> Result<(), CaptureError> {
        self.stop();
        *self.error.lock_or_recover() = None;
        let host = cpal::default_host();
        let device = find_input(&host, device).ok_or(CaptureError::NoDevice)?;
        let supported = input_config(&device)?;
        let channels = usize::from(supported.channels()).max(1);
        let rate = supported.sample_rate();
        let config: StreamConfig = supported.config();
        let ctx = Callback {
            controls: self.controls.clone(),
            sink: sink.clone(),
            channels,
            rate,
        };
        let on_error = {
            let error = self.error.clone();
            // An xrun or a reroute to the new default device leaves the
            // stream running; failing the call microphone for one would
            // stop it and reopen it seconds later.
            let mut errors = StreamErrors::new("call microphone");
            move |err: cpal::Error| {
                let Some(err) = errors.check(err) else {
                    return;
                };
                let text = format!("Microphone stopped: {err}");
                error.lock_or_recover().get_or_insert_with(|| text.clone());
                let failure = if crate::voice_input::is_permission_error(&err) {
                    CaptureError::Permission(crate::voice_input::friendly_error(&err))
                } else {
                    CaptureError::Device(text)
                };
                sink.push_event(CaptureEvent::Failed(failure));
            }
        };
        let stream = match supported.sample_format() {
            SampleFormat::F32 => build::<f32>(&device, config, ctx, on_error),
            SampleFormat::F64 => build::<f64>(&device, config, ctx, on_error),
            SampleFormat::I16 => build::<i16>(&device, config, ctx, on_error),
            SampleFormat::U16 => build::<u16>(&device, config, ctx, on_error),
            SampleFormat::I24 => build::<I24>(&device, config, ctx, on_error),
            SampleFormat::I32 => build::<i32>(&device, config, ctx, on_error),
            other => Err(CaptureError::Unsupported(format!(
                "Unsupported microphone format ({other:?})."
            ))),
        }?;
        stream.play().map_err(|err| classify(&err))?;
        self.stream = Some(stream);
        Ok(())
    }

    fn stop(&mut self) {
        // Dropping the stream closes the device.
        self.stream = None;
    }
}

/// The input device named `name` (as the engine lists it), or the system
/// default when there is no name, the name is the engine's default entry,
/// or no device has that name. See [`requested_input`] for the names.
fn find_input(host: &cpal::Host, name: Option<&str>) -> Option<cpal::Device> {
    if let Some(name) = name
        && let RequestedInput::Named(wanted) = requested_input(name, cfg!(windows))
        && let Ok(mut devices) = host.input_devices()
        && let Some(device) = devices.find(|device| {
            device
                .description()
                .is_ok_and(|description| device_name_matches(wanted, description.name()))
        })
    {
        return Some(device);
    }
    host.default_input_device()
}

/// The device's default format, at 48 kHz when the device offers it (no
/// resampling then), the rate the call library takes.
fn input_config(device: &cpal::Device) -> Result<SupportedStreamConfig, CaptureError> {
    let default = device
        .default_input_config()
        .map_err(|err| classify(&err))?;
    if default.sample_rate() == EXTERNAL_AUDIO_RATE {
        return Ok(default);
    }
    let at_48k = device
        .supported_input_configs()
        .ok()
        .and_then(|mut ranges| {
            ranges.find_map(|range| {
                (range.channels() == default.channels()
                    && range.sample_format() == default.sample_format())
                .then(|| range.try_with_sample_rate(EXTERNAL_AUDIO_RATE))
                .flatten()
            })
        });
    Ok(at_48k.unwrap_or(default))
}

fn classify(err: &cpal::Error) -> CaptureError {
    let text = crate::voice_input::friendly_error(err);
    if crate::voice_input::is_permission_error(err) {
        CaptureError::Permission(text)
    } else {
        CaptureError::Device(text)
    }
}

struct Callback {
    controls: Arc<MicControls>,
    sink: FrameSink<AudioFrame>,
    channels: usize,
    rate: u32,
}

fn build<T>(
    device: &cpal::Device,
    config: StreamConfig,
    ctx: Callback,
    on_error: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<Stream, CaptureError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let Callback {
        controls,
        sink,
        channels,
        rate,
    } = ctx;
    let mut resampler = Resampler::new(rate, EXTERNAL_AUDIO_RATE);
    let mut processor = MicProcessor::new();
    let mut mono = Vec::new();
    let mut resampled = Vec::new();
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                let now = Instant::now();
                downmix_into(data, channels, &mut mono);
                resampled.clear();
                resampler.process(&mono, &mut resampled);
                processor.push(&resampled, &controls, now, |frame| sink.push(frame));
            },
            on_error,
            None,
        )
        .map_err(|err| classify(&err))
}
