//! Tests for `quill::cpal_stream` (see its module docs for why they live
//! in the app target).

use std::sync::mpsc::{Sender, channel};
use std::thread::ThreadId;
use std::time::{Duration, Instant};

use cpal::ErrorKind;
use quill::cpal_stream::*;

/// Every `ErrorKind` cpal 0.18.2 has, with the severity Quill gives it.
const KINDS: [(ErrorKind, Severity); 14] = [
    (ErrorKind::Xrun, Severity::Glitch),
    (ErrorKind::DeviceChanged, Severity::Notice),
    (ErrorKind::RealtimeDenied, Severity::Notice),
    (ErrorKind::DeviceNotAvailable, Severity::Fatal),
    (ErrorKind::StreamInvalidated, Severity::Fatal),
    (ErrorKind::PermissionDenied, Severity::Fatal),
    (ErrorKind::HostUnavailable, Severity::Fatal),
    (ErrorKind::DeviceBusy, Severity::Fatal),
    (ErrorKind::ResourceExhausted, Severity::Fatal),
    (ErrorKind::InvalidInput, Severity::Fatal),
    (ErrorKind::UnsupportedConfig, Severity::Fatal),
    (ErrorKind::UnsupportedOperation, Severity::Fatal),
    (ErrorKind::BackendError, Severity::Fatal),
    (ErrorKind::Other, Severity::Fatal),
];

#[test]
fn every_kind_has_its_severity() {
    for (kind, expected) in KINDS {
        assert_eq!(severity(kind), expected, "{kind:?}");
    }
}

#[test]
fn only_fatal_errors_come_back_from_check() {
    for (kind, expected) in KINDS {
        let mut errors = StreamErrors::new("test");
        let mut lines = Vec::new();
        let back = errors.check_with(cpal::Error::new(kind), Instant::now(), |line| {
            lines.push(line.to_string())
        });
        assert_eq!(back.is_some(), expected == Severity::Fatal, "{kind:?}");
        assert_eq!(back.map(|err| err.kind()).unwrap_or(kind), kind);
        assert_eq!(lines.len(), 1, "the first event of each kind is logged");
    }
}

#[test]
fn xruns_are_logged_once_per_interval_with_a_count() {
    let mut errors = StreamErrors::new("call microphone");
    let xrun = cpal::Error::new(ErrorKind::Xrun);
    let start = Instant::now();
    let first = errors.sort(&xrun, start);
    assert_eq!(first.severity, Severity::Glitch);
    let line = first.log.unwrap_or_default();
    assert!(
        line.starts_with("audio: call microphone: dropout (Xrun)"),
        "{line}"
    );
    for ms in 1..=100 {
        let later = start + Duration::from_millis(ms);
        assert_eq!(errors.sort(&xrun, later).log, None);
    }
    let next = errors.sort(&xrun, start + LOG_EVERY);
    let line = next.log.unwrap_or_default();
    assert!(line.ends_with("(+100 since the last line)"), "{line}");
    // The count starts over after a line.
    let after = errors.sort(&xrun, start + LOG_EVERY + LOG_EVERY);
    assert!(
        !after
            .log
            .unwrap_or_default()
            .contains("since the last line")
    );
}

#[test]
fn notices_are_always_logged() {
    let mut errors = StreamErrors::new("output");
    let changed = cpal::Error::with_message(ErrorKind::DeviceChanged, "default output changed");
    let now = Instant::now();
    for _ in 0..3 {
        let sorted = errors.sort(&changed, now);
        assert_eq!(sorted.severity, Severity::Notice);
        assert_eq!(
            sorted.log.as_deref(),
            Some("audio: output: note (DeviceChanged): default output changed")
        );
    }
}

#[test]
fn repeated_fatal_errors_do_not_flood_the_log() {
    let mut errors = StreamErrors::new("output");
    let gone = cpal::Error::new(ErrorKind::BackendError);
    let now = Instant::now();
    assert!(errors.sort(&gone, now).log.is_some());
    let again = errors.sort(&gone, now + Duration::from_millis(10));
    assert_eq!(again.severity, Severity::Fatal);
    assert_eq!(again.log, None);
}

#[test]
fn glitches_do_not_hold_back_a_fatal_line() {
    let mut errors = StreamErrors::new("output");
    let now = Instant::now();
    let _ = errors.sort(&cpal::Error::new(ErrorKind::Xrun), now);
    let fatal = errors.sort(&cpal::Error::new(ErrorKind::DeviceNotAvailable), now);
    assert!(fatal.log.is_some());
}

#[test]
fn downmix_handles_i24() {
    use cpal::I24;
    let max = I24::new_unchecked((1 << 23) - 1);
    let min = I24::new_unchecked(-(1 << 23));
    let half = I24::new_unchecked(1 << 22);
    let minus_half = I24::new_unchecked(-(1 << 22));
    let mut mono = Vec::new();
    // Stereo frames: (max, max), (min, min), (half, -half), (half, half).
    downmix_into(
        &[max, max, min, min, half, minus_half, half, half],
        2,
        &mut mono,
    );
    assert_eq!(mono.len(), 4);
    let close = |a: f32, b: f32| (a - b).abs() < 1e-4;
    assert!(close(mono.first().copied().unwrap_or(0.0), 1.0), "{mono:?}");
    assert!(close(mono.get(1).copied().unwrap_or(0.0), -1.0), "{mono:?}");
    assert!(close(mono.get(2).copied().unwrap_or(1.0), 0.0), "{mono:?}");
    assert!(close(mono.get(3).copied().unwrap_or(0.0), 0.5), "{mono:?}");
}

