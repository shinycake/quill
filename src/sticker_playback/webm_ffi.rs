//! The WebM sticker decoder inside the runtime-loaded `quillvideo` library
//! (`native/quillvideo/quillsticker.{h,c}`, built into the same library as
//! the video player's shim by `scripts/build-ffmpeg.sh`). Looked up where
//! `video_decode` finds that library: `QUILL_VIDEO_LIB`, next to the binary
//! or in its `lib/`, or a development build under `vendor/ffmpeg/prefix`.
//! A library without the `qs_*` symbols (built before them) counts as not
//! installed, and stickers fall back to the `ffmpeg` binary.

use std::ffi::{c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// `QS_ABI_VERSION` in `quillsticker.h`.
const ABI_VERSION: i32 = 1;
/// `QS_MAX_SIDE` in `quillsticker.c`: the largest box the shim accepts.
const MAX_SIDE: u32 = 4096;

#[repr(C)]
struct QsInfo {
    duration: f64,
    frame_rate: f64,
    width: i32,
    height: i32,
    has_alpha: i32,
}

#[repr(C)]
struct QsFrame {
    pts: f64,
    data: *const u8,
    stride: i32,
    width: i32,
    height: i32,
}

type OpenFn = unsafe extern "C" fn(*const c_char, i32, i32, *mut c_char, i32) -> *mut c_void;
type InfoFn = unsafe extern "C" fn(*mut c_void, *mut QsInfo);
type NextFn = unsafe extern "C" fn(*mut c_void, *mut f64) -> i32;
type PixelsFn = unsafe extern "C" fn(*mut c_void, *mut QsFrame) -> i32;
type CloseFn = unsafe extern "C" fn(*mut c_void);

struct Api {
    _library: libloading::Library,
    open: OpenFn,
    info: InfoFn,
    next: NextFn,
    pixels: PixelsFn,
    close: CloseFn,
}

/// Where the library may live, most specific first (as `video_decode`).
fn candidates() -> Vec<PathBuf> {
    let name = crate::video_decode::library_name();
    let mut out: Vec<PathBuf> = std::env::var_os("QUILL_VIDEO_LIB")
        .map(PathBuf::from)
        .into_iter()
        .collect();
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        out.push(dir.join(name));
        out.push(dir.join("lib").join(name));
        out.push(dir.join("../Frameworks").join(name));
        for ancestor in dir.ancestors() {
            let prefix = ancestor.join("vendor/ffmpeg/prefix");
            out.push(prefix.join("lib").join(name));
            out.push(prefix.join("bin").join(name));
        }
    }
    out
}

fn load_library(path: &Path) -> Result<libloading::Library, libloading::Error> {
    #[cfg(windows)]
    {
        // Resolve the library's FFmpeg DLLs next to it, not only next to
        // the exe (a development build under vendor/ffmpeg/prefix/bin).
        use libloading::os::windows::{LOAD_WITH_ALTERED_SEARCH_PATH, Library};
        // SAFETY: only the owner-configured or bundled library is loaded.
        unsafe { Library::load_with_flags(path, LOAD_WITH_ALTERED_SEARCH_PATH).map(Into::into) }
    }
    #[cfg(not(windows))]
    {
        // SAFETY: only the owner-configured or bundled library is loaded.
        unsafe { libloading::Library::new(path) }
    }
}

impl Api {
    fn load() -> Result<Self, String> {
        let mut last = format!("{} not found", crate::video_decode::library_name());
        for path in candidates() {
            if !path.is_file() {
                continue;
            }
            let library = match load_library(&path) {
                Ok(library) => library,
                Err(err) => {
                    last = format!("{}: {err}", path.display());
                    continue;
                }
            };
            let incompatible = |_| "quillvideo has no sticker decoder".to_string();
            // SAFETY: the signatures match `quillsticker.h` at
            // `ABI_VERSION`, checked first; the library stays loaded for the
            // process (it is kept in the `Api`, which lives in a static).
            unsafe {
                let version = library
                    .get::<unsafe extern "C" fn() -> i32>(b"qs_abi_version\0")
                    .map_err(incompatible)?;
                if version() != ABI_VERSION {
                    return Err(format!(
                        "quillsticker ABI {} (expected {ABI_VERSION})",
                        version()
                    ));
                }
                let open: OpenFn = *library.get(b"qs_open\0").map_err(incompatible)?;
                let info: InfoFn = *library.get(b"qs_info\0").map_err(incompatible)?;
                let next: NextFn = *library.get(b"qs_next\0").map_err(incompatible)?;
                let pixels: PixelsFn = *library.get(b"qs_pixels\0").map_err(incompatible)?;
                let close: CloseFn = *library.get(b"qs_close\0").map_err(incompatible)?;
                return Ok(Self {
                    open,
                    info,
                    next,
                    pixels,
                    close,
                    _library: library,
                });
            }
        }
        Err(last)
    }
}

fn api() -> Result<&'static Api, &'static str> {
    static API: OnceLock<Result<Api, String>> = OnceLock::new();
    API.get_or_init(Api::load).as_ref().map_err(String::as_str)
}

