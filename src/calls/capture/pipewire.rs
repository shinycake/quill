//! A PipeWire video stream read from the screen-cast node the portal hands
//! out (`screen_portal.rs`).
//!
//! libpipewire-0.3 is loaded at run time, as tdesktop's WebRTC loads it
//! (`webrtc::InitializePipeWire`, its "pipewire 0.3 stubs"): Quill starts
//! on systems without PipeWire, and only screen sharing on Wayland needs
//! it. The few functions and structs used are declared here; their layout
//! is libpipewire's stable ABI (checked against the 1.0.5 headers, see
//! `docs/decisions/call-capture-screen-linux.md`). The SPA pods are built
//! and read in `spa_pod.rs`.
//!
//! The stream runs on a `pw_thread_loop` (PipeWire's own thread). Its
//! callbacks only read the buffer, call the [`StreamHandler`] and return;
//! each runs inside [`ffi_boundary`], so no panic reaches C, and the shared
//! state behind them is a `Mutex` (no `RefCell`).
#![deny(clippy::indexing_slicing)]

use super::screen_raw::{Crop, RawPicture, RawPlane};
use super::spa_pod::{
    self, DATA_MEM_FD, DATA_MEM_PTR, META_VIDEO_CROP, PARAM_FORMAT, RawVideoFormat, VideoFormat,
};
use crate::locks::LockExt;
use crate::thread_guard::ffi_boundary;
use std::ffi::{CStr, c_char, c_int, c_void};
use std::os::fd::{IntoRawFd, OwnedFd};
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{Mutex, OnceLock};

/// What the stream reports besides frames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamState {
    Connecting,
    /// Negotiated but not running (also: the producer suspended it).
    Paused,
    Streaming,
    /// The node went away (the cast was stopped).
    Unconnected,
    Error(String),
}

