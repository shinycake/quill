//! Locating the external media tools (`ffmpeg`, `ffprobe`).
//!
//! The Windows zip and the Linux tarball ship LGPL builds of both next to the
//! app (docs/decisions/sweep-ffmpeg-tools-bundled.md); those win, so Quill's
//! video editor, photo downscale and camera work on a machine that has no
//! ffmpeg. Otherwise the search follows `PATH`, then the package-manager
//! folders: an app opened from Finder or the Dock inherits launchd's minimal
//! `PATH` (`/usr/bin:/bin:/usr/sbin:/sbin`), which misses Homebrew and
//! MacPorts, so a bare `Command::new("ffmpeg")` fails there even when it is
//! installed.
#![deny(clippy::indexing_slicing)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{LazyLock, Mutex};

pub use crate::media_capture::{
    CaptureInput, avfoundation_specifiers, camera_input, capture_input, v4l2_capture_devices,
};

/// Package-manager folders searched after `PATH`.
const FALLBACK_DIRS: [&str; 3] = ["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"];

/// How long a bundled tool may take to answer `-version` before it counts as
/// broken (a missing shared library, for example).
const SELF_TEST_LIMIT: std::time::Duration = std::time::Duration::from_secs(5);

static RESOLVED: LazyLock<Mutex<HashMap<String, Vec<PathBuf>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// A `Command` for media tool `name`, resolved once per process: next to
/// the app binary (a bundled copy), then `PATH`, then the package-manager
/// folders. Falls back to the bare name, so a missing tool fails as before.
pub fn command(name: &str) -> Command {
    let mut command = Command::new(resolve(name));
    command.no_console();
    command
}

/// Resolve the tools and list the encoders on a background thread, so the
/// probes (up to 5 s each) are done before the UI thread asks.
/// Runs once per process; later calls (a second window) do nothing.
pub fn warm_up() {
    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(spawn_warm_up);
}

fn spawn_warm_up() {
    let _ = std::thread::Builder::new()
        .name("quill-media-tools".into())
        .spawn(|| {
            for name in ["ffmpeg", "ffprobe"] {
                let _ = candidates(name);
            }
            crate::video_encoder::warm_listings();
        });
}

/// The preferred copy of `name`. Never probes: before [`warm_up`] has
/// finished, a bundled copy is taken as working (a file check only), so the
/// UI thread never waits on a child process here.
fn resolve(name: &str) -> PathBuf {
    cached_or_unprobed(name)
        .into_iter()
        .next()
        .unwrap_or_else(|| PathBuf::from(name))
}

fn cached_or_unprobed(name: &str) -> Vec<PathBuf> {
    if let Ok(cache) = RESOLVED.lock()
        && let Some(found) = cache.get(name)
    {
        return found.clone();
    }
    search(name, |_| true)
}

/// Every copy of media tool `name` that can run, in the order `command`
/// prefers them. Resolved once per process; the first call probes a bundled
/// copy (up to 5 s), so call it off the UI thread (see [`warm_up`]).
pub fn candidates(name: &str) -> Vec<PathBuf> {
    if let Ok(cache) = RESOLVED.lock()
        && let Some(found) = cache.get(name)
    {
        return found.clone();
    }
    let found = search(name, runs);
    if let Ok(mut cache) = RESOLVED.lock() {
        cache.insert(name.to_string(), found.clone());
    }
    found
}

fn search(name: &str, works: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
    let beside_exe = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    let path_dirs = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    find_all(
        name,
        beside_exe.as_deref(),
        path_dirs
            .into_iter()
            .chain(FALLBACK_DIRS.iter().map(PathBuf::from)),
        works,
    )
}

/// The executables called `name`: the copy in `bundled` first, but only when
/// `works` says it starts (a bundled build whose shared library is missing
/// must not hide a good one on `PATH`), then those in `others`, in order and
/// without repeats.
fn find_all(
    name: &str,
    bundled: Option<&Path>,
    others: impl IntoIterator<Item = PathBuf>,
    works: impl Fn(&Path) -> bool,
) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    let bundled_copy = bundled
        .and_then(|dir| find_in(name, [dir.to_path_buf()]))
        .filter(|path| works(path));
    for path in bundled_copy
        .into_iter()
        .chain(others.into_iter().filter_map(|dir| find_in(name, [dir])))
    {
        if !found.contains(&path) {
            found.push(path);
        }
    }
    found
}

