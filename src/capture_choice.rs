//! Which camera and microphone ffmpeg captures from (round videos, the
//! profile-photo camera). The in-call device menu publishes the user's pick
//! here by device name; a pick that is no longer plugged in falls back to
//! the first device, as before.
#![deny(clippy::indexing_slicing)]

use std::path::PathBuf;
use std::sync::Mutex;

/// Names (as the call engine lists them) of the camera and microphone the
/// user picked. `None` means the system default.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CaptureChoice {
    pub camera: Option<String>,
    pub microphone: Option<String>,
}

static CHOICE: Mutex<CaptureChoice> = Mutex::new(CaptureChoice {
    camera: None,
    microphone: None,
});

/// Publish the camera pick (`None` clears it).
pub fn set_camera(name: Option<String>) {
    let mut guard = CHOICE.lock().unwrap_or_else(|e| e.into_inner());
    guard.camera = name;
}

/// Publish the microphone pick (`None` clears it).
pub fn set_microphone(name: Option<String>) {
    let mut guard = CHOICE.lock().unwrap_or_else(|e| e.into_inner());
    guard.microphone = name;
}

/// The current pick.
pub fn current() -> CaptureChoice {
    CHOICE.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

fn same_name(wanted: &str, name: &str) -> bool {
    let (wanted, name) = (wanted.trim(), name.trim());
    name == wanted || name.to_lowercase() == wanted.to_lowercase()
}

/// The device named `preferred` among `available`; when it is unset or gone,
/// the first device. `None` only when there are no devices.
pub fn pick_device<'a>(preferred: Option<&str>, available: &'a [String]) -> Option<&'a String> {
    preferred
        .and_then(|wanted| available.iter().find(|name| same_name(wanted, name)))
        .or_else(|| available.first())
}

/// Same for V4L2 nodes; a pick matches the node path or its file name.
pub fn pick_v4l2<'a>(preferred: Option<&str>, nodes: &'a [PathBuf]) -> Option<&'a PathBuf> {
    preferred
        .and_then(|wanted| {
            nodes.iter().find(|path| {
                same_name(wanted, &path.to_string_lossy())
                    || path
                        .file_name()
                        .is_some_and(|file| same_name(wanted, &file.to_string_lossy()))
            })
        })
        .or_else(|| nodes.first())
}

/// AVFoundation devices ffmpeg lists, as `(index, name)`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct AvfDevices {
    pub video: Vec<(usize, String)>,
    pub audio: Vec<(usize, String)>,
}

/// Parse `ffmpeg -f avfoundation -list_devices true -i ""` (on stderr).
pub fn parse_avfoundation_devices(stderr: &str) -> AvfDevices {
    let mut devices = AvfDevices::default();
    let mut audio = false;
    for line in stderr.lines() {
        if line.contains("AVFoundation video devices") {
            audio = false;
        } else if line.contains("AVFoundation audio devices") {
            audio = true;
        } else if let Some((_, entry)) = line.split_once("] [")
            && let Some((index, name)) = entry.split_once("] ")
            && let Ok(index) = index.parse::<usize>()
        {
            let list = if audio {
                &mut devices.audio
            } else {
                &mut devices.video
            };
            list.push((index, name.trim().to_string()));
        }
    }
    devices
}

/// The AVFoundation index for the pick, or `"default"` when unset or gone.
pub fn avf_index(preferred: Option<&str>, list: &[(usize, String)]) -> String {
    preferred
        .and_then(|wanted| list.iter().find(|(_, name)| same_name(wanted, name)))
        .map_or_else(|| "default".to_string(), |(index, _)| index.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        avf_index, parse_avfoundation_devices, pick_device, pick_v4l2, set_camera, set_microphone,
    };
    use std::path::PathBuf;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn chosen_device_is_used() {
        let list = names(&["OBS Virtual Camera", "Logitech C920"]);
        assert_eq!(
            pick_device(Some("logitech c920"), &list).map(String::as_str),
            Some("Logitech C920")
        );
    }

    #[test]
    fn missing_choice_falls_back_to_first() {
        let list = names(&["OBS Virtual Camera", "Logitech C920"]);
        assert_eq!(
            pick_device(Some("Unplugged"), &list).map(String::as_str),
            Some("OBS Virtual Camera")
        );
        assert_eq!(
            pick_device(None, &list).map(String::as_str),
            Some("OBS Virtual Camera")
        );
    }

    #[test]
    fn no_devices_yields_none() {
        assert_eq!(pick_device(Some("x"), &[]), None);
        assert_eq!(pick_v4l2(Some("x"), &[]), None);
    }

    #[test]
    fn v4l2_matches_path_or_file_name() {
        let nodes = vec![PathBuf::from("/dev/video0"), PathBuf::from("/dev/video2")];
        assert_eq!(pick_v4l2(Some("video2"), &nodes), nodes.get(1));
        assert_eq!(pick_v4l2(Some("/dev/video2"), &nodes), nodes.get(1));
        assert_eq!(pick_v4l2(Some("/dev/video9"), &nodes), nodes.first());
    }

    #[test]
    fn avfoundation_listing_resolves_indices() {
        let listing = "[AVFoundation indev @ 0x1] AVFoundation video devices:\n\
            [AVFoundation indev @ 0x1] [0] FaceTime HD Camera\n\
            [AVFoundation indev @ 0x1] [1] Studio Display Camera\n\
            [AVFoundation indev @ 0x1] AVFoundation audio devices:\n\
            [AVFoundation indev @ 0x1] [0] MacBook Pro Microphone\n";
        let devices = parse_avfoundation_devices(listing);
        assert_eq!(devices.video.len(), 2);
        assert_eq!(
            avf_index(Some("Studio Display Camera"), &devices.video),
            "1"
        );
        assert_eq!(avf_index(Some("Gone"), &devices.video), "default");
        assert_eq!(avf_index(None, &devices.audio), "default");
        assert_eq!(
            avf_index(Some("MacBook Pro Microphone"), &devices.audio),
            "0"
        );
    }

    #[test]
    fn published_choice_round_trips() {
        set_camera(Some("Cam".into()));
        set_microphone(Some("Mic".into()));
        let choice = super::current();
        assert_eq!(choice.camera.as_deref(), Some("Cam"));
        assert_eq!(choice.microphone.as_deref(), Some("Mic"));
        set_camera(None);
        set_microphone(None);
        assert_eq!(super::current(), Default::default());
    }
}
