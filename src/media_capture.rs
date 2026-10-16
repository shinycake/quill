//! Camera and microphone capture through ffmpeg: which devices to open on
//! each platform (honouring the pick in the call device menu) and the input
//! arguments that open them. Split from `media_tools`, which finds the binaries.
#![deny(clippy::indexing_slicing)]

use crate::media_tools::{NoConsole, command};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;

/// ffmpeg input arguments for the camera and microphone of a round video,
/// with the stream specifiers of the audio and video they produce.
pub struct CaptureInput {
    pub args: Vec<String>,
    pub audio: &'static str,
    pub video: &'static str,
}

/// The platform's camera and microphone for ffmpeg: the ones picked in the
/// call device menu (`capture_choice`) when still present, otherwise the
/// system defaults (AVFoundation's on macOS, the first V4L2 capture device
/// and the `default` PulseAudio/ALSA source on Linux, the first DirectShow
/// camera and microphone on Windows).
pub fn capture_input(
    choice: &crate::capture_choice::CaptureChoice,
) -> Result<CaptureInput, String> {
    platform_input(choice, true)
}

/// The same camera without a microphone, for the profile-photo preview: a
/// machine with no microphone still has a camera.
pub fn camera_input(choice: &crate::capture_choice::CaptureChoice) -> Result<Vec<String>, String> {
    platform_input(choice, false).map(|input| input.args)
}

fn platform_input(
    choice: &crate::capture_choice::CaptureChoice,
    with_audio: bool,
) -> Result<CaptureInput, String> {
    if cfg!(target_os = "macos") {
        let (video, audio) = avfoundation_inputs(choice);
        Ok(avfoundation_input(&video, with_audio.then_some(&*audio)))
    } else if cfg!(target_os = "linux") {
        let nodes = v4l2_capture_devices();
        let camera =
            crate::capture_choice::pick_v4l2(choice.camera.as_deref(), &nodes, v4l2_card_name)
                .ok_or("No camera found.")?;
        let audio = with_audio.then(|| linux_audio(choice));
        Ok(v4l2_input(
            camera,
            audio
                .as_ref()
                .map(|(backend, source)| (*backend, &**source)),
        ))
    } else if cfg!(windows) {
        let devices = list_dshow_devices()?;
        let (camera, mic) = choose_dshow(choice, &devices, with_audio)?;
        Ok(dshow_input(&camera, mic.as_deref()))
    } else {
        Err("Recording isn't supported on this system yet.".into())
    }
}

/// AVFoundation takes camera and microphone as one `video:audio` input;
/// `none` leaves the sound out.
fn avfoundation_input(video: &str, audio: Option<&str>) -> CaptureInput {
    let mut args: Vec<String> = [
        "-f",
        "avfoundation",
        "-framerate",
        "30",
        "-pixel_format",
        "nv12",
        "-i",
    ]
    .map(String::from)
    .to_vec();
    args.push(format!("{video}:{}", audio.unwrap_or("none")));
    CaptureInput {
        args,
        audio: "0:a",
        video: "0:v",
    }
}

/// A V4L2 camera, plus, with `audio` as `(backend, source)`, a PulseAudio or
/// ALSA source as the second input.
fn v4l2_input(camera: &Path, audio: Option<(&str, &str)>) -> CaptureInput {
    let mut args: Vec<String> = ["-f", "v4l2", "-framerate", "15", "-i"]
        .map(String::from)
        .to_vec();
    args.push(camera.to_string_lossy().into_owned());
    if let Some((backend, source)) = audio {
        args.extend(["-f", backend, "-i", source].map(String::from));
    }
    CaptureInput {
        args,
        audio: "1:a",
        video: "0:v",
    }
}

/// DirectShow camera and microphone as two inputs, not one `video=:audio=`
/// pair: a camera without a microphone, or a virtual camera, then opens on
/// its own and the chosen microphone is independent of the camera.
fn dshow_input(camera: &str, mic: Option<&str>) -> CaptureInput {
    let mut args: Vec<String> = ["-f", "dshow", "-i"].map(String::from).to_vec();
    args.push(format!("video={camera}"));
    if let Some(mic) = mic {
        args.extend(["-f", "dshow", "-i"].map(String::from));
        args.push(format!("audio={mic}"));
    }
    CaptureInput {
        args,
        audio: "1:a",
        video: "0:v",
    }
}

