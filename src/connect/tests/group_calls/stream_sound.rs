use super::*;
use crate::calls::engine::{GroupCallMode, StreamState};
use crate::calls::stream_audio::watch::{NO_SOUND_LIMIT, PlaybackHealth};
use std::time::{Duration, Instant};

const SILENT: PlaybackHealth = PlaybackHealth {
    frames: 0,
    peak: 0,
    output_failed: false,
};

fn joined_stream() -> GroupCallDriverHarness {
    let (dir, mut driver, recorder, handle, sink, seq) = ready_group_call_driver();
    handle.set_group_call_mode(51, GroupCallMode::Rtmp);
    let extra = driver.join_video_chat(555).expect("join tracked call");
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"text","text":"{{\"rtmp\":true}}","@extra":{}}}"#,
            extra.0
        ),
    );
    (dir, driver, recorder, handle, sink, seq)
}

fn set_stream_state(driver: &mut ConnectDriver<Arc<RecordingSender>>, state: StreamState) {
    if let Some(call) = driver.session.calls.active_group_call.as_mut() {
        call.stream = state;
    }
}

#[test]
fn the_volume_reaches_the_engine_once_per_change() {
    let (dir, mut driver, _recorder, handle, ..) = joined_stream();
    let before = handle.stream_volumes().len();
    driver.set_stream_volume(0.4);
    driver.set_stream_volume(0.4);
    driver.set_stream_volume(7.0);
    assert_eq!(handle.stream_volumes().get(before..), Some(&[0.4, 1.0][..]));
    assert_eq!(
        driver.stream_volume(),
        1.0,
        "the stream range ends at 100 %"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn only_a_stream_quill_plays_has_a_volume() {
    let (dir, driver, _recorder, handle, ..) = joined_stream();
    assert!(!driver.stream_has_volume(555), "device playback: mute only");
    handle.set_stream_sound(555, SILENT);
    assert!(driver.stream_has_volume(555));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_live_stream_without_sound_falls_back_after_five_seconds_once() {
    let (dir, mut driver, _recorder, handle, ..) = joined_stream();
    handle.set_stream_sound(555, SILENT);
    let start = Instant::now();
    driver.request_tick(start);
    driver.request_tick(start + Duration::from_secs(30));
    assert!(handle.device_playbacks().is_empty());
    set_stream_state(&mut driver, StreamState::Live);
    let live = start + Duration::from_secs(31);
    driver.request_tick(live);
    driver.request_tick(live + Duration::from_millis(4_999));
    assert!(handle.device_playbacks().is_empty());
    driver.request_tick(live + NO_SOUND_LIMIT);
    assert_eq!(handle.device_playbacks(), vec![555]);
    assert!(!driver.stream_has_volume(555), "the slider goes with it");
    handle.set_stream_sound(555, SILENT);
    driver.request_tick(live + NO_SOUND_LIMIT * 4);
    assert_eq!(handle.device_playbacks(), vec![555], "once");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_stream_with_sound_stays_on_quills_output() {
    let (dir, mut driver, _recorder, handle, ..) = joined_stream();
    handle.set_stream_sound(
        555,
        PlaybackHealth {
            frames: 12,
            peak: 0,
            output_failed: false,
        },
    );
    set_stream_state(&mut driver, StreamState::Live);
    let start = Instant::now();
    driver.request_tick(start);
    driver.request_tick(start + Duration::from_secs(60));
    assert!(handle.device_playbacks().is_empty());
    assert!(driver.stream_has_volume(555));
    let _ = std::fs::remove_dir_all(&dir);
}