/// Receives a stream's pictures and state changes on PipeWire's thread.
pub trait StreamHandler: Send {
    fn frame(&mut self, picture: &RawPicture<'_>);
    fn state(&mut self, state: StreamState);
}

/// Why the stream could not open.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PipeWireError {
    #[error("PipeWire (libpipewire-0.3) is not installed.")]
    Missing,
    #[error("PipeWire: {0}")]
    Failed(&'static str),
}

// --- libpipewire's ABI -----------------------------------------------------

/// `struct spa_hook` (48 bytes): filled in by `pw_stream_add_listener`.
#[repr(C)]
struct SpaHook {
    link: [*mut c_void; 2],
    funcs: *const c_void,
    data: *mut c_void,
    removed: *mut c_void,
    private: *mut c_void,
}

type DataCallback = unsafe extern "C" fn(*mut c_void);
type Unused = Option<unsafe extern "C" fn()>;

/// `struct pw_stream_events`, version 2 (96 bytes).
#[repr(C)]
struct StreamEvents {
    version: u32,
    destroy: Option<DataCallback>,
    state_changed: Option<unsafe extern "C" fn(*mut c_void, c_int, c_int, *const c_char)>,
    control_info: Unused,
    io_changed: Unused,
    param_changed: Option<unsafe extern "C" fn(*mut c_void, u32, *const c_void)>,
    add_buffer: Unused,
    remove_buffer: Unused,
    process: Option<DataCallback>,
    drained: Unused,
    command: Unused,
    trigger_done: Unused,
}

const _: () = assert!(std::mem::size_of::<SpaHook>() == 48);
const _: () = assert!(std::mem::size_of::<StreamEvents>() == 96);

/// The head of `struct pw_buffer`: only `buffer` is read.
#[repr(C)]
struct PwBuffer {
    buffer: *mut SpaBuffer,
}

#[repr(C)]
struct SpaBuffer {
    n_metas: u32,
    n_datas: u32,
    metas: *mut SpaMeta,
    datas: *mut SpaData,
}

#[repr(C)]
struct SpaMeta {
    kind: u32,
    size: u32,
    data: *mut c_void,
}

#[repr(C)]
struct SpaData {
    kind: u32,
    flags: u32,
    fd: i64,
    mapoffset: u32,
    maxsize: u32,
    data: *mut c_void,
    chunk: *mut SpaChunk,
}

#[repr(C)]
struct SpaChunk {
    offset: u32,
    size: u32,
    stride: i32,
    flags: i32,
}

/// `struct spa_meta_region`.
#[repr(C)]
struct SpaMetaRegion {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

const _: () = assert!(std::mem::size_of::<SpaData>() == 40);
const _: () = assert!(std::mem::size_of::<SpaChunk>() == 16);
const _: () = assert!(std::mem::size_of::<SpaMeta>() == 16);

const STREAM_STATE_UNCONNECTED: c_int = 0;
const STREAM_STATE_CONNECTING: c_int = 1;
const STREAM_STATE_PAUSED: c_int = 2;
const STREAM_STATE_STREAMING: c_int = 3;
const DIRECTION_INPUT: c_int = 0;
const STREAM_FLAG_AUTOCONNECT: c_int = 1 << 0;
const STREAM_FLAG_MAP_BUFFERS: c_int = 1 << 2;
const CHUNK_FLAG_CORRUPTED: i32 = 1 << 0;
/// `PW_ID_ANY`: let the session manager pick (tests only).
pub const ANY_NODE: u32 = u32::MAX;

/// The libpipewire functions Quill calls.
struct Api {
    thread_loop_new: unsafe extern "C" fn(*const c_char, *const c_void) -> *mut c_void,
    thread_loop_get_loop: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    thread_loop_start: unsafe extern "C" fn(*mut c_void) -> c_int,
    thread_loop_stop: unsafe extern "C" fn(*mut c_void),
    thread_loop_lock: unsafe extern "C" fn(*mut c_void),
    thread_loop_unlock: unsafe extern "C" fn(*mut c_void),
    thread_loop_destroy: unsafe extern "C" fn(*mut c_void),
    context_new: unsafe extern "C" fn(*mut c_void, *mut c_void, usize) -> *mut c_void,
    context_destroy: unsafe extern "C" fn(*mut c_void),
    context_connect: unsafe extern "C" fn(*mut c_void, *mut c_void, usize) -> *mut c_void,
    context_connect_fd: unsafe extern "C" fn(*mut c_void, c_int, *mut c_void, usize) -> *mut c_void,
    core_disconnect: unsafe extern "C" fn(*mut c_void) -> c_int,
    properties_new_string: unsafe extern "C" fn(*const c_char) -> *mut c_void,
    stream_new: unsafe extern "C" fn(*mut c_void, *const c_char, *mut c_void) -> *mut c_void,
    stream_add_listener:
        unsafe extern "C" fn(*mut c_void, *mut SpaHook, *const StreamEvents, *mut c_void),
    stream_connect:
        unsafe extern "C" fn(*mut c_void, c_int, u32, c_int, *mut *const c_void, u32) -> c_int,
    stream_update_params: unsafe extern "C" fn(*mut c_void, *mut *const c_void, u32) -> c_int,
    stream_dequeue_buffer: unsafe extern "C" fn(*mut c_void) -> *mut PwBuffer,
    stream_queue_buffer: unsafe extern "C" fn(*mut c_void, *mut PwBuffer) -> c_int,
    stream_disconnect: unsafe extern "C" fn(*mut c_void) -> c_int,
    stream_destroy: unsafe extern "C" fn(*mut c_void),
    _library: libloading::Library,
}

fn api() -> Result<&'static Api, PipeWireError> {
    static API: OnceLock<Option<Api>> = OnceLock::new();
    API.get_or_init(load).as_ref().ok_or(PipeWireError::Missing)
}

/// Whether libpipewire can be loaded here.
pub fn available() -> bool {
    api().is_ok()
}

fn load() -> Option<Api> {
    let library = ["libpipewire-0.3.so.0", "libpipewire-0.3.so"]
        .iter()
        .find_map(|name| {
            // SAFETY: libpipewire's initialisers only register its types;
            // the library stays loaded for the life of the process.
            unsafe { libloading::Library::new(*name) }.ok()
        })?;
    macro_rules! symbol {
        ($name:literal) => {
            // SAFETY: the declared signature matches `pipewire/*.h` of
            // libpipewire 0.3 / 1.x (a stable C ABI).
            *unsafe { library.get(concat!($name, "\0").as_bytes()) }.ok()?
        };
    }
    let init: unsafe extern "C" fn(*mut c_int, *mut *mut *mut c_char) = symbol!("pw_init");
    // SAFETY: `pw_init` accepts null arguments and may be called more than
    // once.
    unsafe { init(std::ptr::null_mut(), std::ptr::null_mut()) };
    Some(Api {
        thread_loop_new: symbol!("pw_thread_loop_new"),
        thread_loop_get_loop: symbol!("pw_thread_loop_get_loop"),
        thread_loop_start: symbol!("pw_thread_loop_start"),
        thread_loop_stop: symbol!("pw_thread_loop_stop"),
        thread_loop_lock: symbol!("pw_thread_loop_lock"),
        thread_loop_unlock: symbol!("pw_thread_loop_unlock"),
        thread_loop_destroy: symbol!("pw_thread_loop_destroy"),
        context_new: symbol!("pw_context_new"),
        context_destroy: symbol!("pw_context_destroy"),
        context_connect: symbol!("pw_context_connect"),
        context_connect_fd: symbol!("pw_context_connect_fd"),
        core_disconnect: symbol!("pw_core_disconnect"),
        properties_new_string: symbol!("pw_properties_new_string"),
        stream_new: symbol!("pw_stream_new"),
        stream_add_listener: symbol!("pw_stream_add_listener"),
        stream_connect: symbol!("pw_stream_connect"),
        stream_update_params: symbol!("pw_stream_update_params"),
        stream_dequeue_buffer: symbol!("pw_stream_dequeue_buffer"),
        stream_queue_buffer: symbol!("pw_stream_queue_buffer"),
        stream_disconnect: symbol!("pw_stream_disconnect"),
        stream_destroy: symbol!("pw_stream_destroy"),
        _library: library,
    })
}

// --- the stream ------------------------------------------------------------

/// What the callbacks share, behind the `data` pointer.
struct Shared {
    api: &'static Api,
    stream: AtomicPtr<c_void>,
    format: Mutex<Option<RawVideoFormat>>,
    handler: Mutex<Box<dyn StreamHandler>>,
}

static EVENTS: StreamEvents = StreamEvents {
    version: 2,
    destroy: None,
    state_changed: Some(on_state_changed),
    control_info: None,
    io_changed: None,
    param_changed: Some(on_param_changed),
    add_buffer: None,
    remove_buffer: None,
    process: Some(on_process),
    drained: None,
    command: None,
    trigger_done: None,
};

/// A connected stream. Dropping it disconnects and frees everything; no
/// callback runs after that.
pub struct PipeWireStream {
    api: &'static Api,
    thread_loop: *mut c_void,
    context: *mut c_void,
    core: *mut c_void,
    stream: *mut c_void,
    /// Must stay at one address while the stream lives.
    _hook: Box<SpaHook>,
    shared: Box<Shared>,
}

// SAFETY: the pointers are only used under the thread loop's lock (or
// after the loop stopped, in `drop`); libpipewire allows that from any
// thread.
unsafe impl Send for PipeWireStream {}

impl PipeWireStream {
    /// Connect to `node` on the PipeWire remote `remote` (the portal's
    /// `OpenPipeWireRemote` fd; `None` is the user's own PipeWire daemon)
    /// and start receiving into `handler`.
    pub fn connect(
        remote: Option<OwnedFd>,
        node: u32,
        default_size: (u32, u32),
        handler: Box<dyn StreamHandler>,
    ) -> Result<Self, PipeWireError> {
        let api = api()?;
        let mut stream = Self {
            api,
            thread_loop: std::ptr::null_mut(),
            context: std::ptr::null_mut(),
            core: std::ptr::null_mut(),
            stream: std::ptr::null_mut(),
            _hook: Box::new(SpaHook {
                link: [std::ptr::null_mut(); 2],
                funcs: std::ptr::null(),
                data: std::ptr::null_mut(),
                removed: std::ptr::null_mut(),
                private: std::ptr::null_mut(),
            }),
            shared: Box::new(Shared {
                api,
                stream: AtomicPtr::new(std::ptr::null_mut()),
                format: Mutex::new(None),
                handler: Mutex::new(handler),
            }),
        };
        // SAFETY: each call gets pointers libpipewire returned just above
        // (checked for null), the loop's lock is held around the calls that
        // need it, and `stream` (with its hook and shared state) outlives
        // every callback: `drop` stops the loop before freeing them.
        unsafe {
            stream.thread_loop = (api.thread_loop_new)(c"quill-screen".as_ptr(), std::ptr::null());
            if stream.thread_loop.is_null() {
                return Err(PipeWireError::Failed("no thread loop"));
            }
            let pw_loop = (api.thread_loop_get_loop)(stream.thread_loop);
            stream.context = (api.context_new)(pw_loop, std::ptr::null_mut(), 0);
            if stream.context.is_null() {
                return Err(PipeWireError::Failed("no context"));
            }
            if (api.thread_loop_start)(stream.thread_loop) < 0 {
                return Err(PipeWireError::Failed("the loop did not start"));
            }
            (api.thread_loop_lock)(stream.thread_loop);
            let connected = stream.connect_locked(remote, node, default_size);
            (api.thread_loop_unlock)(stream.thread_loop);
            connected?;
        }
        Ok(stream)
    }

