//! Helpers shared by every cpal stream Quill opens: the call microphone
//! (`calls::capture::mic_cpal`), the voice-note microphone (`voice_input`),
//! system audio loopback (`calls::capture::system_audio_cpal`) and the
//! rodio playback output (`ui::audio`).
//!
//! - [`StreamErrors`]: which error-callback events stop a stream and which
//!   it survives. cpal 0.18 reports buffer xruns, default-device reroutes
//!   and refused real-time priority through the error callback while the
//!   stream keeps running; only the rest end it.
//! - [`downmix_into`]: any sample format to mono `f32`.
//! - [`requested_input`]: an engine device name as a cpal device choice.
//!
//! Design: `docs/decisions/cpal-0-18-rodio-0-23.md`. The tests are in
//! `src/ui/audio/cpal_stream_tests.rs`, in the app target: CI runs the
//! library's tests without the `ui` feature, which this module needs.
#![cfg(feature = "ui")]
#![deny(clippy::indexing_slicing)]

use std::time::{Duration, Instant};

use cpal::{ErrorKind, FromSample, Sample};

/// How a running stream takes one error-callback event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// A buffer underrun or overrun: at worst an audible click, and the
    /// stream keeps running. Logged, rate-limited.
    Glitch,
    /// The stream keeps running but something changed that is worth a log
    /// line: it followed a new default device, or the OS refused
    /// real-time priority.
    Notice,
    /// The stream stopped or must be rebuilt.
    Fatal,
}

/// cpal's documented meaning of each kind, for an error delivered to a
/// running stream's error callback (`cpal::ErrorKind` docs, 0.18.2).
pub fn severity(kind: ErrorKind) -> Severity {
    match kind {
        // "A buffer underrun or overrun occurred, causing a potential
        // audio glitch." Every backend recovers on its own (ALSA prepares
        // and restarts, WASAPI and CoreAudio only report it).
        ErrorKind::Xrun => Severity::Glitch,
        // "The stream remains active and no rebuild is required."
        // (CoreAudio default output when the system default changes.)
        ErrorKind::DeviceChanged => Severity::Notice,
        // "Audio will still play, but may be subject to increased latency."
        ErrorKind::RealtimeDenied => Severity::Notice,
        // The device went away, or the stream must be rebuilt (WASAPI
        // reports a default-device change this way since 0.18.2 because it
        // does not reroute), or access was revoked.
        ErrorKind::DeviceNotAvailable
        | ErrorKind::StreamInvalidated
        | ErrorKind::PermissionDenied
        | ErrorKind::HostUnavailable => Severity::Fatal,
        // Busy, out of resources, unsupported, invalid, or a backend error
        // cpal could not classify: nothing promises the stream still
        // delivers audio, so it is treated as stopped (as every error was
        // before cpal 0.18). `ErrorKind` is non-exhaustive; a kind added
        // later is fatal until it is reviewed here.
        _ => Severity::Fatal,
    }
}

/// Lines about the same kind of event are written at most this often.
pub const LOG_EVERY: Duration = Duration::from_secs(30);

/// Lets one line through per [`LOG_EVERY`] and counts the rest.
#[derive(Debug, Clone, Default)]
struct Throttle {
    logged_at: Option<Instant>,
    skipped: u32,
}

impl Throttle {
    /// `Some(skipped since the last line)` when a line may be written now.
    fn allow(&mut self, now: Instant) -> Option<u32> {
        let due = self
            .logged_at
            .is_none_or(|at| now.saturating_duration_since(at) >= LOG_EVERY);
        if !due {
            self.skipped = self.skipped.saturating_add(1);
            return None;
        }
        self.logged_at = Some(now);
        Some(std::mem::take(&mut self.skipped))
    }
}

/// What [`StreamErrors::sort`] made of one event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sorted {
    pub severity: Severity,
    /// The debug-log line to write, when one is due.
    pub log: Option<String>,
}

/// The error-callback filter of one stream. It lives inside the callback
/// closure (cpal calls it from its audio thread, one call at a time), so it
/// needs no lock.
#[derive(Debug, Clone)]
pub struct StreamErrors {
    stream: &'static str,
    glitches: Throttle,
    fatal: Throttle,
}

