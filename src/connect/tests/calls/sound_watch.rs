//! Driver tests: the 1:1 call's sound watch (`connect::calls::sound_watch`).
use super::*;
use crate::calls::stream_audio::watch::{NO_SOUND_LIMIT, PlaybackHealth};
use std::time::{Duration, Instant};

fn connected_call() -> (
    std::path::PathBuf,
    ConnectDriver<Arc<RecordingSender>>,
    MockEngine,
) {
    let (dir, mut driver, handle, sink, seq) = ready_call_driver();
    ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
    handle.emit_transport_state(77, TransportState::Connected);
    (dir, driver, handle)
}

#[test]
fn a_connected_call_without_sound_goes_back_to_the_device_once() {
    let (dir, mut driver, handle) = connected_call();
    handle.set_call_sound(77, PlaybackHealth::default());
    let start = Instant::now();
    driver.request_tick(start);
    driver.request_tick(start + Duration::from_millis(4_999));
    assert!(handle.device_call_sounds().is_empty());
    driver.request_tick(start + NO_SOUND_LIMIT);
    assert_eq!(handle.device_call_sounds(), vec![77]);
    handle.set_call_sound(77, PlaybackHealth::default());
    driver.request_tick(start + NO_SOUND_LIMIT * 3);
    assert_eq!(handle.device_call_sounds(), vec![77], "once");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_call_with_sound_stays_on_quills_output() {
    let (dir, mut driver, handle) = connected_call();
    handle.set_call_sound(
        77,
        PlaybackHealth {
            frames: 40,
            peak: 9_000,
            output_failed: false,
        },
    );
    let start = Instant::now();
    driver.request_tick(start);
    driver.request_tick(start + Duration::from_secs(60));
    assert!(handle.device_call_sounds().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_clock_waits_for_the_call_to_connect() {
    let (dir, mut driver, handle, sink, seq) = ready_call_driver();
    ingest_call_json(&mut driver, &seq, &sink, READY_CALL_JSON);
    handle.set_call_sound(77, PlaybackHealth::default());
    let start = Instant::now();
    driver.request_tick(start);
    driver.request_tick(start + Duration::from_secs(30));
    assert!(handle.device_call_sounds().is_empty(), "still connecting");
    handle.emit_transport_state(77, TransportState::Connected);
    let connected = start + Duration::from_secs(31);
    driver.request_tick(connected);
    assert!(handle.device_call_sounds().is_empty());
    driver.request_tick(connected + NO_SOUND_LIMIT);
    assert_eq!(handle.device_call_sounds(), vec![77]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_stopped_output_goes_back_to_the_device_at_once() {
    let (dir, mut driver, handle) = connected_call();
    handle.set_call_sound(
        77,
        PlaybackHealth {
            frames: 500,
            peak: 9_000,
            output_failed: true,
        },
    );
    driver.request_tick(Instant::now());
    assert_eq!(handle.device_call_sounds(), vec![77]);
    let _ = std::fs::remove_dir_all(&dir);
}
