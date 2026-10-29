//! Group calls against the real library, when it's built: each call is
//! made, joined, fed and left without waiting on the network or a clock
//! (a join answer with no candidates connects to nothing).
use super::*;
use serde_json::Value;

fn engine() -> Option<TgcallsEngine> {
    match TgcallsEngine::load() {
        Ok(engine) => Some(engine),
        Err(_) => {
            eprintln!("SKIP: libquill_calls not built; run scripts/build-tgcalls.sh");
            None
        }
    }
}

/// An RTC join answer, as `joinVideoChat` returns it, with nothing to
/// connect to.
const RTC_ANSWER: &str = r#"{"transport":{"ufrag":"abcd","pwd":"0123456789abcdef0123456789","fingerprints":[{"hash":"sha-256","setup":"active","fingerprint":"00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF"}],"candidates":[]}}"#;

fn source(user_id: i64, endpoint: &str, screen: bool) -> GroupVideoSource {
    GroupVideoSource {
        user_id,
        endpoint: endpoint.to_owned(),
        ssrc_groups: vec![GroupVideoSourceGroup {
            semantics: "SIM".to_owned(),
            ssrcs: vec![10, 11, 12],
        }],
        screen,
        audio_ssrc: 0,
    }
}

fn microphone(sender: &Arc<dyn ExternalFrameSender>) -> Result<(), EngineError> {
    let frame = vec![0u8; EXTERNAL_AUDIO_FRAME_BYTES];
    sender.send(
        ExternalDevice::Microphone,
        &frame,
        ExternalFrameMeta::default(),
    )
}

fn screen(sender: &Arc<dyn ExternalFrameSender>) -> Result<(), EngineError> {
    let meta = ExternalFrameMeta {
        timestamp_ms: 0,
        width: 64,
        height: 48,
    };
    sender.send(ExternalDevice::Screen, &[128u8; 64 * 48 * 3 / 2], meta)
}

fn outputs(engine: &TgcallsEngine, group_call_id: i32) -> Vec<String> {
    let mut outputs: Vec<String> = engine.groups[&group_call_id]
        .outputs
        .iter()
        .cloned()
        .collect();
    outputs.sort();
    outputs
}

#[test]
fn the_join_payload_is_tdesktops_and_names_the_audio_ssrc() {
    let Some(mut engine) = engine() else { return };
    let payload = engine
        .create_group_call(1, -100, GroupCallRole::Member)
        .unwrap();
    let json: Value = serde_json::from_str(&payload).unwrap();
    assert!(!json["ufrag"].as_str().unwrap().is_empty());
    assert!(json["fingerprints"].is_array());
    let ssrc = group_offer_audio_source_id(&payload);
    assert_ne!(ssrc, 0);
    assert_eq!(i64::from(ssrc), json["ssrc"].as_i64().unwrap());
    // Nothing is sent before the join answer.
    assert!(
        engine
            .external_frame_sender(CaptureTarget::Group(1))
            .is_none()
    );
    assert_eq!(engine.group_call_mode(-100), None);
    engine.leave_group_call(1).unwrap();
}

#[test]
fn a_group_call_is_joined_fed_and_left() {
    let Some(mut engine) = engine() else { return };
    engine
        .create_group_call(2, -200, GroupCallRole::Member)
        .unwrap();
    engine.set_group_playback(2, None, false).unwrap();
    engine.set_group_muted(2, false).unwrap();
    engine.connect_group_call(2, RTC_ANSWER, None).unwrap();
    assert_eq!(engine.group_call_mode(-200), Some(GroupCallMode::Rtc));
    // Connecting is done once.
    engine.connect_group_call(2, "{}", None).unwrap();
    engine.set_group_camera(2, None).unwrap();
    engine.set_group_muted(2, true).unwrap();
    engine.set_group_playback(2, None, true).unwrap();

    let sender = engine
        .external_frame_sender(CaptureTarget::Group(2))
        .unwrap();
    microphone(&sender).unwrap();
    // No presentation, no screen.
    assert_eq!(screen(&sender), Err(EngineError::Unavailable));

    engine.leave_group_call(2).unwrap();
    assert_eq!(microphone(&sender), Err(EngineError::Unavailable));
    assert!(engine.leave_group_call(2).is_ok());
    assert_eq!(
        engine.set_group_muted(2, false),
        Err(EngineError::NoSuchCall(2))
    );
}

