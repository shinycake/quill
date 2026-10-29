//! Driver tests: everyone's speaking levels in a joined group call.
use super::*;
use crate::calls::audio_level::SOUND_STATUS_KEPT_MS;
use crate::calls::engine::{EngineEvent, GroupAudioLevel};
use crate::telegram::envelope::{ParsedGroupCallParticipant, parse_group_call_participant};
use std::time::Duration;

fn participant(user_id: i64, me: bool, audio_source_id: i32) -> ParsedGroupCallParticipant {
    let json: Value =
        serde_json::from_str(&group_participant_json(user_id, me, "null", "null")).unwrap();
    let mut p = parse_group_call_participant(json.get("participant")).unwrap();
    p.audio_source_id = audio_source_id;
    p
}

/// Joined call 77: you (ssrc 4242, muted) and user 9 (ssrc 99).
fn joined(driver: &mut ConnectDriver<Arc<RecordingSender>>) {
    let mut call = tracked_group_call(false, false, false);
    call.participants = vec![participant(777, true, 4242), participant(9, false, 99)];
    driver.session.calls.active_group_call = Some(call);
}

fn levels(levels: &[(u32, f32, bool)]) -> EngineEvent {
    EngineEvent::GroupAudioLevels {
        group_call_id: 77,
        levels: levels
            .iter()
            .map(|&(ssrc, level, voice)| GroupAudioLevel { ssrc, level, voice })
            .collect(),
    }
}

fn speaking_sent(recorder: &RecordingSender) -> Vec<(i64, bool)> {
    recorder
        .snapshot()
        .iter()
        .map(|s| serde_json::from_str::<Value>(s).unwrap())
        .filter(|v| v["@type"] == "setGroupCallParticipantIsSpeaking")
        .map(|v| {
            (
                v["audio_source"].as_i64().unwrap(),
                v["is_speaking"].as_bool().unwrap(),
            )
        })
        .collect()
}

#[test]
fn levels_drive_rows_and_tell_tdlib_who_speaks() {
    let (dir, mut driver, recorder, handle, _sink, _seq) = ready_group_call_driver();
    joined(&mut driver);
    let t0 = Instant::now();
    handle.emit_engine_event(levels(&[(99, 0.7, true), (0, 0.9, true)]));
    assert!(driver.request_tick(t0), "a row started speaking: redraw");
    let other = driver.group_call_speaker(99, t0);
    assert!(other.sounding && other.speaking);
    assert!((other.level - 0.7).abs() < 1e-6);
    // You are muted: your own level is ignored, as in tdesktop.
    assert!(!driver.group_call_speaker(4242, t0).sounding);
    assert_eq!(speaking_sent(&recorder), vec![(99, true)]);

    // A louder batch only moves the level: the window's clock draws it.
    handle.emit_engine_event(levels(&[(99, 0.9, true)]));
    let t1 = t0 + Duration::from_millis(100);
    assert!(!driver.request_tick(t1));
    assert!((driver.group_call_speaker(99, t1).level - 0.9).abs() < 1e-6);
    assert!(driver.group_call_levels_sounding(t1));

    // The kept-for window runs out: TDLib hears it and the row redraws.
    let quiet = t1 + Duration::from_millis(SOUND_STATUS_KEPT_MS + 1);
    let deadline = driver.next_deadline(t1, 0).unwrap();
    assert!(deadline <= Duration::from_millis(SOUND_STATUS_KEPT_MS + 1));
    assert!(driver.request_tick(quiet));
    assert!(!driver.group_call_speaker(99, quiet).sounding);
    assert_eq!(speaking_sent(&recorder), vec![(99, true), (99, false)]);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn your_own_level_shows_while_you_are_live() {
    let (dir, mut driver, recorder, handle, _sink, _seq) = ready_group_call_driver();
    joined(&mut driver);
    driver
        .session
        .calls
        .active_group_call
        .as_mut()
        .unwrap()
        .is_muted_self = false;
    let t0 = Instant::now();
    handle.emit_engine_event(levels(&[(0, 0.8, true)]));
    assert!(driver.request_tick(t0));
    assert!(driver.group_call_speaker(4242, t0).speaking);
    assert!(
        speaking_sent(&recorder).is_empty(),
        "your own speaking goes through call_audio"
    );
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn quiet_batches_and_other_calls_change_nothing() {
    let (dir, mut driver, recorder, handle, _sink, _seq) = ready_group_call_driver();
    joined(&mut driver);
    let t0 = Instant::now();
    handle.emit_engine_event(levels(&[(99, 0.05, false), (0, 0.0, false)]));
    assert!(!driver.request_tick(t0), "a silent batch never redraws");
    handle.emit_engine_event(EngineEvent::GroupAudioLevels {
        group_call_id: 78,
        levels: vec![GroupAudioLevel {
            ssrc: 99,
            level: 0.9,
            voice: true,
        }],
    });
    assert!(
        !driver.request_tick(t0),
        "another call's levels are dropped"
    );
    assert!(!driver.group_call_speaker(99, t0).sounding);
    assert!(speaking_sent(&recorder).is_empty());
    std::fs::remove_dir_all(dir).ok();
}
