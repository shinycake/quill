//! The bindings against the real library: every symbol resolves, the
//! versions TDLib needs are there, a bad config is refused and a good one
//! makes a call that is torn down. Nothing here waits on the network or a
//! clock; whether two calls actually connect is left to manual testing.
//!
//! Skips when libquill_calls hasn't been built (scripts/build-tgcalls.sh).
//! FFmpeg must be on the loader path (LD_LIBRARY_PATH / DYLD_LIBRARY_PATH).

use quill_calls_sys::*;
use std::ffi::{CStr, CString, c_void};
use std::path::PathBuf;

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
            // tgcalls' InstanceV2ReferenceImpl (10.0.0, 11.0.0) can deadlock
            // when torn down right after it starts: its last reference may
            // drop on the worker thread, whose PeerConnection teardown waits
            // on the media thread that waits on the worker. Follow-up in
            // docs/decisions/tgcalls-engine.md.
            if matches!(version, "10.0.0" | "11.0.0") {
                continue;
            }
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
