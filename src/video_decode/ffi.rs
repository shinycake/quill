//! The runtime-loaded `quillvideo` shim (`native/quillvideo`): FFmpeg
//! behind a six-function C ABI. Loaded like tdjson, from the
//! package (next to the binary or in its `lib/`), `QUILL_VIDEO_LIB`, or a
//! development build under `vendor/ffmpeg/prefix`.

use std::ffi::{c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use super::nv12::{
    BufferPool, PixelOutput, Pixels, YuvColor, YuvMatrix, nv12_len, pack_nv12, plane_span,
};
use super::{AudioChunk, Demuxer, Item, MediaInfo, OpenOptions, VideoFrame, rotate_bgra};

/// `QV_ABI_VERSION` in `quillvideo.h`.
const ABI_VERSION: i32 = 4;
const ITEM_VIDEO: i32 = 1;
const ITEM_AUDIO: i32 = 2;
/// `QV_PIXEL_*` in `quillvideo.h`.
const PIXEL_BGRA: i32 = 0;
const PIXEL_NV12: i32 = 1;
/// `QV_HW_*` in `quillvideo.h`.
const HW_OFF: i32 = 0;
const HW_ALLOWED: i32 = 1;
#[cfg(test)]
const HW_FAIL_THIRD_PICTURE: i32 = 2;
#[cfg(test)]
const HW_FAIL_THIRD_PICTURE_AND_REOPEN: i32 = 3;

/// The shim's stand-in hardware decoders (`QV_HW_FAIL_*`).
#[cfg(test)]
pub(super) enum StandIn {
    FailThirdPicture,
    FailThirdPictureAndReopen,
}

#[repr(C)]
struct QvOptions {
    max_width: i32,
    max_height: i32,
    threads: i32,
    want_audio: i32,
    hw: i32,
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
    hw: i32,
}

impl QvInfo {
    fn empty() -> Self {
        Self {
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
            hw: 0,
        }
    }
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
    format: i32,
    chroma: *const u8,
    chroma_stride: i32,
    matrix: i32,
    full_range: i32,
    convert_us: i32,
}

impl QvItem {
    fn empty() -> Self {
        Self {
            kind: 0,
            pts: 0.0,
            data: std::ptr::null(),
            stride: 0,
            width: 0,
            height: 0,
            samples: 0,
            format: PIXEL_BGRA,
            chroma: std::ptr::null(),
            chroma_stride: 0,
            matrix: 0,
            full_range: 0,
            convert_us: 0,
        }
    }
}

/// The shim's code for a layout.
fn pixel_code(output: PixelOutput) -> i32 {
    match output {
        PixelOutput::Bgra => PIXEL_BGRA,
        PixelOutput::Nv12 => PIXEL_NV12,
    }
}

/// A picture item's planes, checked: sides, strides and pointers, and the
/// bytes each plane spans (what the shim promises stays valid until the
/// next call). Pure, so the rules are tested without the shim.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum PictureLayout {
    Bgra {
        width: u32,
        height: u32,
        stride: usize,
        len: usize,
    },
    Nv12 {
        width: u32,
        height: u32,
        stride: usize,
        len: usize,
        chroma_stride: usize,
        chroma_len: usize,
    },
}

