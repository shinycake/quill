//! Connect-driver tests: your own microphone level in a group call.
use super::super::*;
use super::*;
use crate::connect::{LevelSource, tap_wanted};
use crate::state::ActiveGroupCall;
use crate::telegram::envelope::parse_group_call_participant;
use serde_json::Value;
use std::sync::Arc;
use std::sync::Mutex;

/// A level the test sets; `None` means no window completed.
struct TestLevel(Mutex<Option<f32>>);

impl LevelSource for TestLevel {
    fn take_level(&self) -> Option<f32> {
        self.0.lock().unwrap().take()
    }

    fn error(&self) -> Option<String> {
        None
    }
}

fn joined_unmuted_call(audio_source_id: i32) -> ActiveGroupCall {
    let mut call = tracked_group_call(false, false, false);
    call.is_muted_self = false;
    let json: Value =
        serde_json::from_str(&group_participant_json(777, true, "null", "null")).unwrap();
    let mut me = parse_group_call_participant(json.get("participant")).unwrap();
    me.audio_source_id = audio_source_id;
    me.can_unmute_self = true;
    call.participants = Arc::new(vec![me]);
    call
}

#[test]
fn the_microphone_opens_on_a_worker_and_the_pump_picks_it_up() {
    let (dir, _recorder, mut driver, _seq) = group_call_test_driver();
    let (opened_on, on_thread) = std::sync::mpsc::channel();
    driver.call_audio.open_microphone = Arc::new(move || {
        let _ = opened_on.send(std::thread::current().id());
        Ok(Box::new(TestLevel(Mutex::new(None))) as Box<dyn LevelSource + Send>)
    });
    driver.session.calls.active_group_call = Some(joined_unmuted_call(4242));

    driver.pump_call_audio().unwrap();
    assert!(!driver.call_audio.is_open(), "the pump does not wait");
    assert_ne!(
        on_thread.recv().unwrap(),
        std::thread::current().id(),
        "opened off the driver's thread"
    );

    let answer = driver.call_audio.opening.take().unwrap().recv().unwrap();
    let (worker, opening) = std::sync::mpsc::channel();
    worker.send(answer).unwrap();
    driver.call_audio.opening = Some(opening);
    driver.pump_call_audio().unwrap();
    assert!(driver.call_audio.is_open(), "the pump took the source");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_tap_opened_after_the_call_stopped_wanting_it_is_dropped() {
    let (dir, _recorder, mut driver, _seq) = group_call_test_driver();
    let (release, released) = std::sync::mpsc::channel::<()>();
    let (answered, was_answered) = std::sync::mpsc::channel();
    driver.call_audio.open_microphone = Arc::new({
        let released = Mutex::new(released);
        let answered = Mutex::new(answered);
        move || {
            let _ = released.lock().unwrap().recv();
            let _ = answered.lock().unwrap().send(());
            Err("gone".into())
        }
    });
    driver.session.calls.active_group_call = Some(joined_unmuted_call(4242));
    driver.pump_call_audio().unwrap();

    driver.session.calls.active_group_call = None;
    driver.pump_call_audio().unwrap();
    release.send(()).unwrap();
    was_answered.recv().unwrap();

    driver.session.calls.active_group_call = Some(joined_unmuted_call(4242));
    driver.pump_call_audio().unwrap();
    assert_eq!(driver.call_audio.error, None, "nobody waits for that open");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn tap_runs_only_while_joined_and_allowed_to_speak() {
    let mut call = joined_unmuted_call(4242);
    assert_eq!(tap_wanted(&call), Some((77, 4242)));
    call.is_muted_self = true;
    assert_eq!(tap_wanted(&call), None, "muted: no tap");
    call.is_muted_self = false;
    call.is_joined = false;
    assert_eq!(tap_wanted(&call), None, "not joined: no tap");
    call.is_joined = true;
    let participants = Arc::make_mut(&mut call.participants);
    participants[0].is_muted_for_all_users = true;
    participants[0].can_unmute_self = false;
    assert_eq!(tap_wanted(&call), None, "muted by an admin: no tap");
    Arc::make_mut(&mut call.participants).clear();
    assert_eq!(tap_wanted(&call), None, "no self participant yet: no tap");
}

#[test]
fn speaking_is_sent_to_tdlib_and_withdrawn_on_mute() {
    let (dir, recorder, mut driver, _seq) = group_call_test_driver();
    let sent = || {
        recorder
            .snapshot()
            .iter()
            .map(|s| serde_json::from_str::<Value>(s).unwrap())
            .filter(|v| v["@type"] == "setGroupCallParticipantIsSpeaking")
            .collect::<Vec<_>>()
    };
    let level = Arc::new(TestLevel(Mutex::new(None)));
    driver
        .call_audio
        .inject(Box::new(SharedLevel(level.clone())));

    // No call: nothing opens, nothing is sent.
    driver.pump_call_audio().unwrap();
    assert!(!driver.call_audio.is_open());
    assert!(sent().is_empty());

    // Joined and unmuted: the tap opens; a quiet window sends nothing.
    driver.session.calls.active_group_call = Some(joined_unmuted_call(4242));
    *level.0.lock().unwrap() = Some(0.05);
    driver.pump_call_audio().unwrap();
    assert!(driver.call_audio.is_open());
    assert!(sent().is_empty());
    assert_eq!(driver.group_call_self_level(), (0.05, false));

    // A pump without a completed window changes nothing.
    driver.pump_call_audio().unwrap();
    assert_eq!(driver.group_call_self_level(), (0.05, false));

    // Voice: speaking, sent once with your audio source.
    *level.0.lock().unwrap() = Some(1.3);
    driver.pump_call_audio().unwrap();
    assert_eq!(driver.group_call_self_level(), (1.3, true));
    let requests = sent();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["group_call_id"], 77);
    assert_eq!(requests[0]["audio_source"], 4242);
    assert_eq!(requests[0]["is_speaking"], true);

    // Muting closes the tap and withdraws the speaking mark.
    driver
        .session
        .calls
        .active_group_call
        .as_mut()
        .unwrap()
        .is_muted_self = true;
    driver.pump_call_audio().unwrap();
    assert!(!driver.call_audio.is_open());
    assert_eq!(driver.group_call_self_level(), (0.0, false));
    let requests = sent();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1]["is_speaking"], false);

    // Muted: a loud window is never read.
    *level.0.lock().unwrap() = Some(2.0);
    driver.pump_call_audio().unwrap();
    assert_eq!(sent().len(), 2);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn an_unknown_audio_source_sends_nothing() {
    let (dir, recorder, mut driver, _seq) = group_call_test_driver();
    let level = Arc::new(TestLevel(Mutex::new(Some(1.5))));
    driver.call_audio.inject(Box::new(SharedLevel(level)));
    driver.session.calls.active_group_call = Some(joined_unmuted_call(0));
    driver.pump_call_audio().unwrap();
    assert_eq!(
        driver.group_call_self_level(),
        (1.5, true),
        "the level still shows"
    );
    assert!(
        recorder
            .snapshot()
            .iter()
            .all(|s| !s.contains("setGroupCallParticipantIsSpeaking")),
        "TDLib rejects audio source 0; nothing is sent"
    );
    std::fs::remove_dir_all(dir).ok();
}

