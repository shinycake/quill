//! A PipeWire video stream from a screen cast portal session, through
//! libpipewire-0.3 loaded at run time (`libloading`), so Quill neither
//! needs PipeWire headers to build nor fails to start where PipeWire is
//! missing: screen sharing on Wayland then says so instead.
//!
//! One stream runs on its own `pw_thread_loop`. Quill offers raw video
//! (`spa_pod::video_enum_format`, no DMA-BUF modifier, so buffers are
//! shared memory) and asks PipeWire to map them. The callbacks hand the
//! negotiated format, each buffer's planes and state changes to a
//! [`StreamHandler`]; they run on PipeWire's thread with its loop locked.
//!
//! The declarations follow `pipewire/stream.h`, `pipewire/thread-loop.h`,
//! `pipewire/context.h` and `spa/buffer/buffer.h` of PipeWire 0.3, whose
//! ABI has been stable since 0.3.0.
#![deny(clippy::indexing_slicing)]

use super::spa_pod::{NegotiatedFormat, PARAM_FORMAT, parse_video_format, video_enum_format};
use std::ffi::{CStr, c_char, c_int, c_void};
use std::os::fd::{IntoRawFd, OwnedFd};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr::{null, null_mut};
use std::sync::OnceLock;

/// The PipeWire stream states (`enum pw_stream_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamState {
    Error,
    Unconnected,
    Connecting,
    Paused,
    Streaming,
}

impl StreamState {
    fn from_raw(state: c_int) -> Self {
        match state {
            1 => StreamState::Connecting,
            2 => StreamState::Paused,
            3 => StreamState::Streaming,
            0 => StreamState::Unconnected,
            _ => StreamState::Error,
        }
    }
}

/// One data block of a buffer: its bytes and row stride.
#[derive(Debug, Clone, Copy)]
pub struct PlaneData<'a> {
    pub bytes: &'a [u8],
    /// Bytes between rows; 0 when the producer did not say.
    pub stride: usize,
}