impl StreamErrors {
    /// `stream` names the stream in log lines ("call microphone").
    pub fn new(stream: &'static str) -> Self {
        Self {
            stream,
            glitches: Throttle::default(),
            fatal: Throttle::default(),
        }
    }

    /// Log what the stream survives and hand back the error when it ends
    /// the stream.
    pub fn check(&mut self, err: cpal::Error) -> Option<cpal::Error> {
        self.check_with(err, Instant::now(), crate::advanced::debug_mode::debug_log)
    }

    /// [`Self::check`] at `now`, writing log lines to `log`.
    pub fn check_with(
        &mut self,
        err: cpal::Error,
        now: Instant,
        log: impl FnOnce(&str),
    ) -> Option<cpal::Error> {
        let sorted = self.sort(&err, now);
        if let Some(line) = &sorted.log {
            log(line);
        }
        (sorted.severity == Severity::Fatal).then_some(err)
    }

    /// How `err` is taken at `now`, and the log line due for it.
    pub fn sort(&mut self, err: &cpal::Error, now: Instant) -> Sorted {
        let kind = err.kind();
        let severity = severity(kind);
        let throttle = match severity {
            Severity::Glitch => Some(&mut self.glitches),
            // A failing backend can repeat its error every period until
            // the stream is dropped.
            Severity::Fatal => Some(&mut self.fatal),
            Severity::Notice => None,
        };
        let skipped = match throttle {
            Some(throttle) => throttle.allow(now),
            None => Some(0),
        };
        let log = skipped.map(|skipped| {
            let what = match severity {
                Severity::Glitch => "dropout",
                Severity::Notice => "note",
                Severity::Fatal => "stopped",
            };
            let mut line = format!("audio: {}: {what} ({kind:?}): {err}", self.stream);
            if skipped > 0 {
                line.push_str(&format!(" (+{skipped} since the last line)"));
            }
            line
        });
        Sorted { severity, log }
    }
}

/// Down-mix interleaved `data` of `channels` channels to mono `f32` in
/// `mono` (cleared first). Works for every sample type cpal delivers,
/// including `I24` (24-bit values in an `i32`) and `f64`.
pub fn downmix_into<T>(data: &[T], channels: usize, mono: &mut Vec<f32>)
where
    T: Copy,
    f32: FromSample<T>,
{
    let channels = channels.max(1);
    mono.clear();
    mono.extend(data.chunks(channels).map(|frame| {
        frame.iter().map(|&s| f32::from_sample(s)).sum::<f32>() / frame.len().max(1) as f32
    }));
}

/// Which microphone an engine device name asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestedInput<'a> {
    /// The system default input.
    Default,
    /// The input whose cpal name is this.
    Named(&'a str),
}

/// What `engine_name`, a microphone name from the engine's device list, asks
/// cpal for. On Windows WebRTC lists Core Audio names: first
/// `Default - <name>` and `Communication - <name>` for the two default
/// roles, then every device by its `FriendlyName`, which is also the name
/// cpal 0.18 reports. `Communication - <name>` asks for that device by
/// name (cpal has no communications role). `windows` is `cfg!(windows)`;
/// other systems take the name as it is.
pub fn requested_input(engine_name: &str, windows: bool) -> RequestedInput<'_> {
    let name = engine_name.trim();
    if windows {
        if name.starts_with("Default - ") {
            return RequestedInput::Default;
        }
        if let Some(rest) = name.strip_prefix("Communication - ") {
            return RequestedInput::Named(rest.trim());
        }
    }
    RequestedInput::Named(name)
}

/// Whether cpal's `device_name` is the device `wanted` names. Exact after
/// trimming, then ignoring case, since the two lists read the same OS
/// property through different APIs.
pub fn device_name_matches(wanted: &str, device_name: &str) -> bool {
    let device_name = device_name.trim();
    device_name == wanted || device_name.to_lowercase() == wanted.to_lowercase()
}