#[test]
fn incoming_videos_follow_the_participants() {
    let Some(mut engine) = engine() else { return };
    engine
        .create_group_call(3, -300, GroupCallRole::Member)
        .unwrap();
    engine.connect_group_call(3, RTC_ANSWER, None).unwrap();
    let both = [
        source(42, "camera-42", false),
        source(42, "screen-42", true),
    ];
    engine.sync_group_video(3, &both).unwrap();
    assert_eq!(outputs(&engine, 3), vec!["camera-42", "screen-42"]);
    let owner = engine.groups[&3].context.endpoints.lock().unwrap()["screen-42"];
    assert_eq!(
        owner,
        LiveKey::Video {
            call_id: 3,
            user_id: 42,
            screen: true
        }
    );
    // The same set again changes nothing.
    let issued = engine.groups[&3].requested_video.clone();
    engine.sync_group_video(3, &both).unwrap();
    assert_eq!(engine.groups[&3].requested_video, issued);

    engine
        .sync_group_video(3, &[source(43, "camera-43", false)])
        .unwrap();
    assert_eq!(outputs(&engine, 3), vec!["camera-43"]);
    let routes = engine.groups[&3].context.endpoints.lock().unwrap().clone();
    assert_eq!(routes.len(), 1);
    assert!(routes.contains_key("camera-43"));

    engine.sync_group_video(3, &[]).unwrap();
    assert!(outputs(&engine, 3).is_empty());
    engine.leave_group_call(3).unwrap();
}

#[test]
fn the_screen_is_shared_through_the_presentation() {
    let Some(mut engine) = engine() else { return };
    engine
        .create_group_call(4, -400, GroupCallRole::Member)
        .unwrap();
    engine.connect_group_call(4, RTC_ANSWER, None).unwrap();
    let sender = engine
        .external_frame_sender(CaptureTarget::Group(4))
        .unwrap();
    assert!(
        engine.connect_screen_share(4, RTC_ANSWER).is_err(),
        "no presentation to connect"
    );

    let payload = engine.start_screen_share(4).unwrap();
    let json: Value = serde_json::from_str(&payload).unwrap();
    assert_ne!(json["ssrc"].as_i64().unwrap(), 0);
    assert!(engine.presentation_active(4));
    // Pictures go to the presentation only once it is connected.
    assert_eq!(screen(&sender), Err(EngineError::Unavailable));
    engine.connect_screen_share(4, RTC_ANSWER).unwrap();
    engine.connect_screen_share(4, RTC_ANSWER).unwrap();
    screen(&sender).unwrap();

    engine.stop_screen_share(4).unwrap();
    assert!(!engine.presentation_active(4));
    assert_eq!(screen(&sender), Err(EngineError::Unavailable));
    microphone(&sender).unwrap();

    // Leaving stops a running presentation with the call.
    engine.start_screen_share(4).unwrap();
    engine.connect_screen_share(4, RTC_ANSWER).unwrap();
    engine.leave_group_call(4).unwrap();
    assert!(!engine.presentation_active(4));
    assert_eq!(screen(&sender), Err(EngineError::Unavailable));
}