fn find_in(name: &str, dirs: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    dirs.into_iter()
        .map(|dir| dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX)))
        .find(|candidate| is_executable(candidate))
}

/// True when `tool -version` exits successfully within [`SELF_TEST_LIMIT`].
fn runs(tool: &Path) -> bool {
    use std::process::Stdio;
    let Ok(mut child) = Command::new(tool)
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .no_console()
        .spawn()
    else {
        return false;
    };
    let deadline = std::time::Instant::now() + SELF_TEST_LIMIT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
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
    let os = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(windows) {
        "windows"
    } else {
        "linux"
    };
    missing_message(what, os)
}

fn missing_message(what: &str, os: &str) -> String {
    let how = match os {
        "macos" => "Install it with `brew install ffmpeg`.",
        "windows" => {
            "Quill ships ffmpeg.exe next to Quill.exe; unzip the whole folder, or install \
             it with `winget install ffmpeg`."
        }
        _ => {
            "Quill ships ffmpeg and ffprobe in its folder; run it from there, or install the \
             ffmpeg package (for example `sudo apt install ffmpeg`)."
        }
    };
    format!("{what} need ffmpeg, which isn't installed. {how}")
}

/// True when Windows' privacy settings block this device for desktop apps
/// (Settings › Privacy & security › Camera / Microphone). Reads the
/// CapabilityAccessManager consent store in process, as tdesktop does;
/// `false` when it can't tell.
pub fn windows_privacy_denied(camera: bool) -> bool {
    #[cfg(windows)]
    {
        any_key_denies(&consent_store_keys(camera))
    }
    #[cfg(not(windows))]
    {
        let _ = camera;
        false
    }
}

#[cfg(windows)]
fn consent_store_keys(camera: bool) -> [String; 2] {
    let device = if camera { "webcam" } else { "microphone" };
    let base = format!(
        r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\{device}"
    );
    [base.clone(), format!(r"{base}\NonPackaged")]
}

#[cfg(windows)]
fn any_key_denies(keys: &[String]) -> bool {
    keys.iter().any(|key| {
        consent_denies(
            crate::winreg::get_string(key, "Value")
                .ok()
                .flatten()
                .as_deref(),
        )
    })
}

/// The consent store's `Value` string blocks the device; a missing value
/// does not.
pub fn consent_denies(value: Option<&str>) -> bool {
    value == Some("Deny")
}

/// Same as [`command`], which already hides the console window on Windows.
pub fn quiet_command(name: &str) -> Command {
    command(name)
}