/// Receives what the stream delivers, on PipeWire's thread.
pub trait StreamHandler: Send {
    /// The compositor fixed (or cleared) the format.
    fn format_changed(&mut self, format: Option<NegotiatedFormat>);
    /// A buffer's data blocks, valid only during the call.
    fn frame(&mut self, planes: &[PlaneData<'_>]);
    fn state_changed(&mut self, state: StreamState, error: Option<String>);
}

#[repr(C)]
struct SpaChunk {
    offset: u32,
    size: u32,
    stride: i32,
    flags: i32,
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
struct SpaBuffer {
    n_metas: u32,
    n_datas: u32,
    metas: *mut c_void,
    datas: *mut SpaData,
}

/// The head of `struct pw_buffer`; only `buffer` is read.
#[repr(C)]
struct PwBuffer {
    buffer: *mut SpaBuffer,
}

/// `struct spa_hook` is six pointers; extra room costs nothing.
#[repr(C)]
struct SpaHook([usize; 16]);

/// `struct pw_stream_events`, version 2.
#[repr(C)]
struct StreamEvents {
    version: u32,
    destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    state_changed: Option<unsafe extern "C" fn(*mut c_void, c_int, c_int, *const c_char)>,
    control_info: Option<unsafe extern "C" fn(*mut c_void, u32, *const c_void)>,
    io_changed: Option<unsafe extern "C" fn(*mut c_void, u32, *mut c_void, u32)>,
    param_changed: Option<unsafe extern "C" fn(*mut c_void, u32, *const u32)>,
    add_buffer: Option<unsafe extern "C" fn(*mut c_void, *mut c_void)>,
    remove_buffer: Option<unsafe extern "C" fn(*mut c_void, *mut c_void)>,
    process: Option<unsafe extern "C" fn(*mut c_void)>,
    drained: Option<unsafe extern "C" fn(*mut c_void)>,
    command: Option<unsafe extern "C" fn(*mut c_void, *const c_void)>,
    trigger_done: Option<unsafe extern "C" fn(*mut c_void)>,
}

const STREAM_EVENTS_VERSION: u32 = 2;
/// `SPA_DIRECTION_INPUT`.
const DIRECTION_INPUT: u32 = 0;
/// `PW_STREAM_FLAG_AUTOCONNECT | PW_STREAM_FLAG_MAP_BUFFERS`.
const STREAM_FLAGS: u32 = (1 << 0) | (1 << 2);
/// `SPA_CHUNK_FLAG_CORRUPTED`.
const CHUNK_CORRUPTED: i32 = 1;

type Opaque = *mut c_void;

struct Api {
    thread_loop_new: unsafe extern "C" fn(*const c_char, *const c_void) -> Opaque,
    thread_loop_start: unsafe extern "C" fn(Opaque) -> c_int,
    thread_loop_stop: unsafe extern "C" fn(Opaque),
    thread_loop_destroy: unsafe extern "C" fn(Opaque),
    thread_loop_lock: unsafe extern "C" fn(Opaque),
    thread_loop_unlock: unsafe extern "C" fn(Opaque),
    thread_loop_get_loop: unsafe extern "C" fn(Opaque) -> Opaque,
    context_new: unsafe extern "C" fn(Opaque, Opaque, usize) -> Opaque,
    context_destroy: unsafe extern "C" fn(Opaque),
    context_connect_fd: unsafe extern "C" fn(Opaque, c_int, Opaque, usize) -> Opaque,
    core_disconnect: unsafe extern "C" fn(Opaque) -> c_int,
    properties_new_string: unsafe extern "C" fn(*const c_char) -> Opaque,
    stream_new: unsafe extern "C" fn(Opaque, *const c_char, Opaque) -> Opaque,
    stream_add_listener: unsafe extern "C" fn(Opaque, *mut SpaHook, *const StreamEvents, Opaque),
    stream_connect: unsafe extern "C" fn(Opaque, u32, u32, u32, *mut *const c_void, u32) -> c_int,
    stream_disconnect: unsafe extern "C" fn(Opaque) -> c_int,
    stream_destroy: unsafe extern "C" fn(Opaque),
    stream_dequeue_buffer: unsafe extern "C" fn(Opaque) -> *mut PwBuffer,
    stream_queue_buffer: unsafe extern "C" fn(Opaque, *mut PwBuffer) -> c_int,
    /// Keeps the functions above loaded.
    _library: libloading::Library,
}

fn api() -> Option<&'static Api> {
    static API: OnceLock<Option<Api>> = OnceLock::new();
    API.get_or_init(load).as_ref()
}

/// Whether libpipewire-0.3 could be loaded.
pub fn available() -> bool {
    api().is_some()
}

fn load() -> Option<Api> {
    let library = ["libpipewire-0.3.so.0", "libpipewire-0.3.so"]
        .iter()
        .find_map(|name| {
            // SAFETY: libpipewire's initialisers only set up its own state.
            unsafe { libloading::Library::new(*name) }.ok()
        })?;
    macro_rules! symbol {
        ($name:literal) => {
            // SAFETY: the declared signature matches PipeWire 0.3's headers
            // (see the module docs).
            *unsafe { library.get(concat!($name, "\0").as_bytes()) }.ok()?
        };
    }
    let init: unsafe extern "C" fn(*mut c_int, *mut *mut *mut c_char) = symbol!("pw_init");
    // SAFETY: `pw_init` accepts null arguments and may be called again.
    unsafe { init(null_mut(), null_mut()) };
    Some(Api {
        thread_loop_new: symbol!("pw_thread_loop_new"),
        thread_loop_start: symbol!("pw_thread_loop_start"),
        thread_loop_stop: symbol!("pw_thread_loop_stop"),
        thread_loop_destroy: symbol!("pw_thread_loop_destroy"),
        thread_loop_lock: symbol!("pw_thread_loop_lock"),
        thread_loop_unlock: symbol!("pw_thread_loop_unlock"),
        thread_loop_get_loop: symbol!("pw_thread_loop_get_loop"),
        context_new: symbol!("pw_context_new"),
        context_destroy: symbol!("pw_context_destroy"),
        context_connect_fd: symbol!("pw_context_connect_fd"),
        core_disconnect: symbol!("pw_core_disconnect"),
        properties_new_string: symbol!("pw_properties_new_string"),
        stream_new: symbol!("pw_stream_new"),
        stream_add_listener: symbol!("pw_stream_add_listener"),
        stream_connect: symbol!("pw_stream_connect"),
        stream_disconnect: symbol!("pw_stream_disconnect"),
        stream_destroy: symbol!("pw_stream_destroy"),
        stream_dequeue_buffer: symbol!("pw_stream_dequeue_buffer"),
        stream_queue_buffer: symbol!("pw_stream_queue_buffer"),
        _library: library,
    })
}

/// What the callbacks see: the stream (for buffers) and the handler.
struct CallbackData {
    api: &'static Api,
    stream: Opaque,
    handler: Box<dyn StreamHandler>,
}

static EVENTS: StreamEvents = StreamEvents {
    version: STREAM_EVENTS_VERSION,
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

/// Runs `body` with the callback data, never unwinding into C.
///
/// # Safety
/// `data` must be the `CallbackData` the stream was registered with.
unsafe fn with_data(data: *mut c_void, body: impl FnOnce(&mut CallbackData)) {
    // SAFETY: PipeWire passes back the pointer `PwStream::connect` gave
    // it, alive until the stream is destroyed; callbacks never overlap.
    let Some(data) = (unsafe { data.cast::<CallbackData>().as_mut() }) else {
        return;
    };
    let _ = catch_unwind(AssertUnwindSafe(|| body(data)));
}

unsafe extern "C" fn on_state_changed(
    data: *mut c_void,
    _old: c_int,
    state: c_int,
    error: *const c_char,
) {
    // SAFETY: called by PipeWire with our data; `error` is null or a C
    // string valid for the call.
    unsafe {
        with_data(data, |data| {
            let error =
                (!error.is_null()).then(|| CStr::from_ptr(error).to_string_lossy().into_owned());
            data.handler
                .state_changed(StreamState::from_raw(state), error);
        });
    }
}

unsafe extern "C" fn on_param_changed(data: *mut c_void, id: u32, param: *const u32) {
    if id != PARAM_FORMAT {
        return;
    }
    // SAFETY: called by PipeWire with our data; `param` is null or a POD
    // whose header gives its body size.
    unsafe {
        with_data(data, |data| {
            let format = if param.is_null() {
                None
            } else {
                let size = (*param as usize).saturating_add(8);
                parse_video_format(std::slice::from_raw_parts(param.cast::<u8>(), size))
            };
            data.handler.format_changed(format);
        });
    }
}

unsafe extern "C" fn on_process(data: *mut c_void) {
    // SAFETY: called by PipeWire with our data, on its thread with the
    // loop locked: buffers dequeued here are ours until queued back.
    unsafe {
        with_data(data, |data| {
            let buffer = (data.api.stream_dequeue_buffer)(data.stream);
            if buffer.is_null() {
                return;
            }
            let planes = planes_of(buffer);
            if !planes.is_empty() {
                data.handler.frame(&planes);
            }
            (data.api.stream_queue_buffer)(data.stream, buffer);
        });
    }
}

/// The mapped data blocks of a dequeued buffer; empty when any is not
/// mapped or is marked corrupted.
///
/// # Safety
/// `buffer` must be a buffer dequeued from a stream and not yet queued.
unsafe fn planes_of<'a>(buffer: *mut PwBuffer) -> Vec<PlaneData<'a>> {
    // SAFETY: the caller's buffer; PipeWire keeps its `spa_buffer`, data
    // array and chunks valid until it is queued back.
    unsafe {
        let Some(spa) = (*buffer).buffer.as_ref() else {
            return Vec::new();
        };
        if spa.datas.is_null() {
            return Vec::new();
        }
        let datas = std::slice::from_raw_parts(spa.datas, spa.n_datas as usize);
        let mut planes = Vec::with_capacity(datas.len());
        for data in datas {
            let Some(chunk) = data.chunk.as_ref() else {
                return Vec::new();
            };
            if data.data.is_null() || chunk.flags & CHUNK_CORRUPTED != 0 {
                return Vec::new();
            }
            let max = data.maxsize as usize;
            let offset = (chunk.offset as usize).min(max);
            let size = (chunk.size as usize).min(max - offset);
            let bytes = std::slice::from_raw_parts(data.data.cast::<u8>().add(offset), size);
            planes.push(PlaneData {
                bytes,
                stride: usize::try_from(chunk.stride).unwrap_or(0),
            });
        }
        planes
    }
}

/// A connected stream. Dropping it disconnects and frees everything.
pub struct PwStream {
    api: &'static Api,
    thread_loop: Opaque,
    context: Opaque,
    core: Opaque,
    stream: Opaque,
    hook: *mut SpaHook,
    data: *mut CallbackData,
}

impl PwStream {
    /// Connect to the portal's PipeWire remote `fd` and stream `node`.
    pub fn connect(
        fd: OwnedFd,
        node: u32,
        handler: Box<dyn StreamHandler>,
    ) -> Result<Self, String> {
        let api = api().ok_or("PipeWire (libpipewire-0.3) is not installed.")?;
        let mut this = Self {
            api,
            thread_loop: null_mut(),
            context: null_mut(),
            core: null_mut(),
            stream: null_mut(),
            hook: Box::into_raw(Box::new(SpaHook([0; 16]))),
            data: Box::into_raw(Box::new(CallbackData {
                api,
                stream: null_mut(),
                handler,
            })),
        };
        // SAFETY: each call gets the objects made just before it; on any
        // failure `Drop` frees what exists, in reverse order.
        unsafe {
            this.thread_loop = (api.thread_loop_new)(c"quill-screen".as_ptr(), null());
            if this.thread_loop.is_null() {
                return Err("Couldn't start PipeWire.".into());
            }
            let main_loop = (api.thread_loop_get_loop)(this.thread_loop);
            this.context = (api.context_new)(main_loop, null_mut(), 0);
            if this.context.is_null() || (api.thread_loop_start)(this.thread_loop) < 0 {
                return Err("Couldn't start PipeWire.".into());
            }
            (api.thread_loop_lock)(this.thread_loop);
            let connected = this.connect_locked(fd, node);
            (api.thread_loop_unlock)(this.thread_loop);
            connected?;
        }
        Ok(this)
    }

    /// # Safety
    /// The thread loop must be locked and the context made.
    unsafe fn connect_locked(&mut self, fd: OwnedFd, node: u32) -> Result<(), String> {
        let api = self.api;
        // SAFETY: the caller holds the loop lock; PipeWire owns `fd` from
        // here (it closes it with the core).
        unsafe {
            self.core = (api.context_connect_fd)(self.context, fd.into_raw_fd(), null_mut(), 0);
            if self.core.is_null() {
                return Err("Couldn't connect to PipeWire.".into());
            }
            let props = (api.properties_new_string)(
                c"media.type=Video media.category=Capture media.role=Screen".as_ptr(),
            );
            self.stream = (api.stream_new)(self.core, c"Quill screen".as_ptr(), props);
            if self.stream.is_null() {
                return Err("Couldn't open the screen stream.".into());
            }
            if let Some(data) = self.data.as_mut() {
                data.stream = self.stream;
            }
            (api.stream_add_listener)(self.stream, self.hook, &EVENTS, self.data.cast());
            let offer = video_enum_format();
            let mut params = [offer.as_ptr().cast::<c_void>()];
            let result = (api.stream_connect)(
                self.stream,
                DIRECTION_INPUT,
                node,
                STREAM_FLAGS,
                params.as_mut_ptr(),
                1,
            );
            if result < 0 {
                return Err("Couldn't open the screen stream.".into());
            }
        }
        Ok(())
    }
}

impl Drop for PwStream {
    fn drop(&mut self) {
        let api = self.api;
        // SAFETY: tear down in reverse order of creation, the stream and
        // core under the loop lock; the loop is stopped before the context
        // and itself are destroyed, after which no callback can run and
        // the hook and data are freed.
        unsafe {
            if !self.thread_loop.is_null() {
                (api.thread_loop_lock)(self.thread_loop);
                if !self.stream.is_null() {
                    (api.stream_disconnect)(self.stream);
                    (api.stream_destroy)(self.stream);
                }
                if !self.core.is_null() {
                    (api.core_disconnect)(self.core);
                }
                (api.thread_loop_unlock)(self.thread_loop);
                (api.thread_loop_stop)(self.thread_loop);
            }
            if !self.context.is_null() {
                (api.context_destroy)(self.context);
            }
            if !self.thread_loop.is_null() {
                (api.thread_loop_destroy)(self.thread_loop);
            }
            drop(Box::from_raw(self.hook));
            drop(Box::from_raw(self.data));
        }
    }
}
