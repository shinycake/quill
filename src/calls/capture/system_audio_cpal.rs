//! System audio through WASAPI loopback (Windows): cpal opens an input
//! stream on the default output device, which WASAPI records in loopback
//! mode. Samples are down-mixed to mono, resampled to 48 kHz and queued
//! in the [`SystemAudioMix`] the microphone path mixes from.
#![deny(clippy::indexing_slicing)]

use super::CaptureError;
use super::system_audio::{SystemAudioCapture, SystemAudioMix};
use crate::calls::engine::EXTERNAL_AUDIO_RATE;
use crate::cpal_stream::{StreamErrors, downmix_into};
use crate::resample::Resampler;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, I24, SampleFormat, SizedSample, Stream, StreamConfig};
use std::sync::Arc;

/// A running loopback stream. Dropping it closes the device.
pub struct LoopbackCapture {
    stream: Option<Stream>,
}

impl SystemAudioCapture for LoopbackCapture {
    fn stop(&mut self) {
        self.stream = None;
    }
}

/// Start recording what the default output device plays into `mix`.
pub fn start_loopback(mix: Arc<SystemAudioMix>) -> Result<LoopbackCapture, CaptureError> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| CaptureError::Device("No speakers to share the sound of.".into()))?;
    let supported = device
        .default_output_config()
        .map_err(|err| CaptureError::Device(format!("System audio: {err}")))?;
    let channels = usize::from(supported.channels()).max(1);
    let rate = supported.sample_rate();
    let config: StreamConfig = supported.config();
    let stream = match supported.sample_format() {
        SampleFormat::F32 => build::<f32>(&device, config, channels, rate, mix),
        SampleFormat::F64 => build::<f64>(&device, config, channels, rate, mix),
        SampleFormat::I16 => build::<i16>(&device, config, channels, rate, mix),
        SampleFormat::I24 => build::<I24>(&device, config, channels, rate, mix),
        SampleFormat::I32 => build::<i32>(&device, config, channels, rate, mix),
        SampleFormat::U16 => build::<u16>(&device, config, channels, rate, mix),
        other => Err(CaptureError::Unsupported(format!(
            "Unsupported system audio format ({other:?})."
        ))),
    }?;
    stream
        .play()
        .map_err(|err| CaptureError::Device(format!("System audio: {err}")))?;
    Ok(LoopbackCapture {
        stream: Some(stream),
    })
}

fn build<T>(
    device: &cpal::Device,
    config: StreamConfig,
    channels: usize,
    rate: u32,
    mix: Arc<SystemAudioMix>,
) -> Result<Stream, CaptureError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let mut resampler = Resampler::new(rate, EXTERNAL_AUDIO_RATE);
    let mut mono = Vec::new();
    let mut resampled = Vec::new();
    let needs_resampling = rate != EXTERNAL_AUDIO_RATE;
    let mut errors = StreamErrors::new("system audio");
    let failures = mix.clone();
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                downmix_into(data, channels, &mut mono);
                if needs_resampling {
                    resampled.clear();
                    resampler.process(&mono, &mut resampled);
                    mix.push(&resampled);
                } else {
                    mix.push(&mono);
                }
            },
            // A lost output device ends the system audio, not the screen
            // or the microphone: the driver reads the failure from the mix
            // and turns the audio share off.
            move |err| {
                if let Some(err) = errors.check(err) {
                    failures.fail(format!("System audio stopped: {err}"));
                    crate::wake::UPDATES.poke();
                }
            },
            None,
        )
        .map_err(|err| CaptureError::Device(format!("System audio: {err}")))
}