#[test]
fn downmix_handles_f64() {
    let mut mono = vec![9.0];
    downmix_into(&[0.25_f64, 0.75, -1.0, 0.0, 0.5, 0.5], 2, &mut mono);
    assert_eq!(mono, vec![0.5, -0.5, 0.5]);
}

#[test]
fn downmix_matches_the_older_formats() {
    let mut mono = Vec::new();
    downmix_into(&[i16::MAX, i16::MAX], 1, &mut mono);
    assert_eq!(mono.len(), 2);
    assert!(mono.iter().all(|s| (s - 1.0).abs() < 1e-3));
    downmix_into(&[u16::MIN, u16::MIN, u16::MIN], 3, &mut mono);
    assert_eq!(mono, vec![-1.0]);
    downmix_into(&[0.5_f32, 0.5, 0.5], 0, &mut mono);
    assert_eq!(mono, vec![0.5, 0.5, 0.5], "zero channels counts as mono");
}

#[test]
fn engine_names_on_windows() {
    assert_eq!(
        requested_input("Default - Microphone (Realtek(R) Audio)", true),
        RequestedInput::Default
    );
    assert_eq!(
        requested_input("Communication - Headset Microphone (Jabra)", true),
        RequestedInput::Named("Headset Microphone (Jabra)")
    );
    assert_eq!(
        requested_input("Microphone (Realtek(R) Audio)", true),
        RequestedInput::Named("Microphone (Realtek(R) Audio)")
    );
}

#[test]
fn engine_names_elsewhere_are_taken_as_they_are() {
    assert_eq!(
        requested_input("Default - something", false),
        RequestedInput::Named("Default - something")
    );
    assert_eq!(
        requested_input(" USB Audio ", false),
        RequestedInput::Named("USB Audio")
    );
}

#[test]
fn device_names_match_exactly_or_ignoring_case() {
    // cpal 0.18 on Windows reports the FriendlyName, as the engine lists it.
    assert!(device_name_matches(
        "Microphone (Realtek(R) Audio)",
        "Microphone (Realtek(R) Audio)"
    ));
    assert!(device_name_matches("Микрофон (USB)", "микрофон (usb) "));
    // cpal 0.17 reported the DeviceDesc ("Microphone"), which never
    // matched.
    assert!(!device_name_matches(
        "Microphone (Realtek(R) Audio)",
        "Microphone"
    ));
}

struct FakeMic {
    events: Sender<(&'static str, ThreadId)>,
}

impl FakeMic {
    fn note(&self, event: &'static str) {
        let _ = self.events.send((event, std::thread::current().id()));
    }
}

impl Drop for FakeMic {
    fn drop(&mut self) {
        self.note("closed");
    }
}

#[test]
fn a_mic_thread_opens_runs_and_closes_off_the_calling_thread_in_order() {
    let (events, heard) = channel();
    let device = MicThread::open(
        "test-mic",
        move || {
            let mic = FakeMic { events };
            mic.note("opened");
            Ok(mic)
        },
        |mic: &mut FakeMic, command: &'static str| mic.note(command),
    )
    .expect("open");
    device.send("pause");
    device.send("play");
    drop(device);

    let order: Vec<_> = heard.iter().collect();
    let names: Vec<_> = order.iter().map(|(event, _)| *event).collect();
    assert_eq!(names, ["opened", "pause", "play", "closed"]);
    let here = std::thread::current().id();
    assert!(order.iter().all(|(_, thread)| *thread != here));
}

#[test]
fn a_mic_thread_command_never_waits_for_the_device() {
    let (gate, gated) = channel::<()>();
    let (events, heard) = channel();
    let device = MicThread::open(
        "test-mic",
        move || Ok(FakeMic { events }),
        move |mic: &mut FakeMic, command: &'static str| {
            let _ = gated.recv();
            mic.note(command);
        },
    )
    .expect("open");
    device.send("pause");
    device.send("play");
    drop(device);
    gate.send(()).expect("the first command waits");
    gate.send(()).expect("the second command waits");
    let names: Vec<_> = heard.iter().map(|(event, _)| event).collect();
    assert_eq!(names, ["pause", "play", "closed"]);
}

#[test]
fn a_mic_that_will_not_open_reports_why() {
    let opened = MicThread::<()>::open(
        "test-mic",
        || Err::<(), String>("No microphone found.".into()),
        |_, ()| {},
    );
    assert_eq!(opened.err().as_deref(), Some("No microphone found."));
}

#[test]
fn a_mic_thread_that_panics_while_opening_reports_it() {
    let opened = MicThread::<()>::open(
        "test-mic",
        || -> Result<(), String> { panic!("backend bug") },
        |_, ()| {},
    );
    assert_eq!(
        opened.err().as_deref(),
        Some("The microphone thread stopped.")
    );
}
