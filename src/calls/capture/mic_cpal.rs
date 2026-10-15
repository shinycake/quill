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
use crate::locks::LockExt;
use crate::voice_opus::Resampler;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig, SupportedStreamConfig,
};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// A cpal microphone feeding a [`FrameSink`].
pub struct CpalMic {
    controls: Arc<MicControls>,
    stream: Option<Stream>,
    /// Set by the stream's error callback (device unplugged, …).
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
            move |err: cpal::StreamError| {
                let text = format!("Microphone stopped: {err}");
                error.lock_or_recover().get_or_insert_with(|| text.clone());
                sink.push_event(CaptureEvent::Failed(CaptureError::Device(text)));
            }
        };
        let stream = match supported.sample_format() {
            SampleFormat::F32 => build::<f32>(&device, &config, ctx, on_error),
            SampleFormat::I16 => build::<i16>(&device, &config, ctx, on_error),
            SampleFormat::U16 => build::<u16>(&device, &config, ctx, on_error),
            SampleFormat::I32 => build::<i32>(&device, &config, ctx, on_error),
            other => Err(CaptureError::Unsupported(format!(
                "Unsupported microphone format ({other:?})."
            ))),
        }?;
        stream.play().map_err(|err| classify(&err.to_string()))?;
        self.stream = Some(stream);
        Ok(())
    }

    fn stop(&mut self) {
        // Dropping the stream closes the device.
        self.stream = None;
    }
}

/// The input device named `name` (as the engine lists it), or the system
/// default when there is no name or no device by that name.
fn find_input(host: &cpal::Host, name: Option<&str>) -> Option<cpal::Device> {
    if let Some(name) = name
        && let Ok(mut devices) = host.input_devices()
        && let Some(device) = devices.find(|device| {
            device
                .description()
                .is_ok_and(|description| description.name() == name)
        })
    {
        return Some(device);
    }
    host.default_input_device()
}

/// The device's default format, at 48 kHz when the device offers it (no
/// resampling then), as ntgcalls' own device module does.
fn input_config(device: &cpal::Device) -> Result<SupportedStreamConfig, CaptureError> {
    let default = device
        .default_input_config()
        .map_err(|err| classify(&err.to_string()))?;
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

fn classify(raw: &str) -> CaptureError {
    let text = crate::voice_input::friendly(raw);
    if text.starts_with("Quill can't use the microphone") {
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
    config: &StreamConfig,
    ctx: Callback,
    on_error: impl FnMut(cpal::StreamError) + Send + 'static,
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
    let mut resampler = Resampler::new(rate);
    let mut processor = MicProcessor::new();
    let mut mono = Vec::new();
    let mut resampled = Vec::new();
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                let now = Instant::now();
                mono.clear();
                mono.extend(data.chunks(channels).map(|frame| {
                    frame.iter().map(|&s| f32::from_sample(s)).sum::<f32>()
                        / frame.len().max(1) as f32
                }));
                resampled.clear();
                resampler.process(&mono, &mut resampled);
                processor.push(&resampled, &controls, now, |frame| sink.push(frame));
            },
            on_error,
            None,
        )
        .map_err(|err| classify(&err.to_string()))
}