/// An `Arc` level source, so the test keeps a handle after injecting.
struct SharedLevel(Arc<TestLevel>);

impl LevelSource for SharedLevel {
    fn take_level(&self) -> Option<f32> {
        self.0.take_level()
    }

    fn error(&self) -> Option<String> {
        self.0.error()
    }
}

/// The peer's microphone on/off (the engine's remote audio state)
/// lands on the tracked 1:1 call through the driver pump, and only on
/// that call.
#[test]
fn peer_microphone_state_follows_the_engine_hook() {
    let (dir, mut driver, handle, sink, seq) = ready_call_driver();
    let pump = r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#;
    let muted = |driver: &ConnectDriver<Arc<RecordingSender>>| {
        driver
            .session
            .calls
            .active_call
            .as_ref()
            .unwrap()
            .remote_audio_muted
    };
    assert!(!muted(&driver));
    handle.emit_remote_audio_muted(77, true);
    ingest_call_json(&mut driver, &seq, &sink, pump);
    assert!(muted(&driver));
    // Another call's state is dropped.
    handle.emit_remote_audio_muted(78, false);
    ingest_call_json(&mut driver, &seq, &sink, pump);
    assert!(muted(&driver));
    handle.emit_remote_audio_muted(77, false);
    ingest_call_json(&mut driver, &seq, &sink, pump);
    assert!(!muted(&driver));
    std::fs::remove_dir_all(dir).ok();
}

/// The peer's battery flag (an engine event) lands on the tracked 1:1
/// call at the next poll, and only on that call; the driver redraws.
#[test]
fn peer_battery_low_follows_the_engine_event() {
    use crate::calls::engine::EngineEvent;
    let (dir, mut driver, handle, _sink, _seq) = ready_call_driver();
    let low = |driver: &ConnectDriver<Arc<RecordingSender>>| {
        driver
            .session
            .calls
            .active_call
            .as_ref()
            .unwrap()
            .remote_battery_low
    };
    assert!(!low(&driver));
    let report = |call_id, is_low| EngineEvent::PeerBatteryLow { call_id, is_low };
    handle.emit_engine_event(report(77, true));
    assert!(driver.request_tick(std::time::Instant::now()));
    assert!(low(&driver));
    // Another call's report is dropped.
    handle.emit_engine_event(report(78, false));
    driver.request_tick(std::time::Instant::now());
    assert!(low(&driver));
    handle.emit_engine_event(report(77, false));
    driver.request_tick(std::time::Instant::now());
    assert!(!low(&driver));
    std::fs::remove_dir_all(dir).ok();
}

/// The connection's signal bars (an engine event) land on the tracked
/// 1:1 call; another call's are dropped.
#[test]
fn signal_bars_follow_the_engine_event() {
    use crate::calls::engine::EngineEvent;
    let (dir, mut driver, handle, _sink, _seq) = ready_call_driver();
    let bars = |driver: &ConnectDriver<Arc<RecordingSender>>| {
        driver
            .session
            .calls
            .active_call
            .as_ref()
            .unwrap()
            .signal_bars
    };
    assert_eq!(bars(&driver), None, "nothing until the engine reports");
    handle.emit_engine_event(EngineEvent::SignalBars {
        call_id: 77,
        bars: 3,
    });
    assert!(driver.request_tick(std::time::Instant::now()));
    assert_eq!(bars(&driver), Some(3));
    handle.emit_engine_event(EngineEvent::SignalBars {
        call_id: 78,
        bars: 1,
    });
    driver.request_tick(std::time::Instant::now());
    assert_eq!(bars(&driver), Some(3));
    std::fs::remove_dir_all(dir).ok();
}
