use super::*;

/// Phase C2g: the audio SSRC is the first `a=ssrc:` line of the
/// audio `m=` section; video sections are ignored, and 0 is the
/// honest fallback (the join still goes out).
#[test]
fn group_offer_audio_source_id_parses_first_audio_ssrc() {
    let offer = "v=0\r\n\
            m=audio 9 UDP/TLS/RTP/SAVPF 111\r\n\
            a=ssrc:12345 cname:audio\r\n\
            a=ssrc:12346 cname:audio\r\n\
            m=video 9 UDP/TLS/RTP/SAVPF 96\r\n\
            a=ssrc:99999 cname:video\r\n";
    assert_eq!(group_offer_audio_source_id(offer), 12345);
}

/// tgcalls' payload is tdesktop's `GroupJoinPayload` JSON: its `ssrc`
/// is the audio ssrc, sent as a signed 32-bit number.
#[test]
fn group_offer_audio_source_id_reads_the_json_payload() {
    let payload = r#"{"ufrag":"u","pwd":"p","fingerprints":[],"ssrc":-1234567,"ssrc-groups":[]}"#;
    assert_eq!(group_offer_audio_source_id(payload), -1234567);
    assert_eq!(group_offer_audio_source_id(r#"{"ssrc":42}"#), 42);
    assert_eq!(group_offer_audio_source_id(r#"{"ufrag":"u"}"#), 0);
}

#[test]
fn group_offer_audio_source_id_zero_without_audio_ssrc() {
    assert_eq!(
        group_offer_audio_source_id("v=0\r\nm=video 9 UDP/TLS/RTP/SAVPF 96\r\n"),
        0
    );
    assert_eq!(group_offer_audio_source_id(""), 0);
}

#[test]
fn mock_engine_lifecycle_and_protocol() {
    let mut engine = MockEngine::new();
    let handle = engine.clone();
    let emitted = Arc::new(Mutex::new(Vec::new()));
    let emitted_for_hook = emitted.clone();
    engine.set_signaling_emitted_callback(Arc::new(move |call_id, data| {
        emitted_for_hook
            .lock()
            .expect("emitted signaling")
            .push((call_id, data));
    }));
    let devices = vec![MediaDevice {
        id: "mic-1".into(),
        name: "Test microphone".into(),
        kind: MediaDeviceKind::Microphone,
    }];
    engine.set_devices(devices.clone());

    engine.start_call(77, 41, false).unwrap();
    engine.accept_call(77).unwrap();
    let params = ConnectParams {
        custom_parameters: String::new(),
        encryption_key: vec![1; 256],
        is_outgoing: false,
        servers: vec![RtcServer {
            id: 7,
            ipv4: "149.154.167.40".into(),
            ipv6: String::new(),
            port: 443,
            username: String::new(),
            password: String::new(),
            turn: true,
            stun: false,
            tcp: true,
            peer_tag: vec![0, 1, 2],
        }],
        library_versions: vec!["13.0.0".into()],
        p2p_allowed: true,
        mic_input: Some("mic-1".into()),
        speaker_input: Some("speaker-1".into()),
        camera_input: None,
    };
    engine.connect(77, &params).unwrap();
    engine
        .select_devices(77, Some("mic-1"), Some("speaker-1"))
        .unwrap();
    engine.send_signaling_data(77, b"inbound").unwrap();
    handle.receive_signaling_data(77, b"outbound");
    engine.set_muted(77, true).unwrap();
    assert_eq!(engine.media_devices().unwrap(), devices);
    assert_eq!(engine.protocol(), MockEngine::engine_protocol());
    assert!(engine.is_available());
    engine.hangup(77).unwrap();

    assert_eq!(handle.started_calls(), vec![(77, 41, false)]);
    assert_eq!(handle.accepted_calls(), vec![77]);
    assert_eq!(handle.connects(), vec![(77, params)]);
    assert_eq!(
        handle.device_selections(),
        vec![(77, Some("mic-1".to_string()), Some("speaker-1".to_string()))]
    );
    assert_eq!(handle.signaling_received(), vec![(77, b"inbound".to_vec())]);
    assert_eq!(handle.mute_changes(), vec![(77, true)]);
    assert_eq!(handle.hung_up_calls(), vec![77]);
    assert_eq!(
        *emitted.lock().expect("emitted signaling"),
        vec![(77, b"outbound".to_vec())]
    );
}

#[test]
fn mock_engine_unknown_calls_and_idempotent_hangup() {
    let mut engine = MockEngine::new();
    assert_eq!(engine.accept_call(77), Err(EngineError::NoSuchCall(77)));
    assert_eq!(
        engine.send_signaling_data(77, b"data"),
        Err(EngineError::NoSuchCall(77))
    );
    assert_eq!(engine.set_muted(77, true), Err(EngineError::NoSuchCall(77)));
    assert_eq!(engine.hangup(77), Ok(()));
}

#[test]
fn unavailable_mock_rejects_operations() {
    let mut engine = MockEngine::unavailable();
    assert!(!engine.is_available());
    assert_eq!(engine.protocol(), EngineProtocol::signaling_only());
    assert_eq!(
        engine.start_call(77, 41, false),
        Err(EngineError::Unavailable)
    );
    assert_eq!(engine.accept_call(77), Err(EngineError::Unavailable));
    assert_eq!(
        engine.send_signaling_data(77, b"data"),
        Err(EngineError::Unavailable)
    );
    assert_eq!(engine.hangup(77), Err(EngineError::Unavailable));
    assert_eq!(engine.set_muted(77, true), Err(EngineError::Unavailable));
    assert_eq!(engine.media_devices(), Err(EngineError::Unavailable));
}

#[test]
fn mock_engine_replaces_signaling_hook() {
    let mut engine = MockEngine::new();
    let first = Arc::new(Mutex::new(Vec::new()));
    let first_hook = first.clone();
    engine.set_signaling_emitted_callback(Arc::new(move |_, data| {
        first_hook.lock().expect("first hook").push(data);
    }));
    let second = Arc::new(Mutex::new(Vec::new()));
    let second_hook = second.clone();
    engine.set_signaling_emitted_callback(Arc::new(move |_, data| {
        second_hook.lock().expect("second hook").push(data);
    }));

    engine.receive_signaling_data(77, b"replacement");
    assert!(first.lock().expect("first hook").is_empty());
    assert_eq!(
        *second.lock().expect("second hook"),
        vec![b"replacement".to_vec()]
    );
}

#[test]
fn mock_engine_delivers_transport_state_and_replaces_hook() {
    let mut engine = MockEngine::new();
    let first = Arc::new(Mutex::new(Vec::new()));
    let first_hook = first.clone();
    engine.set_transport_state_callback(Arc::new(move |call_id, state| {
        first_hook
            .lock()
            .expect("first transport hook")
            .push((call_id, state));
    }));
    let second = Arc::new(Mutex::new(Vec::new()));
    let second_hook = second.clone();
    engine.set_transport_state_callback(Arc::new(move |call_id, state| {
        second_hook
            .lock()
            .expect("second transport hook")
            .push((call_id, state));
    }));

    engine.emit_transport_state(77, TransportState::Connected);
    assert!(first.lock().expect("first transport hook").is_empty());
    assert_eq!(
        *second.lock().expect("second transport hook"),
        vec![(77, TransportState::Connected)]
    );
}

#[test]
fn mock_engine_connect_and_device_selection_need_known_call() {
    let mut engine = MockEngine::new();
    let params = ConnectParams {
        custom_parameters: String::new(),
        encryption_key: vec![1; 256],
        is_outgoing: true,
        servers: Vec::new(),
        library_versions: Vec::new(),
        p2p_allowed: false,
        mic_input: None,
        speaker_input: None,
        camera_input: None,
    };
    assert_eq!(
        engine.connect(77, &params),
        Err(EngineError::NoSuchCall(77))
    );
    assert_eq!(
        engine.select_devices(77, Some("mic-1"), None),
        Err(EngineError::NoSuchCall(77))
    );
    assert!(engine.connects().is_empty());
    assert!(engine.device_selections().is_empty());
}

/// Phase C2i: validates the 1:1 screen-share toggle contract the
/// UI will drive (mirrors `mock_camera_toggle_state_machine`).
#[test]
fn mock_p2p_screen_share_toggle_state_machine() {
    let mut engine = MockEngine::new();
    engine.start_call(77, 41, false).unwrap();
    engine.set_screen_share_enabled(77, true).unwrap();
    engine.set_screen_share_enabled(77, false).unwrap();
    assert_eq!(
        engine.p2p_screen_share_changes(),
        vec![(77, true), (77, false)]
    );
    assert_eq!(
        engine.set_screen_share_enabled(99, true),
        Err(EngineError::NoSuchCall(99))
    );

    let mut unavailable = MockEngine::unavailable();
    assert_eq!(
        unavailable.set_screen_share_enabled(77, true),
        Err(EngineError::Unavailable)
    );
}

/// Phase C2i: a mid-call screen-share intent survives a transport
/// reconnect — `connect` re-runs with the retained params and the
/// engine must re-issue screen-only (not flip back to the camera).
#[test]
fn retained_call_media_preserves_screen_share_across_reconnect() {
    let params = ConnectParams {
        custom_parameters: String::new(),
        encryption_key: vec![1; 256],
        is_outgoing: false,
        servers: Vec::new(),
        library_versions: vec!["13.0.0".into()],
        p2p_allowed: true,
        mic_input: Some("mic-1".into()),
        speaker_input: Some("speaker-1".into()),
        camera_input: Some("cam-1".into()),
    };
    // Fresh connect: no prior entry, share starts off.
    let fresh = retained_call_media(&params, None);
    assert!(!fresh.screen_share_on);
    assert_eq!(fresh.mic.as_deref(), Some("mic-1"));
    // Mid-call the user starts sharing, then the transport drops
    // and `connect` re-runs with the same retained params: the
    // share intent must survive so `set_media_sources` re-issues
    // screen-only (`camera.is_some() && !screen_share_on` stays
    // false for the camera).
    let mut sharing = fresh;
    sharing.screen_share_on = true;
    sharing.camera = None;
    let reconnected = retained_call_media(&params, Some(&sharing));
    assert!(reconnected.screen_share_on);
    // Stopping share before the drop must not stick the flag on
    // either.
    let mut stopped = sharing;
    stopped.screen_share_on = false;
    assert!(!retained_call_media(&params, Some(&stopped)).screen_share_on);
}

/// Phase C2e: validates the toggle contract the UI will drive.
#[test]
fn mock_camera_toggle_state_machine() {
    let mut engine = MockEngine::new();
    engine.start_call(77, 41, false).unwrap();
    engine.set_camera(77, Some("cam-1")).unwrap();
    engine.set_camera(77, None).unwrap();
    assert_eq!(
        engine.camera_changes(),
        vec![(77, Some("cam-1".to_string())), (77, None)]
    );
    assert_eq!(
        engine.set_camera(99, Some("cam-1")),
        Err(EngineError::NoSuchCall(99))
    );

    let mut unavailable = MockEngine::unavailable();
    assert_eq!(
        unavailable.set_camera(77, Some("cam-1")),
        Err(EngineError::Unavailable)
    );
}

/// Phase C2e: validates frame delivery and hook replacement.
#[test]
fn mock_frame_callback_plumbing() {
    use crate::calls::live_video::{LiveFrame, LiveKey, LiveRotation};
    let mut engine = MockEngine::new();
    let first = Arc::new(Mutex::new(Vec::new()));
    let first_hook = first.clone();
    engine.set_live_frame_callback(Arc::new(move |key, frame| {
        first_hook
            .lock()
            .expect("first frame hook")
            .push((key, frame));
    }));
    let frame = || {
        // seq 999 is overwritten by the mock's internal counter.
        LiveFrame::new(
            999,
            2,
            2,
            LiveRotation::None,
            vec![0; 6],
            std::time::Instant::now(),
        )
        .expect("a 2x2 frame")
    };
    let local = LiveKey::Call {
        call_id: 77,
        local: true,
        screen: false,
    };
    engine.emit_live_frame(local, frame());
    let second = Arc::new(Mutex::new(Vec::new()));
    let second_hook = second.clone();
    engine.set_live_frame_callback(Arc::new(move |key, frame| {
        second_hook
            .lock()
            .expect("second frame hook")
            .push((key, frame));
    }));
    engine.emit_live_frame(local, frame());

    let first = first.lock().expect("first frame hook");
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].0, local);
    assert_eq!(first[0].1.seq, 0);
    let second = second.lock().expect("second frame hook");
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].1.seq, 1);
    assert_eq!(second[0].1.width, 2);
}

