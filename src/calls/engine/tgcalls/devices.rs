//! The devices a tgcalls call can use. The library opens cameras itself
//! (`qc_video_input_devices`); microphones and speakers are Quill's own
//! (cpal), listed by name, which is also the id the capture and output
//! code look them up by.
use super::super::{MediaDevice, MediaDeviceKind};
use quill_calls_sys::Loader;
use serde::Deserialize;
use std::ffi::CStr;

#[derive(Deserialize)]
struct Camera {
    id: String,
    name: String,
}

/// Cameras from the library's JSON list; malformed entries are skipped.
pub(super) fn parse_cameras(json: &str) -> Vec<MediaDevice> {
    serde_json::from_str::<Vec<serde_json::Value>>(json)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|entry| serde_json::from_value::<Camera>(entry).ok())
        .map(|camera| MediaDevice {
            id: camera.id,
            name: camera.name,
            kind: MediaDeviceKind::Camera,
        })
        .collect()
}

pub(super) fn cameras(api: &Loader) -> Vec<MediaDevice> {
    // SAFETY: no arguments; the string is the caller's to free.
    let raw = unsafe { (api.qc_video_input_devices)() };
    if raw.is_null() {
        return Vec::new();
    }
    // SAFETY: non-null and NUL-ended, freed once below.
    let json = unsafe { CStr::from_ptr(raw) }
        .to_string_lossy()
        .into_owned();
    // SAFETY: allocated by the library for the caller.
    unsafe { (api.qc_string_free)(raw) };
    parse_cameras(&json)
}

/// The cpal output name for speaker id `speaker` (they're the same).
pub(super) fn speaker_name(speaker: Option<&str>) -> Option<String> {
    speaker.map(str::to_owned)
}

#[cfg(feature = "ui")]
pub(super) fn sound_devices() -> Vec<MediaDevice> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let host = cpal::default_host();
    let named = |device: cpal::Device, kind| {
        let name = device.description().ok()?.name().to_owned();
        Some(MediaDevice {
            id: name.clone(),
            name,
            kind,
        })
    };
    let mut devices = Vec::new();
    if let Ok(inputs) = host.input_devices() {
        devices.extend(inputs.filter_map(|device| named(device, MediaDeviceKind::Microphone)));
    }
    if let Ok(outputs) = host.output_devices() {
        devices.extend(outputs.filter_map(|device| named(device, MediaDeviceKind::Speaker)));
    }
    devices
}

#[cfg(not(feature = "ui"))]
pub(super) fn sound_devices() -> Vec<MediaDevice> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cameras_come_from_the_library_list() {
        let cameras = parse_cameras(
            r#"[{"id":"cam0","name":"FaceTime HD"},{"id":5},{"id":"cam1","name":"USB"}]"#,
        );
        assert_eq!(cameras.len(), 2);
        assert_eq!(cameras[0].id, "cam0");
        assert_eq!(cameras[0].name, "FaceTime HD");
        assert_eq!(cameras[1].kind, MediaDeviceKind::Camera);
    }

    #[test]
    fn a_broken_list_is_empty() {
        assert!(parse_cameras("not json").is_empty());
    }
}
