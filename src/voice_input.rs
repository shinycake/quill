//! Microphone capture through cpal (CoreAudio / WASAPI / ALSA), the same
//! backend the app already plays sound through. The device's own format is
//! down-mixed to mono and resampled to 48 kHz here, then handed to a worker
//! that encodes it ([`crate::voice_opus`]).
#![deny(clippy::indexing_slicing)]

use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{ErrorKind, FromSample, I24, SampleFormat, SizedSample, Stream, StreamConfig};

use crate::cpal_stream::{MicThread, StreamErrors, downmix_into};
use crate::resample::Resampler;

/// An open, running input stream on its own thread. Dropping it stops the
/// microphone, on that thread.
pub struct MicStream {
    device: MicThread<MicCommand>,
    /// Set by the stream's error callback (device unplugged, …).
    pub error: Arc<Mutex<Option<String>>>,
}

enum MicCommand {
    Pause,
    Play,
}

/// Open the default input device and send mono 48 kHz chunks to `out`.
/// Waits for the device, so call it off the UI thread.
pub fn open_default(out: Sender<Vec<f32>>) -> Result<MicStream, String> {
    let error = Arc::new(Mutex::new(None));
    let stream_error = error.clone();
    let device = MicThread::open(
        "quill-voice-mic",
        move || open_stream(out, stream_error),
        |stream: &mut Stream, command| {
            let _ = match command {
                MicCommand::Pause => stream.pause(),
                MicCommand::Play => stream.play(),
            };
        },
    )?;
    Ok(MicStream { device, error })
}

fn open_stream(
    out: Sender<Vec<f32>>,
    error: Arc<Mutex<Option<String>>>,
) -> Result<Stream, String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "No microphone found.".to_string())?;
    let supported = device
        .default_input_config()
        .map_err(|err| friendly_error(&err))?;
    let channels = usize::from(supported.channels()).max(1);
    let rate = supported.sample_rate();
    let format = supported.sample_format();
    let config: StreamConfig = supported.config();
    let on_error = {
        // Xruns and reroutes leave the recording running; only the rest
        // end the voice note.
        let mut errors = StreamErrors::new("voice note microphone");
        move |err: cpal::Error| {
            let Some(err) = errors.check(err) else {
                return;
            };
            if let Ok(mut slot) = error.lock() {
                slot.get_or_insert_with(|| format!("Microphone stopped: {err}"));
            }
        }
    };
    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, config, channels, rate, out, on_error),
        SampleFormat::F64 => build::<f64>(&device, config, channels, rate, out, on_error),
        SampleFormat::I16 => build::<i16>(&device, config, channels, rate, out, on_error),
        SampleFormat::U16 => build::<u16>(&device, config, channels, rate, out, on_error),
        SampleFormat::I24 => build::<I24>(&device, config, channels, rate, out, on_error),
        SampleFormat::I32 => build::<i32>(&device, config, channels, rate, out, on_error),
        other => Err(format!("Unsupported microphone format ({other:?}).")),
    }?;
    stream.play().map_err(|err| friendly_error(&err))?;
    Ok(stream)
}

impl MicStream {
    /// Release the microphone for now. A backend that cannot pause leaves
    /// the stream running; the encoder drops its audio either way.
    pub fn pause(&self) {
        self.device.send(MicCommand::Pause);
    }

    /// Listen again after [`Self::pause`].
    pub fn play(&self) {
        self.device.send(MicCommand::Play);
    }
}

fn build<T>(
    device: &cpal::Device,
    config: StreamConfig,
    channels: usize,
    rate: u32,
    out: Sender<Vec<f32>>,
    on_error: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<Stream, String>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let mut resampler = Resampler::new(rate, crate::voice_opus::RATE);
    let mut mono = Vec::new();
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                downmix_into(data, channels, &mut mono);
                let mut converted = Vec::with_capacity(mono.len() + 8);
                resampler.process(&mono, &mut converted);
                let _ = out.send(converted);
            },
            on_error,
            None,
        )
        .map_err(|err| friendly_error(&err))
}

/// Whether `err` is the OS refusing microphone access: cpal's
/// `PermissionDenied` kind, or a backend message that says so (a backend
/// that reports it as `BackendError` or `Other`).
pub(crate) fn is_permission_error(err: &cpal::Error) -> bool {
    err.kind() == ErrorKind::PermissionDenied || reads_as_permission(&err.to_string())
}

/// [`friendly`] for a cpal error, going by its kind before its text.
pub(crate) fn friendly_error(err: &cpal::Error) -> String {
    let raw = err.to_string();
    if is_permission_error(err) {
        permission_hint(&raw)
    } else {
        friendly(&raw)
    }
}

fn reads_as_permission(raw: &str) -> bool {
    let lower = raw.to_ascii_lowercase();
    lower.contains("access") && lower.contains("denied")
        || lower.contains("permission")
        || lower.contains("not authorized")
}

/// The backend's message, with a Settings hint when the OS refused access.
pub(crate) fn friendly(raw: &str) -> String {
    if reads_as_permission(raw) {
        return permission_hint(raw);
    }
    format!("Couldn't start the microphone: {raw}")
}

fn permission_hint(raw: &str) -> String {
    if cfg!(windows) {
        "Quill can't use the microphone. Allow it in Settings › Privacy & security › \
             Microphone."
            .into()
    } else if cfg!(target_os = "macos") {
        "Quill can't use the microphone. Allow it in System Settings → Privacy & \
             Security → Microphone."
            .into()
    } else {
        format!("Quill can't use the microphone: {raw}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_errors_get_the_settings_hint() {
        // The hint names Settings on Windows and macOS and quotes the
        // backend elsewhere; every form starts the same way.
        assert!(
            friendly("Access is denied. (0x80070005)")
                .starts_with("Quill can't use the microphone")
        );
        assert!(friendly("Permission denied").starts_with("Quill can't use the microphone"));
        assert_eq!(
            friendly("device busy"),
            "Couldn't start the microphone: device busy"
        );
    }

    #[test]
    fn permission_kind_wins_over_the_text() {
        // cpal 0.18's own text for the kind says "Permission denied", but a
        // backend message may not; the kind decides.
        let err = cpal::Error::with_message(ErrorKind::PermissionDenied, "AudioUnit error -10875");
        assert!(is_permission_error(&err));
        assert!(friendly_error(&err).starts_with("Quill can't use the microphone"));
        let bare = cpal::Error::new(ErrorKind::PermissionDenied);
        assert!(friendly_error(&bare).starts_with("Quill can't use the microphone"));
    }

    #[test]
    fn permission_text_still_counts_without_the_kind() {
        let err = cpal::Error::with_message(ErrorKind::BackendError, "Access is denied.");
        assert!(is_permission_error(&err));
        let busy = cpal::Error::with_message(ErrorKind::DeviceBusy, "device busy");
        assert!(!is_permission_error(&busy));
        assert_eq!(
            friendly_error(&busy),
            "Couldn't start the microphone: device busy"
        );
    }
}
