//! Locating the external media tools (`ffmpeg`, `ffprobe`).
//!
//! An app opened from Finder or the Dock inherits launchd's minimal `PATH`
//! (`/usr/bin:/bin:/usr/sbin:/sbin`), which misses Homebrew and MacPorts,
//! so a bare `Command::new("ffmpeg")` fails there even when it is installed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{LazyLock, Mutex};

/// Package-manager folders searched after `PATH`.
const FALLBACK_DIRS: [&str; 3] = ["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"];

static RESOLVED: LazyLock<Mutex<HashMap<String, PathBuf>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// A `Command` for media tool `name`, resolved once per process: next to
/// the app binary (a bundled copy), then `PATH`, then the package-manager
/// folders. Falls back to the bare name, so a missing tool fails as before.
pub fn command(name: &str) -> Command {
    Command::new(resolve(name))
}

fn resolve(name: &str) -> PathBuf {
    if let Ok(cache) = RESOLVED.lock()
        && let Some(path) = cache.get(name)
    {
        return path.clone();
    }
    let beside_exe = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    let path_dirs = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    let found = find_in(
        name,
        beside_exe
            .into_iter()
            .chain(path_dirs)
            .chain(FALLBACK_DIRS.iter().map(PathBuf::from)),
    )
    .unwrap_or_else(|| PathBuf::from(name));
    if let Ok(mut cache) = RESOLVED.lock() {
        cache.insert(name.to_string(), found.clone());
    }
    found
}

fn find_in(name: &str, dirs: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    dirs.into_iter()
        .map(|dir| dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX)))
        .find(|candidate| is_executable(candidate))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// True when media tool `name` can be found (beside the app, on `PATH`, or
/// in a package-manager folder).
pub fn is_installed(name: &str) -> bool {
    resolve(name).is_file()
}

/// What to do when `ffmpeg` is not installed, for the recording that needs it.
pub fn ffmpeg_missing_message(what: &str) -> String {
    let how = if cfg!(target_os = "macos") {
        "Install it with `brew install ffmpeg`."
    } else if cfg!(windows) {
        "Install it with `winget install ffmpeg`, or put ffmpeg.exe next to Quill.exe."
    } else {
        "Install the ffmpeg package (for example `sudo apt install ffmpeg`)."
    };
    format!("{what} need ffmpeg, which isn't installed. {how}")
}

/// ffmpeg input arguments for the camera and microphone of a round video,
/// with the stream specifiers of the audio and video they produce.
pub struct CaptureInput {
    pub args: Vec<String>,
    pub audio: &'static str,
    pub video: &'static str,
}

