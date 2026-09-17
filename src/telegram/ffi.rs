use libloading::Library;
use std::ffi::{CStr, CString, c_char, c_int};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

const ENV_PATH: &str = "QUILL_TDJSON_PATH";

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
        let library = unsafe { Library::new(path) }.map_err(|_| TdJsonError::Load)?;
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
        unsafe {
            (self.set_log_message_callback)(max_verbosity, Some(redacted_native_log));
        }
    }

    pub fn native_log_count() -> u64 {
        NATIVE_LOG_COUNT.load(Ordering::Relaxed)
    }

    pub fn create_client_id(&self) -> i32 {
        unsafe { (self.create_client_id)() }
    }

    pub fn send(&self, client_id: i32, request: &str) -> Result<(), TdJsonError> {
        let c = CString::new(request).map_err(|_| TdJsonError::InvalidRequest)?;
        unsafe { (self.send)(client_id, c.as_ptr()) };
        Ok(())
    }

    /// Copy the received C string before the next `td_receive`/`td_execute` call.
    pub fn receive(&self, timeout: f64) -> Option<String> {
        let ptr = unsafe { (self.receive)(timeout) };
        if ptr.is_null() {
            return None;
        }
        let copied = unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned();
        Some(copied)
    }

    pub fn execute(&self, request: &str) -> Result<Option<String>, TdJsonError> {
        let c = CString::new(request).map_err(|_| TdJsonError::InvalidRequest)?;
        let ptr = unsafe { (self.execute)(c.as_ptr()) };
        if ptr.is_null() {
            return Ok(None);
        }
        let copied = unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned();
        Ok(Some(copied))
    }
}

static NATIVE_LOG_COUNT: AtomicU64 = AtomicU64::new(0);

unsafe extern "C" fn redacted_native_log(_verbosity: c_int, _message: *const c_char) {
    // Do not call any TDLib function from this callback.
    // Do not inspect `_message`; native logs can contain account data.
    NATIVE_LOG_COUNT.fetch_add(1, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn does_not_search_homebrew_paths() {
        // Bundled lookup is relative to the executable, never /opt/homebrew.
        if let Some(path) = bundled_candidate() {
            let rendered = path.to_string_lossy();
            assert!(!rendered.contains("/opt/homebrew"));
            assert!(!rendered.contains("/usr/local/opt"));
        }
    }
}
