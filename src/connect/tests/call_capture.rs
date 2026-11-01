//! Connect-driver tests: the call microphone Quill captures and feeds
//! into the engine's external source (`connect::call_capture`).
use super::super::*;
use super::support::{pump_until_mic_answers, until};
use super::*;
use crate::calls::capture::{
    AudioFrame, CaptureError, CaptureEvent, FrameSink, MicCapture, MicControls,
};
use crate::calls::engine::{
    CallEngine, CaptureTarget, ConnectParams, ExternalDevice, MediaDevice, MediaDeviceKind,
    MockEngine, TransportState,
};
use crate::state::ActiveGroupCall;
use crate::telegram::envelope::parse_group_call_participant;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// What the fake microphone was opened with, and its sink.
#[derive(Default)]
struct FakeMics {
    opened: Mutex<Vec<Option<String>>>,
    sink: Mutex<Option<FrameSink<AudioFrame>>>,
    controls: Mutex<Option<Arc<MicControls>>>,
    stopped: Arc<AtomicBool>,
    fail: AtomicBool,
}

struct FakeMic(Arc<AtomicBool>);

impl MicCapture for FakeMic {
    fn start(
        &mut self,
        _device: Option<&str>,
        _sink: FrameSink<AudioFrame>,
    ) -> Result<(), CaptureError> {
        Ok(())
    }

    fn stop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

struct SilentMic;

impl MicCapture for SilentMic {
    fn start(
        &mut self,
        _device: Option<&str>,
        _sink: FrameSink<AudioFrame>,
    ) -> Result<(), CaptureError> {
        Ok(())
    }

