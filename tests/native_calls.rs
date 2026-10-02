//! Opt-in check of the actual sidecar. Enumerates devices; never starts a call.
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
