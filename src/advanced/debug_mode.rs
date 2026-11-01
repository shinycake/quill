//! Debug log mode, after tdesktop's `debugmode` code (`Logs::DebugEnabled`,
//! `Application::switchDebugMode`, the `tdata/withdebug` flag file).
//!
//! The mode is off by default. The `withdebug` file next to the app data
//! keeps it across launches, and `--debug` on the command line forces it for
//! one run (tdesktop's `-debug`). While it is on, Quill writes its own debug
//! lines to `logs/quill-debug.log` and points TDLib's native log at
//! `logs/tdlib.log`; normally the native stream is emptied so nothing leaves
//! TDLib (`TdJson::secure_native_logging`).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// The flag file that keeps the mode on or off across launches.
pub const FLAG_FILE: &str = "withdebug";
/// TDLib verbosity while the mode is on: 4 is debug, which includes the
/// connection and network events tdesktop's debug logs are for.
pub const DEBUG_TDLIB_VERBOSITY: i32 = 4;
/// TDLib rotates its own log at this size.
const TDLIB_LOG_MAX_BYTES: u64 = 64 * 1024 * 1024;
/// Quill's log is moved to `.old` past this size.
const QUILL_LOG_MAX_BYTES: u64 = 16 * 1024 * 1024;
const ALWAYS_ON_LOG_MAX_BYTES: u64 = 1024 * 1024;

static ENABLED: OnceLock<AtomicBool> = OnceLock::new();

fn state() -> &'static AtomicBool {
    ENABLED.get_or_init(|| {
        let forced = forced_by_args(std::env::args());
        let stored = crate::settings::safe_app_root().is_some_and(|root| read_flag(&root));
        AtomicBool::new(forced || stored)
    })
}

/// Whether the debug logs are on right now.
pub fn enabled() -> bool {
    state().load(Ordering::Relaxed)
}

/// Switch the logs on or off for this run (see [`persist`] for next time).
pub fn set_enabled(on: bool) {
    state().store(on, Ordering::Relaxed);
}

/// `--debug` forces the mode on for the run.
pub fn forced_by_args<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter().any(|arg| arg.as_ref() == "--debug")
}

/// Where the logs live, inside the app data root.
pub fn log_dir(root: &Path) -> PathBuf {
    root.join("logs")
}

pub fn quill_log_path(root: &Path) -> PathBuf {
    log_dir(root).join("quill-debug.log")
}

/// The always-on call log: connection steps, whether the other side's
/// sound arrived, and the call engine's warnings. No message content.
pub fn call_log_path(root: &Path) -> PathBuf {
    log_dir(root).join("calls.log")
}

pub fn platform_log_path(root: &Path) -> PathBuf {
    log_dir(root).join("platform.log")
}

pub fn tdlib_log_path(root: &Path) -> PathBuf {
    log_dir(root).join("tdlib.log")
}

/// The flag file's meaning: `0` is off, anything else is on, no file is off
/// (tdesktop `ComputeDebugMode`).
pub fn read_flag(root: &Path) -> bool {
    std::fs::read(root.join(FLAG_FILE))
        .ok()
        .and_then(|bytes| bytes.first().copied())
        .is_some_and(|first| first != b'0')
}

/// Remember the mode for the next launch (`WriteDebugModeSetting`).
pub fn persist(root: &Path, on: bool) -> std::io::Result<()> {
    std::fs::create_dir_all(root)?;
    std::fs::write(root.join(FLAG_FILE), if on { "1" } else { "0" })
}

/// The two synchronous TDLib requests that send its native log to a file.
pub fn tdlib_file_stream_requests(path: &Path) -> [String; 2] {
    [
        serde_json::json!({
            "@type": "setLogStream",
            "log_stream": {
                "@type": "logStreamFile",
                "path": path.to_string_lossy(),
                "max_file_size": TDLIB_LOG_MAX_BYTES,
                "redirect_stderr": false,
            },
        })
        .to_string(),
        serde_json::json!({
            "@type": "setLogVerbosityLevel",
            "new_verbosity_level": DEBUG_TDLIB_VERBOSITY,
        })
        .to_string(),
    ]
}

/// Point TDLib's native log at the debug file. Fails closed: a rejected
/// request puts the empty stream back before reporting the error.
pub fn start_tdlib_log(api: &crate::telegram::ffi::TdJson, root: &Path) -> Result<(), String> {
    let dir = log_dir(root);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for request in tdlib_file_stream_requests(&tdlib_log_path(root)) {
        let ok = matches!(api.execute(&request), Ok(Some(body)) if body.contains("\"ok\""));
        if !ok {
            let _ = api.secure_native_logging();
            return Err("TDLib did not accept the log stream".into());
        }
    }
    Ok(())
}

/// Back to the closed native stream (tdesktop restarts here; TDLib can drop
/// the file stream in place).
pub fn stop_tdlib_log(
    api: &crate::telegram::ffi::TdJson,
) -> Result<(), crate::telegram::ffi::TdJsonError> {
    api.secure_native_logging()
}

/// `DEBUG_LOG`: one line in Quill's debug log while the mode is on.
pub fn debug_log(message: &str) {
    if !enabled() {
        return;
    }
    let Some(root) = crate::settings::safe_app_root() else {
        return;
    };
    append_line(&quill_log_path(&root), message, QUILL_LOG_MAX_BYTES);
}