/// Hide the console window a child would flash on Windows GUI apps.
pub trait NoConsole {
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
    program: &Path,
    args: &[String],
    stdout: std::process::Stdio,
    stderr: std::process::Stdio,
) -> std::io::Result<std::process::Child> {
    let mut command = Command::new(program);
    command.args(args);
    answer_sigint(&mut command);
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

/// Make the child answer SIGINT whatever Quill's own disposition is: an
/// ignored signal stays ignored across exec, and a stop that ffmpeg never
/// sees only ends in the kill fallback with a dropped file.
#[cfg(unix)]
fn answer_sigint(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    // SAFETY: `signal` is async-signal-safe and touches no Rust state, so
    // it may run between fork and exec.
    unsafe {
        command.pre_exec(|| {
            libc::signal(libc::SIGINT, libc::SIG_DFL);
            Ok(())
        });
    }
}

#[cfg(not(unix))]
fn answer_sigint(_: &mut Command) {}

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
            Some(end) if line.starts_with('[') => line.get(end + 2..).unwrap_or_default(),
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
        if let Ok(pid) = i32::try_from(child.id()) {
            // SAFETY: plain syscall on the id of our own live child; it
            // dereferences nothing. A failure falls through to the deadline.
            unsafe { libc::kill(pid, libc::SIGINT) };
        }
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
    fn graceful_stop_interrupts_the_child_without_a_kill_binary() {
        use std::os::unix::process::ExitStatusExt;
        let mut command = Command::new("sleep");
        command.arg("30");
        // Another test in this binary leaves SIGINT ignored, which a child
        // inherits: the same situation `answer_sigint` guards the app from.
        answer_sigint(&mut command);
        let mut child = command.spawn().unwrap();
        stop_capture(&mut child, true);
        // SIGINT ended it, not the SIGKILL the 8 s deadline falls back to.
        let status = child.wait().unwrap();
        assert_eq!(status.signal(), Some(libc::SIGINT), "{status:?}");
    }

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

    /// An executable file `name` (plus the platform's suffix) in a fresh dir.
    fn fake_tool(root: &Path, dir: &str, name: &str) -> PathBuf {
        let dir = root.join(dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        std::fs::write(&path, b"#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        path
    }

    fn scratch(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("quill-tools-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn the_bundled_copy_comes_before_path() {
        let root = scratch("order");
        let bundled = fake_tool(&root, "app", "ffprobe");
        let on_path = fake_tool(&root, "path", "ffprobe");
        let found = find_all(
            "ffprobe",
            bundled.parent(),
            [on_path.parent().unwrap().to_path_buf()],
            |_| true,
        );
        assert_eq!(found, vec![bundled, on_path]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_bundled_copy_that_cannot_start_does_not_hide_path() {
        let root = scratch("broken");
        let bundled = fake_tool(&root, "app", "ffmpeg");
        let on_path = fake_tool(&root, "path", "ffmpeg");
        let found = find_all(
            "ffmpeg",
            bundled.parent(),
            [on_path.parent().unwrap().to_path_buf()],
            |path| path != bundled,
        );
        assert_eq!(found, vec![on_path]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn path_alone_is_used_when_nothing_is_bundled_and_repeats_collapse() {
        let root = scratch("path");
        let on_path = fake_tool(&root, "path", "ffmpeg");
        let dir = on_path.parent().unwrap().to_path_buf();
        let found = find_all(
            "ffmpeg",
            Some(&root.join("empty")),
            [dir.clone(), dir],
            |_| true,
        );
        assert_eq!(found, vec![on_path]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_missing_tool_resolves_to_its_bare_name_and_is_not_installed() {
        let root = scratch("missing");
        assert!(find_all("ffmpeg", Some(&root), [root.join("nope")], |_| true).is_empty());
        assert_eq!(
            resolve("quill-no-such-media-tool"),
            PathBuf::from("quill-no-such-media-tool")
        );
        assert!(!is_installed("quill-no-such-media-tool"));
        let _ = std::fs::remove_dir_all(root);
    }

    /// The stand-in tools are checked in with their executable bit, never
    /// written here: a child another test thread is starting shares this
    /// process's open files until its exec, so a script started right after
    /// it was written failed now and then with `ETXTBSY` ("Text file busy").
    #[cfg(unix)]
    #[test]
    fn a_tool_that_exits_with_an_error_or_hangs_fails_the_self_test() {
        let fixture = |name: &str| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/media-tools")
                .join(name)
        };
        assert!(runs(&fixture("good.sh")));
        assert!(!runs(&fixture("bad.sh")));
        assert!(!runs(&fixture("absent")));
    }

    #[test]
    fn the_missing_message_names_the_bundled_tool_where_one_ships() {
        let windows = missing_message("Video messages", "windows");
        assert!(windows.starts_with("Video messages need ffmpeg"));
        assert!(windows.contains("ffmpeg.exe next to Quill.exe"));
        assert!(windows.contains("winget"));
        assert!(missing_message("The camera", "macos").contains("brew install ffmpeg"));
        let linux = missing_message("The camera", "linux");
        assert!(linux.contains("ships ffmpeg and ffprobe") && linux.contains("apt install"));
    }

    #[test]
    fn registry_consent_value_deny_is_detected() {
        assert!(consent_denies(Some("Deny")));
        assert!(!consent_denies(Some("Allow")));
        assert!(!consent_denies(None));
    }

    #[cfg(windows)]
    #[test]
    fn consent_store_value_is_read_in_process() {
        let subkey = format!(r"Software\Quill\Tests\consent-{}", std::process::id());
        let missing = format!(r"{subkey}\Missing");
        let keys = [subkey.clone(), missing];
        assert!(!any_key_denies(&keys));
        crate::winreg::set_string(&subkey, "Value", "Allow").unwrap();
        assert!(!any_key_denies(&keys));
        crate::winreg::set_string(&subkey, "Value", "Deny").unwrap();
        assert!(any_key_denies(&keys));
        crate::winreg::delete_tree(&subkey).unwrap();
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
