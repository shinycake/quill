//! Driver tests: call video frames, cameras and screen sharing.
use super::*;
use crate::calls::live_video::{LiveFrame, LiveKey, LiveRotation};
use std::time::Instant;

/// Phase C2e: the engine's peer camera state reaches the tracked
/// call through the pump; emissions for other call ids are dropped.
#[test]
fn remote_video_state_drain() {
    let (dir, mut driver, handle, sink, seq) = ready_call_driver();
    ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
    handle.emit_remote_video_state(999, RemoteVideoState::Active);
    handle.emit_remote_video_state(77, RemoteVideoState::Paused);
    ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
    assert_eq!(
        driver
            .session
            .calls
            .active_call
            .as_ref()
            .unwrap()
            .remote_video,
        RemoteVideoState::Paused
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A 2 × 2 I420 picture.
fn planes() -> LiveFrame {
    LiveFrame::new(0, 2, 2, LiveRotation::None, vec![0; 6], Instant::now()).expect("frame")
}

/// A 1:1 call 77 picture's feed key.
fn call_key(local: bool, screen: bool) -> LiveKey {
    LiveKey::Call {
        call_id: 77,
        local,
        screen,
    }
}

const PUMP: &str = r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#;

/// The peer's camera and screen-share states as the engine reports them,
/// pumped onto the tracked call.
fn peer_states(
    driver: &mut ConnectDriver<Arc<RecordingSender>>,
    handle: &MockEngine,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
    camera: RemoteVideoState,
    screen: RemoteVideoState,
) {
    handle.emit_remote_video_state(77, camera);
    handle.emit_remote_screen_state(77, screen);
    ingest_call_json(driver, seq, sink, PUMP);
}

/// A 1:1 picture goes to its live feed as I420 planes, only the newest
/// frame waiting; a feed exists only while its video is on.
#[test]
fn one_to_one_pictures_go_to_live_feeds_newest_first() {
    let (dir, mut driver, handle, sink, seq) = ready_call_driver();
    // The peer's camera is off: no feed, and a frame goes nowhere.
    assert!(driver.live_sink(call_key(false, false)).is_none());
    peer_states(
        &mut driver,
        &handle,
        &seq,
        &sink,
        RemoteVideoState::Active,
        RemoteVideoState::Inactive,
    );
    let camera = driver
        .live_sink(call_key(false, false))
        .expect("the peer's camera feed");
    handle.emit_live_frame(call_key(false, false), planes());
    handle.emit_live_frame(call_key(false, false), planes());
    let snapshot = camera.snapshot();
    assert_eq!(snapshot.pushed, 2);
    assert_eq!(snapshot.dropped, 1, "an untaken frame is replaced");
    assert_eq!(camera.take().map(|frame| frame.seq), Some(1));
    // Your camera is off: no preview feed.
    assert!(driver.live_sink(call_key(true, false)).is_none());
    // Another call's id leaves nothing behind.
    assert!(
        driver
            .live_sink(LiveKey::Call {
                call_id: 78,
                local: false,
                screen: false
            })
            .is_none()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Your camera preview has its own feed while the camera is on; turning
/// it off ends the feed, and on again starts a new one.
#[test]
fn the_camera_preview_feed_follows_the_camera() {
    let (dir, mut driver, handle, _sink, _seq) = ready_call_driver();
    assert!(driver.set_call_camera(77, true).is_ok());
    let preview = driver
        .live_sink(call_key(true, false))
        .expect("the preview feed");
    handle.emit_live_frame(call_key(true, false), planes());
    assert!(preview.latest().is_some());
    assert!(driver.set_call_camera(77, false).is_ok());
    assert!(preview.snapshot().end.is_some(), "the old feed ended");
    assert!(driver.live_sink(call_key(true, false)).is_none());
    assert!(driver.set_call_camera(77, true).is_ok());
    let again = driver
        .live_sink(call_key(true, false))
        .expect("a new preview feed");
    assert!(!again.same_as(&preview));
    assert!(again.latest().is_none(), "no frame from before");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Phase C2j: the peer's 1:1 screen share has its own feed and never
/// clobbers the camera's; when the share goes inactive its feed ends
/// (no stale picture can render) and the camera's stays.
#[test]
fn the_screen_share_feed_ends_when_the_share_goes_inactive() {
    let (dir, mut driver, handle, sink, seq) = ready_call_driver();
    let remote_screen = |driver: &ConnectDriver<Arc<RecordingSender>>| {
        driver
            .session
            .calls
            .active_call
            .as_ref()
            .unwrap()
            .remote_screen
    };
    assert_eq!(remote_screen(&driver), RemoteVideoState::Inactive);
    peer_states(
        &mut driver,
        &handle,
        &seq,
        &sink,
        RemoteVideoState::Active,
        RemoteVideoState::Active,
    );
    assert_eq!(remote_screen(&driver), RemoteVideoState::Active);
    let camera = driver.live_sink(call_key(false, false)).expect("camera");
    let screen = driver.live_sink(call_key(false, true)).expect("screen");
    assert!(!camera.same_as(&screen));
    handle.emit_live_frame(call_key(false, false), planes());
    handle.emit_live_frame(call_key(false, true), planes());
    handle.emit_live_frame(call_key(false, true), planes());
    assert_eq!(camera.snapshot().pushed, 1);
    assert_eq!(screen.snapshot().pushed, 2);
    peer_states(
        &mut driver,
        &handle,
        &seq,
        &sink,
        RemoteVideoState::Active,
        RemoteVideoState::Paused,
    );
    assert!(
        screen.snapshot().end.is_none(),
        "a paused share keeps its feed"
    );
    peer_states(
        &mut driver,
        &handle,
        &seq,
        &sink,
        RemoteVideoState::Active,
        RemoteVideoState::Inactive,
    );
    assert_eq!(remote_screen(&driver), RemoteVideoState::Inactive);
    assert!(screen.snapshot().end.is_some());
    assert!(driver.live_sink(call_key(false, true)).is_none());
    assert!(camera.snapshot().end.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Phase C2e: the UI camera toggle reaches the engine with the
/// call id, the new state, and the selected camera — but only
/// once a transport exists; before that the intent is stored
/// cleanly (the real engine errors on an untracked call) so it
/// can apply on connect.
#[test]
fn set_call_camera_gates_engine_on_transport() {
    let (dir, mut driver, handle, sink, seq) = ready_call_driver();
    // No transport yet: intent stored, engine untouched.
    assert!(driver.set_call_camera(77, false).is_ok());
    assert!(handle.camera_changes().is_empty());
    assert!(!driver.session.calls.active_call.as_ref().unwrap().camera_on);
    // Transport connected: the toggle drives the engine.
    ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
    assert!(driver.set_call_camera(77, true).is_ok());
    assert_eq!(handle.camera_changes(), vec![(77, true, None)]);
    assert!(driver.session.calls.active_call.as_ref().unwrap().camera_on);
    assert_eq!(
        driver.set_call_camera(999, true),
        Err(crate::calls::engine::EngineError::NoSuchCall(999))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Phase C2i: the UI screen-share toggle reaches the engine with
/// the call id and the new state — but only once a transport
/// exists; before that the intent is stored cleanly so it can
/// apply on connect. Without an enumerated screen source the
/// toggle is rejected (`NoScreenSource`) and the flag stays put.
#[test]
fn set_call_screen_share_gates_engine_on_transport() {
    let (dir, mut driver, handle, sink, seq) = ready_call_driver();
    handle.set_devices(vec![MediaDevice {
        id: "screen-0".into(),
        name: "Test Screen".into(),
        kind: MediaDeviceKind::Screen,
    }]);
    driver.refresh_call_devices();
    // No transport yet: intent stored, engine untouched.
    assert!(driver.set_call_screen_share(77, true).is_ok());
    assert!(handle.p2p_screen_share_changes().is_empty());
    assert!(
        driver
            .session
            .calls
            .active_call
            .as_ref()
            .unwrap()
            .screen_sharing
    );
    // Phase C2i: screen share clears the camera intent (ntgcalls
    // forbids camera+screen in Capture mode).
    assert!(!driver.session.calls.active_call.as_ref().unwrap().camera_on);
    // Transport connected: the pre-transport screen-share intent
    // applies on connect (pump_call_engine forwards it), then the
    // toggle drives the engine directly.
    ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
    assert!(driver.set_call_screen_share(77, false).is_ok());
    assert_eq!(
        handle.p2p_screen_share_changes(),
        vec![(77, true), (77, false)]
    );
    assert!(
        !driver
            .session
            .calls
            .active_call
            .as_ref()
            .unwrap()
            .screen_sharing
    );
    // Phase C2i: enabling the camera clears the screen-share
    // intent symmetrically.
    assert!(driver.set_call_camera(77, true).is_ok());
    assert!(driver.session.calls.active_call.as_ref().unwrap().camera_on);
    assert!(
        !driver
            .session
            .calls
            .active_call
            .as_ref()
            .unwrap()
            .screen_sharing
    );
    assert_eq!(
        driver.set_call_screen_share(999, true),
        Err(crate::calls::engine::EngineError::NoSuchCall(999))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Phase C2i: no enumerated screen source → *enabling* is
/// rejected and the tracked flag is untouched; *stopping* still
/// works (a vanished display must not trap the user in "sharing").
#[test]
fn set_call_screen_share_rejected_without_screen_source() {
    let (dir, mut driver, handle, sink, seq) = ready_call_driver();
    driver.refresh_call_devices();
    ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
    assert_eq!(
        driver.set_call_screen_share(77, true),
        Err(crate::calls::engine::EngineError::NoScreenSource)
    );
    assert!(
        !driver
            .session
            .calls
            .active_call
            .as_ref()
            .unwrap()
            .screen_sharing
    );
    assert!(handle.p2p_screen_share_changes().is_empty());
    // Stopping needs no source: simulate a stranded sharing flag
    // and verify it can still be cleared.
    driver
        .session
        .calls
        .active_call
        .as_mut()
        .unwrap()
        .screen_sharing = true;
    assert!(driver.set_call_screen_share(77, false).is_ok());
    assert!(
        !driver
            .session
            .calls
            .active_call
            .as_ref()
            .unwrap()
            .screen_sharing
    );
    assert_eq!(handle.p2p_screen_share_changes(), vec![(77, false)]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Phase C2e: picking a camera before the transport exists stores
/// the selection cleanly without error and without an engine
/// forward.
#[test]
fn select_call_camera_without_transport_stores_selection() {
    let (dir, mut driver, handle, _sink, _seq) = ready_call_driver();
    assert!(driver.select_call_camera(Some("cam-1".into())).is_ok());
    assert!(handle.camera_changes().is_empty());
    assert_eq!(driver.selected_call_camera(), Some("cam-1"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Phase C2e: no camera enumerated → a video call must not
/// negotiate video.
#[test]
fn connect_params_video_honors_no_camera() {
    let (dir, mut driver, handle, sink, seq) = ready_call_driver();
    driver.refresh_call_devices();
    ingest_call_json(&mut driver, &seq, &sink, &ready_video_call_json());
    let connects = handle.connects();
    assert_eq!(connects.len(), 1);
    assert!(!connects[0].1.video_enabled);
    assert_eq!(connects[0].1.camera_input, None);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Phase C2e: a camera-off toggle set before the transport exists
/// survives into the connect params — the call is negotiated
/// without video even though a camera is available.
#[test]
fn connect_params_camera_off_intent_survives_pre_connect() {
    let (dir, mut driver, _recorder, sink, seq) = call_driver();
    let mock = MockEngine::new();
    mock.set_devices(vec![MediaDevice {
        id: "cam-1".into(),
        name: "Test Cam".into(),
        kind: MediaDeviceKind::Camera,
    }]);
    let handle = mock.clone();
    driver.set_call_engine(Box::new(mock));
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
    );
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":true,"is_video":true,"state":{"@type":"callStatePending","is_created":true,"is_received":false}}}"#,
    );
    // Camera toggled off before the transport exists: stores the
    // intent without an engine forward.
    assert!(driver.set_call_camera(77, false).is_ok());
    assert!(handle.camera_changes().is_empty());
    ingest_call_json(&mut driver, &seq, &sink, &ready_video_call_json());
    let connects = handle.connects();
    assert_eq!(connects.len(), 1);
    assert!(!connects[0].1.video_enabled);
    assert_eq!(connects[0].1.camera_input, None);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Phase C2e: a camera enumerated → video negotiated with the
/// user's camera pick (or the engine default when unset).
#[test]
fn connect_params_video_selects_camera() {
    let (dir, mut driver, _recorder, sink, seq) = call_driver();
    let mock = MockEngine::new();
    mock.set_devices(vec![MediaDevice {
        id: "cam-1".into(),
        name: "Test Cam".into(),
        kind: MediaDeviceKind::Camera,
    }]);
    let handle = mock.clone();
    driver.set_call_engine(Box::new(mock));
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
    );
    driver.select_call_camera(Some("cam-1".into())).unwrap();
    ingest_call_json(&mut driver, &seq, &sink, &ready_video_call_json());
    let connects = handle.connects();
    assert_eq!(connects.len(), 1);
    assert!(connects[0].1.video_enabled);
    assert_eq!(connects[0].1.camera_input, Some("cam-1".to_string()));
    assert_eq!(driver.selected_call_camera(), Some("cam-1"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Phase C2e: the reducer initializes the camera intent from
/// `is_video` — video calls start with the camera on.
#[test]
fn active_call_camera_on_from_is_video() {
    for (is_video, expected) in [(false, false), (true, true)] {
        let (dir, mut driver, _recorder, sink, seq) = call_driver();
        ingest_call_json(
            &mut driver,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"updateCall","call":{{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":true,"is_video":{is_video},"state":{{"@type":"callStatePending","is_created":true,"is_received":false}}}}}}"#
            ),
        );
        let call = driver.session.calls.active_call.as_ref().unwrap();
        assert_eq!(call.camera_on, expected);
        assert_eq!(call.remote_video, RemoteVideoState::Inactive);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Phase C2e: an ended call's feeds end and go: the UI can never render
/// a stale picture from a previous call.
#[test]
fn feeds_end_with_the_call() {
    let (dir, mut driver, handle, sink, seq) = ready_call_driver();
    ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
    peer_states(
        &mut driver,
        &handle,
        &seq,
        &sink,
        RemoteVideoState::Active,
        RemoteVideoState::Inactive,
    );
    let camera = driver.live_sink(call_key(false, false)).expect("camera");
    handle.emit_live_frame(call_key(false, false), planes());
    assert!(camera.latest().is_some());
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":true,"is_video":false,"state":{"@type":"callStateDiscarded","reason":{"@type":"callDiscardReasonHungUp"},"need_rating":false,"need_debug_information":false,"need_log":false}}}"#,
    );
    assert!(camera.snapshot().end.is_some());
    assert!(driver.live_sink(call_key(false, false)).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}