/// One line in the call log, whether or not the debug mode is on, and in
/// the debug log while it is: a call that goes wrong can be diagnosed
/// without being repeated.
pub fn call_log(message: &str) {
    debug_log(message);
    let Some(root) = crate::settings::safe_app_root() else {
        return;
    };
    append_line(&call_log_path(&root), message, ALWAYS_ON_LOG_MAX_BYTES);
}

pub fn platform_log(message: &str) {
    debug_log(message);
    let Some(root) = crate::settings::safe_app_root() else {
        return;
    };
    append_line(&platform_log_path(&root), message, ALWAYS_ON_LOG_MAX_BYTES);
}

fn append_line(path: &Path, message: &str, max_bytes: u64) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::metadata(path).is_ok_and(|m| m.len() > max_bytes) {
        let _ = std::fs::rename(path, path.with_extension("log.old"));
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| format!("{}.{:03}", d.as_secs(), d.subsec_millis()))
        .unwrap_or_else(|_| "0".into());
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "[{stamp}] {message}");
    }
}

/// The confirm text of the `debugmode` code (tdesktop wording, plus where
/// the files go because Quill keeps them in its data folder).
pub fn confirm_text(currently_on: bool) -> &'static str {
    if currently_on {
        "Do you want to disable DEBUG logs?"
    } else {
        "Do you want to enable DEBUG logs?\n\nAll network events will be logged. The logs can contain account data and stay in the logs folder on this device."
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("quill-debug-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_flag_file_reads_like_tdesktop() {
        let dir = temp_dir("flag");
        assert!(!read_flag(&dir), "no file means off");
        persist(&dir, true).unwrap();
        assert!(read_flag(&dir));
        persist(&dir, false).unwrap();
        assert!(!read_flag(&dir));
        std::fs::write(dir.join(FLAG_FILE), "x").unwrap();
        assert!(read_flag(&dir), "anything but 0 is on");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_debug_argument_forces_the_mode() {
        assert!(forced_by_args(["quill", "--debug"]));
        assert!(!forced_by_args(["quill", "--debugger"]));
        assert!(!forced_by_args(["quill"]));
    }

    #[test]
    fn tdlib_gets_a_file_stream_then_the_debug_level() {
        let [stream, level] = tdlib_file_stream_requests(Path::new("/tmp/logs/tdlib.log"));
        let stream: serde_json::Value = serde_json::from_str(&stream).unwrap();
        assert_eq!(stream["@type"], "setLogStream");
        assert_eq!(stream["log_stream"]["@type"], "logStreamFile");
        assert_eq!(stream["log_stream"]["path"], "/tmp/logs/tdlib.log");
        assert_eq!(stream["log_stream"]["redirect_stderr"], false);
        assert!(stream["log_stream"]["max_file_size"].as_u64().unwrap() > 0);
        let level: serde_json::Value = serde_json::from_str(&level).unwrap();
        assert_eq!(level["@type"], "setLogVerbosityLevel");
        assert_eq!(level["new_verbosity_level"], DEBUG_TDLIB_VERBOSITY);
    }

    /// The real TDLib: the file stream takes the debug level, writes the log
    /// and goes back to the closed stream. No network and no account. Run with
    /// `QUILL_TDJSON_PATH=<libtdjson> cargo test --lib tdlib_writes_its_log -- --ignored`.
    #[test]
    #[ignore = "needs libtdjson (QUILL_TDJSON_PATH)"]
    fn tdlib_writes_its_log_while_the_mode_is_on() {
        let Ok(api) = crate::telegram::ffi::TdJson::load_default() else {
            eprintln!("libtdjson not found; skipped");
            return;
        };
        api.secure_native_logging().unwrap();
        let dir = temp_dir("tdlib");
        start_tdlib_log(&api, &dir).unwrap();
        let client = api.create_client_id();
        api.send(client, r#"{"@type":"getAuthorizationState"}"#)
            .unwrap();
        for _ in 0..20 {
            if api.receive(0.2).is_some() {
                break;
            }
        }
        stop_tdlib_log(&api).unwrap();
        let len = std::fs::metadata(tdlib_log_path(&dir))
            .map(|m| m.len())
            .unwrap_or(0);
        assert!(len > 0, "the debug log file stayed empty");
        let before = len;
        // Closed again: nothing more is written.
        api.send(client, r#"{"@type":"getAuthorizationState"}"#)
            .unwrap();
        let _ = api.receive(0.5);
        let after = std::fs::metadata(tdlib_log_path(&dir))
            .map(|m| m.len())
            .unwrap_or(0);
        assert_eq!(before, after, "the stream was not closed");
        // TDLib's exit destructors race a client that is still open.
        api.send(client, r#"{"@type":"close"}"#).unwrap();
        for _ in 0..100 {
            if api
                .receive(0.2)
                .is_some_and(|m| m.contains("authorizationStateClosed"))
            {
                break;
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn log_lines_append_and_rotate() {
        let dir = temp_dir("lines");
        let path = quill_log_path(&dir);
        append_line(&path, "first", QUILL_LOG_MAX_BYTES);
        append_line(&path, "second", QUILL_LOG_MAX_BYTES);
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 2);
        assert!(text.lines().next().unwrap().ends_with("] first"));
        append_line(&path, "third", 8);
        assert!(
            std::fs::read_to_string(path.with_extension("log.old"))
                .unwrap()
                .contains("] second")
        );
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 1, "a full log moves aside");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_confirm_text_follows_the_state() {
        assert!(confirm_text(false).starts_with("Do you want to enable DEBUG logs?"));
        assert!(confirm_text(false).contains("All network events will be logged."));
        assert_eq!(confirm_text(true), "Do you want to disable DEBUG logs?");
    }
}