    fn stop(&mut self) {}
}

fn install_fake_mic(driver: &mut ConnectDriver<Arc<RecordingSender>>) -> Arc<FakeMics> {
    let mics = Arc::new(FakeMics::default());
    let handle = mics.clone();
    driver.inject_call_microphone(Arc::new(move |device, controls, sink| {
        if handle.fail.load(Ordering::SeqCst) {
            return Err(CaptureError::Permission(
                "Quill can't use the microphone.".into(),
            ));
        }
        handle
            .opened
            .lock()
            .unwrap()
            .push(device.map(str::to_owned));
        *handle.sink.lock().unwrap() = Some(sink);
        *handle.controls.lock().unwrap() = Some(controls);
        handle.stopped.store(false, Ordering::SeqCst);
        Ok(Box::new(FakeMic(handle.stopped.clone())))
    }));
    mics
}

fn push_frames(mics: &FakeMics, count: usize) {
    let sink = mics.sink.lock().unwrap().clone().expect("microphone open");
    for i in 0..count {
        sink.push(AudioFrame {
            pcm: vec![i16::try_from(i).unwrap(); 480],
            captured_at: Instant::now(),
        });
    }
}

fn wait_for(mut done: impl FnMut() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(start.elapsed() < Duration::from_secs(5), "timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn joined_group_call(muted: bool) -> ActiveGroupCall {
    let mut call = tracked_group_call(false, false, false);
    call.is_muted_self = muted;
    call.transport_ready = true;
    let json: Value =
        serde_json::from_str(&group_participant_json(777, true, "null", "null")).unwrap();
    let mut me = parse_group_call_participant(json.get("participant")).unwrap();
    me.audio_source_id = 4242;
    me.can_unmute_self = true;
    call.participants = vec![me].into();
    call
}

fn group_driver() -> (
    std::path::PathBuf,
    ConnectDriver<Arc<RecordingSender>>,
    MockEngine,
    Arc<FakeMics>,
) {
    let (dir, _recorder, mut driver, _seq) = group_call_test_driver();
    let mock = MockEngine::new();
    let mut handle = mock.clone();
    driver.set_call_engine(Box::new(mock));
    // The group transport is up: the engine has the call's sources.
    handle
        .create_group_call(77, -100, crate::calls::engine::GroupCallRole::Member)
        .unwrap();
    handle.connect_group_call(77, "answer", None).unwrap();
    let mics = install_fake_mic(&mut driver);
    (dir, driver, handle, mics)
}

#[test]
fn group_call_microphone_feeds_the_engine_and_follows_mute() {
    let (dir, mut driver, engine, mics) = group_driver();

    // Not in a call: nothing opens.
    pump_until_mic_answers(&mut driver);
    assert!(mics.opened.lock().unwrap().is_empty());

    // Joined but the transport is not up yet: still nothing.
    let mut call = joined_group_call(false);
    call.transport_ready = false;
    driver.session.calls.active_group_call = Some(call);
    pump_until_mic_answers(&mut driver);
    assert!(mics.opened.lock().unwrap().is_empty());

    // Transport up: the default microphone opens and its frames reach the
    // group's external microphone.
    driver.session.calls.active_group_call = Some(joined_group_call(false));
    pump_until_mic_answers(&mut driver);
    assert_eq!(*mics.opened.lock().unwrap(), vec![None]);
    assert_eq!(
        driver.call_capture().target(),
        Some(CaptureTarget::Group(77))
    );
    assert_eq!(driver.call_capture().noise_suppression(), Some(true));
    push_frames(&mics, 3);
    wait_for(|| engine.external_frames().len() == 3);
    let frames = engine.external_frames();
    assert!(frames.iter().all(|frame| {
        frame.target == CaptureTarget::Group(77)
            && frame.device == ExternalDevice::Microphone
            && frame.bytes.len() == 960
    }));
    assert!(
        frames
            .windows(2)
            .all(|w| w[0].meta.timestamp_ms <= w[1].meta.timestamp_ms)
    );

    // Muting keeps the stream and switches it to silence.
    driver
        .session
        .calls
        .active_group_call
        .as_mut()
        .unwrap()
        .is_muted_self = true;
    pump_until_mic_answers(&mut driver);
    assert_eq!(driver.call_capture().muted(), Some(true));
    assert_eq!(mics.opened.lock().unwrap().len(), 1, "no reopen on mute");

    // Leaving closes the microphone.
    driver.session.calls.active_group_call = None;
    pump_until_mic_answers(&mut driver);
    until(|| mics.stopped.load(Ordering::SeqCst));
    assert_eq!(driver.call_capture().target(), None);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn noise_suppression_switch_applies_to_the_running_microphone() {
    let (dir, mut driver, _engine, mics) = group_driver();
    driver.session.calls.prefs.call_noise_suppression = false;
    driver.session.calls.active_group_call = Some(joined_group_call(false));
    pump_until_mic_answers(&mut driver);
    let controls = mics.controls.lock().unwrap().clone().unwrap();
    assert!(!controls.noise_suppression());
    // The switch flips it at once, without a pump.
    driver.session.calls.prefs.call_noise_suppression = true;
    driver.apply_call_noise_suppression();
    assert!(controls.noise_suppression());
    assert_eq!(mics.opened.lock().unwrap().len(), 1, "same stream");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_lost_microphone_is_reported_and_reopened_later() {
    let (dir, mut driver, _engine, mics) = group_driver();
    driver.session.calls.active_group_call = Some(joined_group_call(false));
    pump_until_mic_answers(&mut driver);
    let sink = mics.sink.lock().unwrap().clone().unwrap();
    sink.push_event(CaptureEvent::Failed(CaptureError::Device(
        "Microphone stopped: device gone".into(),
    )));
    pump_until_mic_answers(&mut driver);
    until(|| mics.stopped.load(Ordering::SeqCst));
    assert_eq!(
        driver.group_call_level_error(),
        Some("Microphone stopped: device gone")
    );
    // Within the retry delay nothing reopens.
    pump_until_mic_answers(&mut driver);
    assert_eq!(mics.opened.lock().unwrap().len(), 1);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_refused_microphone_shows_the_error_and_the_call_goes_on() {
    let (dir, mut driver, _engine, mics) = group_driver();
    mics.fail.store(true, Ordering::SeqCst);
    driver.session.calls.active_group_call = Some(joined_group_call(false));
    pump_until_mic_answers(&mut driver);
    assert_eq!(driver.call_capture().target(), None);
    assert_eq!(
        driver.group_call_level_error(),
        Some("Quill can't use the microphone.")
    );
    // Leaving clears the note.
    driver.session.calls.active_group_call = None;
    pump_until_mic_answers(&mut driver);
    assert_eq!(driver.group_call_level_error(), None);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn your_level_comes_from_the_call_microphone() {
    let (dir, mut driver, _engine, _mics) = group_driver();
    driver.session.calls.active_group_call = Some(joined_group_call(false));
    pump_until_mic_answers(&mut driver);
    driver.pump_call_audio().unwrap();
    assert!(driver.call_audio.is_open());
    driver.call_capture().level().record(1.3);
    driver.pump_call_audio().unwrap();
    assert_eq!(driver.group_call_self_level(), (1.3, true));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn private_call_uses_the_chosen_microphone() {
    let (dir, mut driver, handle, _sink, _seq) = ready_call_driver();
    let mics = install_fake_mic(&mut driver);
    driver.call_devices_cache = vec![MediaDevice {
        id: r#"{"uid":"usb-1"}"#.into(),
        name: "USB Mic".into(),
        kind: MediaDeviceKind::Microphone,
    }];
    driver.selected_devices.0 = Some(r#"{"uid":"usb-1"}"#.into());
    // The switch belongs to group calls; 1:1 calls always denoise.
    driver.session.calls.prefs.call_noise_suppression = false;
    // No transport yet: nothing opens.
    pump_until_mic_answers(&mut driver);
    assert!(mics.opened.lock().unwrap().is_empty());

    let mut engine = handle.clone();
    engine
        .connect(
            77,
            &ConnectParams {
                encryption_key: vec![1; 256],
                custom_parameters: String::new(),
                is_outgoing: true,
                servers: Vec::new(),
                library_versions: Vec::new(),
                p2p_allowed: false,
                mic_input: None,
                speaker_input: None,
                camera_input: None,
            },
        )
        .unwrap();
    driver.session.calls.active_call.as_mut().unwrap().transport = Some(TransportState::Connected);
    pump_until_mic_answers(&mut driver);
    assert_eq!(
        *mics.opened.lock().unwrap(),
        vec![Some("USB Mic".to_string())]
    );
    assert_eq!(driver.call_capture().noise_suppression(), Some(true));
    driver.apply_call_noise_suppression();
    assert_eq!(driver.call_capture().noise_suppression(), Some(true));
    push_frames(&mics, 2);
    wait_for(|| handle.external_frames().len() == 2);
    assert!(
        handle
            .external_frames()
            .iter()
            .all(|frame| frame.target == CaptureTarget::Private(77))
    );

    // Picking another microphone reopens on it.
    driver.selected_devices.0 = None;
    pump_until_mic_answers(&mut driver);
    assert_eq!(
        *mics.opened.lock().unwrap(),
        vec![Some("USB Mic".to_string()), None]
    );

    // A failed transport stops the microphone.
    driver.session.calls.active_call.as_mut().unwrap().transport = Some(TransportState::Failed);
    pump_until_mic_answers(&mut driver);
    until(|| mics.stopped.load(Ordering::SeqCst));
    std::fs::remove_dir_all(dir).ok();
}

/// Talking while muted: the raw microphone's peaks reach the hint, which
/// shows after three loud 500 ms checks and goes when you unmute.
#[test]
fn talking_while_muted_shows_the_hint_until_you_unmute() {
    let (dir, mut driver, _engine, mics) = group_driver();
    driver.session.calls.active_group_call = Some(joined_group_call(true));
    pump_until_mic_answers(&mut driver);
    let controls = mics.controls.lock().unwrap().clone().expect("open");
    let t0 = Instant::now();
    assert!(!driver.request_tick(t0), "starts listening");
    assert_eq!(
        driver.next_deadline(t0, 0),
        Some(Duration::from_millis(
            crate::calls::muted_speech::CHECK_EVERY_MS
        ))
    );
    let mut shown_at = None;
    for step in 1..=8u64 {
        // A peak of half of full scale is loud (tdesktop: 0.2).
        controls
            .muted_level()
            .record(crate::calls::audio_level::level_from_peak(0.5));
        let now = t0 + Duration::from_millis(step * 250);
        if driver.request_tick(now) {
            shown_at = Some(step * 250);
            break;
        }
    }
    assert_eq!(shown_at, Some(1500), "the third check shows it");
    assert!(driver.group_call_muted_speech_hint());
    driver
        .session
        .calls
        .active_group_call
        .as_mut()
        .unwrap()
        .is_muted_self = false;
    assert!(driver.request_tick(t0 + Duration::from_millis(1600)));
    assert!(!driver.group_call_muted_speech_hint());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_lost_microphone_needs_no_telegram_update() {
    static WAKE: crate::wake::Signal = crate::wake::Signal::new();
    let (dir, mut driver, _engine, mics) = group_driver();
    driver.call_capture.inject_sink_wake(&WAKE);
    driver.session.calls.active_group_call = Some(joined_group_call(false));
    let t0 = Instant::now();
    pump_until_mic_answers(&mut driver);
    assert_eq!(mics.opened.lock().unwrap().len(), 1);
    let sink = mics.sink.lock().unwrap().clone().expect("open");

    let woken = WAKE.epoch();
    sink.push_event(CaptureEvent::Ended);
    assert_ne!(WAKE.epoch(), woken, "the poll loop wakes");

    assert!(driver.request_tick(t0), "the call window shows the note");
    assert_eq!(driver.call_capture().target(), None);
    assert_eq!(
        driver.call_capture().error.as_deref(),
        Some("The microphone was disconnected.")
    );
    let t1 = t0 + Duration::from_secs(1);
    assert_eq!(driver.next_deadline(t1, 0), Some(Duration::from_secs(1)));
    assert!(!driver.request_tick(t1), "too early to try again");
    assert_eq!(mics.opened.lock().unwrap().len(), 1);

    let t2 = t0 + Duration::from_secs(2);
    driver.request_tick(t2);
    assert!(
        driver.call_capture().opening(),
        "the poll asks for it again"
    );
    while driver.call_capture().opening() {
        std::thread::yield_now();
        driver.request_tick(t2);
    }
    assert_eq!(mics.opened.lock().unwrap().len(), 2);
    assert!(driver.call_capture().target().is_some());
    assert_eq!(driver.call_capture().error, None);
    assert_eq!(driver.next_deadline(t0, 0), None);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_slow_microphone_does_not_hold_the_pump() {
    let (dir, mut driver, engine, _mics) = group_driver();
    let (release, released) = std::sync::mpsc::channel::<()>();
    let released = Mutex::new(released);
    let pump_thread = std::thread::current().id();
    driver.inject_call_microphone(Arc::new(move |_device, _controls, _sink| {
        assert_ne!(std::thread::current().id(), pump_thread);
        let _ = released.lock().unwrap().recv();
        Ok(Box::new(SilentMic))
    }));
    driver.session.calls.active_group_call = Some(joined_group_call(false));

    driver.pump_call_capture(Instant::now());

    assert!(driver.call_capture().opening());
    assert_eq!(driver.call_capture().target(), None);
    assert_eq!(engine.external_frames().len(), 0);

    release.send(()).unwrap();
    pump_until_mic_answers(&mut driver);
    assert_eq!(
        driver.call_capture().target(),
        Some(CaptureTarget::Group(77))
    );
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_microphone_that_opens_after_you_left_closes() {
    let (dir, mut driver, _engine, _mics) = group_driver();
    let (release, released) = std::sync::mpsc::channel::<()>();
    let released = Mutex::new(released);
    let stopped = Arc::new(AtomicBool::new(false));
    let flag = stopped.clone();
    driver.inject_call_microphone(Arc::new(move |_device, _controls, _sink| {
        let _ = released.lock().unwrap().recv();
        Ok(Box::new(FakeMic(flag.clone())))
    }));
    driver.session.calls.active_group_call = Some(joined_group_call(false));
    driver.pump_call_capture(Instant::now());
    assert!(driver.call_capture().opening());

    driver.session.calls.active_group_call = None;
    driver.pump_call_capture(Instant::now());
    assert!(!driver.call_capture().opening());
    assert!(!stopped.load(Ordering::SeqCst));

    release.send(()).unwrap();
    until(|| stopped.load(Ordering::SeqCst));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_call_you_are_in_warms_the_devices_once() {
    let (dir, mut driver, engine, _mics) = group_driver();
    driver.request_tick(Instant::now());
    assert_eq!(engine.device_lists(), 0, "no call, no frameworks loaded");
    let mut call = joined_group_call(false);
    call.is_joined = false;
    driver.session.calls.active_group_call = Some(call);
    driver.request_tick(Instant::now());
    assert_eq!(engine.device_lists(), 0, "a chat you only look at");
    driver.session.calls.active_group_call = Some(joined_group_call(false));
    driver.request_tick(Instant::now());
    driver.request_tick(Instant::now());
    assert_eq!(engine.device_lists(), 1);
    std::fs::remove_dir_all(dir).ok();
}

fn a_camera() -> Vec<MediaDevice> {
    vec![MediaDevice {
        id: "cam".into(),
        name: "Camera".into(),
        kind: MediaDeviceKind::Camera,
    }]
}

#[test]
fn a_slow_device_list_holds_nobody_and_redraws_when_it_lands() {
    let (dir, mut driver, engine, _mics) = group_driver();
    engine.set_devices(a_camera());
    engine.hold_device_lists();

    driver.refresh_call_devices();
    driver.refresh_call_devices();
    assert_eq!(engine.device_lists(), 1, "one list at a time");
    assert!(driver.call_devices().is_empty());
    assert!(!driver.pump_call_media(Instant::now()));

    engine.release_device_lists();
    assert!(driver.pump_call_media(Instant::now()));
    assert_eq!(driver.call_devices(), a_camera());
    assert!(!driver.pump_call_media(Instant::now()));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_camera_wanted_before_the_first_list_lands_is_not_refused() {
    let (dir, mut driver, engine, _mics) = group_driver();
    engine.set_devices(a_camera());
    engine.hold_device_lists();
    assert_eq!(driver.camera_to_turn_on(), Ok("cam".to_string()));
    std::fs::remove_dir_all(dir).ok();
}
