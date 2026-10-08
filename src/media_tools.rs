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
        .map(|dir| dir.join(name))
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

/// ffmpeg input arguments for the default microphone (and camera), with
/// the stream specifiers of the audio (and video) they produce.
pub struct CaptureInput {
    pub args: Vec<String>,
    pub audio: &'static str,
    pub video: &'static str,
}

/// The platform's capture devices for ffmpeg: AVFoundation's default
/// camera and microphone on macOS, V4L2 + PulseAudio on Linux.
pub fn capture_input(camera: bool) -> Result<CaptureInput, String> {
    let args = |list: &[&str]| list.iter().map(|arg| arg.to_string()).collect();
    if cfg!(target_os = "macos") {
        Ok(if camera {
            CaptureInput {
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
            }
        } else {
            CaptureInput {
                args: args(&["-f", "avfoundation", "-i", ":default"]),
                audio: "0:a",
                video: "",
            }
        })
    } else if cfg!(target_os = "linux") {
        const CAMERA: &str = "/dev/video0";
        if camera && !Path::new(CAMERA).exists() {
            return Err("No camera found.".into());
        }
        Ok(if camera {
            CaptureInput {
                args: args(&[
                    "-f",
                    "v4l2",
                    "-framerate",
                    "15",
                    "-i",
                    CAMERA,
                    "-f",
                    "pulse",
                    "-i",
                    "default",
                ]),
                audio: "1:a",
                video: "0:v",
            }
        } else {
            CaptureInput {
                args: args(&["-f", "pulse", "-i", "default"]),
                audio: "0:a",
                video: "",
            }
        })
    } else {
        Err("Recording isn't supported on this system yet.".into())
    }
}

/// Why a capture's ffmpeg stopped early, from the log it wrote: missing
/// permission gets the settings hint, anything else its last line.
pub fn capture_failure(log: &Path, camera: bool) -> String {
    let text = std::fs::read_to_string(log).unwrap_or_default();
    let lower = text.to_ascii_lowercase();
    if lower.contains("authoriz") || lower.contains("permission") || lower.contains("denied") {
        return if camera {
            "Quill can't use the camera or microphone. Allow it in System Settings → \
             Privacy & Security."
                .into()
        } else {
            "Quill can't use the microphone. Allow it in System Settings → Privacy & \
             Security → Microphone."
                .into()
        };
    }
    // ffmpeg prefixes lines with `[component @ 0x…] `; device format
    // listings are noise, not the reason.
    let reason = text
        .lines()
        .map(|line| match line.find("] ") {
            Some(end) if line.starts_with('[') => &line[end + 2..],
            _ => line,
        })
        .rfind(|line| {
            let line = line.trim();
            !line.is_empty()
                && !line.to_ascii_lowercase().contains("pixel format")
                && line.contains(' ')
        });
    match reason {
        Some(line) => format!("Recording failed: {}", line.trim()),
        None if camera => "Couldn't start the camera.".into(),
        None => "Couldn't start the microphone.".into(),
    }
}

/// Stop a capture's ffmpeg: SIGINT lets it finish the file, a kill drops it.
pub fn stop_capture(child: &mut std::process::Child, graceful: bool) {
    if graceful {
        let _ = Command::new("kill")
            .args(["-INT", &child.id().to_string()])
            .status();
    } else {
        let _ = child.kill();
    }
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
    fn capture_failure_explains_permission_and_skips_format_noise() {
        let dir = std::env::temp_dir().join(format!("quill-capture-log-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("capture.log");
        std::fs::write(
            &log,
            "[in#0 @ 0x1] Failed to create AV capture input device: Not authorized\n",
        )
        .unwrap();
        assert!(capture_failure(&log, false).contains("Microphone"));
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
