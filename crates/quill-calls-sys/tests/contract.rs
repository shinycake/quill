//! The bindings against the real library: every symbol resolves, the
//! versions TDLib needs are there, a bad config is refused and a good one
//! makes a call that is torn down; a group call and its presentation, plain
//! or end-to-end encrypted, give a join payload, take every setting and are
//! torn down. Nothing here waits on the network or a
//! clock; whether two calls actually connect is left to manual testing.
//!
//! Skips when libquill_calls hasn't been built (scripts/build-tgcalls.sh).
//! FFmpeg must be on the loader path (LD_LIBRARY_PATH / DYLD_LIBRARY_PATH,
//! or PATH or beside the DLL on Windows).

use quill_calls_sys::*;
use std::ffi::{CStr, CString, c_void};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

fn built_library() -> Option<PathBuf> {
    let path = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../vendor/quill-calls/lib"
    ))
    .join(library_filename());
    path.exists().then_some(path)
}

fn config(version: &str, max_layer: i32) -> String {
    // 256 bytes of 0x2a, base64.
    let key = format!("{}Kg==", "Kioq".repeat(85));
    format!(
        r#"{{"version":"{version}","outgoing":true,"encryption_key":"{key}","allow_p2p":true,"max_layer":{max_layer},"connect_timeout_ms":30000,"packet_timeout_ms":10000,"servers":[]}}"#
    )
}

fn no_callbacks() -> qc_p2p_callbacks {
    qc_p2p_callbacks {
        user: std::ptr::null_mut::<c_void>(),
        state: None,
        signal_bars: None,
        remote_media_state: None,
        remote_battery_low: None,
        signaling: None,
        audio_out: None,
        remote_video: None,
    }
}

#[test]
fn library_contract() {
    let Some(path) = built_library() else {
        eprintln!("SKIP: libquill_calls not built; run scripts/build-tgcalls.sh");
        return;
    };
    let qc = Loader::load(&path).unwrap_or_else(|error| {
        panic!(
            "loading {}: {error} (is FFmpeg on the library path?)",
            path.display()
        )
    });

    // SAFETY: all calls below follow quill_calls.h: strings are NUL-ended,
    // returned strings are freed once, and each handle is destroyed once.
    unsafe {
        assert_eq!((qc.qc_api_version)(), QC_API_VERSION);
        let max_layer = (qc.qc_max_layer)();
        assert!(max_layer >= 92);
        let versions = CStr::from_ptr((qc.qc_library_versions)())
            .to_str()
            .unwrap()
            .to_owned();
        let versions: Vec<&str> = versions.lines().collect();
        // One version from each of tdesktop's three 1:1 implementations.
        for wanted in ["2.7.7", "7.0.0", "10.0.0"] {
            assert!(versions.contains(&wanted), "{versions:?} lacks {wanted}");
        }

        let devices = (qc.qc_video_input_devices)();
        assert!(CStr::from_ptr(devices).to_str().unwrap().starts_with('['));
        (qc.qc_string_free)(devices);

        let callbacks = no_callbacks();
        let bad = CString::new("{}").unwrap();
        assert!((qc.qc_p2p_create)(bad.as_ptr(), &callbacks, std::ptr::null_mut()).is_null());
        let unknown = CString::new(config("0.0.1", max_layer)).unwrap();
        assert!((qc.qc_p2p_create)(unknown.as_ptr(), &callbacks, std::ptr::null_mut()).is_null());

        for version in versions {
            let good = CString::new(config(version, max_layer)).unwrap();
            let call = (qc.qc_p2p_create)(good.as_ptr(), &callbacks, std::ptr::null_mut());
            assert!(!call.is_null(), "version {version} was refused");
            let silence = [0i16; QC_AUDIO_FRAME_SAMPLES];
            (qc.qc_p2p_push_audio)(call, silence.as_ptr(), silence.len());
            let debug = (qc.qc_p2p_debug_info)(call);
            assert!(!debug.is_null());
            (qc.qc_string_free)(debug);
            (qc.qc_p2p_destroy)(call);
        }
    }
}

/// What the group callbacks saw; `late` counts callbacks after destroy.
#[derive(Default)]
struct Seen {
    destroyed: AtomicBool,
    late: AtomicUsize,
}

