//! The runtime-loaded `quillvideo` shim (`native/quillvideo`): FFmpeg
//! behind a six-function C ABI. Loaded like rlottie and tdjson, from the
//! package (next to the binary or in its `lib/`), `QUILL_VIDEO_LIB`, or a
//! development build under `vendor/ffmpeg/prefix`.

use std::ffi::{CString, c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use super::{AudioChunk, Demuxer, Item, MediaInfo, OpenOptions, VideoFrame, rotate_bgra};

/// `QV_ABI_VERSION` in `quillvideo.h`.
const ABI_VERSION: i32 = 1;
const ITEM_VIDEO: i32 = 1;
const ITEM_AUDIO: i32 = 2;

#[repr(C)]
struct QvOptions {
    max_width: i32,
    max_height: i32,
    threads: i32,
    want_audio: i32,
}

#[repr(C)]
struct QvInfo {
    duration: f64,
    frame_rate: f64,
    width: i32,
    height: i32,
    rotation: i32,
    has_video: i32,
    has_audio: i32,
    sample_rate: i32,
    channels: i32,
    video_codec: [u8; 24],
    audio_codec: [u8; 24],
}

#[repr(C)]
struct QvItem {
    kind: i32,
    pts: f64,
    data: *const u8,
    stride: i32,
    width: i32,
    height: i32,
    samples: i32,
}

type OpenFn = unsafe extern "C" fn(*const c_char, *const QvOptions, *mut c_char, i32) -> *mut c_void;
type InfoFn = unsafe extern "C" fn(*mut c_void, *mut QvInfo);
type NextFn = unsafe extern "C" fn(*mut c_void, *mut QvItem) -> i32;
type SeekFn = unsafe extern "C" fn(*mut c_void, f64) -> i32;
type CloseFn = unsafe extern "C" fn(*mut c_void);

struct Api {
    _library: libloading::Library,
    open: OpenFn,
    info: InfoFn,
    next: NextFn,
    seek: SeekFn,
    close: CloseFn,
}

/// The library file name on this platform.
pub(super) fn library_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "libquillvideo.dylib"
    } else if cfg!(target_os = "windows") {
        "quillvideo.dll"
    } else {
        "libquillvideo.so"
    }
}

/// Where the shim may live, most specific first.
fn candidates() -> Vec<PathBuf> {
    let name = library_name();
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
        // Resolve the shim's FFmpeg DLLs next to it, not only next to the exe
        // (a development build under vendor/ffmpeg/prefix/bin).
        use libloading::os::windows::{LOAD_WITH_ALTERED_SEARCH_PATH, Library};
        // SAFETY: only the owner-configured or bundled shim is loaded.
        unsafe { Library::load_with_flags(path, LOAD_WITH_ALTERED_SEARCH_PATH).map(Into::into) }
    }
    #[cfg(not(windows))]
    {
        // SAFETY: only the owner-configured or bundled shim is loaded.
        unsafe { libloading::Library::new(path) }
    }
}

