//! Connect-driver tests: the shared screen Quill captures and feeds into
//! the engine's external screen source (`connect::call_screen`).
use super::super::*;
use super::*;
use crate::calls::capture::screen::{PreviewSlot, ScreenCapture, SourceId};
use crate::calls::capture::{CaptureError, CaptureEvent, FrameSink, VideoFrame};
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
    driver.inject_call_screen(Box::new(move |source, _options, sink, preview| {
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
    call.participants = vec![me];
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
    handle.create_group_call(77, -100).unwrap();
    handle.connect_group_call(77, "answer", false).unwrap();
    let screens = install_fake_screen(&mut driver);
    (dir, driver, handle, screens)
}

#[test]
fn the_chosen_window_feeds_the_group_presentation() {
    let (dir, mut driver, engine, screens) = group_driver();
    // Sharing but nothing chosen: nothing starts.
    driver.session.calls.active_group_call = Some(sharing_group_call());
    driver.pump_call_screen();
    assert!(screens.started.lock().unwrap().is_empty());

    driver.choose_call_screen_source(SourceId("window:42".into()));
    driver.pump_call_screen();
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
    driver.choose_call_screen_source(SourceId("display:1".into()));
    driver.pump_call_screen();
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
    driver.pump_call_screen();
    assert!(screens.stopped.load(Ordering::SeqCst));
    assert_eq!(driver.call_screen().target(), None);
    assert!(driver.call_screen().chosen().is_none());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_hidden_window_pauses_once_per_edge() {
    let (dir, mut driver, engine, screens) = group_driver();
    driver.session.calls.active_group_call = Some(sharing_group_call());
    driver.choose_call_screen_source(SourceId("window:42".into()));
    driver.pump_call_screen();
    let sink = sink_of(&screens);

    sink.push_event(CaptureEvent::Paused);
    driver.pump_call_screen();
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
    driver.pump_call_screen();
    sink.push_event(CaptureEvent::Resumed);
    sink.push_event(CaptureEvent::Paused);
    driver.pump_call_screen();
    assert_eq!(
        sent_of(&driver, "toggleGroupCallScreenSharingIsPaused").len(),
        1
    );

    sink.push_event(CaptureEvent::Resumed);
    driver.pump_call_screen();
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
    driver.choose_call_screen_source(SourceId("window:42".into()));
    driver.pump_call_screen();
    sink_of(&screens).push_event(CaptureEvent::Ended);
    driver.pump_call_screen();
    assert!(screens.stopped.load(Ordering::SeqCst));
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
    driver.choose_call_screen_source(SourceId("display:1".into()));
    driver.pump_call_screen();
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
    driver.choose_call_screen_source(SourceId("display:1".into()));
    assert!(driver.group_call_screen_source_available());
    assert!(driver.toggle_group_call_screen_share().is_ok());
    assert_eq!(sent_of(&driver, "startGroupCallScreenSharing").len(), 1);
    // A share in flight stops even with no pick left.
    let mut forgot = crate::connect::CallScreen::default();
    forgot.inject_opener(Box::new(|_, _, _, _| Err(CaptureError::NoDevice)));
    driver.call_screen = forgot;
    assert!(driver.toggle_group_call_screen_share().is_ok());
    assert_eq!(sent_of(&driver, "endGroupCallScreenSharing").len(), 1);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn your_shared_screen_shows_in_your_tile() {
    let (dir, mut driver, _engine, screens) = group_driver();
    driver.session.calls.active_group_call = Some(sharing_group_call());
    driver.choose_call_screen_source(SourceId("display:1".into()));
    driver.pump_call_screen();
    let preview = screens.preview.lock().unwrap().clone().unwrap();
    preview.offer(&frame(128));
    driver.pump_call_screen();
    let shown = driver.latest_group_video_frame(77, 777, true).unwrap();
    assert!(shown.is_screen && shown.is_local);
    assert_eq!((shown.width, shown.height), (320, 240));
    let first = shown.seq;
    // The same preview is not handed over twice.
    driver.pump_call_screen();
    assert_eq!(
        driver.latest_group_video_frame(77, 777, true).unwrap().seq,
        first
    );
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
                video_enabled: false,
                camera_input: None,
            },
        )
        .unwrap();
    driver.choose_call_screen_source(SourceId("display:1".into()));
    {
        let call = driver.session.calls.active_call.as_mut().unwrap();
        call.transport = Some(TransportState::Connected);
        call.screen_sharing = true;
    }
    driver.pump_call_screen();
    assert_eq!(
        driver.call_screen().target(),
        Some(CaptureTarget::Private(77))
    );
    let sink = sink_of(&screens);
    sink.push(frame(5));
    wait_for(|| handle.external_frames().len() == 1);
    sink.push_event(CaptureEvent::Paused);
    driver.pump_call_screen();
    assert!(driver.call_screen().paused());
    assert!(sent_of(&driver, "toggleGroupCallScreenSharingIsPaused").is_empty());
    sink.push(frame(6));
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(handle.external_frames().len(), 1);
    std::fs::remove_dir_all(dir).ok();
}