/// Why opening a sticker in process did not work.
#[derive(Debug)]
pub(super) enum OpenError {
    /// The decoder is not installed here (use the `ffmpeg` binary).
    Unavailable,
    /// The decoder is installed but cannot read this file.
    Failed(String),
}

/// What [`StickerDecoder::open`] learned about the file.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct StickerInfo {
    /// Seconds, when the file says.
    pub(super) duration: Option<f64>,
    /// Output size: the picture fit into the requested box.
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) has_alpha: bool,
}

/// One open sticker file in the shim, used from one thread.
pub(super) struct StickerDecoder {
    api: &'static Api,
    ctx: *mut c_void,
    info: StickerInfo,
}

impl StickerDecoder {
    /// Open `path`, scaling pictures to fit an `edge` × `edge` box.
    pub(super) fn open(path: &Path, edge: u32) -> Result<Self, OpenError> {
        let api = api().map_err(|_| OpenError::Unavailable)?;
        let failed = |what: &str| OpenError::Failed(what.to_string());
        let edge = i32::try_from(edge.clamp(1, MAX_SIDE)).map_err(|_| failed("bad size"))?;
        let c_path = crate::ffi_path::c_path(path).ok_or_else(|| failed("invalid path"))?;
        let mut err = [0u8; 256];
        // SAFETY: a valid NUL-terminated path and an error buffer of the
        // stated length, both alive for the call.
        let ctx = unsafe {
            (api.open)(
                c_path.as_ptr(),
                edge,
                edge,
                err.as_mut_ptr().cast(),
                err.len() as i32,
            )
        };
        if ctx.is_null() {
            let end = err.iter().position(|b| *b == 0).unwrap_or(err.len());
            let message = String::from_utf8_lossy(err.get(..end).unwrap_or_default()).into_owned();
            return Err(OpenError::Failed(message));
        }
        let mut raw = QsInfo {
            duration: 0.0,
            frame_rate: 0.0,
            width: 0,
            height: 0,
            has_alpha: 0,
        };
        // SAFETY: `ctx` is a live context; `raw` is a valid out pointer.
        unsafe { (api.info)(ctx, &mut raw) };
        let info = StickerInfo {
            duration: (raw.duration.is_finite() && raw.duration > 0.0).then_some(raw.duration),
            width: u32::try_from(raw.width).unwrap_or(0),
            height: u32::try_from(raw.height).unwrap_or(0),
            has_alpha: raw.has_alpha != 0,
        };
        let decoder = Self { api, ctx, info };
        if info.width == 0 || info.height == 0 {
            // Dropping `decoder` closes the context.
            return Err(failed("the sticker has no size"));
        }
        Ok(decoder)
    }

    pub(super) fn info(&self) -> StickerInfo {
        self.info
    }

    /// Decode the next picture: its time in seconds, `None` at the end.
    pub(super) fn next(&mut self) -> Result<Option<f64>, String> {
        let mut pts = 0.0f64;
        // SAFETY: live context, valid out pointer.
        let r = unsafe { (self.api.next)(self.ctx, &mut pts) };
        match r {
            1 => Ok(Some(if pts.is_finite() { pts } else { 0.0 })),
            0 => Ok(None),
            code => Err(format!("decoding failed ({code})")),
        }
    }

    /// The picture [`next`](Self::next) returned as tightly packed
    /// straight-alpha BGRA: width, height and `width * height * 4` bytes.
    pub(super) fn pixels(&mut self) -> Result<(u32, u32, Vec<u8>), String> {
        let mut frame = QsFrame {
            pts: 0.0,
            data: std::ptr::null(),
            stride: 0,
            width: 0,
            height: 0,
        };
        // SAFETY: live context, valid out pointer.
        let r = unsafe { (self.api.pixels)(self.ctx, &mut frame) };
        if r < 0 {
            return Err(format!("converting failed ({r})"));
        }
        let malformed = || "decoder returned a malformed picture".to_string();
        let width = usize::try_from(frame.width).map_err(|_| malformed())?;
        let height = usize::try_from(frame.height).map_err(|_| malformed())?;
        let stride = usize::try_from(frame.stride).map_err(|_| malformed())?;
        let row = width.checked_mul(4).ok_or_else(malformed)?;
        if width == 0 || height == 0 || stride < row || frame.data.is_null() {
            return Err(malformed());
        }
        let len = stride.checked_mul(height).ok_or_else(malformed)?;
        // SAFETY: the shim hands out `stride * height` bytes that stay valid
        // until the next call on this context; none happens while `rows`
        // is alive (`&mut self`).
        let rows = unsafe { std::slice::from_raw_parts(frame.data, len) };
        let bgra = if stride == row {
            rows.to_vec()
        } else {
            rows.chunks_exact(stride)
                .flat_map(|line| line.get(..row).unwrap_or_default())
                .copied()
                .collect()
        };
        Ok((width as u32, height as u32, bgra))
    }
}

impl Drop for StickerDecoder {
    fn drop(&mut self) {
        // SAFETY: closes the context exactly once.
        unsafe { (self.api.close)(self.ctx) };
    }
}