/// The DirectShow camera for the pick (else the first), and with
/// `need_audio` the microphone for the pick (else the first).
fn choose_dshow(
    choice: &crate::capture_choice::CaptureChoice,
    devices: &DshowDevices,
    need_audio: bool,
) -> Result<(String, Option<String>), String> {
    let camera = crate::capture_choice::pick_device(choice.camera.as_deref(), &devices.video)
        .ok_or("No camera found.")?
        .clone();
    if !need_audio {
        return Ok((camera, None));
    }
    let mic = crate::capture_choice::pick_device(choice.microphone.as_deref(), &devices.audio)
        .ok_or("No microphone found.")?
        .clone();
    Ok((camera, Some(mic)))
}

/// Linux audio input: the audio backend and the source name. PulseAudio
/// sources (PipeWire serves them too) are matched to the picked microphone
/// by the name the call engine lists; otherwise, and for ALSA, `default`.
fn linux_audio(choice: &crate::capture_choice::CaptureChoice) -> (&'static str, String) {
    let backend = linux_audio_backend();
    let source = match (backend, choice.microphone.as_deref()) {
        ("pulse", Some(_)) => {
            let mut list = command("ffmpeg");
            list.args(["-hide_banner", "-sources", "pulse"]);
            let listing =
                capture_within(&mut list, DEVICE_LIST_LIMIT, Stream::Stdout).unwrap_or_default();
            crate::capture_choice::pick_pulse_source(
                choice.microphone.as_deref(),
                &crate::capture_choice::parse_pulse_sources(&listing),
            )
        }
        _ => "default".to_string(),
    };
    (backend, source)
}

/// How long a device-listing ffmpeg may run. Capture starts on the UI
/// thread, so a stuck device probe must not hang the app.
const DEVICE_LIST_LIMIT: std::time::Duration = std::time::Duration::from_secs(3);

/// Which stream of a device-listing command carries the list.
#[derive(Clone, Copy)]
enum Stream {
    Stdout,
    Stderr,
}

/// Run `cmd` and return what it prints on stderr, or `None` when it cannot
/// start or does not finish within `limit` (it is killed then). ffmpeg's
/// device lists go to stderr and the command itself "fails", so the exit
/// status is ignored.
fn stderr_within(cmd: &mut Command, limit: std::time::Duration) -> Option<String> {
    capture_within(cmd, limit, Stream::Stderr)
}

/// [`stderr_within`] for stdout: what `ffmpeg -encoders` prints.
pub(crate) fn stdout_within(cmd: &mut Command, limit: std::time::Duration) -> Option<String> {
    capture_within(cmd, limit, Stream::Stdout)
}

/// [`stderr_within`] for either stream (`-sources` prints on stdout).
fn capture_within(cmd: &mut Command, limit: std::time::Duration, stream: Stream) -> Option<String> {
    use std::io::Read;
    use std::process::Stdio;
    let (out, err) = match stream {
        Stream::Stdout => (Stdio::piped(), Stdio::null()),
        Stream::Stderr => (Stdio::null(), Stdio::piped()),
    };
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .spawn()
        .ok()?;
    let mut pipe: Box<dyn Read + Send> = match stream {
        Stream::Stdout => Box::new(child.stdout.take()?),
        Stream::Stderr => Box::new(child.stderr.take()?),
    };
    let (tx, rx) = std::sync::mpsc::channel();
    let reader = std::thread::Builder::new()
        .name("quill-device-list".into())
        .spawn(move || {
            let mut bytes = Vec::new();
            let _ = pipe.read_to_end(&mut bytes);
            let _ = tx.send(bytes);
        });
    let bytes = match reader {
        Ok(_) => rx.recv_timeout(limit).ok(),
        Err(_) => None,
    };
    if bytes.is_none() {
        let _ = child.kill();
    }
    let _ = child.wait();
    bytes.map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

/// AVFoundation `(video, audio)` input specifiers: the picked devices'
/// indices, `default` for an unset or vanished pick. Lists the devices only
/// when something was picked.
fn avfoundation_inputs(choice: &crate::capture_choice::CaptureChoice) -> (String, String) {
    if choice.camera.is_none() && choice.microphone.is_none() {
        return ("default".into(), "default".into());
    }
    let mut list = command("ffmpeg");
    list.args([
        "-hide_banner",
        "-f",
        "avfoundation",
        "-list_devices",
        "true",
        "-i",
        "",
    ]);
    let listing = stderr_within(&mut list, DEVICE_LIST_LIMIT).unwrap_or_default();
    avfoundation_specifiers(
        choice,
        &crate::capture_choice::parse_avfoundation_devices(&listing),
    )
}

/// The `(video, audio)` AVFoundation specifiers for `choice` among the
/// listed `devices`: the picked device's index, `default` when unset or gone.
pub fn avfoundation_specifiers(
    choice: &crate::capture_choice::CaptureChoice,
    devices: &crate::capture_choice::AvfDevices,
) -> (String, String) {
    (
        crate::capture_choice::avf_index(choice.camera.as_deref(), &devices.video),
        crate::capture_choice::avf_index(choice.microphone.as_deref(), &devices.audio),
    )
}

/// Audio capture backend ffmpeg uses on Linux: PulseAudio (which PipeWire
/// also serves) when this ffmpeg has it, otherwise ALSA.
fn linux_audio_backend() -> &'static str {
    static BACKEND: LazyLock<&'static str> = LazyLock::new(|| {
        let listing = command("ffmpeg")
            .args(["-hide_banner", "-devices"])
            .stdin(std::process::Stdio::null())
            .output()
            .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
            .unwrap_or_default();
        pick_audio_backend(&listing)
    });
    *BACKEND
}