/// The platform's camera and microphone for ffmpeg: AVFoundation's defaults
/// on macOS, the first V4L2 capture device + PulseAudio (or ALSA) on Linux,
/// the first DirectShow camera and microphone on Windows.
pub fn capture_input() -> Result<CaptureInput, String> {
    let args = |list: &[&str]| list.iter().map(|arg| arg.to_string()).collect();
    if cfg!(target_os = "macos") {
        Ok(CaptureInput {
            args: args(&[
                "-f",
                "avfoundation",
                "-framerate",
                "30",
                "-pixel_format",
                "nv12",
                "-i",
                "default:default",
            ]),
            audio: "0:a",
            video: "0:v",
        })
    } else if cfg!(target_os = "linux") {
        let camera = v4l2_capture_devices()
            .into_iter()
            .next()
            .ok_or("No camera found.")?;
        let audio = linux_audio_backend();
        Ok(CaptureInput {
            args: vec![
                "-f".into(),
                "v4l2".into(),
                "-framerate".into(),
                "15".into(),
                "-i".into(),
                camera.to_string_lossy().into_owned(),
                "-f".into(),
                audio.into(),
                "-i".into(),
                "default".into(),
            ],
            audio: "1:a",
            video: "0:v",
        })
    } else if cfg!(windows) {
        let devices = list_dshow_devices()?;
        let camera = devices.video.first().ok_or("No camera found.")?;
        let mic = devices.audio.first().ok_or("No microphone found.")?;
        Ok(CaptureInput {
            args: vec![
                "-f".into(),
                "dshow".into(),
                "-i".into(),
                format!("video={camera}:audio={mic}"),
            ],
            audio: "0:a",
            video: "0:v",
        })
    } else {
        Err("Recording isn't supported on this system yet.".into())
    }
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
    let output = command("ffmpeg")
        .args([
            "-hide_banner",
            "-list_devices",
            "true",
            "-f",
            "dshow",
            "-i",
            "dummy",
        ])
        .stdin(std::process::Stdio::null())
        .no_console()
        .output()
        .map_err(|err| format!("Couldn't run ffmpeg ({err})."))?;
    Ok(parse_dshow_devices(&String::from_utf8_lossy(
        &output.stderr,
    )))
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
        let name = line[open + 1..close].to_string();
        let tail = &lower[close..];
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

/// True when Windows' privacy settings block this device for desktop apps
/// (Settings › Privacy & security › Camera / Microphone). Reads the
/// CapabilityAccessManager consent store; `false` when it can't tell.
pub fn windows_privacy_denied(camera: bool) -> bool {
    #[cfg(windows)]
    {
        let device = if camera { "webcam" } else { "microphone" };
        let base = format!(
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\{device}"
        );
        [base.clone(), format!(r"{base}\NonPackaged")]
            .iter()
            .any(|key| {
                Command::new("reg")
                    .args(["query", key, "/v", "Value"])
                    .no_console()
                    .output()
                    .is_ok_and(|out| consent_denies(&String::from_utf8_lossy(&out.stdout)))
            })
    }
    #[cfg(not(windows))]
    {
        let _ = camera;
        false
    }
}

/// `reg query … /v Value` output with the value set to `Deny`.
pub fn consent_denies(reg_output: &str) -> bool {
    reg_output.lines().any(|line| {
        let mut parts = line.split_whitespace();
        parts.next() == Some("Value")
            && parts.next() == Some("REG_SZ")
            && parts.next() == Some("Deny")
    })
}

/// Hide the console window a child would flash on Windows GUI apps.
trait NoConsole {
    fn no_console(&mut self) -> &mut Self;
}

impl NoConsole for Command {
    fn no_console(&mut self) -> &mut Self {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            self.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        self
    }
}

/// Spawn ffmpeg for a capture: no console window on Windows, and on Windows
/// stdin piped (ffmpeg stops cleanly on `q`; elsewhere SIGINT does that).
pub fn spawn_capture(
    args: &[String],
    stdout: std::process::Stdio,
    stderr: std::process::Stdio,
) -> std::io::Result<std::process::Child> {
    let mut command = command("ffmpeg");
    command.args(args);
    command
        .stdin(if cfg!(windows) {
            std::process::Stdio::piped()
        } else {
            std::process::Stdio::null()
        })
        .stdout(stdout)
        .stderr(stderr)
        .no_console();
    command.spawn()
}

/// Why nothing was recorded.
pub fn no_audio_message(camera: bool) -> String {
    let device = if camera { "camera" } else { "microphone" };
    if cfg!(windows) {
        format!(
            "Nothing was recorded. Check that the {device} is plugged in and allowed in \
             Settings › Privacy & security."
        )
    } else if cfg!(target_os = "macos") {
        format!(
            "Nothing was recorded. Check that the {device} works and Quill may use it in \
             System Settings → Privacy & Security."
        )
    } else {
        format!("Nothing was recorded. Check that the {device} is connected and not in use.")
    }
}

/// Why a capture's ffmpeg stopped early, from the log it wrote: missing
/// permission gets the settings hint, anything else its last line.
pub fn capture_failure(log: &Path, camera: bool) -> String {
    let text = std::fs::read_to_string(log).unwrap_or_default();
    failure_from_log(&text, camera)
}

fn failure_from_log(text: &str, camera: bool) -> String {
    let lower = text.to_ascii_lowercase();
    // DirectShow reports a privacy block as a generic graph/open failure.
    let windows_blocked = cfg!(windows)
        && (lower.contains("could not run graph")
            || lower.contains("could not set video options")
            || lower.contains("error opening input"))
        && windows_privacy_denied(camera);
    if lower.contains("authoriz")
        || lower.contains("permission")
        || lower.contains("denied")
        || windows_blocked
    {
        return if cfg!(windows) {
            if camera {
                "Quill can't use the camera or microphone. Allow desktop apps in Settings › \
                 Privacy & security › Camera and Microphone."
                    .into()
            } else {
                "Quill can't use the microphone. Allow desktop apps in Settings › Privacy & \
                 security › Microphone."
                    .into()
            }
        } else if cfg!(target_os = "macos") {
            if camera {
                "Quill can't use the camera or microphone. Allow it in System Settings → \
                 Privacy & Security."
                    .into()
            } else {
                "Quill can't use the microphone. Allow it in System Settings → Privacy & \
                 Security → Microphone."
                    .into()
            }
        } else {
            let detail = last_reason(text).unwrap_or_default();
            format!(
                "Quill can't open the {}: {detail}",
                if camera { "camera" } else { "microphone" }
            )
        };
    }
    match last_reason(text) {
        Some(line) => format!("Recording failed: {line}"),
        None if camera => "Couldn't start the camera.".into(),
        None => "Couldn't start the microphone.".into(),
    }
}

/// ffmpeg prefixes lines with `[component @ 0x…] `; device format listings
/// are noise, not the reason.
fn last_reason(text: &str) -> Option<String> {
    text.lines()
        .map(|line| match line.find("] ") {
            Some(end) if line.starts_with('[') => &line[end + 2..],
            _ => line,
        })
        .rfind(|line| {
            let line = line.trim();
            !line.is_empty()
                && !line.to_ascii_lowercase().contains("pixel format")
                && line.contains(' ')
        })
        .map(|line| line.trim().to_string())
}

/// Stop a capture's ffmpeg: SIGINT (or `q` on Windows) lets it finish the
/// file, a kill drops it. A graceful stop that takes over 8 s is killed.
pub fn stop_capture(child: &mut std::process::Child, graceful: bool) {
    if graceful {
        #[cfg(unix)]
        let _ = Command::new("kill")
            .args(["-INT", &child.id().to_string()])
            .status();
        // Windows has no SIGINT: ffmpeg stops and finalizes the file on `q`
        // from stdin ([`spawn_capture`] pipes it).
        #[cfg(not(unix))]
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            let _ = stdin.write_all(b"q");
            let _ = stdin.flush();
        } else {
            let _ = child.kill();
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        while std::time::Instant::now() < deadline {
            if child.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn finds_the_first_executable_and_skips_plain_files() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("quill-media-tools-{}", std::process::id()));
        let (plain, tool) = (root.join("plain"), root.join("tool"));
        std::fs::create_dir_all(&plain).unwrap();
        std::fs::create_dir_all(&tool).unwrap();
        std::fs::write(plain.join("ffmpeg"), b"").unwrap();
        let exe = tool.join("ffmpeg");
        std::fs::write(&exe, b"#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        let missing = root.join("missing");
        assert_eq!(find_in("ffmpeg", [missing, plain, tool.clone()]), Some(exe));
        assert_eq!(find_in("ffprobe", [tool]), None);
        let _ = std::fs::remove_dir_all(root);
    }

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
    fn registry_consent_value_deny_is_detected() {
        let deny = "\nHKEY_CURRENT_USER\\Software\\X\\microphone\n    Value    REG_SZ    Deny\n";
        assert!(consent_denies(deny));
        assert!(!consent_denies(&deny.replace("Deny", "Allow")));
        assert!(!consent_denies(
            "ERROR: The system was unable to find the registry key"
        ));
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
    fn missing_ffmpeg_message_says_how_to_install() {
        let note = ffmpeg_missing_message("Video messages");
        assert!(note.starts_with("Video messages need ffmpeg"));
        assert!(note.to_lowercase().contains("install"));
    }

    #[test]
    fn capture_failure_explains_permission_and_skips_format_noise() {
        let dir = std::env::temp_dir().join(format!("quill-capture-log-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("capture.log");
        std::fs::write(
            &log,
            "[in#0 @ 0x1] Failed to create AV capture input device: Not authorized\n",
        )
        .unwrap();
        assert!(
            capture_failure(&log, false)
                .to_lowercase()
                .contains("microphone")
        );
        std::fs::write(
            &log,
            "[in#0 @ 0x1] Video device not found\n\
             [in#0 @ 0x1] Selected pixel format (yuv420p) is not supported.\n\
             [in#0 @ 0x1]   nv12\n",
        )
        .unwrap();
        assert_eq!(
            capture_failure(&log, true),
            "Recording failed: Video device not found"
        );
        std::fs::write(&log, "").unwrap();
        assert_eq!(capture_failure(&log, true), "Couldn't start the camera.");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
