#![deny(clippy::indexing_slicing)]

use super::gain::scaled;
use super::ring::Playout;
use super::{PcmFormat, StreamAudio};
use crate::cpal_stream::{RequestedInput, StreamErrors, device_name_matches, requested_input};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    FromSample, I24, Sample, SampleFormat, SizedSample, Stream, StreamConfig, SupportedStreamConfig,
};
use std::sync::Arc;

/// The call library's sound is 48 kHz; the output runs at it too when the
/// device offers it.
const PREFERRED_RATE: u32 = 48_000;

pub(super) struct Output {
    _stream: Stream,
}

impl Output {
    pub(super) fn open(
        device: Option<&str>,
        volume: f32,
        muted: bool,
    ) -> Result<(Self, Arc<StreamAudio>), String> {
        let host = cpal::default_host();
        let device = find_output(&host, device).ok_or("No audio output found.")?;
        let supported = output_config(&device)?;
        let device_channels = supported.channels();
        let format = PcmFormat::for_output(supported.sample_rate(), device_channels);
        let audio = Arc::new(StreamAudio::new(format, volume, muted));
        let config: StreamConfig = supported.config();
        let ctx = Callback {
            audio: audio.clone(),
            device_channels: usize::from(device_channels).max(1),
        };
        let on_error = {
            let audio = audio.clone();
            let mut errors = StreamErrors::new("stream sound");
            move |err: cpal::Error| {
                if errors.check(err).is_some() {
                    audio.mark_output_failed();
                }
            }
        };
        let stream = match supported.sample_format() {
            SampleFormat::F32 => build::<f32>(&device, config, ctx, on_error),
            SampleFormat::F64 => build::<f64>(&device, config, ctx, on_error),
            SampleFormat::I16 => build::<i16>(&device, config, ctx, on_error),
            SampleFormat::U16 => build::<u16>(&device, config, ctx, on_error),
            SampleFormat::I24 => build::<I24>(&device, config, ctx, on_error),
            SampleFormat::I32 => build::<i32>(&device, config, ctx, on_error),
            other => Err(format!("Unsupported output format ({other:?}).")),
        }?;
        stream.play().map_err(|err| err.to_string())?;
        Ok((Self { _stream: stream }, audio))
    }
}

/// The output named `name` as the engine lists speakers, or the system
/// default (no name, the default entry, or no device by that name).
fn find_output(host: &cpal::Host, name: Option<&str>) -> Option<cpal::Device> {
    if let Some(name) = name
        && let RequestedInput::Named(wanted) = requested_input(name, cfg!(windows))
        && let Ok(mut devices) = host.output_devices()
        && let Some(device) = devices.find(|device| {
            device
                .description()
                .is_ok_and(|description| device_name_matches(wanted, description.name()))
        })
    {
        return Some(device);
    }
    host.default_output_device()
}

fn output_config(device: &cpal::Device) -> Result<SupportedStreamConfig, String> {
    let default = device
        .default_output_config()
        .map_err(|err| err.to_string())?;
    if default.sample_rate() == PREFERRED_RATE {
        return Ok(default);
    }
    let preferred = device
        .supported_output_configs()
        .ok()
        .and_then(|mut ranges| {
            ranges.find_map(|range| {
                (range.channels() == default.channels()
                    && range.sample_format() == default.sample_format())
                .then(|| range.try_with_sample_rate(PREFERRED_RATE))
                .flatten()
            })
        });
    Ok(preferred.unwrap_or(default))
}

struct Callback {
    audio: Arc<StreamAudio>,
    device_channels: usize,
}

fn build<T>(
    device: &cpal::Device,
    config: StreamConfig,
    ctx: Callback,
    on_error: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let Callback {
        audio,
        device_channels,
    } = ctx;
    let ring_channels = usize::from(audio.format().channels).max(1);
    let mut playout = Playout::new(audio.prime_samples());
    let max_frames = (audio.ring().capacity() / ring_channels).max(1);
    let mut pcm = vec![0i16; max_frames * ring_channels];
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                let factor = audio.gain().factor();
                for part in data.chunks_mut(max_frames * device_channels) {
                    let frames = part.len() / device_channels;
                    let Some(pcm) = pcm.get_mut(..frames * ring_channels) else {
                        part.fill(T::EQUILIBRIUM);
                        continue;
                    };
                    playout.fill(audio.ring(), pcm);
                    for (out, frame) in part
                        .chunks_mut(device_channels)
                        .zip(pcm.chunks(ring_channels))
                    {
                        spread(frame, out, factor);
                    }
                }
            },
            on_error,
            None,
        )
        .map_err(|err| err.to_string())
}

fn spread<T: Sample + FromSample<f32>>(frame: &[i16], out: &mut [T], factor: f32) {
    let mono = frame.len() == 1;
    for (channel, sample) in out.iter_mut().enumerate() {
        let source = if mono {
            frame.first()
        } else {
            frame.get(channel)
        };
        *sample = T::from_sample(source.map_or(0.0, |&value| scaled(value, factor)));
    }
}

#[cfg(test)]
mod tests {
    use super::spread;

    #[test]
    fn mono_fills_every_channel_and_stereo_the_first_two() {
        let mut out = [9.0f32; 4];
        spread(&[16_384], &mut out, 1.0);
        assert_eq!(out, [0.5; 4]);
        spread(&[16_384, -16_384], &mut out, 0.5);
        assert_eq!(out, [0.25, -0.25, 0.0, 0.0]);
        let mut pair = [0i16; 2];
        spread(&[16_384, 8_192], &mut pair, 1.0);
        assert_eq!(pair, [16_384, 8_192]);
    }
}