fn pick_audio_backend(devices_listing: &str) -> &'static str {
    let has = |name: &str| {
        devices_listing
            .lines()
            .any(|line| line.split_whitespace().nth(1) == Some(name))
    };
    if has("pulse") || !has("alsa") {
        "pulse"
    } else {
        "alsa"
    }
}

/// The V4L2 card name of `/dev/videoN` (what the call engine lists).
#[cfg(target_os = "linux")]
fn v4l2_card_name(path: &Path) -> Option<String> {
    let file = path.file_name()?;
    let name = std::fs::read_to_string(Path::new("/sys/class/video4linux").join(file).join("name"))
        .ok()?;
    Some(name.trim().to_string())
}

#[cfg(not(target_os = "linux"))]
fn v4l2_card_name(_path: &Path) -> Option<String> {
    None
}

/// `/dev/video*` nodes that can capture video, in numeric order. A camera
/// often exposes extra nodes (metadata, IR); only real capture nodes count.
pub fn v4l2_capture_devices() -> Vec<PathBuf> {
    let mut nodes: Vec<(u32, PathBuf)> = std::fs::read_dir("/dev")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let number = name.strip_prefix("video")?.parse::<u32>().ok()?;
            Some((number, entry.path()))
        })
        .collect();
    nodes.sort();
    nodes
        .into_iter()
        .filter(|(_, path)| v4l2_can_capture(path))
        .map(|(_, path)| path)
        .collect()
}

/// `VIDIOC_QUERYCAP`: the node's `device_caps` (falling back to
/// `capabilities`) include `V4L2_CAP_VIDEO_CAPTURE`.
#[cfg(target_os = "linux")]
fn v4l2_can_capture(path: &Path) -> bool {
    use std::os::fd::AsRawFd;
    // struct v4l2_capability: driver[16], card[32], bus_info[32], version,
    // capabilities, device_caps, reserved[3] = 104 bytes.
    let mut caps = [0u32; 26];
    // _IOR('V', 0, struct v4l2_capability)
    const VIDIOC_QUERYCAP: libc::c_ulong = 0x8068_5600;
    let Ok(file) = std::fs::File::open(path) else {
        return false;
    };
    // SAFETY: `caps` is 104 writable bytes, the size QUERYCAP fills in.
    let status = unsafe { libc::ioctl(file.as_raw_fd(), VIDIOC_QUERYCAP, caps.as_mut_ptr()) };
    status == 0 && v4l2_caps_allow_capture(caps[21], caps[22])
}

#[cfg(not(target_os = "linux"))]
fn v4l2_can_capture(_path: &Path) -> bool {
    false
}

/// `V4L2_CAP_DEVICE_CAPS` says the per-node `device_caps` are valid;
/// `V4L2_CAP_VIDEO_CAPTURE` / `_MPLANE` mark a capture node.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn v4l2_caps_allow_capture(capabilities: u32, device_caps: u32) -> bool {
    const DEVICE_CAPS: u32 = 0x8000_0000;
    const CAPTURE: u32 = 0x0000_0001 | 0x0000_1000;
    let caps = if capabilities & DEVICE_CAPS != 0 {
        device_caps
    } else {
        capabilities
    };
    caps & CAPTURE != 0
}

/// Camera and microphone names DirectShow reports, in its order.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct DshowDevices {
    pub video: Vec<String>,
    pub audio: Vec<String>,
}