#[test]
fn a_stream_is_received_only() {
    let Some(mut engine) = engine() else { return };
    engine
        .create_group_call(5, -500, GroupCallRole::Viewer)
        .unwrap();
    engine
        .connect_group_call(5, r#"{"stream":true,"rtmp":true}"#, Some("camera"))
        .unwrap();
    assert_eq!(engine.group_call_mode(-500), Some(GroupCallMode::Rtmp));
    assert!(
        engine
            .external_frame_sender(CaptureTarget::Group(5))
            .is_none()
    );
    // The camera is kept, not opened.
    assert!(engine.groups[&5].camera.is_none());
    // The connect asks for the unified video and routes it to the stream.
    assert_eq!(outputs(&engine, 5), vec!["unified"]);
    let route = engine.groups[&5].context.endpoints.lock().unwrap()["unified"];
    assert_eq!(route, LiveKey::Stream { chat_id: -500 });
    let min_quality = |engine: &TgcallsEngine| {
        let requested = engine.groups[&5].requested_video.clone().unwrap();
        let json: Value = serde_json::from_str(&requested).unwrap();
        json[0]["min_quality"].as_i64().unwrap()
    };
    assert_eq!(
        min_quality(&engine),
        i64::from(quill_calls_sys::QC_GROUP_QUALITY_THUMBNAIL)
    );
    // A Full tile asks for Full at once, without a driver sync.
    engine.set_stream_quality(-500, StreamQuality::Full);
    assert_eq!(engine.stream_quality(-500), Some(StreamQuality::Full));
    assert_eq!(
        min_quality(&engine),
        i64::from(quill_calls_sys::QC_GROUP_QUALITY_FULL)
    );
    // The driver's own sync keeps it.
    engine.sync_group_video(5, &[]).unwrap();
    assert_eq!(outputs(&engine, 5), vec!["unified"]);
    engine.set_stream_volume(0.5);
    // Dropped with the engine, without a leave.
}

#[test]
fn an_mtproto_stream_shows_one_video_on_the_stream() {
    let Some(mut engine) = engine() else { return };
    engine
        .create_group_call(7, -700, GroupCallRole::Viewer)
        .unwrap();
    // Asked for before the connect: routed again once it says stream.
    engine
        .sync_group_video(7, &[source(42, "camera-42", false)])
        .unwrap();
    engine
        .connect_group_call(7, r#"{"stream":true}"#, None)
        .unwrap();
    assert_eq!(engine.group_call_mode(-700), Some(GroupCallMode::Stream));
    assert_eq!(outputs(&engine, 7), vec!["camera-42"]);
    let route = engine.groups[&7].context.endpoints.lock().unwrap()["camera-42"];
    assert_eq!(route, LiveKey::Stream { chat_id: -700 });
    // Two cameras and a screen: only the screen is asked for.
    engine
        .sync_group_video(
            7,
            &[
                source(42, "camera-42", false),
                source(43, "camera-43", false),
                source(42, "screen-42", true),
            ],
        )
        .unwrap();
    assert_eq!(outputs(&engine, 7), vec!["screen-42"]);
    engine.leave_group_call(7).unwrap();
}

#[test]
fn a_rejoin_replaces_the_instance_and_its_presentation() {
    let Some(mut engine) = engine() else { return };
    let first = engine
        .create_group_call(6, -600, GroupCallRole::Member)
        .unwrap();
    engine.start_screen_share(6).unwrap();
    let second = engine
        .create_group_call(6, -600, GroupCallRole::Member)
        .unwrap();
    assert_ne!(first, second);
    assert!(!engine.presentation_active(6));
    assert!(!engine.groups[&6].connected);
    engine.leave_group_call(6).unwrap();
}

#[test]
fn unknown_group_calls_are_refused_and_leave_succeeds() {
    let Some(mut engine) = engine() else { return };
    let missing = Err(EngineError::NoSuchCall(9));
    assert_eq!(engine.connect_group_call(9, RTC_ANSWER, None), missing);
    assert_eq!(engine.sync_group_video(9, &[]), missing);
    assert_eq!(engine.set_group_camera(9, None), missing);
    assert_eq!(engine.set_group_playback(9, None, false), missing);
    assert_eq!(engine.stop_screen_share(9), missing);
    assert_eq!(
        engine.start_screen_share(9),
        Err(EngineError::NoSuchCall(9))
    );
    assert!(!engine.presentation_active(9));
    assert!(engine.leave_group_call(9).is_ok());
}

/// Drops every frame: nothing here encrypts for real.
struct Refusing;

impl FrameCrypto for Refusing {
    fn encrypt(&self, _frame: &[u8], _unencrypted_prefix: i32) -> Option<Vec<u8>> {
        None
    }

    fn decrypt(&self, _sender_user_id: i64, _frame: &[u8]) -> Option<Vec<u8>> {
        None
    }
}

#[test]
fn a_conference_call_is_made_renamed_joined_and_left() {
    let Some(mut engine) = engine() else { return };
    let payload = engine
        .create_conference_call(-1, Arc::new(Refusing))
        .unwrap();
    assert_ne!(group_offer_audio_source_id(&payload), 0);
    // The join answer names the call.
    assert_eq!(
        engine.rekey_group_call(-2, 8),
        Err(EngineError::NoSuchCall(-2))
    );
    engine.rekey_group_call(-1, 8).unwrap();
    assert!(!engine.groups.contains_key(&-1));
    assert_eq!(engine.groups[&8].context.group_call_id(), 8);
    let owners = SsrcOwners {
        owners: vec![(100, 42)],
        complete: true,
    };
    engine.set_group_ssrc_owners(8, &owners);
    engine.connect_group_call(8, RTC_ANSWER, None).unwrap();
    let sender = engine
        .external_frame_sender(CaptureTarget::Group(8))
        .unwrap();
    microphone(&sender).unwrap();
    // The presentation is encrypted too.
    engine.start_screen_share(8).unwrap();
    assert!(
        engine.groups[&8]
            .presentation
            .as_ref()
            .is_some_and(|presentation| presentation.context.conference.is_some())
    );
    engine.leave_group_call(8).unwrap();
    assert_eq!(microphone(&sender), Err(EngineError::Unavailable));
}
