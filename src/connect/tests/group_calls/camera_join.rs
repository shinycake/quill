//! A camera that cannot start at join does not fail the voice chat join.
use super::*;

const UNRELATED: &str =
    r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateReady"}}"#;

#[test]
fn a_busy_camera_at_join_still_connects_with_audio_and_reports_the_camera() {
    let (dir, mut driver, _recorder, handle, sink, seq) = ready_group_call_driver();
    handle.set_devices(vec![MediaDevice {
        id: "cam-1".into(),
        name: "cam-1".into(),
        kind: MediaDeviceKind::Camera,
    }]);
    handle.set_camera_refuses(true);
    driver
        .session
        .calls
        .group_call_mut(555)
        .expect("tracked")
        .is_my_video_enabled = true;
    let extra = driver.join_video_chat(555).expect("join tracked call");
    let answer = format!(
        r#"{{"@type":"text","text":"join-answer","@extra":{}}}"#,
        extra.0
    );
    ingest_call_json(&mut driver, &seq, &sink, &answer);

    // The call is up (the microphone follows `transport_ready`) ...
    let call = driver.session.calls.group_call(555).expect("tracked");
    assert!(call.transport_ready);
    // ... and the camera's failure is what the window shows.
    assert_eq!(
        call.transport_error.as_deref(),
        Some(
            crate::calls::engine::EngineError::NoCamera
                .to_string()
                .as_str()
        )
    );
    assert_eq!(handle.group_connects().len(), 1);

    // The camera step retries it once it can open.
    handle.set_camera_refuses(false);
    ingest_call_json(&mut driver, &seq, &sink, UNRELATED);
    assert_eq!(
        handle.group_camera_changes(),
        vec![(555, Some("cam-1".to_string()))]
    );
    let call = driver.session.calls.group_call(555).expect("tracked");
    assert!(call.transport_ready);
    assert_eq!(call.transport_error, None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_join_offers_rejoin_but_a_failed_camera_does_not() {
    let mut call = crate::state::ActiveGroupCall::fresh(555);
    assert!(!call.offers_rejoin());
    // The join failed: nothing connected, an error stands.
    call.transport_error = Some("join failed".into());
    assert!(call.offers_rejoin());
    // Connected with audio and a camera error: no Rejoin.
    call.transport_ready = true;
    assert!(!call.offers_rejoin());
}
