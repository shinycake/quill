use super::*;

#[test]
fn native_default_audio_sources_before_connect() {
    if std::env::var_os("QUILL_VERIFY_NATIVE_CALLS").is_none() {
        return;
    }
    let mut engine = NtgcallsEngine::load().expect("Native engine");
    engine.start_call(1, 42, true).expect("Local transport");
    let screen = engine
        .device_input(None, MediaDeviceKind::Screen)
        .expect("Enumerated screen");
    let screen = screen.expect("Screen metadata");
    let metadata: serde_json::Value = serde_json::from_slice(screen.as_bytes()).unwrap();
    assert!(
        metadata.get("id").is_some(),
        "Desktop capturer requires a source ID"
    );
    let description = NtgcallsEngine::screen_video_description(Some(&screen));
    assert_eq!(
        unsafe { CStr::from_ptr(description.input) },
        screen.as_c_str()
    );
    let mut params = ConnectParams {
        encryption_key: Vec::new(),
        custom_parameters: String::new(),
        is_outgoing: true,
        servers: Vec::new(),
        library_versions: Vec::new(),
        p2p_allowed: false,
        mic_input: None,
        speaker_input: None,
        video_enabled: false,
        camera_input: None,
    };
    for length in [0, 1, 255, 257] {
        params.encryption_key = vec![1; length];
        assert!(matches!(
            engine.connect(1, &params),
            Err(EngineError::Engine {
                op: "ntg_skip_exchange",
                code: NTG_ERR_INVALID_PARAMS
            })
        ));
    }
    engine.call_media.insert(
        1,
        CallMediaConfig {
            mic: None,
            speaker: None,
            camera: None,
            camera_enabled: false,
            screen_share_on: false,
        },
    );
    // Before connect, ntgcalls constructs readers without opening them.
    // This checks device metadata without recording or making a call.
    engine.set_media_sources(1).expect("Default audio sources");
    let key = [1_u8; 256];
    let instance = engine.instance.expect("Live native instance");
    // Exercise the native operation reported in the user's screenshot.
    // No connection follows, so these synthetic bytes never leave the process.
    let result = unsafe {
        (engine.api.ntg_skip_exchange)(instance.as_ptr(), 42, key.as_ptr(), key.len(), true)
    };
    assert_eq!(
        result, NTG_OK,
        "Native key exchange must find the local call"
    );
    params.encryption_key = key.to_vec();
    // Unsupported versions fail before creating a network connection.
    params.library_versions = vec!["unsupported-offline-test".into()];
    for _ in 0..2 {
        assert!(
            matches!(
                engine.connect(1, &params),
                Err(EngineError::Engine {
                    op: "ntg_connect_p2p",
                    ..
                })
            ),
            "Retry must replace the previous native key exchange"
        );
    }
    engine.hangup(1).expect("Teardown");
}
