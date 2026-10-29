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
    // SAFETY: `input` points into `screen`, a live NUL-terminated CString.
    let input = unsafe { CStr::from_ptr(description.input) };
    assert_eq!(input, screen.as_c_str());
    let mut params = ConnectParams {
        encryption_key: Vec::new(),
        custom_parameters: String::new(),
        is_outgoing: true,
        servers: Vec::new(),
        library_versions: Vec::new(),
        p2p_allowed: false,
        mic_input: None,
        speaker_input: None,
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
    // SAFETY: `instance` is the live native instance and `key` outlives the call.
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

/// Stream mode on the vendored binary, offline: `{"rtmp": true}` is the
/// join answer ntgcalls reads as an RTMP stream (`response_payload.cpp`).
/// The connect must report `Rtmp`, issue playback-only sources (no capture
/// device opens) and ask for the stream's time through the broadcast
/// callback. Nothing reaches Telegram: segments would come from TDLib,
/// which this test never calls.
#[test]
fn native_rtmp_connect_reports_stream_mode() {
    if std::env::var_os("QUILL_VERIFY_NATIVE_CALLS").is_none() {
        return;
    }
    let mut engine = NtgcallsEngine::load().expect("Native engine");
    let (events_tx, events_rx) = std::sync::mpsc::channel();
    engine.set_engine_event_callback(Arc::new(move |event| {
        let _ = events_tx.send(event);
    }));
    let chat_id = -100_123;
    engine
        .create_group_call(7, chat_id, GroupCallRole::Member)
        .expect("Native group offer");
    let result = engine.connect_group_call(7, r#"{"rtmp":true}"#, None);
    eprintln!("rtmp connect: {result:?}");
    assert_eq!(engine.group_call_mode(chat_id), Some(GroupCallMode::Rtmp));
    result.expect("RTMP connect with playback-only sources");
    let event = events_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("A broadcast timestamp request");
    let EngineEvent::BroadcastTimeRequested {
        chat_id: asked,
        request,
    } = event
    else {
        panic!("RTMP asks for the stream's current time first: {event:?}");
    };
    assert_eq!(asked, chat_id);
    // "No stream yet": ntgcalls asks again a second later on its own.
    engine
        .answer_broadcast_time(chat_id, request, 0)
        .expect("Timestamp answer");
    let again = events_rx.recv_timeout(std::time::Duration::from_secs(5));
    eprintln!("after a 0 answer: {again:?}");
    assert!(matches!(
        again,
        Ok(EngineEvent::BroadcastTimeRequested { .. })
    ));
    engine.leave_group_call(7).expect("Teardown");
}

/// MTProto stream mode offline: connects without any capture source (no
/// microphone for a listener), asks for parts on its own (no timestamp
/// request), and keeps asking for parts after a NOT_READY answer.
#[test]
fn native_stream_connect_needs_no_capture_and_asks_for_parts() {
    if std::env::var_os("QUILL_VERIFY_NATIVE_CALLS").is_none() {
        return;
    }
    let mut engine = NtgcallsEngine::load().expect("Native engine");
    let (events_tx, events_rx) = std::sync::mpsc::channel();
    engine.set_engine_event_callback(Arc::new(move |event| {
        let _ = events_tx.send(event);
    }));
    let chat_id = -100_124;
    engine
        .create_group_call(8, chat_id, GroupCallRole::Member)
        .expect("Native group offer");
    let result = engine.connect_group_call(8, r#"{"stream":true}"#, None);
    eprintln!("stream connect: {result:?}");
    result.expect("Stream connect with playback-only sources");
    assert_eq!(engine.group_call_mode(chat_id), Some(GroupCallMode::Stream));
    let event = events_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("A broadcast part request");
    eprintln!("first stream event: {event:?}");
    let (request, part) = match event {
        EngineEvent::BroadcastPartRequested { request, part, .. } => Some((request, part)),
        EngineEvent::BroadcastTimeRequested { .. }
        | EngineEvent::BroadcastRequestCancelled { .. }
        | EngineEvent::GroupAudioLevels { .. } => None,
    }
    .expect("An MTProto stream asks for parts, not the time");
    assert_eq!(
        part.channel,
        BroadcastChannel::Audio,
        "the audio part comes first"
    );
    engine
        .answer_broadcast_part(
            chat_id,
            &BroadcastPartAnswer {
                request,
                status: BroadcastPartStatus::NotReady,
                timestamp_ms: part.timestamp_ms,
                response_time: 0.0,
                data: Vec::new(),
            },
        )
        .expect("Part answer");
    let again = events_rx.recv_timeout(std::time::Duration::from_secs(5));
    eprintln!("after NOT_READY: {again:?}");
    assert!(matches!(
        again,
        Ok(EngineEvent::BroadcastPartRequested { .. })
    ));
    engine.leave_group_call(8).expect("Teardown");
}

/// Group playback must carry external camera and screen writers: without
/// them ntgcalls v3.0.3 `optimize_sources` disables incoming video and the
/// frames callback never sees a participant's picture.
#[test]
fn group_playback_video_is_external() {
    let (camera, screen) = NtgcallsEngine::external_playback_video();
    assert_eq!(camera.media_source, NTG_MEDIA_SOURCE_EXTERNAL);
    assert_eq!(screen.media_source, NTG_MEDIA_SOURCE_EXTERNAL);
    assert!(camera.input.is_null() && screen.input.is_null());
}

mod stream_sound {
    use crate::calls::engine::{CallbackShared, frames_trampoline};
    use crate::calls::stream_audio::{PcmFormat, StreamAudio};
    use ntgcalls_sys::{
        NTG_STREAM_DEVICE_CAMERA, NTG_STREAM_DEVICE_MICROPHONE, NTG_STREAM_MODE_CAPTURE,
        NTG_STREAM_MODE_PLAYBACK, NTG_VIDEO_ROTATION_VIDEO_ROTATION_0, ntg_frame, ntg_frame_data,
        ntg_stream_device, ntg_stream_mode,
    };
    use std::ffi::c_void;
    use std::sync::Arc;

    const CHAT: i64 = -100_777;

    fn frame(ssrc: i64, bytes: &mut [u8]) -> ntg_frame {
        ntg_frame {
            ssrc,
            data: bytes.as_mut_ptr(),
            data_len: bytes.len(),
            frame_data: ntg_frame_data {
                absolute_capture_timestamp_ms: 0,
                rotation: NTG_VIDEO_ROTATION_VIDEO_ROTATION_0,
                width: 0,
                height: 0,
            },
        }
    }

    fn pcm(samples: &[i16]) -> Vec<u8> {
        samples.iter().flat_map(|s| s.to_ne_bytes()).collect()
    }

    /// What ntgcalls' frames callback does: one batch for `chat_id`.
    fn deliver(
        shared: &CallbackShared,
        chat_id: i64,
        mode: ntg_stream_mode,
        device: ntg_stream_device,
        frames: &[ntg_frame],
    ) {
        let user_data = std::ptr::from_ref(shared).cast_mut().cast::<c_void>();
        // SAFETY: `frames` and `user_data` outlive this synchronous call, as
        // ntgcalls guarantees for its own callback.
        unsafe {
            frames_trampoline(
                std::ptr::null_mut(),
                chat_id,
                mode,
                device,
                frames.as_ptr(),
                frames.len(),
                user_data,
            );
        }
    }

    fn routed() -> (CallbackShared, Arc<StreamAudio>) {
        let shared = CallbackShared::new();
        let audio = Arc::new(StreamAudio::new(
            PcmFormat::for_output(48_000, 2),
            1.0,
            false,
        ));
        shared.route_stream_audio(CHAT, Some(audio.clone()));
        (shared, audio)
    }

    #[test]
    fn a_streams_playback_frames_reach_its_ring_mixed() {
        let (shared, audio) = routed();
        let mut a = pcm(&[100, 200, -300, 400]);
        let mut b = pcm(&[1, 2, 3, 4]);
        let batch = [frame(1, &mut a), frame(2, &mut b)];
        deliver(
            &shared,
            CHAT,
            NTG_STREAM_MODE_PLAYBACK,
            NTG_STREAM_DEVICE_MICROPHONE,
            &batch,
        );
        let mut out = [0i16; 4];
        assert_eq!(audio.ring().pop(&mut out), 4);
        assert_eq!(out, [101, 202, -297, 404]);
        assert_eq!(audio.health().frames, 1);
    }

    #[test]
    fn other_chats_and_other_kinds_never_reach_the_ring() {
        let (shared, audio) = routed();
        let mut bytes = pcm(&[9, 9]);
        let batch = [frame(1, &mut bytes)];
        deliver(
            &shared,
            CHAT + 1,
            NTG_STREAM_MODE_PLAYBACK,
            NTG_STREAM_DEVICE_MICROPHONE,
            &batch,
        );
        deliver(
            &shared,
            CHAT,
            NTG_STREAM_MODE_CAPTURE,
            NTG_STREAM_DEVICE_MICROPHONE,
            &batch,
        );
        deliver(
            &shared,
            CHAT,
            NTG_STREAM_MODE_PLAYBACK,
            NTG_STREAM_DEVICE_CAMERA,
            &batch,
        );
        assert!(audio.ring().is_empty());
        assert_eq!(audio.health().frames, 0);
    }

    #[test]
    fn leaving_the_call_stops_the_routing() {
        let (shared, audio) = routed();
        shared.forget_group(CHAT);
        let mut bytes = pcm(&[5, 5]);
        let batch = [frame(1, &mut bytes)];
        deliver(
            &shared,
            CHAT,
            NTG_STREAM_MODE_PLAYBACK,
            NTG_STREAM_DEVICE_MICROPHONE,
            &batch,
        );
        assert!(audio.ring().is_empty());
    }
}
