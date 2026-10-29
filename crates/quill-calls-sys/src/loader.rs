//! The runtime loader that resolves every `qc_*` function.
use super::*;
use std::ffi::OsStr;

/// The libquill_calls API, opened at runtime.
///
/// The library is never closed, even when the `Loader` drops: tgcalls keeps
/// its threads for the life of the process and finishes tearing calls down
/// on them after `qc_p2p_destroy` returns, so unloading its code would crash
/// those threads.
pub struct Loader {
    _lib: std::mem::ManuallyDrop<libloading::Library>,
    pub qc_api_version: unsafe extern "C" fn() -> u32,
    pub qc_library_versions: unsafe extern "C" fn() -> *const c_char,
    pub qc_max_layer: unsafe extern "C" fn() -> i32,
    pub qc_string_free: unsafe extern "C" fn(*mut c_char),
    pub qc_set_log: unsafe extern "C" fn(qc_log_fn, *mut c_void, i32),
    pub qc_video_input_devices: unsafe extern "C" fn() -> *mut c_char,
    pub qc_video_capture_camera: unsafe extern "C" fn(*const c_char) -> *mut qc_video_capture,
    pub qc_video_capture_external: unsafe extern "C" fn(i32) -> *mut qc_video_capture,
    pub qc_video_capture_push_i420:
        unsafe extern "C" fn(*mut qc_video_capture, *const qc_video_frame),
    pub qc_video_capture_switch_camera: unsafe extern "C" fn(*mut qc_video_capture, *const c_char),
    pub qc_video_capture_set_state: unsafe extern "C" fn(*mut qc_video_capture, i32),
    pub qc_video_capture_set_preview:
        unsafe extern "C" fn(*mut qc_video_capture, qc_video_frame_fn, *mut c_void),
    pub qc_video_capture_free: unsafe extern "C" fn(*mut qc_video_capture),
    pub qc_p2p_create: unsafe extern "C" fn(
        *const c_char,
        *const qc_p2p_callbacks,
        *mut qc_video_capture,
    ) -> *mut qc_p2p,
    pub qc_p2p_receive_signaling: unsafe extern "C" fn(*mut qc_p2p, *const u8, usize),
    pub qc_p2p_push_audio: unsafe extern "C" fn(*mut qc_p2p, *const i16, usize),
    pub qc_p2p_set_muted: unsafe extern "C" fn(*mut qc_p2p, i32),
    pub qc_p2p_set_video_capture: unsafe extern "C" fn(*mut qc_p2p, *mut qc_video_capture),
    pub qc_p2p_set_requested_aspect: unsafe extern "C" fn(*mut qc_p2p, f32),
    pub qc_p2p_set_low_battery: unsafe extern "C" fn(*mut qc_p2p, i32),
    pub qc_p2p_debug_info: unsafe extern "C" fn(*mut qc_p2p) -> *mut c_char,
    pub qc_p2p_last_error: unsafe extern "C" fn(*mut qc_p2p) -> *mut c_char,
    pub qc_p2p_destroy: unsafe extern "C" fn(*mut qc_p2p),
}

macro_rules! resolve {
    ($lib:expr, $name:ident) => {{
        let name = concat!(stringify!($name), "\0");
        // SAFETY: the field's type is the C signature quill_calls.h declares
        // for this symbol, and the API version check in `load` ties the two
        // together. The pointer stays valid because the library is never
        // unloaded.
        let symbol = unsafe { $lib.get(name.as_bytes()) };
        *symbol.map_err(|_| LoadError::SymbolMissing {
            symbol: stringify!($name),
        })?
    }};
}

