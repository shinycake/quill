//! Microphone capture through cpal (CoreAudio / WASAPI / ALSA), the same
//! backend the app already plays sound through. The device's own format is
//! down-mixed to mono and resampled to 48 kHz here, then handed to a worker
//! that encodes it ([`crate::voice_opus`]).

use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig};

use crate::voice_opus::Resampler;

/// An open, running input stream. Dropping it stops the microphone.
pub struct MicStream {
    _stream: Stream,
    /// Set by the stream's error callback (device unplugged, …).
    pub error: Arc<Mutex<Option<String>>>,
}

/// Open the default input device and send mono 48 kHz chunks to `out`.
pub fn open_default(out: Sender<Vec<f32>>) -> Result<MicStream, String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "No microphone found.".to_string())?;
    let supported = device
        .default_input_config()
        .map_err(|err| friendly(&err.to_string()))?;
    let channels = usize::from(supported.channels()).max(1);
    let rate = supported.sample_rate();
    let format = supported.sample_format();
    let config: StreamConfig = supported.config();
    let error = Arc::new(Mutex::new(None));
    let on_error = {
        let error = error.clone();
        move |err: cpal::StreamError| {
            if let Ok(mut slot) = error.lock() {
                slot.get_or_insert_with(|| format!("Microphone stopped: {err}"));
            }
        }
    };
    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, &config, channels, rate, out, on_error),
        SampleFormat::I16 => build::<i16>(&device, &config, channels, rate, out, on_error),
        SampleFormat::U16 => build::<u16>(&device, &config, channels, rate, out, on_error),
        SampleFormat::I32 => build::<i32>(&device, &config, channels, rate, out, on_error),
        other => Err(format!("Unsupported microphone format ({other:?}).")),
    }?;
    stream.play().map_err(|err| friendly(&err.to_string()))?;
    Ok(MicStream {
        _stream: stream,
        error,
    })
}

fn build<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    channels: usize,
    rate: u32,
    out: Sender<Vec<f32>>,
    on_error: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<Stream, String>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let mut resampler = Resampler::new(rate);
    let mut mono = Vec::new();
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                mono.clear();
                mono.extend(data.chunks(channels).map(|frame| {
                    frame.iter().map(|&s| f32::from_sample(s)).sum::<f32>() / frame.len() as f32
                }));
                let mut converted = Vec::with_capacity(mono.len() + 8);
                resampler.process(&mono, &mut converted);
                let _ = out.send(converted);
            },
            on_error,
            None,
        )
        .map_err(|err| friendly(&err.to_string()))
}

/// The backend's message, with a Settings hint when the OS refused access.
fn friendly(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    if lower.contains("access") && lower.contains("denied")
        || lower.contains("permission")
        || lower.contains("not authorized")
    {
        return if cfg!(windows) {
            "Quill can't use the microphone. Allow it in Settings › Privacy & security › \
             Microphone."
                .into()
        } else if cfg!(target_os = "macos") {
            "Quill can't use the microphone. Allow it in System Settings → Privacy & \
             Security → Microphone."
                .into()
        } else {
            format!("Quill can't use the microphone: {raw}")
        };
    }
    format!("Couldn't start the microphone: {raw}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_errors_get_the_settings_hint() {
        assert!(friendly("Access is denied. (0x80070005)").contains("Microphone"));
        assert!(friendly("Permission denied").contains("microphone"));
        assert_eq!(
            friendly("device busy"),
            "Couldn't start the microphone: device busy"
        );
    }
}