/// Check a picture item's layout fields; `Err` for anything a plane read
/// could go wrong on (non-positive or odd sides, strides narrower than a
/// row, null planes, an unknown layout, sizes that overflow).
pub(super) fn picture_layout(
    format: i32,
    (width, height): (i32, i32),
    (stride, chroma_stride): (i32, i32),
    (data_null, chroma_null): (bool, bool),
) -> Result<PictureLayout, &'static str> {
    let malformed = "decoder returned a malformed picture";
    let side = |v: i32| u32::try_from(v).ok().filter(|v| *v > 0);
    let (Some(w), Some(h)) = (side(width), side(height)) else {
        return Err(malformed);
    };
    let stride = usize::try_from(stride).map_err(|_| malformed)?;
    if data_null {
        return Err(malformed);
    }
    match format {
        PIXEL_BGRA => {
            let row = (w as usize).checked_mul(4).ok_or(malformed)?;
            let len = plane_span(stride, h as usize, row).ok_or(malformed)?;
            Ok(PictureLayout::Bgra {
                width: w,
                height: h,
                stride,
                len,
            })
        }
        PIXEL_NV12 => {
            if !w.is_multiple_of(2) || !h.is_multiple_of(2) || chroma_null {
                return Err(malformed);
            }
            let chroma_stride = usize::try_from(chroma_stride).map_err(|_| malformed)?;
            let len = plane_span(stride, h as usize, w as usize).ok_or(malformed)?;
            // `width / 2` pairs of two bytes: `width` bytes a row.
            let chroma_len =
                plane_span(chroma_stride, h as usize / 2, w as usize).ok_or(malformed)?;
            Ok(PictureLayout::Nv12 {
                width: w,
                height: h,
                stride,
                len,
                chroma_stride,
                chroma_len,
            })
        }
        _ => Err("decoder returned an unknown picture layout"),
    }
}

type OpenFn =
    unsafe extern "C" fn(*const c_char, *const QvOptions, *mut c_char, i32) -> *mut c_void;
type InfoFn = unsafe extern "C" fn(*mut c_void, *mut QvInfo);
type NextFn = unsafe extern "C" fn(*mut c_void, *mut QvItem) -> i32;
type SeekFn = unsafe extern "C" fn(*mut c_void, f64) -> i32;
type CloseFn = unsafe extern "C" fn(*mut c_void);
type SetOutputFn = unsafe extern "C" fn(*mut c_void, i32) -> i32;

struct Api {
    _library: libloading::Library,
    open: OpenFn,
    info: InfoFn,
    next: NextFn,
    next_sound: NextFn,
    seek: SeekFn,
    close: CloseFn,
    set_output: SetOutputFn,
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
                let next_sound: NextFn = *library.get(b"qv_next_sound\0").map_err(incompatible)?;
                let seek: SeekFn = *library.get(b"qv_seek\0").map_err(incompatible)?;
                let close: CloseFn = *library.get(b"qv_close\0").map_err(incompatible)?;
                let set_output: SetOutputFn =
                    *library.get(b"qv_set_output\0").map_err(incompatible)?;
                return Ok(Self {
                    open,
                    info,
                    next,
                    next_sound,
                    seek,
                    close,
                    set_output,
                    _library: library,
                });
            }
        }
        Err(last)
    }
}

static API: OnceLock<Result<Api, String>> = OnceLock::new();

fn api() -> Result<&'static Api, &'static str> {
    API.get_or_init(load_traced)
        .as_ref()
        .map_err(String::as_str)
}

/// `Api::load` (dlopen of the shim and its FFmpeg libraries), with
/// `QUILL_STARTUP_TRACE` marks that say whether the UI thread paid for it.
fn load_traced() -> Result<Api, String> {
    let ui_thread = std::thread::current().name() == Some("main");
    crate::startup_trace::mark(if ui_thread {
        "video-decoder-load-start-ui-thread"
    } else {
        "video-decoder-load-start-worker"
    });
    let api = Api::load();
    crate::startup_trace::mark(if ui_thread {
        "video-decoder-load-end-ui-thread"
    } else {
        "video-decoder-load-end-worker"
    });
    api
}

/// Whether the shim loaded, without loading it: `None` before the first
/// [`availability`] call has finished.
pub(super) fn loaded() -> Option<bool> {
    API.get().map(Result::is_ok)
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
    /// Buffers NV12 pictures are packed into.
    nv12_pool: BufferPool,
}

// SAFETY: a context is used by one thread at a time (the decode thread
// owns the demuxer); the shim keeps no thread-affine state.
unsafe impl Send for FfiDemuxer {}

fn codec_name(raw: &[u8; 24]) -> String {
    let end = raw.iter().position(|b| *b == 0).unwrap_or(raw.len());
    String::from_utf8_lossy(raw.get(..end).unwrap_or_default()).into_owned()
}

