//! Opt-in checks of the actual sidecar. Never contacts Telegram or captures media.
use quill::calls::engine::{CallEngine, MediaDeviceKind, NtgcallsEngine};

#[test]
fn native_protocol_and_device_enumeration() {
    if std::env::var_os("QUILL_VERIFY_NATIVE_CALLS").is_none() {
        eprintln!("SKIP: set QUILL_VERIFY_NATIVE_CALLS=1 to check the native sidecar");
        return;
    }
    let engine = NtgcallsEngine::load().expect("Native call engine must load");
    let protocol = engine.protocol();
    assert!(protocol.min_layer > 0 && protocol.min_layer <= protocol.max_layer);
    assert!(!protocol.library_versions.is_empty());
    let devices = engine.media_devices().expect("Native device enumeration");
    for kind in [
        MediaDeviceKind::Microphone,
        MediaDeviceKind::Speaker,
        MediaDeviceKind::Camera,
        MediaDeviceKind::Screen,
    ] {
        eprintln!(
            "native {kind:?}: {} devices",
            devices.iter().filter(|d| d.kind == kind).count()
        );
    }
    assert!(devices.iter().all(|device| !device.id.is_empty()));
}

#[test]
fn native_p2p_creation_and_teardown() {
    if std::env::var_os("QUILL_VERIFY_NATIVE_CALLS").is_none() {
        return;
    }
    let mut engine = NtgcallsEngine::load().expect("Native call engine must load");
    // A local native transport only: no TDLib request, connection, or media sources.
    for call_id in [1, 2] {
        engine
            .start_call(call_id, 42, true)
            .expect("Create P2P transport");
        engine.accept_call(call_id).expect("Find created transport");
        engine.hangup(call_id).expect("Destroy P2P transport");
    }
}