impl Loader {
    /// Opens the library at `path`, checks its API version and resolves every
    /// `qc_*` function.
    pub fn load(path: impl AsRef<OsStr>) -> Result<Self, LoadError> {
        let path = path.as_ref();
        // SAFETY: opening runs the library's initializers, which only set up
        // tgcalls' own static state (its protocol version registry).
        let lib =
            unsafe { libloading::Library::new(path) }.map_err(|_| LoadError::LibraryMissing {
                searched: vec![PathBuf::from(path)],
            })?;
        let qc_api_version: unsafe extern "C" fn() -> u32 = resolve!(lib, qc_api_version);
        // SAFETY: qc_api_version takes nothing and only returns a constant.
        let library = unsafe { qc_api_version() };
        if library != QC_API_VERSION {
            return Err(LoadError::ApiMismatch { library });
        }
        Ok(Self {
            qc_api_version,
            qc_library_versions: resolve!(lib, qc_library_versions),
            qc_max_layer: resolve!(lib, qc_max_layer),
            qc_string_free: resolve!(lib, qc_string_free),
            qc_set_log: resolve!(lib, qc_set_log),
            qc_video_input_devices: resolve!(lib, qc_video_input_devices),
            qc_video_capture_camera: resolve!(lib, qc_video_capture_camera),
            qc_video_capture_external: resolve!(lib, qc_video_capture_external),
            qc_video_capture_push_i420: resolve!(lib, qc_video_capture_push_i420),
            qc_video_capture_switch_camera: resolve!(lib, qc_video_capture_switch_camera),
            qc_video_capture_set_state: resolve!(lib, qc_video_capture_set_state),
            qc_video_capture_set_preview: resolve!(lib, qc_video_capture_set_preview),
            qc_video_capture_free: resolve!(lib, qc_video_capture_free),
            qc_p2p_create: resolve!(lib, qc_p2p_create),
            qc_p2p_receive_signaling: resolve!(lib, qc_p2p_receive_signaling),
            qc_p2p_push_audio: resolve!(lib, qc_p2p_push_audio),
            qc_p2p_set_muted: resolve!(lib, qc_p2p_set_muted),
            qc_p2p_set_video_capture: resolve!(lib, qc_p2p_set_video_capture),
            qc_p2p_set_requested_aspect: resolve!(lib, qc_p2p_set_requested_aspect),
            qc_p2p_set_low_battery: resolve!(lib, qc_p2p_set_low_battery),
            qc_p2p_debug_info: resolve!(lib, qc_p2p_debug_info),
            qc_p2p_last_error: resolve!(lib, qc_p2p_last_error),
            qc_p2p_destroy: resolve!(lib, qc_p2p_destroy),
            _lib: std::mem::ManuallyDrop::new(lib),
        })
    }

    /// Searches the standard locations and loads the first library found.
    ///
    /// Order: the `QUILL_CALLS_LIB` env var; `lib/` beside the executable
    /// (Linux and Windows packages); `Frameworks/` or `vendor/quill-calls/lib`
    /// at any ancestor of the executable (the macOS bundle, `cargo run`);
    /// then the bare file name on the system search path.
    pub fn load_default() -> Result<Self, LoadError> {
        let mut searched: Vec<PathBuf> = Vec::new();

        if let Some(path) = std::env::var_os("QUILL_CALLS_LIB").map(PathBuf::from) {
            searched.push(path.clone());
            if let Ok(loader) = Self::load(&path) {
                return Ok(loader);
            }
        }

        if let Ok(exe) = std::env::current_exe() {
            if let Some(candidate) = exe
                .parent()
                .map(|dir| dir.join("lib").join(library_filename()))
                .filter(|candidate| candidate.exists())
            {
                return Self::load(&candidate);
            }
            for ancestor in exe
                .parent()
                .into_iter()
                .flat_map(std::path::Path::ancestors)
            {
                for directory in [
                    ancestor.join("Frameworks"),
                    ancestor.join("vendor/quill-calls/lib"),
                    #[cfg(windows)]
                    ancestor.to_path_buf(),
                ] {
                    let candidate = directory.join(library_filename());
                    if candidate.exists() {
                        return Self::load(&candidate);
                    }
                }
            }
        }

        let system = PathBuf::from(library_filename());
        searched.push(system.clone());
        Self::load(&system).map_err(|_| LoadError::LibraryMissing { searched })
    }
}