    /// The part of [`Self::connect`] under the loop's lock.
    ///
    /// # Safety
    /// The thread loop is locked and the context is live.
    unsafe fn connect_locked(
        &mut self,
        remote: Option<OwnedFd>,
        node: u32,
        default_size: (u32, u32),
    ) -> Result<(), PipeWireError> {
        let api = self.api;
        // SAFETY: the caller holds the lock; the fd's ownership moves to
        // libpipewire, which closes it with the core.
        unsafe {
            self.core = match remote {
                Some(fd) => (api.context_connect_fd)(
                    self.context,
                    fd.into_raw_fd(),
                    std::ptr::null_mut(),
                    0,
                ),
                None => (api.context_connect)(self.context, std::ptr::null_mut(), 0),
            };
            if self.core.is_null() {
                return Err(PipeWireError::Failed("could not connect"));
            }
            let props = (api.properties_new_string)(
                c"media.type=Video media.category=Capture media.role=Screen".as_ptr(),
            );
            self.stream = (api.stream_new)(self.core, c"quill-screen".as_ptr(), props);
            if self.stream.is_null() {
                return Err(PipeWireError::Failed("no stream"));
            }
            self.shared.stream.store(self.stream, Ordering::Release);
            let data = std::ptr::from_ref::<Shared>(&self.shared).cast_mut().cast();
            (api.stream_add_listener)(self.stream, &raw mut *self._hook, &EVENTS, data);
            let formats = spa_pod::enum_formats(default_size);
            let mut params: Vec<*const c_void> =
                formats.iter().map(|pod| pod.as_ptr().cast()).collect();
            let count = u32::try_from(params.len()).unwrap_or(0);
            let result = (api.stream_connect)(
                self.stream,
                DIRECTION_INPUT,
                node,
                STREAM_FLAG_AUTOCONNECT | STREAM_FLAG_MAP_BUFFERS,
                params.as_mut_ptr(),
                count,
            );
            if result < 0 {
                return Err(PipeWireError::Failed("the stream did not connect"));
            }
        }
        Ok(())
    }
}

impl Drop for PipeWireStream {
    fn drop(&mut self) {
        let api = self.api;
        // SAFETY: every pointer is null or live and owned by this stream;
        // the loop is locked while the stream and core go, then stopped
        // (joining its thread) before the context and loop are freed, so
        // no callback runs once `shared` and the hook drop.
        unsafe {
            if !self.thread_loop.is_null() {
                (api.thread_loop_lock)(self.thread_loop);
            }
            if !self.stream.is_null() {
                (api.stream_disconnect)(self.stream);
                (api.stream_destroy)(self.stream);
            }
            if !self.core.is_null() {
                (api.core_disconnect)(self.core);
            }
            if !self.thread_loop.is_null() {
                (api.thread_loop_unlock)(self.thread_loop);
                (api.thread_loop_stop)(self.thread_loop);
            }
            if !self.context.is_null() {
                (api.context_destroy)(self.context);
            }
            if !self.thread_loop.is_null() {
                (api.thread_loop_destroy)(self.thread_loop);
            }
        }
    }
}

/// # Safety
/// `data` is the `Shared` this stream registered, alive for the callback.
unsafe fn shared<'a>(data: *mut c_void) -> Option<&'a Shared> {
    // SAFETY: per the caller.
    unsafe { data.cast::<Shared>().cast_const().as_ref() }
}

unsafe extern "C" fn on_state_changed(
    data: *mut c_void,
    _old: c_int,
    state: c_int,
    error: *const c_char,
) {
    ffi_boundary((), || {
        // SAFETY: registered with this stream's `Shared`.
        let Some(shared) = (unsafe { shared(data) }) else {
            return;
        };
        let state = match state {
            STREAM_STATE_CONNECTING => StreamState::Connecting,
            STREAM_STATE_PAUSED => StreamState::Paused,
            STREAM_STATE_STREAMING => StreamState::Streaming,
            STREAM_STATE_UNCONNECTED => StreamState::Unconnected,
            // `PW_STREAM_STATE_ERROR` (-1), or a state this build does not know.
            _ => {
                let text = if error.is_null() {
                    "stream error".to_string()
                } else {
                    // SAFETY: a NUL-terminated message valid for the call.
                    unsafe { CStr::from_ptr(error) }
                        .to_string_lossy()
                        .into_owned()
                };
                StreamState::Error(text)
            }
        };
        shared.handler.lock_or_recover().state(state);
    });
}

unsafe extern "C" fn on_param_changed(data: *mut c_void, id: u32, param: *const c_void) {
    ffi_boundary((), || {
        // SAFETY: registered with this stream's `Shared`.
        let Some(shared) = (unsafe { shared(data) }) else {
            return;
        };
        if id != PARAM_FORMAT {
            return;
        }
        let format = if param.is_null() {
            None
        } else {
            // SAFETY: a pod is an 8-byte header whose first word is the
            // body size, valid for the call.
            let pod = unsafe {
                let size = param.cast::<u32>().read_unaligned() as usize;
                std::slice::from_raw_parts(param.cast::<u8>(), size.saturating_add(8))
            };
            spa_pod::parse_format(pod)
        };
        *shared.format.lock_or_recover() = format;
        if format.is_none() {
            return;
        }
        let buffers = spa_pod::buffers_param();
        let crop = spa_pod::crop_meta_param();
        let mut params: [*const c_void; 2] = [buffers.as_ptr().cast(), crop.as_ptr().cast()];
        let stream = shared.stream.load(Ordering::Acquire);
        if !stream.is_null() {
            // SAFETY: the stream is live during its own callback, and the
            // pods outlive the call (libpipewire copies them).
            unsafe { (shared.api.stream_update_params)(stream, params.as_mut_ptr(), 2) };
        }
    });
}

unsafe extern "C" fn on_process(data: *mut c_void) {
    ffi_boundary((), || {
        // SAFETY: registered with this stream's `Shared`.
        let Some(shared) = (unsafe { shared(data) }) else {
            return;
        };
        let stream = shared.stream.load(Ordering::Acquire);
        if stream.is_null() {
            return;
        }
        // Only the newest buffer matters; older ones go straight back.
        let mut newest: *mut PwBuffer = std::ptr::null_mut();
        loop {
            // SAFETY: the stream is live during its own callback.
            let next = unsafe { (shared.api.stream_dequeue_buffer)(stream) };
            if next.is_null() {
                break;
            }
            if !newest.is_null() {
                // SAFETY: a buffer this callback dequeued.
                unsafe { (shared.api.stream_queue_buffer)(stream, newest) };
            }
            newest = next;
        }
        if newest.is_null() {
            return;
        }
        let format = *shared.format.lock_or_recover();
        if let Some(format) = format {
            // SAFETY: a dequeued buffer stays valid until it is queued back
            // below.
            if let Some(picture) = unsafe { read_buffer(newest, format) } {
                shared.handler.lock_or_recover().frame(&picture);
            }
        }
        // SAFETY: the buffer dequeued above.
        unsafe { (shared.api.stream_queue_buffer)(stream, newest) };
    });
}

/// The picture in a dequeued buffer, cut to its crop region.
///
/// # Safety
/// `buffer` is a dequeued `pw_buffer`, valid for `'a`.
unsafe fn read_buffer<'a>(buffer: *mut PwBuffer, format: RawVideoFormat) -> Option<RawPicture<'a>> {
    // SAFETY: per the caller: libpipewire's buffer, its datas and metas
    // arrays (`n_datas` / `n_metas` long) and their mapped memory
    // (`maxsize` bytes at `data`, mapped by MAP_BUFFERS) are valid.
    unsafe {
        let spa = (*buffer).buffer.as_ref()?;
        let datas = std::slice::from_raw_parts(spa.datas, spa.n_datas as usize);
        let metas = if spa.metas.is_null() {
            &[][..]
        } else {
            std::slice::from_raw_parts(spa.metas, spa.n_metas as usize)
        };
        let mut planes = Vec::with_capacity(datas.len());
        for data in datas {
            if !matches!(data.kind, DATA_MEM_PTR | DATA_MEM_FD) || data.data.is_null() {
                return None;
            }
            let chunk = data.chunk.as_ref()?;
            if chunk.flags & CHUNK_FLAG_CORRUPTED != 0 {
                return None;
            }
            let mapped = std::slice::from_raw_parts(data.data.cast::<u8>(), data.maxsize as usize);
            let offset = (chunk.offset as usize).min(mapped.len());
            let bytes = mapped.get(offset..)?;
            let bytes = match chunk.size as usize {
                0 => bytes,
                size => bytes.get(..size.min(bytes.len()))?,
            };
            let stride = usize::try_from(chunk.stride).ok().filter(|s| *s > 0);
            planes.push((bytes, stride));
        }
        let picture = picture_from_planes(format, &planes)?;
        match crop_of(metas) {
            Some(crop) => picture.cropped(crop),
            None => Some(picture),
        }
    }
}

/// The crop region in a buffer's metas, when there is a usable one.
///
/// # Safety
/// Each meta's `data` holds `size` bytes.
unsafe fn crop_of(metas: &[SpaMeta]) -> Option<Crop> {
    let meta = metas
        .iter()
        .find(|meta| meta.kind == META_VIDEO_CROP && meta.size as usize >= 16)?;
    // SAFETY: per the caller, a `spa_meta_region` is there.
    let region = unsafe { meta.data.cast::<SpaMetaRegion>().as_ref()? };
    if region.width == 0 || region.height == 0 {
        return None;
    }
    Some(Crop {
        x: usize::try_from(region.x).ok()?,
        y: usize::try_from(region.y).ok()?,
        width: region.width as usize,
        height: region.height as usize,
    })
}

/// A picture from a buffer's planes: `(bytes, stride)` per data block.
/// A single block of a planar format holds every plane.
fn picture_from_planes<'a>(
    format: RawVideoFormat,
    planes: &[(&'a [u8], Option<usize>)],
) -> Option<RawPicture<'a>> {
    let (width, height) = (format.width as usize, format.height as usize);
    let default_stride = match format.format {
        VideoFormat::I420 | VideoFormat::Nv12 => width,
        _ => width.checked_mul(4)?,
    };
    let wanted = match format.format {
        VideoFormat::I420 => 3,
        VideoFormat::Nv12 => 2,
        _ => 1,
    };
    let (first, first_stride) = *planes.first()?;
    let first_stride = first_stride.unwrap_or(default_stride);
    if planes.len() < wanted {
        return RawPicture::from_single_buffer(format.format, width, height, first, first_stride);
    }
    let chroma_default = match format.format {
        VideoFormat::Nv12 => width,
        _ => width.div_ceil(2),
    };
    let planes = planes
        .iter()
        .take(wanted)
        .enumerate()
        .map(|(index, (data, stride))| RawPlane {
            data,
            stride: stride.unwrap_or(if index == 0 {
                default_stride
            } else {
                chroma_default
            }),
        })
        .collect();
    Some(RawPicture {
        format: format.format,
        width,
        height,
        planes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planes_come_from_one_block_or_several() {
        let format = RawVideoFormat {
            format: VideoFormat::I420,
            width: 4,
            height: 2,
        };
        let block = [0u8; 12];
        let one = picture_from_planes(format, &[(&block, None)]).unwrap();
        assert_eq!(one.planes.len(), 3);
        assert_eq!(one.planes[1].stride, 2);
        let (y, u, v) = ([1u8; 8], [2u8; 2], [3u8; 2]);
        let three =
            picture_from_planes(format, &[(&y, Some(4)), (&u, None), (&v, Some(2))]).unwrap();
        assert_eq!(three.planes[2].data, &v);
        let bgrx = RawVideoFormat {
            format: VideoFormat::Bgrx,
            width: 2,
            height: 2,
        };
        let packed = [0u8; 16];
        assert_eq!(
            picture_from_planes(bgrx, &[(&packed, None)])
                .unwrap()
                .planes[0]
                .stride,
            8
        );
        assert!(picture_from_planes(bgrx, &[]).is_none());
    }

    /// Needs a running PipeWire daemon with a video source, e.g.
    /// `gst-launch-1.0 videotestsrc ! video/x-raw,format=BGRx,width=640,height=480 ! pipewiresink`,
    /// and `QUILL_PW_TEST_NODE=<node id>` (or nothing for any node).
    #[test]
    #[ignore]
    fn receives_frames_from_a_real_pipewire_node() {
        use std::sync::Arc;
        use std::sync::atomic::AtomicUsize;
        struct Count(Arc<AtomicUsize>, Arc<Mutex<Vec<StreamState>>>);
        impl StreamHandler for Count {
            fn frame(&mut self, picture: &RawPicture<'_>) {
                assert!(picture.width > 0 && picture.height > 0);
                let i420 = super::super::screen_raw::raw_to_i420(picture, (320, 240));
                assert!(i420.is_some(), "{:?}", picture.format);
                self.0.fetch_add(1, Ordering::Relaxed);
            }
            fn state(&mut self, state: StreamState) {
                self.1.lock().unwrap().push(state);
            }
        }
        let node = std::env::var("QUILL_PW_TEST_NODE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(ANY_NODE);
        let frames = Arc::new(AtomicUsize::new(0));
        let states = Arc::new(Mutex::new(Vec::new()));
        let stream = PipeWireStream::connect(
            None,
            node,
            (640, 480),
            Box::new(Count(frames.clone(), states.clone())),
        )
        .expect("connect");
        let started = std::time::Instant::now();
        while frames.load(Ordering::Relaxed) < 10 && started.elapsed().as_secs() < 10 {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        drop(stream);
        let states = states.lock().unwrap().clone();
        eprintln!("states: {states:?}");
        assert!(frames.load(Ordering::Relaxed) >= 10, "frames arrived");
        assert!(states.contains(&StreamState::Streaming));
    }
}