impl Seen {
    fn record(user: *mut c_void) {
        // SAFETY: `user` is the `Seen` the test passed as callbacks.user,
        // which outlives the group instance and is never moved.
        let seen = unsafe { &*user.cast::<Seen>() };
        if seen.destroyed.load(Ordering::SeqCst) {
            seen.late.fetch_add(1, Ordering::SeqCst);
        }
    }
}

unsafe extern "C" fn on_network_state(user: *mut c_void, _connected: i32, _to_rtc: i32) {
    Seen::record(user);
}
unsafe extern "C" fn on_levels(user: *mut c_void, _levels: *const qc_group_level, _count: usize) {
    Seen::record(user);
}
unsafe extern "C" fn on_audio(user: *mut c_void, _samples: *const i16, _count: usize) {
    Seen::record(user);
}
unsafe extern "C" fn on_video(
    user: *mut c_void,
    _endpoint: *const std::ffi::c_char,
    _frame: *const qc_video_frame,
) {
    Seen::record(user);
}
unsafe extern "C" fn on_media_channels(
    user: *mut c_void,
    _id: u64,
    _ssrcs: *const u32,
    _count: usize,
) {
    Seen::record(user);
}
unsafe extern "C" fn on_request(user: *mut c_void, _id: u64) {
    Seen::record(user);
}
unsafe extern "C" fn on_part(user: *mut c_void, _id: u64, _part: *const qc_group_broadcast_part) {
    Seen::record(user);
}

unsafe extern "C" fn on_e2e(
    user: *mut c_void,
    _frame: *const qc_group_e2e_frame,
    _set_result: qc_group_e2e_result_fn,
    _result: *mut c_void,
) {
    Seen::record(user);
}

fn group_callbacks(seen: &Seen) -> qc_group_callbacks {
    qc_group_callbacks {
        user: std::ptr::from_ref(seen).cast_mut().cast::<c_void>(),
        network_state: Some(on_network_state),
        audio_levels: Some(on_levels),
        audio_out: Some(on_audio),
        incoming_video: Some(on_video),
        request_media_channels: Some(on_media_channels),
        request_current_time: Some(on_request),
        request_broadcast_part: Some(on_part),
        request_cancelled: Some(on_request),
        e2e: Some(on_e2e),
    }
}