/// Phase C2j: validates peer screen-share state delivery ordering.
#[test]
fn mock_remote_screen_state_emission() {
    let mut engine = MockEngine::new();
    let states = Arc::new(Mutex::new(Vec::new()));
    let states_hook = states.clone();
    engine.set_remote_screen_state_callback(Arc::new(move |call_id, state| {
        states_hook
            .lock()
            .expect("remote screen hook")
            .push((call_id, state));
    }));
    engine.emit_remote_screen_state(77, RemoteVideoState::Active);
    engine.emit_remote_screen_state(77, RemoteVideoState::Inactive);
    assert_eq!(
        *states.lock().expect("remote screen hook"),
        vec![
            (77, RemoteVideoState::Active),
            (77, RemoteVideoState::Inactive)
        ]
    );
}

/// Phase C2e: validates peer-state delivery ordering.
#[test]
fn mock_remote_video_state_emission() {
    let mut engine = MockEngine::new();
    let states = Arc::new(Mutex::new(Vec::new()));
    let states_hook = states.clone();
    engine.set_remote_video_state_callback(Arc::new(move |call_id, state| {
        states_hook
            .lock()
            .expect("remote video hook")
            .push((call_id, state));
    }));
    engine.emit_remote_video_state(77, RemoteVideoState::Inactive);
    engine.emit_remote_video_state(77, RemoteVideoState::Active);
    assert_eq!(
        *states.lock().expect("remote video hook"),
        vec![
            (77, RemoteVideoState::Inactive),
            (77, RemoteVideoState::Active)
        ]
    );
}

#[test]
fn pick_camera_prefers_the_listed_pick() {
    let device = |id: &str, kind| MediaDevice {
        id: id.into(),
        name: id.into(),
        kind,
    };
    let devices = [
        device("mic-1", MediaDeviceKind::Microphone),
        device("cam-1", MediaDeviceKind::Camera),
        device("cam-2", MediaDeviceKind::Camera),
    ];
    assert_eq!(pick_camera(None, &devices), Some("cam-1".to_string()));
    assert_eq!(
        pick_camera(Some("cam-2"), &devices),
        Some("cam-2".to_string())
    );
    // An unplugged pick falls back to the first camera, never to a
    // microphone with the same id.
    assert_eq!(
        pick_camera(Some("gone"), &devices),
        Some("cam-1".to_string())
    );
    assert_eq!(
        pick_camera(Some("mic-1"), &devices),
        Some("cam-1".to_string())
    );
    assert_eq!(pick_camera(Some("cam-1"), &devices[..1]), None);
}