impl FfiDemuxer {
    pub(super) fn open(path: &Path, options: &OpenOptions) -> Result<Self, String> {
        let hw = if options.hardware { HW_ALLOWED } else { HW_OFF };
        Self::open_with(path, options, hw)
    }

    #[cfg(test)]
    pub(super) fn open_stand_in(
        path: &Path,
        options: &OpenOptions,
        stand_in: StandIn,
    ) -> Result<Self, String> {
        let hw = match stand_in {
            StandIn::FailThirdPicture => HW_FAIL_THIRD_PICTURE,
            StandIn::FailThirdPictureAndReopen => HW_FAIL_THIRD_PICTURE_AND_REOPEN,
        };
        Self::open_with(path, options, hw)
    }

    fn open_with(path: &Path, options: &OpenOptions, hw: i32) -> Result<Self, String> {
        let api = api().map_err(str::to_string)?;
        let c_path = crate::ffi_path::c_path(path).ok_or("invalid video path")?;
        let opts = QvOptions {
            max_width: i32::try_from(options.max_width).unwrap_or(i32::MAX),
            max_height: i32::try_from(options.max_height).unwrap_or(i32::MAX),
            threads: i32::try_from(options.threads).unwrap_or(0),
            want_audio: i32::from(options.want_audio),
            hw,
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
            let message = String::from_utf8_lossy(err.get(..end).unwrap_or_default()).into_owned();
            return Err(if message.is_empty() {
                "This video can't be played.".into()
            } else {
                message
            });
        }
        let mut raw = QvInfo::empty();
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
            hardware: raw.hw != 0,
        };
        trace_decoder(&info, "opened");
        Ok(Self {
            api,
            ctx,
            info,
            nv12_pool: BufferPool::default(),
        })
    }

    fn refresh_hardware_flag(&mut self) {
        if !self.info.hardware {
            return;
        }
        let mut raw = QvInfo::empty();
        // SAFETY: `ctx` is a live context; `raw` is a valid out pointer.
        unsafe { (self.api.info)(self.ctx, &mut raw) };
        if raw.hw == 0 {
            self.info.hardware = false;
            trace_decoder(&self.info, "fell back");
        }
    }

    fn sound(&self, item: &QvItem, pts: f64) -> AudioChunk {
        let channels = usize::from(self.info.channels.max(1));
        if item.samples <= 0 || item.data.is_null() {
            return AudioChunk {
                pts,
                samples: Vec::new(),
            };
        }
        let len = item.samples as usize * channels;
        // SAFETY: `samples * channels` interleaved f32 owned by the shim
        // until the next call; read unaligned to be safe.
        let samples = unsafe {
            let ptr = item.data.cast::<f32>();
            if ptr.is_aligned() {
                std::slice::from_raw_parts(ptr, len).to_vec()
            } else {
                (0..len).map(|i| ptr.add(i).read_unaligned()).collect()
            }
        };
        AudioChunk { pts, samples }
    }
}

fn trace_decoder(info: &MediaInfo, event: &str) {
    static ON: OnceLock<bool> = OnceLock::new();
    if *ON.get_or_init(|| std::env::var_os("QUILL_TRACE_VIDEO").is_some()) && info.has_video {
        let decoder = if info.hardware {
            "hardware"
        } else {
            "software"
        };
        eprintln!("VD,{event},{},{decoder}", info.video_codec);
    }
}

impl Demuxer for FfiDemuxer {
    fn info(&self) -> MediaInfo {
        self.info.clone()
    }

    fn next(&mut self) -> Result<Option<Item>, String> {
        let mut item = QvItem::empty();
        // SAFETY: live context, valid out pointer.
        let r = unsafe { (self.api.next)(self.ctx, &mut item) };
        if r == 0 {
            return Ok(None);
        }
        if r < 0 {
            self.refresh_hardware_flag();
            return Err(format!("decoding failed ({r})"));
        }
        let pts = if item.pts.is_finite() { item.pts } else { 0.0 };
        match item.kind {
            ITEM_VIDEO => {
                let frame = self.picture(&item, pts);
                self.refresh_hardware_flag();
                frame.map(|frame| Some(Item::Video(frame)))
            }
            ITEM_AUDIO => Ok(Some(Item::Audio(self.sound(&item, pts)))),
            other => Err(format!("unknown decoder item {other}")),
        }
    }

