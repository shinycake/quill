use libloading::Library;
use std::ffi::{CStr, CString, c_char, c_int};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

const ENV_PATH: &str = "QUILL_TDJSON_PATH";

/// Verbosity ceiling for TDLib's native logging. Matches the callback's
/// max_verbosity in `install_redacted_log`; the stream itself is emptied
/// by `secure_native_logging`, this is defense in depth.
pub const MAX_NATIVE_LOG_VERBOSITY: i32 = 1;

#[derive(Debug, Error)]
pub enum TdJsonError {
    #[error("tdjson library not found")]
    NotFound,
    #[error("tdjson library could not be loaded")]
    Load,
    #[error("tdjson symbol missing: {0}")]
    MissingSymbol(&'static str),
    #[error("request is not valid UTF-8 / CString")]
    InvalidRequest,
    #[error("TDLib native log stream could not be secured")]
    LogStreamNotSecured,
    #[error("tdjson is TDLib {found}, this build needs {expected}")]
    VersionMismatch {
        found: String,
        expected: &'static str,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryOrigin {
    EnvPath(PathBuf),
    Bundled(PathBuf),
}

pub fn resolve_tdjson_path() -> Option<LibraryOrigin> {
    if let Ok(path) = std::env::var(ENV_PATH) {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(LibraryOrigin::EnvPath(path));
        }
    }
    if let Some(path) = bundled_candidate()
        && path.is_file()
    {
        return Some(LibraryOrigin::Bundled(path));
    }
    None
}

fn bundled_candidate() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let names = [
        "libtdjson.dylib",
        "libtdjson.so",
        "tdjson.dll",
        "libtdjson.1.8.67.dylib",
    ];
    let search_dirs = [
        dir.to_path_buf(),
        dir.join("lib"),
        dir.join("Frameworks"),
        dir.join("../Frameworks"),
        dir.join("../Resources/tdjson"),
    ];
    for search in search_dirs {
        for name in names {
            let candidate = search.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

pub fn loaded_library_origin() -> Option<LibraryOrigin> {
    resolve_tdjson_path()
}

pub struct TdJson {
    _library: Library,
    create_client_id: unsafe extern "C" fn() -> c_int,
    send: unsafe extern "C" fn(c_int, *const c_char),
    receive: unsafe extern "C" fn(f64) -> *const c_char,
    execute: unsafe extern "C" fn(*const c_char) -> *const c_char,
    set_log_message_callback:
        unsafe extern "C" fn(c_int, Option<unsafe extern "C" fn(c_int, *const c_char)>),
}

impl TdJson {
    pub fn load_from(path: &Path) -> Result<Self, TdJsonError> {
        // SAFETY: loading tdjson runs its initializers, which only set up
        // TDLib's own static state; the path is the bundled or configured tdjson.
        let library = unsafe { Library::new(path) }.map_err(|_| TdJsonError::Load)?;
        // SAFETY: each field's type is the signature td_json_client.h
        // declares for that symbol, and `_library` keeps the library loaded
        // for as long as the copied function pointers live.
        unsafe {
            Ok(Self {
                create_client_id: *library
                    .get(b"td_create_client_id\0")
                    .map_err(|_| TdJsonError::MissingSymbol("td_create_client_id"))?,
                send: *library
                    .get(b"td_send\0")
                    .map_err(|_| TdJsonError::MissingSymbol("td_send"))?,
                receive: *library
                    .get(b"td_receive\0")
                    .map_err(|_| TdJsonError::MissingSymbol("td_receive"))?,
                execute: *library
                    .get(b"td_execute\0")
                    .map_err(|_| TdJsonError::MissingSymbol("td_execute"))?,
                set_log_message_callback: *library
                    .get(b"td_set_log_message_callback\0")
                    .map_err(|_| TdJsonError::MissingSymbol("td_set_log_message_callback"))?,
                _library: library,
            })
        }
    }

    pub fn load_default() -> Result<Self, TdJsonError> {
        let origin = resolve_tdjson_path().ok_or(TdJsonError::NotFound)?;
        let path = match origin {
            LibraryOrigin::EnvPath(path) | LibraryOrigin::Bundled(path) => path,
        };
        Self::load_from(&path)
    }

    /// Install a callback that never forwards the native message body.
    /// Native TDLib methods must not be called from this callback.
    pub fn install_redacted_log(&self, max_verbosity: i32) {
        // SAFETY: `redacted_native_log` is a valid `extern "C"` callback
        // for the process lifetime; it only bumps an atomic and cannot unwind.
        unsafe {
            (self.set_log_message_callback)(max_verbosity, Some(redacted_native_log));
        }
    }

    /// Point TDLib's *native* log stream at nothing and drop its verbosity
    /// before any client exists. `install_redacted_log` only registers a
    /// callback; TDLib still writes every message to its default stream
    /// (stderr) via do_append first. Fails closed: callers must not create
    /// a client when this returns Err.
    pub fn secure_native_logging(&self) -> Result<(), TdJsonError> {
        let stream = r#"{"@type":"setLogStream","log_stream":{"@type":"logStreamEmpty"}}"#;
        let verbosity = format!(
            r#"{{"@type":"setLogVerbosityLevel","new_verbosity_level":{}}}"#,
            MAX_NATIVE_LOG_VERBOSITY
        );
        for request in [stream, verbosity.as_str()] {
            match self.execute(request)? {
                Some(body) if is_ok_response(&body) => {}
                _ => return Err(TdJsonError::LogStreamNotSecured),
            }
        }
        Ok(())
    }

    /// Refuse a `libtdjson` other than the pinned one before any client
    /// opens the database. An older TDLib reading a database a newer one
    /// wrote can abort on options it doesn't know.
    pub fn check_pinned_version(&self) -> Result<(), TdJsonError> {
        let request = r#"{"@type":"getOption","name":"version"}"#;
        let found = self
            .execute(request)?
            .and_then(|body| option_string_value(&body))
            .unwrap_or_default();
        let expected = crate::pins::TDLIB_CMAKE_VERSION;
        if found == expected {
            Ok(())
        } else {
            Err(TdJsonError::VersionMismatch { found, expected })
        }
    }

    pub fn native_log_count() -> u64 {
        NATIVE_LOG_COUNT.load(Ordering::Relaxed)
    }

    pub fn create_client_id(&self) -> i32 {
        // SAFETY: `td_create_client_id` takes no arguments and is thread-safe.
        unsafe { (self.create_client_id)() }
    }

    pub fn send(&self, client_id: i32, request: &str) -> Result<(), TdJsonError> {
        let c = CString::new(request).map_err(|_| TdJsonError::InvalidRequest)?;
        // SAFETY: `c` is a NUL-terminated string alive for the call; `td_send`
        // copies it before returning and is thread-safe.
        unsafe { (self.send)(client_id, c.as_ptr()) };
        Ok(())
    }

    /// End a `td_receive` in progress, or the next one: TDLib answers a
    /// request to a client id it never made at once, with an error.
    pub fn wake_receive(&self) {
        const NEVER_MADE: i32 = 0;
        let _ = self.send(NEVER_MADE, r#"{"@type":"getOption","name":"version"}"#);
    }

    /// Copy the received C string before the next `td_receive`/`td_execute` call.
    pub fn receive(&self, timeout: f64) -> Option<String> {
        // SAFETY: `td_receive` takes a plain timeout; Quill calls it from one
        // receive thread at a time, as TDLib requires.
        let ptr = unsafe { (self.receive)(timeout) };
        if ptr.is_null() {
            return None;
        }
        // SAFETY: non-null `td_receive` results are NUL-terminated and valid
        // until the next `td_receive`/`td_execute`; we copy before returning.
        let copied = unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned();
        Some(copied)
    }

    pub fn execute(&self, request: &str) -> Result<Option<String>, TdJsonError> {
        let c = CString::new(request).map_err(|_| TdJsonError::InvalidRequest)?;
        // SAFETY: `c` is a NUL-terminated request alive for the call.
        let ptr = unsafe { (self.execute)(c.as_ptr()) };
        if ptr.is_null() {
            return Ok(None);
        }
        // SAFETY: non-null `td_execute` results are NUL-terminated and valid
        // until the next `td_execute` on this thread; we copy before returning.
        let copied = unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned();
        Ok(Some(copied))
    }
}

impl Drop for TdJson {
    fn drop(&mut self) {
        // Clear the native log callback before `dlclose`. Do not call other
        // TDLib methods from a log callback; this runs on the owner thread
        // after the receive loop has joined.
        // SAFETY: clearing the callback with `None` is always allowed and
        // the library is still loaded (`_library` drops after this).
        unsafe {
            (self.set_log_message_callback)(0, None);
        }
    }
}

/// The verbosity TDLib logs fatal errors at.
const FATAL_LOG_VERBOSITY: c_int = 0;

static NATIVE_LOG_COUNT: AtomicU64 = AtomicU64::new(0);

fn is_ok_response(body: &str) -> bool {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else {
        return false;
    };
    v.get("@type")
        .and_then(|t| t.as_str())
        .is_some_and(|t| t == "ok")
}

fn option_string_value(body: &str) -> Option<String> {
    let v = serde_json::from_str::<serde_json::Value>(body).ok()?;
    if v.get("@type")?.as_str()? != "optionValueString" {
        return None;
    }
    Some(v.get("value")?.as_str()?.to_owned())
}

unsafe extern "C" fn redacted_native_log(verbosity: c_int, _message: *const c_char) {
    // Do not call any TDLib function from this callback.
    // Do not inspect `_message`; native logs can contain account data.
    NATIVE_LOG_COUNT.fetch_add(1, Ordering::Relaxed);
    if verbosity == FATAL_LOG_VERBOSITY {
        // TDLib aborts once this returns. Leave a breadcrumb (no message
        // text) so the next launch can say what happened.
        crate::thread_guard::ffi_boundary((), crate::crash_log::note_native_fatal);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_option_string_values() {
        let body = r#"{"@type":"optionValueString","value":"1.8.69"}"#;
        assert_eq!(option_string_value(body).as_deref(), Some("1.8.69"));
        let error = r#"{"@type":"error","code":400,"message":"Option not found"}"#;
        assert_eq!(option_string_value(error), None);
        assert_eq!(option_string_value("not json"), None);
    }

    #[test]
    fn does_not_search_homebrew_paths() {
        // Bundled lookup is relative to the executable, never /opt/homebrew.
        if let Some(path) = bundled_candidate() {
            let rendered = path.to_string_lossy();
            assert!(!rendered.contains("/opt/homebrew"));
            assert!(!rendered.contains("/usr/local/opt"));
        }
    }

    /// Synthetic canary, never a real credential.
    const LOG_CANARY: &str = "QUILL_CANARY_7f3a9c2e5b1d4a6f";

    /// Exercises the REAL native log stream (stderr), not just the callback.
    /// Re-execs the test binary as a child so the child's stderr can be
    /// captured; skips gracefully when no tdjson library is present.
    #[test]
    fn native_log_stream_swallows_canary() {
        if let Ok(mode) = std::env::var("QUILL_TEST_NATIVE_LOG_CHILD") {
            native_log_child_main(&mode);
        }
        let exe = std::env::current_exe().expect("test executable");
        for mode in ["control", "secured"] {
            let out = std::process::Command::new(&exe)
                .env("QUILL_TEST_NATIVE_LOG_CHILD", mode)
                .args([
                    "--exact",
                    "telegram::ffi::tests::native_log_stream_swallows_canary",
                    // Bypass libtest's output capture so the child's real
                    // stderr (TDLib's native stream) reaches the parent.
                    "--nocapture",
                ])
                .output()
                .expect("spawn child test");
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.contains("QUILL_TEST_NO_TDJSON") {
                eprintln!("skipping native log stream test: no tdjson library");
                return;
            }
            assert!(out.status.success(), "child {mode} exited badly: {stderr}");
            let leaked = stderr.contains(LOG_CANARY);
            match mode {
                // Default stream (stderr) + default verbosity: the canary
                // must appear, proving the test exercises the real stream.
                "control" => assert!(
                    leaked,
                    "control child did not log the canary — test is vacuous"
                ),
                "secured" => assert!(
                    !leaked,
                    "canary leaked to native stderr despite secure_native_logging"
                ),
                other => panic!("unknown child mode {other}"),
            }
        }
    }

    #[expect(
        clippy::exit,
        reason = "the re-executed child test process reports its result through its exit status"
    )]
    fn native_log_child_main(mode: &str) -> ! {
        let api = match TdJson::load_default() {
            Ok(api) => api,
            Err(_) => {
                eprintln!("QUILL_TEST_NO_TDJSON");
                std::process::exit(0);
            }
        };
        if mode == "secured" {
            // The exact sequence TdHub::shared runs.
            if api.secure_native_logging().is_err() {
                eprintln!("QUILL_TEST_SETUP_FAILED");
                std::process::exit(2);
            }
            api.install_redacted_log(MAX_NATIVE_LOG_VERBOSITY);
        }
        let client_id = api.create_client_id();
        let request =
            format!(r#"{{"@type":"getTextEntities","text":"{LOG_CANARY}","@extra":"canary"}}"#);
        let _ = api.send(client_id, &request);
        // td_send is async; give TDLib's logging a moment to reach stderr.
        std::thread::sleep(std::time::Duration::from_secs(2));
        std::process::exit(0);
    }
}
