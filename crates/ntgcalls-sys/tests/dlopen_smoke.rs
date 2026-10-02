//! Smoke test: dlopen the vendored ntgcalls sidecar and resolve the key symbols.
//!
//! Skips gracefully when the library was not vendored (run
//! `scripts/vendor-ntgcalls.sh`). No network, no audio, no call is ever
//! started — the only engine call is the pure `ntg_get_version()` getter.

use std::path::PathBuf;

fn vendored_lib() -> Option<PathBuf> {
    let path = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../vendor/ntgcalls/lib"
    ))
    .join(ntgcalls_sys::library_filename());
    path.exists().then_some(path)
}

/// The key symbols Quill's call roadmap needs must all resolve at load time.
#[test]
fn key_symbols_resolve() {
    let Some(path) = vendored_lib() else {
        eprintln!("SKIP: ntgcalls not vendored — run scripts/vendor-ntgcalls.sh first");
        return;
    };

    let loader =
        ntgcalls_sys::Loader::load(&path).expect("dlopen of the vendored libntgcalls.so failed");

    // Lifecycle (note: the header has no `ntg_destroy`; it is
    // ntg_instance_create / ntg_instance_destroy).
    let _ = loader.ntg_instance_create;
    let _ = loader.ntg_instance_destroy;
    let _ = loader.ntg_create_p2p_call;
    let _ = loader.ntg_create_call;
    let _ = loader.ntg_init_conference;
    let _ = loader.ntg_stop;
    // Signaling bridge for TDLib's updateCall / updateNewCallSignalingData.
    let _ = loader.ntg_on_signaling_data_callback;
    let _ = loader.ntg_send_signaling_data;
    // Mute / pause / resume + state.
    let _ = loader.ntg_mute;
    let _ = loader.ntg_unmute;
    let _ = loader.ntg_pause;
    let _ = loader.ntg_resume;
    let _ = loader.ntg_get_state;
    // Device enumeration (fills the TDLib device-enumeration gap).
    let _ = loader.ntg_get_media_devices;
    // E2E verification emojis.
    let _ = loader.ntg_get_emojis_fingerprint;
    // Video path.
    let _ = loader.ntg_add_incoming_video;
    let _ = loader.ntg_send_external_frame;
    // Screen sharing.
    let _ = loader.ntg_init_presentation;
    let _ = loader.ntg_stop_presentation;
    // Protocol negotiation (C2b gate: must speak TDLib 1.8.67's layers).
    let _ = loader.ntg_get_protocol;
}

/// `ntg_get_version()` is a pure getter — safe to call in a test.
#[test]
fn version_string_is_non_empty() {
    let Some(path) = vendored_lib() else {
        eprintln!("SKIP: ntgcalls not vendored — run scripts/vendor-ntgcalls.sh first");
        return;
    };

    let loader =
        ntgcalls_sys::Loader::load(&path).expect("dlopen of the vendored libntgcalls.so failed");

    let version = unsafe { (loader.ntg_get_version)() };
    assert!(!version.is_null(), "ntg_get_version() returned null");
    let version = unsafe { std::ffi::CStr::from_ptr(version) };
    assert!(
        !version.to_bytes().is_empty(),
        "ntg_get_version() returned an empty string"
    );
    eprintln!("vendored ntgcalls version: {}", version.to_string_lossy());

    // ntg_last_error() must be either null or a valid C string.
    let last_error = unsafe { (loader.ntg_last_error)() };
    if !last_error.is_null() {
        let text = unsafe { std::ffi::CStr::from_ptr(last_error) };
        eprintln!("ntg_last_error(): {}", text.to_string_lossy());
    }
}

/// Missing library must produce a clear diagnostic, not a panic.
#[test]
fn missing_library_errors_clearly() {
    let missing = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../vendor/ntgcalls/lib/definitely-not-here.so"
    ));
    let err = match ntgcalls_sys::Loader::load(&missing) {
        Ok(_) => panic!("expected a LoadError for a missing library"),
        Err(err) => err,
    };
    let message = err.to_string();
    assert!(
        message.contains("not found"),
        "diagnostic should say the library was not found: {message}"
    );
    assert!(
        message.contains("vendor-ntgcalls.sh"),
        "diagnostic should point at the vendor script: {message}"
    );
}

/// Exercise actual executable-relative loading from an app bundle, without an
/// environment override or the development checkout's vendor directory.
#[cfg(target_os = "macos")]
#[test]
fn packaged_sidecar_loads() {
    if std::env::var_os("QUILL_BUNDLED_CALL_PROBE").is_some() {
        let loader = ntgcalls_sys::Loader::load_default().expect("Bundled sidecar must load");
        assert!(!unsafe { (loader.ntg_get_version)() }.is_null());
        return;
    }
    let Some(library) = vendored_lib() else {
        return;
    };
    let root =
        std::env::temp_dir().join(format!("quill-bundled-call-proof-{}", std::process::id()));
    let contents = root.join("Proof.app/Contents");
    std::fs::create_dir_all(contents.join("MacOS")).unwrap();
    std::fs::create_dir_all(contents.join("Frameworks")).unwrap();
    let executable = contents.join("MacOS/probe");
    std::fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
    std::fs::copy(
        library,
        contents
            .join("Frameworks")
            .join(ntgcalls_sys::library_filename()),
    )
    .unwrap();
    let result = std::process::Command::new(executable)
        .args(["--exact", "packaged_sidecar_loads", "--nocapture"])
        .env("QUILL_BUNDLED_CALL_PROBE", "1")
        .env_remove("QUILL_NTGCALLS_LIB")
        .output()
        .unwrap();
    std::fs::remove_dir_all(root).unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