#[test]
fn group_contract() {
    let Some(path) = built_library() else {
        eprintln!("SKIP: libquill_calls not built; run scripts/build-tgcalls.sh");
        return;
    };
    let qc = Loader::load(&path).unwrap_or_else(|error| {
        panic!(
            "loading {}: {error} (is FFmpeg on the library path?)",
            path.display()
        )
    });

    // SAFETY: all calls below follow quill_calls.h: strings are NUL-ended,
    // returned strings are freed once, `seen` outlives each instance, and
    // each handle is destroyed once and not used afterwards.
    unsafe {
        let seen = Seen::default();
        let callbacks = group_callbacks(&seen);
        let bad = CString::new("[").unwrap();
        assert!((qc.qc_group_create)(bad.as_ptr(), &callbacks, std::ptr::null_mut()).is_null());

        // Video chats and conference calls, each with its presentation.
        for (presentation, e2e) in [(false, false), (true, false), (false, true), (true, true)] {
            let seen = Seen::default();
            let callbacks = group_callbacks(&seen);
            let config = CString::new(format!(
                r#"{{"presentation":{presentation},"prioritize_vp8":false,"e2e":{e2e}}}"#
            ))
            .unwrap();
            let video = (qc.qc_video_capture_external)(i32::from(presentation));
            assert!(!video.is_null());
            let group = (qc.qc_group_create)(config.as_ptr(), &callbacks, video);
            assert!(!group.is_null(), "presentation {presentation} was refused");

            // tdesktop's GroupJoinPayload, as joinGroupCall carries it.
            let mut ssrc = 0u32;
            let payload = (qc.qc_group_join_payload)(group, &mut ssrc);
            assert!(!payload.is_null());
            let text = CStr::from_ptr(payload).to_str().unwrap().to_owned();
            (qc.qc_string_free)(payload);
            let json: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert_ne!(ssrc, 0, "{text}");
            assert_eq!(json["ssrc"].as_i64().unwrap() as i32 as u32, ssrc, "{text}");
            assert!(!json["ufrag"].as_str().unwrap().is_empty(), "{text}");
            assert!(!json["pwd"].as_str().unwrap().is_empty(), "{text}");
            let fingerprints = json["fingerprints"].as_array().unwrap();
            assert!(!fingerprints.is_empty(), "{text}");
            for fingerprint in fingerprints {
                assert_eq!(fingerprint["setup"], "passive", "{text}");
                assert!(!fingerprint["hash"].as_str().unwrap().is_empty(), "{text}");
                assert!(
                    fingerprint["fingerprint"].as_str().unwrap().contains(':'),
                    "{text}"
                );
            }
            assert!(json["ssrc-groups"].is_array(), "{text}");

            // A second payload announces the same ssrc.
            let mut again = 0u32;
            let payload = (qc.qc_group_join_payload)(group, &mut again);
            (qc.qc_string_free)(payload);
            assert_eq!(again, ssrc);

            for (mode, unified) in [
                (QC_GROUP_MODE_RTC, 0),
                (QC_GROUP_MODE_BROADCAST, 1),
                (QC_GROUP_MODE_BROADCAST, 0),
                (QC_GROUP_MODE_NONE, 0),
            ] {
                (qc.qc_group_set_connection_mode)(group, mode, unified);
            }
            (qc.qc_group_set_muted)(group, 0);
            (qc.qc_group_set_muted)(group, 1);
            (qc.qc_group_set_volume)(group, 1234, 0.5);
            (qc.qc_group_set_volume)(group, 1, 2.0);
            let gone = [1234u32, 5678];
            (qc.qc_group_remove_ssrcs)(group, gone.as_ptr(), gone.len());

            let channels = CString::new(
                r#"[{"audio_ssrc":1234,"user_id":42,"endpoint":"camera-42","ssrc_groups":[{"semantics":"SIM","ssrcs":[1,2,3]},{"semantics":"FID","ssrcs":[1,4]}],"min_quality":0,"max_quality":2},{"audio_ssrc":"4294967295","user_id":"9007199254740993","endpoint":"screen-42","ssrc_groups":[],"min_quality":2,"max_quality":2}]"#,
            )
            .unwrap();
            assert_eq!(
                (qc.qc_group_set_requested_video)(group, channels.as_ptr()),
                1
            );
            let none = CString::new("[]").unwrap();
            assert_eq!((qc.qc_group_set_requested_video)(group, none.as_ptr()), 1);
            for malformed in ["{}", "[{\"audio_ssrc\":1}]", "nope"] {
                let malformed = CString::new(malformed).unwrap();
                assert_eq!(
                    (qc.qc_group_set_requested_video)(group, malformed.as_ptr()),
                    0
                );
            }

            let endpoint = CString::new("camera-42").unwrap();
            (qc.qc_group_add_incoming_video)(group, endpoint.as_ptr());
            (qc.qc_group_add_incoming_video)(group, endpoint.as_ptr());
            (qc.qc_group_remove_incoming_video)(group, endpoint.as_ptr());
            (qc.qc_group_add_incoming_video)(group, endpoint.as_ptr());

            let silence = [0i16; QC_AUDIO_FRAME_SAMPLES];
            (qc.qc_group_push_audio)(group, silence.as_ptr(), silence.len());

            // Ids tgcalls never asked about are ignored.
            let channel = qc_group_media_channel {
                audio_ssrc: 1234,
                user_id: 42,
            };
            (qc.qc_group_answer_media_channels)(group, u64::MAX, &channel, 1);
            (qc.qc_group_answer_current_time)(group, u64::MAX, 0);
            let bytes = [0u8; 16];
            (qc.qc_group_answer_broadcast_part)(
                group,
                u64::MAX,
                QC_BROADCAST_PART_SUCCESS,
                1_000,
                1.0,
                bytes.as_ptr(),
                bytes.len(),
            );

            (qc.qc_group_set_video_capture)(group, std::ptr::null_mut());
            (qc.qc_group_set_video_capture)(group, video);
            (qc.qc_video_capture_free)(video);

            (qc.qc_group_destroy)(group);
            seen.destroyed.store(true, Ordering::SeqCst);
            assert_eq!(seen.late.load(Ordering::SeqCst), 0);
        }
    }
}