    fn next_sound(&mut self) -> Result<Option<AudioChunk>, String> {
        let mut item = QvItem::empty();
        // SAFETY: live context, valid out pointer.
        let r = unsafe { (self.api.next_sound)(self.ctx, &mut item) };
        if r < 0 {
            return Err(format!("decoding failed ({r})"));
        }
        let pts = if item.pts.is_finite() { item.pts } else { 0.0 };
        Ok((r > 0).then(|| self.sound(&item, pts)))
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

    fn set_output(&mut self, output: PixelOutput) {
        // SAFETY: live context; the shim checks the layout code. A refused
        // code leaves the layout as it was (BGRA at worst).
        let _ = unsafe { (self.api.set_output)(self.ctx, pixel_code(output)) };
    }
}

impl FfiDemuxer {
    /// The video item as an upright, tightly packed picture. Every length
    /// is checked ([`picture_layout`]) before a plane is read.
    fn picture(&mut self, item: &QvItem, pts: f64) -> Result<VideoFrame, String> {
        let started = std::time::Instant::now();
        let layout = picture_layout(
            item.format,
            (item.width, item.height),
            (item.stride, item.chroma_stride),
            (item.data.is_null(), item.chroma.is_null()),
        )?;
        let rotation = self.info.rotation;
        let (pixels, width, height) = match layout {
            PictureLayout::Bgra {
                width,
                height,
                stride,
                len,
            } => {
                // SAFETY: the shim hands out `height` rows `stride` bytes
                // apart (`len` spans them) that stay valid until the next
                // call on this context.
                let rows = unsafe { std::slice::from_raw_parts(item.data, len) };
                let (bgra, w, h) = rotate_bgra(rows, width, height, stride, rotation);
                (Pixels::Bgra(bgra), w, h)
            }
            PictureLayout::Nv12 {
                width,
                height,
                stride,
                len,
                chroma_stride,
                chroma_len,
            } => {
                // SAFETY: as above for the luma plane (`len`) and the
                // chroma plane (`chroma_len`: `height / 2` rows
                // `chroma_stride` apart), both owned by the shim until the
                // next call.
                let (luma, chroma) = unsafe {
                    (
                        std::slice::from_raw_parts(item.data, len),
                        std::slice::from_raw_parts(item.chroma, chroma_len),
                    )
                };
                let malformed = "decoder returned a malformed picture";
                let mut bytes = self
                    .nv12_pool
                    .take(nv12_len(width, height).ok_or(malformed)?);
                let out = Arc::get_mut(&mut bytes).ok_or(malformed)?;
                let (w, h) = pack_nv12(
                    luma,
                    stride,
                    chroma,
                    chroma_stride,
                    (width, height),
                    rotation,
                    out,
                )
                .ok_or(malformed)?;
                self.nv12_pool.keep(&bytes);
                let color = YuvColor {
                    matrix: YuvMatrix::from_shim(item.matrix, height),
                    full_range: item.full_range != 0,
                };
                // Shared with the renderer without another copy.
                (Pixels::Nv12 { bytes, color }, w, h)
            }
        };
        if pixels.is_empty() {
            return Err("decoder returned a malformed picture".into());
        }
        let packed = u32::try_from(started.elapsed().as_micros()).unwrap_or(u32::MAX);
        let shim = u32::try_from(item.convert_us).unwrap_or(0);
        Ok(VideoFrame {
            pts,
            width,
            height,
            pixels,
            convert_us: shim.saturating_add(packed),
        })
    }
}

impl Drop for FfiDemuxer {
    fn drop(&mut self) {
        // SAFETY: closes the context exactly once.
        unsafe { (self.api.close)(self.ctx) };
    }
}