/// `ffmpeg -list_devices true -f dshow -i dummy` (it prints the list on
/// stderr and then fails, which is expected).
pub fn list_dshow_devices() -> Result<DshowDevices, String> {
    let mut list = command("ffmpeg");
    list.args([
        "-hide_banner",
        "-list_devices",
        "true",
        "-f",
        "dshow",
        "-i",
        "dummy",
    ])
    .no_console();
    let stderr = stderr_within(&mut list, DEVICE_LIST_LIMIT)
        .ok_or("Couldn't list the cameras and microphones.")?;
    Ok(parse_dshow_devices(&stderr))
}

/// Parse ffmpeg's DirectShow device list, in both layouts: newer builds tag
/// each line `"Name" (video)`, older ones use `DirectShow video devices`
/// section headers. `Alternative name` lines are skipped.
pub fn parse_dshow_devices(stderr: &str) -> DshowDevices {
    let mut devices = DshowDevices::default();
    let mut section = "";
    for line in stderr.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("directshow video devices") {
            section = "video";
            continue;
        }
        if lower.contains("directshow audio devices") {
            section = "audio";
            continue;
        }
        if lower.contains("alternative name") {
            continue;
        }
        let (Some(open), Some(close)) = (line.find('"'), line.rfind('"')) else {
            continue;
        };
        if close <= open {
            continue;
        }
        let name = line.get(open + 1..close).unwrap_or_default().to_string();
        let tail = lower.get(close..).unwrap_or_default();
        let kind = if tail.contains("(video)") {
            "video"
        } else if tail.contains("(audio)") {
            "audio"
        } else if tail.contains("(none)") {
            continue;
        } else {
            section
        };
        match kind {
            "video" => devices.video.push(name),
            "audio" => devices.audio.push(name),
            _ => {}
        }
    }
    devices
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dshow_devices_parse_in_both_ffmpeg_layouts() {
        let new = "[dshow @ 000001] DirectShow video devices (some may be both video and audio devices)\n\
[dshow @ 000001]  \"Integrated Camera\" (video)\n\
[dshow @ 000001]   Alternative name \"@device_pnp_\\\\?\\usb#vid_0c45\"\n\
[dshow @ 000001] DirectShow audio devices\n\
[dshow @ 000001]  \"Microphone Array (Realtek)\" (audio)\n\
[dshow @ 000001]   Alternative name \"@device_cm_{33D9A762}\"\n\
[dshow @ 000001]  \"Virtual Thing\" (none)\n\
dummy: Immediate exit requested\n";
        let want = DshowDevices {
            video: vec!["Integrated Camera".into()],
            audio: vec!["Microphone Array (Realtek)".into()],
        };
        assert_eq!(parse_dshow_devices(new), want);
        let old = "[dshow @ 1] DirectShow video devices\n[dshow @ 1]  \"USB Cam\"\n\
[dshow @ 1]     Alternative name \"@device_pnp\"\n[dshow @ 1] DirectShow audio devices\n\
[dshow @ 1]  \"Mic\"\n";
        assert_eq!(
            parse_dshow_devices(old),
            DshowDevices {
                video: vec!["USB Cam".into()],
                audio: vec!["Mic".into()]
            }
        );
        assert_eq!(parse_dshow_devices("dummy: error"), DshowDevices::default());
    }

    #[test]
    fn v4l2_capture_nodes_are_told_from_metadata_nodes() {
        let device_caps = 0x8000_0000u32;
        // UVC capture node: per-node caps carry VIDEO_CAPTURE (+ STREAMING).
        assert!(v4l2_caps_allow_capture(
            device_caps | 0x0400_0001,
            0x0400_0001
        ));
        // Its metadata node lists the device's caps but not capture itself.
        assert!(!v4l2_caps_allow_capture(
            device_caps | 0x0400_0001,
            0x0480_0000
        ));
        // Old drivers without DEVICE_CAPS use the plain capabilities.
        assert!(v4l2_caps_allow_capture(0x0000_0001, 0));
        assert!(v4l2_caps_allow_capture(device_caps, 0x0000_1000), "mplane");
    }

    #[test]
    fn ffmpeg_audio_backend_prefers_pulse_and_falls_back_to_alsa() {
        assert_eq!(
            pick_audio_backend(" D  alsa\n DE pulse\n DE v4l2\n"),
            "pulse"
        );
        assert_eq!(pick_audio_backend(" D  alsa\n DE v4l2\n"), "alsa");
        assert_eq!(pick_audio_backend(""), "pulse");
    }

    #[test]
    fn stderr_within_returns_output_and_gives_up_on_a_stuck_command() {
        if cfg!(windows) {
            return;
        }
        let mut quick = Command::new("sh");
        quick.args(["-c", "echo listed >&2"]);
        let out = stderr_within(&mut quick, std::time::Duration::from_secs(10));
        assert_eq!(out.as_deref().map(str::trim), Some("listed"));
        let mut stuck = Command::new("sh");
        stuck.args(["-c", "sleep 30"]);
        let started = std::time::Instant::now();
        assert_eq!(
            stderr_within(&mut stuck, std::time::Duration::from_millis(100)),
            None
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
    }

    fn joined(args: &[String]) -> String {
        args.join(" ")
    }

    #[test]
    fn dshow_opens_camera_and_microphone_as_separate_inputs() {
        let both = dshow_input("OBS Virtual Camera", Some("Microphone (Realtek)"));
        assert_eq!(
            joined(&both.args),
            "-f dshow -i video=OBS Virtual Camera -f dshow -i audio=Microphone (Realtek)"
        );
        assert_eq!((both.video, both.audio), ("0:v", "1:a"));
        let camera_only = dshow_input("Integrated Camera", None);
        assert_eq!(
            joined(&camera_only.args),
            "-f dshow -i video=Integrated Camera"
        );
    }

    fn dshow_devices() -> DshowDevices {
        DshowDevices {
            video: vec!["OBS Virtual Camera".into(), "Logitech C920".into()],
            audio: vec!["Microphone (Realtek)".into(), "C920 Microphone".into()],
        }
    }

    fn choice(
        camera: Option<&str>,
        microphone: Option<&str>,
    ) -> crate::capture_choice::CaptureChoice {
        crate::capture_choice::CaptureChoice {
            camera: camera.map(String::from),
            microphone: microphone.map(String::from),
        }
    }

    #[test]
    fn dshow_honours_the_call_settings_pick_and_falls_back_to_the_first() {
        let devices = dshow_devices();
        let picked = choose_dshow(
            &choice(Some("logitech c920"), Some("C920 Microphone")),
            &devices,
            true,
        );
        assert_eq!(
            picked,
            Ok((
                "Logitech C920".to_string(),
                Some("C920 Microphone".to_string())
            ))
        );
        let defaults = choose_dshow(&choice(None, None), &devices, true);
        assert_eq!(
            defaults,
            Ok((
                "OBS Virtual Camera".to_string(),
                Some("Microphone (Realtek)".to_string())
            ))
        );
        let gone = choose_dshow(
            &choice(Some("Unplugged"), Some("Unplugged")),
            &devices,
            true,
        );
        assert_eq!(gone, defaults);
    }

    #[test]
    fn dshow_camera_works_without_any_microphone_when_none_is_needed() {
        let no_mic = DshowDevices {
            video: vec!["USB Cam".into()],
            audio: vec![],
        };
        assert_eq!(
            choose_dshow(&choice(None, None), &no_mic, false),
            Ok(("USB Cam".to_string(), None))
        );
        assert_eq!(
            choose_dshow(&choice(None, None), &no_mic, true),
            Err("No microphone found.".to_string())
        );
        assert_eq!(
            choose_dshow(&choice(None, None), &DshowDevices::default(), false),
            Err("No camera found.".to_string())
        );
    }

    #[test]
    fn v4l2_uses_the_chosen_node_and_audio_source() {
        let input = v4l2_input(
            Path::new("/dev/video2"),
            Some(("pulse", "alsa_input.usb-046d_C920-02.analog-stereo")),
        );
        assert_eq!(
            joined(&input.args),
            "-f v4l2 -framerate 15 -i /dev/video2 -f pulse -i alsa_input.usb-046d_C920-02.analog-stereo"
        );
        assert_eq!((input.video, input.audio), ("0:v", "1:a"));
        let alsa = v4l2_input(Path::new("/dev/video0"), Some(("alsa", "default")));
        assert!(joined(&alsa.args).ends_with("-f alsa -i default"));
        let camera_only = v4l2_input(Path::new("/dev/video0"), None);
        assert_eq!(
            joined(&camera_only.args),
            "-f v4l2 -framerate 15 -i /dev/video0"
        );
    }

    #[test]
    fn avfoundation_pairs_video_and_audio_or_drops_the_sound() {
        let both = avfoundation_input("1", Some("0"));
        assert!(joined(&both.args).ends_with("-pixel_format nv12 -i 1:0"));
        assert_eq!((both.video, both.audio), ("0:v", "0:a"));
        let camera_only = avfoundation_input("default", None);
        assert!(joined(&camera_only.args).ends_with("-i default:none"));
    }
}
