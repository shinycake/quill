//! Connect-driver tests: the shared screen Quill captures and feeds into
//! the engine's external screen source (`connect::call_screen`).
use super::super::*;
use super::support::{pump_until_mic_answers, pump_until_screen_answers, until};
use super::*;
use crate::calls::capture::screen::{PreviewSlot, ScreenCapture, SourceId};
use crate::calls::capture::system_audio::{SystemAudioCapture, SystemAudioMix};
use crate::calls::capture::{
    AudioFrame, CaptureError, CaptureEvent, FrameSink, MicCapture, MicControls, VideoFrame,
};
use crate::calls::engine::{
    CallEngine, CaptureTarget, ConnectParams, ExternalDevice, MockEngine, TransportState,
};
use crate::state::ActiveGroupCall;
use crate::telegram::envelope::parse_group_call_participant;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// What the fake screen capture was started with, and its sink.
#[derive(Default)]
struct FakeScreens {
    started: Mutex<Vec<String>>,
    sink: Mutex<Option<FrameSink<VideoFrame>>>,
    preview: Mutex<Option<Arc<PreviewSlot>>>,
    stopped: Arc<AtomicBool>,
    fail: AtomicBool,
}

struct FakeScreen(Arc<AtomicBool>);

impl ScreenCapture for FakeScreen {
    fn stop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

fn install_fake_screen(driver: &mut ConnectDriver<Arc<RecordingSender>>) -> Arc<FakeScreens> {
    let screens = Arc::new(FakeScreens::default());
    let handle = screens.clone();
    driver.inject_call_screen(Arc::new(move |source, _options, sink, preview| {
        if handle.fail.load(Ordering::SeqCst) {
            return Err(CaptureError::Permission("no access".into()));
        }
        handle.started.lock().unwrap().push(source.0.clone());
        *handle.sink.lock().unwrap() = Some(sink);
        *handle.preview.lock().unwrap() = Some(preview);
        handle.stopped.store(false, Ordering::SeqCst);
        Ok(Box::new(FakeScreen(handle.stopped.clone())))
    }));
    screens
}

fn frame(fill: u8) -> VideoFrame {
    VideoFrame {
        i420: vec![fill; 320 * 240 * 3 / 2],
        width: 320,
        height: 240,
        captured_at: Instant::now(),
    }
}

fn sink_of(screens: &FakeScreens) -> FrameSink<VideoFrame> {
    screens
        .sink
        .lock()
        .unwrap()
        .clone()
        .expect("capture started")
}

fn wait_for(mut done: impl FnMut() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(start.elapsed() < Duration::from_secs(5), "timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn sent_of(driver: &ConnectDriver<Arc<RecordingSender>>, kind: &str) -> Vec<Value> {
    driver
        .sender
        .snapshot()
        .iter()
        .filter_map(|json| serde_json::from_str::<Value>(json).ok())
        .filter(|value| value["@type"] == kind)
        .collect()
}

fn sharing_group_call() -> ActiveGroupCall {
    let mut call = tracked_group_call(false, false, false);
    call.transport_ready = true;
    call.screen_sharing = true;
    let json: Value =
        serde_json::from_str(&group_participant_json(777, true, "null", "null")).unwrap();
    let me = parse_group_call_participant(json.get("participant")).unwrap();
    call.participants = vec![me].into();
    call
}

fn group_driver() -> (
    std::path::PathBuf,
    ConnectDriver<Arc<RecordingSender>>,
    MockEngine,
    Arc<FakeScreens>,
) {
    let (dir, _recorder, mut driver, _seq) = group_call_test_driver();
    let mock = MockEngine::new();
    let mut handle = mock.clone();
    driver.set_call_engine(Box::new(mock));
    handle
        .create_group_call(77, -100, crate::calls::engine::GroupCallRole::Member)
        .unwrap();
    handle.connect_group_call(77, "answer", None).unwrap();
    let screens = install_fake_screen(&mut driver);
    (dir, driver, handle, screens)
}

#[test]
fn the_chosen_window_feeds_the_group_presentation() {
    let (dir, mut driver, engine, screens) = group_driver();
    // Sharing but nothing chosen: nothing starts.
    driver.session.calls.active_group_call = Some(sharing_group_call());
    pump_until_screen_answers(&mut driver);
    assert!(screens.started.lock().unwrap().is_empty());

    driver.choose_call_screen_source(SourceId("window:42".into()), false);
    pump_until_screen_answers(&mut driver);
    assert_eq!(*screens.started.lock().unwrap(), vec!["window:42"]);
    assert_eq!(
        driver.call_screen().target(),
        Some(CaptureTarget::Group(77))
    );
    let sink = sink_of(&screens);
    sink.push(frame(1));
    sink.push(frame(2));
    wait_for(|| engine.external_frames().len() == 2);
    assert!(engine.external_frames().iter().all(|f| {
        f.device == ExternalDevice::Screen
            && (f.meta.width, f.meta.height) == (320, 240)
            && f.bytes.len() == 320 * 240 * 3 / 2
    }));
    // Nothing paused yet: no pause request went out.
    assert!(sent_of(&driver, "toggleGroupCallScreenSharingIsPaused").is_empty());

    // Picking another source switches the running share.
    driver.choose_call_screen_source(SourceId("display:1".into()), false);
    pump_until_screen_answers(&mut driver);
    assert_eq!(
        *screens.started.lock().unwrap(),
        vec!["window:42", "display:1"]
    );

    // Sharing stops: the capture closes and the pick is forgotten.
    driver
        .session
        .calls
        .active_group_call
        .as_mut()
        .unwrap()
        .screen_sharing = false;
    pump_until_screen_answers(&mut driver);
    until(|| screens.stopped.load(Ordering::SeqCst));
    assert_eq!(driver.call_screen().target(), None);
    assert!(driver.call_screen().chosen().is_none());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_hidden_window_pauses_once_per_edge() {
    let (dir, mut driver, engine, screens) = group_driver();
    driver.session.calls.active_group_call = Some(sharing_group_call());
    driver.choose_call_screen_source(SourceId("window:42".into()), false);
    pump_until_screen_answers(&mut driver);
    let sink = sink_of(&screens);

    sink.push_event(CaptureEvent::Paused);
    pump_until_screen_answers(&mut driver);
    let paused = sent_of(&driver, "toggleGroupCallScreenSharingIsPaused");
    assert_eq!(paused.len(), 1);
    assert_eq!(paused[0]["group_call_id"], 77);
    assert_eq!(paused[0]["is_paused"], true);
    assert!(driver.call_screen().paused());
    assert!(
        driver
            .session
            .calls
            .active_group_call
            .as_ref()
            .unwrap()
            .screen_share_paused
    );
    // Frames while paused are dropped, not sent.
    sink.push(frame(3));
    std::thread::sleep(Duration::from_millis(50));
    assert!(engine.external_frames().is_empty());

    // A second Paused (and a Paused/Resumed/Paused burst) is no new edge.
    sink.push_event(CaptureEvent::Paused);
    pump_until_screen_answers(&mut driver);
    sink.push_event(CaptureEvent::Resumed);
    sink.push_event(CaptureEvent::Paused);
    pump_until_screen_answers(&mut driver);
    assert_eq!(
        sent_of(&driver, "toggleGroupCallScreenSharingIsPaused").len(),
        1
    );

    sink.push_event(CaptureEvent::Resumed);
    pump_until_screen_answers(&mut driver);
    let toggles = sent_of(&driver, "toggleGroupCallScreenSharingIsPaused");
    assert_eq!(toggles.len(), 2);
    assert_eq!(toggles[1]["is_paused"], false);
    assert!(!driver.call_screen().paused());
    sink.push(frame(4));
    wait_for(|| engine.external_frames().len() == 1);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_closed_window_ends_the_share() {
    let (dir, mut driver, _engine, screens) = group_driver();
    driver.session.calls.active_group_call = Some(sharing_group_call());
    driver.choose_call_screen_source(SourceId("window:42".into()), false);
    pump_until_screen_answers(&mut driver);
    sink_of(&screens).push_event(CaptureEvent::Ended);
    assert!(
        driver.request_tick(Instant::now()),
        "no Telegram update needed"
    );
    until(|| screens.stopped.load(Ordering::SeqCst));
    assert_eq!(sent_of(&driver, "endGroupCallScreenSharing").len(), 1);
    let call = driver.session.calls.active_group_call.as_ref().unwrap();
    assert!(!call.screen_sharing && !call.screen_share_paused);
    assert!(driver.call_screen().chosen().is_none());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_refused_capture_ends_the_share_with_its_error() {
    let (dir, mut driver, _engine, screens) = group_driver();
    screens.fail.store(true, Ordering::SeqCst);
    driver.session.calls.active_group_call = Some(sharing_group_call());
    driver.choose_call_screen_source(SourceId("display:1".into()), false);
    pump_until_screen_answers(&mut driver);
    assert_eq!(driver.call_screen().error.as_deref(), Some("no access"));
    assert_eq!(sent_of(&driver, "endGroupCallScreenSharing").len(), 1);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn starting_needs_a_chosen_source_but_stopping_does_not() {
    let (dir, mut driver, _engine, _screens) = group_driver();
    let mut call = sharing_group_call();
    call.screen_sharing = false;
    call.is_joined = true;
    driver.session.calls.active_group_call = Some(call);
    assert!(!driver.group_call_screen_source_available());
    assert!(driver.toggle_group_call_screen_share().is_err());
    driver.choose_call_screen_source(SourceId("display:1".into()), false);
    assert!(driver.group_call_screen_source_available());
    assert!(driver.toggle_group_call_screen_share().is_ok());
    assert_eq!(sent_of(&driver, "startGroupCallScreenSharing").len(), 1);
    // A share in flight stops even with no pick left.
    let mut forgot = crate::connect::CallScreen::default();
    forgot.inject_opener(Arc::new(|_, _, _, _| Err(CaptureError::NoDevice)));
    driver.call_screen = forgot;
    assert!(driver.toggle_group_call_screen_share().is_ok());
    assert_eq!(sent_of(&driver, "endGroupCallScreenSharing").len(), 1);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn your_shared_screen_shows_in_your_tile() {
    let (dir, mut driver, _engine, screens) = group_driver();
    driver.session.calls.active_group_call = Some(sharing_group_call());
    driver.choose_call_screen_source(SourceId("display:1".into()), false);
    pump_until_screen_answers(&mut driver);
    let preview = screens.preview.lock().unwrap().clone().unwrap();
    preview.offer(&frame(128));
    pump_until_screen_answers(&mut driver);
    let tile = driver
        .live_sink(crate::calls::live_video::LiveKey::Video {
            call_id: 77,
            user_id: 777,
            screen: true,
        })
        .unwrap();
    let shown = tile.take().unwrap();
    assert_eq!((shown.width, shown.height), (320, 240));
    // The same preview is not handed over twice.
    pump_until_screen_answers(&mut driver);
    assert!(tile.take().is_none());
    assert_eq!(tile.snapshot().pushed, 1);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_private_call_shares_without_pause_requests() {
    let (dir, mut driver, handle, _sink, _seq) = ready_call_driver();
    let screens = install_fake_screen(&mut driver);
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
    driver.choose_call_screen_source(SourceId("display:1".into()), false);
    {
        let call = driver.session.calls.active_call.as_mut().unwrap();
        call.transport = Some(TransportState::Connected);
        call.screen_sharing = true;
    }
    pump_until_screen_answers(&mut driver);
    assert_eq!(
        driver.call_screen().target(),
        Some(CaptureTarget::Private(77))
    );
    let sink = sink_of(&screens);
    sink.push(frame(5));
    wait_for(|| handle.external_frames().len() == 1);
    sink.push_event(CaptureEvent::Paused);
    pump_until_screen_answers(&mut driver);
    assert!(driver.call_screen().paused());
    assert!(sent_of(&driver, "toggleGroupCallScreenSharingIsPaused").is_empty());
    sink.push(frame(6));
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(handle.external_frames().len(), 1);
    std::fs::remove_dir_all(dir).ok();
}

/// What the fake loopback was started with.
#[derive(Default)]
struct FakeLoopbacks {
    started: Mutex<Vec<Arc<SystemAudioMix>>>,
    stopped: Arc<AtomicBool>,
    fail: AtomicBool,
}

struct FakeLoopback(Arc<AtomicBool>);

impl SystemAudioCapture for FakeLoopback {
    fn stop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

struct SilentMic;

impl MicCapture for SilentMic {
    fn start(&mut self, _: Option<&str>, _: FrameSink<AudioFrame>) -> Result<(), CaptureError> {
        Ok(())
    }

    fn stop(&mut self) {}
}

type KeptControls = Arc<Mutex<Option<Arc<MicControls>>>>;

/// A group driver with a fake loopback and a fake microphone whose
/// controls are kept.
fn audio_driver() -> (
    std::path::PathBuf,
    ConnectDriver<Arc<RecordingSender>>,
    Arc<FakeLoopbacks>,
    KeptControls,
) {
    let (dir, mut driver, _engine, _screens) = group_driver();
    let loopbacks = Arc::new(FakeLoopbacks::default());
    let handle = loopbacks.clone();
    driver.inject_call_system_audio(Arc::new(move |mix| {
        if handle.fail.load(Ordering::SeqCst) {
            return Err(CaptureError::Device("no speakers".into()));
        }
        handle.started.lock().unwrap().push(mix);
        handle.stopped.store(false, Ordering::SeqCst);
        Ok(Box::new(FakeLoopback(handle.stopped.clone())))
    }));
    let controls: KeptControls = Arc::new(Mutex::new(None));
    let kept = controls.clone();
    driver.inject_call_microphone(Arc::new(move |_device, controls, _sink| {
        *kept.lock().unwrap() = Some(controls);
        Ok(Box::new(SilentMic))
    }));
    driver.session.calls.active_group_call = Some(sharing_group_call());
    (dir, driver, loopbacks, controls)
}

fn mic_mix(controls: &Mutex<Option<Arc<MicControls>>>) -> Option<Arc<SystemAudioMix>> {
    controls.lock().unwrap().as_ref()?.system_audio()
}

#[test]
fn shared_system_audio_reaches_the_microphone_while_the_share_runs() {
    let (dir, mut driver, loopbacks, controls) = audio_driver();
    pump_until_mic_answers(&mut driver);
    driver.choose_call_screen_source(SourceId("display:1".into()), true);
    pump_until_screen_answers(&mut driver);
    assert!(driver.call_screen().with_audio() && driver.call_screen().sharing_audio());
    let started = loopbacks.started.lock().unwrap().clone();
    assert_eq!(started.len(), 1);
    // The running microphone mixes exactly the loopback's queue.
    let mixed = mic_mix(&controls).expect("handed to the microphone");
    assert!(Arc::ptr_eq(&mixed, &started[0]));
    // Pumping again neither restarts it nor swaps the queue.
    pump_until_screen_answers(&mut driver);
    assert_eq!(loopbacks.started.lock().unwrap().len(), 1);

    // Picking the same screen without the box stops only the audio.
    driver.choose_call_screen_source(SourceId("display:1".into()), false);
    pump_until_screen_answers(&mut driver);
    until(|| loopbacks.stopped.load(Ordering::SeqCst));
    assert!(!driver.call_screen().sharing_audio());
    assert!(driver.call_screen().target().is_some());
    assert!(mic_mix(&controls).is_none());

    // Back on, then the share stops: the audio goes with it.
    driver.choose_call_screen_source(SourceId("display:1".into()), true);
    pump_until_screen_answers(&mut driver);
    assert!(mic_mix(&controls).is_some());
    driver
        .session
        .calls
        .active_group_call
        .as_mut()
        .unwrap()
        .screen_sharing = false;
    pump_until_screen_answers(&mut driver);
    until(|| loopbacks.stopped.load(Ordering::SeqCst));
    assert!(mic_mix(&controls).is_none());
    assert!(!driver.call_screen().with_audio());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_microphone_opened_later_still_gets_the_system_audio() {
    let (dir, mut driver, loopbacks, controls) = audio_driver();
    driver.choose_call_screen_source(SourceId("display:1".into()), true);
    pump_until_screen_answers(&mut driver);
    assert!(mic_mix(&controls).is_none(), "no microphone yet");
    pump_until_mic_answers(&mut driver);
    let started = loopbacks.started.lock().unwrap().clone();
    let mixed = mic_mix(&controls).expect("handed over on open");
    assert!(Arc::ptr_eq(&mixed, &started[0]));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn failed_system_audio_keeps_the_screen_and_says_why() {
    let (dir, mut driver, loopbacks, controls) = audio_driver();
    loopbacks.fail.store(true, Ordering::SeqCst);
    pump_until_mic_answers(&mut driver);
    driver.choose_call_screen_source(SourceId("display:1".into()), true);
    pump_until_screen_answers(&mut driver);
    assert!(driver.call_screen().target().is_some());
    assert!(!driver.call_screen().with_audio(), "not retried every pump");
    assert_eq!(driver.call_screen().error.as_deref(), Some("no speakers"));
    assert!(mic_mix(&controls).is_none());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn without_loopback_the_box_is_ignored() {
    let (dir, mut driver, _engine, _screens) = group_driver();
    assert!(!driver.call_screen().system_audio_enabled());
    driver.session.calls.active_group_call = Some(sharing_group_call());
    driver.choose_call_screen_source(SourceId("display:1".into()), true);
    pump_until_screen_answers(&mut driver);
    assert!(driver.call_screen().target().is_some());
    assert!(!driver.call_screen().with_audio());
    assert!(!driver.call_screen().sharing_audio());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_slow_screen_capture_does_not_hold_the_pump() {
    let (dir, mut driver, _engine, _screens) = group_driver();
    let (release, released) = std::sync::mpsc::channel::<()>();
    let released = Mutex::new(released);
    let stopped = Arc::new(AtomicBool::new(false));
    let flag = stopped.clone();
    let pump_thread = std::thread::current().id();
    driver.inject_call_screen(Arc::new(move |_source, _options, _sink, _preview| {
        assert_ne!(std::thread::current().id(), pump_thread);
        let _ = released.lock().unwrap().recv();
        Ok(Box::new(FakeScreen(flag.clone())))
    }));
    driver.session.calls.active_group_call = Some(sharing_group_call());
    driver.choose_call_screen_source(SourceId("display:1".into()), false);

    driver.pump_call_screen();

    assert!(driver.call_screen().opening());
    assert_eq!(driver.call_screen().target(), None);
    release.send(()).unwrap();
    pump_until_screen_answers(&mut driver);
    assert_eq!(
        driver.call_screen().target(),
        Some(CaptureTarget::Group(77))
    );
    std::fs::remove_dir_all(dir).ok();
}