impl Api {
    fn load() -> Result<Self, String> {
        let mut last = format!("{} not found", library_name());
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
            // SAFETY: the signatures match `quillvideo.h` at `ABI_VERSION`,
            // checked first; the library stays loaded for the process.
            unsafe {
                let version = library
                    .get::<unsafe extern "C" fn() -> i32>(b"qv_abi_version\0")
                    .map_err(|_| "incompatible quillvideo library".to_string())?;
                if version() != ABI_VERSION {
                    return Err(format!(
                        "quillvideo ABI {} (expected {ABI_VERSION})",
                        version()
                    ));
                }
                let incompatible = |_| "incompatible quillvideo library".to_string();
                let open: OpenFn = *library.get(b"qv_open\0").map_err(incompatible)?;
                let info: InfoFn = *library.get(b"qv_info\0").map_err(incompatible)?;
                let next: NextFn = *library.get(b"qv_next\0").map_err(incompatible)?;
                let seek: SeekFn = *library.get(b"qv_seek\0").map_err(incompatible)?;
                let close: CloseFn = *library.get(b"qv_close\0").map_err(incompatible)?;
                return Ok(Self {
                    open,
                    info,
                    next,
                    seek,
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

/// Whether the in-process decoder is installed (and why not).
pub(super) fn availability() -> Result<(), &'static str> {
    api().map(|_| ())
}

/// One open file in the shim.
pub(super) struct FfiDemuxer {
    api: &'static Api,
    ctx: *mut c_void,
    info: MediaInfo,
}

// SAFETY: a context is used by one thread at a time (the decode thread
// owns the demuxer); the shim keeps no thread-affine state.
unsafe impl Send for FfiDemuxer {}

fn codec_name(raw: &[u8; 24]) -> String {
    let end = raw.iter().position(|b| *b == 0).unwrap_or(raw.len());
    String::from_utf8_lossy(&raw[..end]).into_owned()
}

impl FfiDemuxer {
    pub(super) fn open(path: &Path, options: &OpenOptions) -> Result<Self, String> {
        let api = api().map_err(str::to_string)?;
        let utf8 = path.to_str().ok_or("video path is not valid UTF-8")?;
        let c_path = CString::new(utf8).map_err(|_| "invalid video path")?;
        let opts = QvOptions {
            max_width: i32::try_from(options.max_width).unwrap_or(i32::MAX),
            max_height: i32::try_from(options.max_height).unwrap_or(i32::MAX),
            threads: i32::try_from(options.threads).unwrap_or(0),
            want_audio: i32::from(options.want_audio),
        };
        let mut err = [0u8; 256];
        // SAFETY: valid NUL-terminated path, options struct and error buffer
        // of the stated length.
        let ctx = unsafe {
            (api.open)(
                c_path.as_ptr(),
                &opts,
                err.as_mut_ptr().cast(),
                err.len() as i32,
            )
        };
        if ctx.is_null() {
            let end = err.iter().position(|b| *b == 0).unwrap_or(err.len());
            let message = String::from_utf8_lossy(&err[..end]).into_owned();
            return Err(if message.is_empty() {
                "This video can't be played.".into()
            } else {
                message
            });
        }
        let mut raw = QvInfo {
            duration: 0.0,
            frame_rate: 0.0,
            width: 0,
            height: 0,
            rotation: 0,
            has_video: 0,
            has_audio: 0,
            sample_rate: 0,
            channels: 0,
            video_codec: [0; 24],
            audio_codec: [0; 24],
        };
        // SAFETY: `ctx` is a live context; `raw` is a valid out pointer.
        unsafe { (api.info)(ctx, &mut raw) };
        let rotation = match raw.rotation {
            90 | 180 | 270 => raw.rotation as u32,
            _ => 0,
        };
        let (width, height) = if rotation % 180 == 90 {
            (raw.height, raw.width)
        } else {
            (raw.width, raw.height)
        };
        let info = MediaInfo {
            duration: (raw.duration.is_finite() && raw.duration > 0.0).then_some(raw.duration),
            frame_rate: raw.frame_rate.max(0.0),
            width: width.max(0) as u32,
            height: height.max(0) as u32,
            rotation,
            has_video: raw.has_video != 0,
            has_audio: raw.has_audio != 0 && raw.sample_rate > 0 && raw.channels > 0,
            sample_rate: raw.sample_rate.max(0) as u32,
            channels: raw.channels.clamp(0, 2) as u16,
            video_codec: codec_name(&raw.video_codec),
            audio_codec: codec_name(&raw.audio_codec),
        };
        Ok(Self { api, ctx, info })
    }
}

impl Demuxer for FfiDemuxer {
    fn info(&self) -> MediaInfo {
        self.info.clone()
    }

    fn next(&mut self) -> Result<Option<Item>, String> {
        let mut item = QvItem {
            kind: 0,
            pts: 0.0,
            data: std::ptr::null(),
            stride: 0,
            width: 0,
            height: 0,
            samples: 0,
        };
        // SAFETY: live context, valid out pointer.
        let r = unsafe { (self.api.next)(self.ctx, &mut item) };
        if r == 0 {
            return Ok(None);
        }
        if r < 0 {
            return Err(format!("decoding failed ({r})"));
        }
        let pts = if item.pts.is_finite() { item.pts } else { 0.0 };
        match item.kind {
            ITEM_VIDEO => {
                let (w, h, stride) = (item.width, item.height, item.stride);
                if w <= 0 || h <= 0 || stride < w * 4 || item.data.is_null() {
                    return Err("decoder returned a malformed picture".into());
                }
                let len = stride as usize * h as usize;
                // SAFETY: the shim hands out `stride * height` bytes that
                // stay valid until the next call on this context.
                let rows = unsafe { std::slice::from_raw_parts(item.data, len) };
                let (bgra, width, height) =
                    rotate_bgra(rows, w as u32, h as u32, stride as usize, self.info.rotation);
                Ok(Some(Item::Video(VideoFrame {
                    pts,
                    width,
                    height,
                    bgra,
                })))
            }
            ITEM_AUDIO => {
                let channels = usize::from(self.info.channels.max(1));
                if item.samples <= 0 || item.data.is_null() {
                    return Ok(Some(Item::Audio(AudioChunk {
                        pts,
                        samples: Vec::new(),
                    })));
                }
                let len = item.samples as usize * channels;
                // SAFETY: `samples * channels` interleaved f32 owned by the
                // shim until the next call; read unaligned to be safe.
                let samples = unsafe {
                    let ptr = item.data.cast::<f32>();
                    if ptr.is_aligned() {
                        std::slice::from_raw_parts(ptr, len).to_vec()
                    } else {
                        (0..len).map(|i| ptr.add(i).read_unaligned()).collect()
                    }
                };
                Ok(Some(Item::Audio(AudioChunk { pts, samples })))
            }
            other => Err(format!("unknown decoder item {other}")),
        }
    }

    fn seek(&mut self, secs: f64) -> Result<(), String> {
        // SAFETY: live context.
        let r = unsafe { (self.api.seek)(self.ctx, secs.max(0.0)) };
        if r < 0 {
            Err(format!("seek failed ({r})"))
        } else {
            Ok(())
        }
    }
}

impl Drop for FfiDemuxer {
    fn drop(&mut self) {
        // SAFETY: closes the context exactly once.
        unsafe { (self.api.close)(self.ctx) };
    }
}
