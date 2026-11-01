//! Windows screen capture through Windows.Graphics.Capture (Windows 10
//! 1903 and later; older systems get [`CaptureError::Unsupported`]):
//! - Screens come from `EnumDisplayMonitors` (the primary first), windows
//!   from `EnumWindows`: visible, unowned, titled, not tool windows, not
//!   cloaked (`DWMWA_CLOAKED`: other virtual desktops, suspended UWP
//!   apps), not minimised, at least 100 x 100, and not Quill's own.
//! - `IGraphicsCaptureItemInterop` makes the capture item for a monitor
//!   or window. A free-threaded `Direct3D11CaptureFramePool` (BGRA, one
//!   buffer) delivers frames on a system thread; each due frame (15 fps)
//!   is copied to a staging texture, mapped, and converted to I420 at the
//!   call's size (`screen_bgra`), then pushed into the sink.
//! - Thumbnails: one frame of the same capture, then it stops (tdesktop's
//!   chooser shows one frame per source).
//! - A shared window is polled every 500 ms: minimised (`IsIconic`) is
//!   `Paused`, restored is `Resumed`, gone is `Ended`. The item's `Closed`
//!   event (window closed, monitor unplugged) is `Ended` too.
//!
//! No screen recording permission exists on Windows, so access is always
//! granted.
#![deny(clippy::indexing_slicing)]

use super::screen::{
    PreviewSlot, ScreenAccess, ScreenBackend, ScreenCapture, ScreenOptions, ScreenSource, SourceId,
    SourceKind, Thumbnail, fit_capture_size,
};
use super::screen_bgra::{FramePacer, bgra_thumbnail, bgra_to_i420};
use super::{CaptureError, CaptureEvent, FrameSink, VideoFrame};
use crate::locks::LockExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::Foundation::Metadata::ApiInformation;
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{
    Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::SizeInt32;
use windows::Win32::Foundation::{HMODULE, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ,
    D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
};
use windows::Win32::System::WinRT::Direct3D11::{
    CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GWL_EXSTYLE, GetWindow, GetWindowLongW, GetWindowRect, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, MONITORINFOF_PRIMARY,
    WS_EX_TOOLWINDOW,
};
use windows::core::{BOOL, HSTRING, IInspectable, Interface};

/// How often a shared window's state is checked (the design's 500 ms).
const PAUSE_POLL: Duration = Duration::from_millis(500);
/// How long a thumbnail waits for its frame.
const THUMBNAIL_TIMEOUT: Duration = Duration::from_secs(2);
/// Windows smaller than this are toolbars and helpers, not shareable.
const MIN_WINDOW_SIDE: i32 = 100;

/// The Windows.Graphics.Capture backend. Stateless: every call asks the
/// system.
pub struct WinScreen;

impl ScreenBackend for WinScreen {
    fn access(&self) -> ScreenAccess {
        ScreenAccess::Granted
    }

    fn ask_access(&self) -> Option<&'static str> {
        None
    }

    fn sources(&self) -> Result<Vec<ScreenSource>, CaptureError> {
        ensure_supported()?;
        let mut sources = monitors();
        sources.extend(windows());
        Ok(sources)
    }

    fn thumbnail(&self, source: &ScreenSource, max: (u32, u32)) -> Option<Thumbnail> {
        ensure_supported().ok()?;
        let gpu = Gpu::new().ok()?;
        let item = capture_item(&source.id).ok()?;
        let size = item.Size().ok()?;
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &gpu.winrt,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            1,
            size,
        )
        .ok()?;
        let session = pool.CreateCaptureSession(&item).ok()?;
        let _ = session.SetIsCursorCaptureEnabled(false);
        // Windows 11 can skip the yellow border; older systems refuse.
        let _ = session.SetIsBorderRequired(false);
        session.StartCapture().ok()?;
        let started = Instant::now();
        let mut thumbnail = None;
        while started.elapsed() < THUMBNAIL_TIMEOUT {
            if let Ok(frame) = pool.TryGetNextFrame() {
                let mut staging = None;
                thumbnail = gpu.read(&frame, &mut staging, |bgra, stride, width, height| {
                    bgra_thumbnail(bgra, stride, width, height, max)
                });
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = session.Close();
        let _ = pool.Close();
        thumbnail.flatten()
    }

    fn start(
        &self,
        source: &SourceId,
        options: ScreenOptions,
        sink: FrameSink<VideoFrame>,
        preview: Arc<PreviewSlot>,
    ) -> Result<Box<dyn ScreenCapture>, CaptureError> {
        ensure_supported()?;
        let gpu = Gpu::new().map_err(|err| device_error(&err))?;
        let item = capture_item(source)
            .map_err(|_| CaptureError::Device("The shared window or screen is gone.".into()))?;
        let size = item.Size().map_err(|err| device_error(&err))?;
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &gpu.winrt,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            1,
            size,
        )
        .map_err(|err| device_error(&err))?;
        let session = pool
            .CreateCaptureSession(&item)
            .map_err(|err| device_error(&err))?;
        let _ = session.SetIsCursorCaptureEnabled(true);
        let _ = session.SetIsBorderRequired(false);
        let shared = Arc::new(Shared {
            sink: sink.clone(),
            preview,
            stopped: AtomicBool::new(false),
            stream: Mutex::new(StreamState {
                gpu,
                staging: None,
                pacer: FramePacer::new(options.fps),
                size,
                options,
            }),
        });
        let on_frame = shared.clone();
        let frame_token = pool
            .FrameArrived(&TypedEventHandler::new(
                move |pool: windows::core::Ref<Direct3D11CaptureFramePool>, _| {
                    if let Some(pool) = pool.as_ref() {
                        on_frame.on_frame(pool);
                    }
                    Ok(())
                },
            ))
            .map_err(|err| device_error(&err))?;
        let on_closed = shared.clone();
        let closed_token = item
            .Closed(&TypedEventHandler::new(move |_, _| {
                on_closed.end();
                Ok(())
            }))
            .map_err(|err| device_error(&err))?;
        session.StartCapture().map_err(|err| device_error(&err))?;
        let monitor = window_handle(source)
            .and_then(|hwnd| WindowMonitor::start(hwnd, sink, shared.clone()).ok());
        Ok(Box::new(WinCapture {
            shared,
            item,
            pool,
            session,
            frame_token,
            closed_token,
            monitor,
            closed: false,
        }))
    }
}

/// Windows 10 1903 (UniversalApiContract 8) brought the capture item
/// interop the backend needs.
fn ensure_supported() -> Result<(), CaptureError> {
    // SAFETY: plain initialisation of this thread's WinRT apartment; an
    // already initialised thread returns an error we ignore.
    let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
    let contract = ApiInformation::IsApiContractPresentByMajor(
        &HSTRING::from("Windows.Foundation.UniversalApiContract"),
        8,
    )
    .unwrap_or(false);
    let supported = contract && GraphicsCaptureSession::IsSupported().unwrap_or(false);
    if supported {
        Ok(())
    } else {
        Err(CaptureError::Unsupported(
            crate::lang::tr(crate::lang::keys::Q_OPS_SCREEN_UNSUPPORTED).to_string(),
        ))
    }
}

fn device_error(err: &windows::core::Error) -> CaptureError {
    CaptureError::Device(format!("Screen capture failed: {}", err.message()))
}

/// The D3D11 device frames are read with, and its WinRT face for the
/// frame pool.
struct Gpu {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    winrt: IDirect3DDevice,
}

// SAFETY: the D3D11 device is free-threaded. The immediate context is
// not, so it is only used behind `Shared::stream`'s mutex (or on the one
// thread that made a thumbnail's `Gpu`).
unsafe impl Send for Gpu {}

impl Gpu {
    fn new() -> windows::core::Result<Self> {
        let mut device = None;
        let mut context = None;
        // SAFETY: out-pointers to locals, filled on success.
        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )?;
        }
        let device: ID3D11Device = device.ok_or_else(windows::core::Error::empty)?;
        let context = context.ok_or_else(windows::core::Error::empty)?;
        let dxgi: IDXGIDevice = device.cast()?;
        // SAFETY: a live DXGI device.
        let inspectable: IInspectable = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi)? };
        Ok(Self {
            device,
            context,
            winrt: inspectable.cast()?,
        })
    }

    /// Copy `frame` into a CPU-readable texture (made or remade in
    /// `staging` to the frame's size) and hand its BGRA rows to `read`:
    /// `(pixels, stride, width, height)`, cropped to the content size.
    fn read<T>(
        &self,
        frame: &Direct3D11CaptureFrame,
        staging: &mut Option<ID3D11Texture2D>,
        read: impl FnOnce(&[u8], usize, usize, usize) -> T,
    ) -> Option<T> {
        let content = frame.ContentSize().ok()?;
        let access: IDirect3DDxgiInterfaceAccess = frame.Surface().ok()?.cast().ok()?;
        // SAFETY: the surface wraps a D3D11 texture.
        let texture: ID3D11Texture2D = unsafe { access.GetInterface().ok()? };
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        // SAFETY: fills the local description.
        unsafe { texture.GetDesc(&mut desc) };
        let fits = staging.as_ref().is_some_and(|current| {
            let mut have = D3D11_TEXTURE2D_DESC::default();
            // SAFETY: fills the local description.
            unsafe { current.GetDesc(&mut have) };
            have.Width == desc.Width && have.Height == desc.Height
        });
        if !fits {
            let wanted = D3D11_TEXTURE2D_DESC {
                Usage: D3D11_USAGE_STAGING,
                BindFlags: 0,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                MiscFlags: 0,
                ..desc
            };
            let mut made = None;
            // SAFETY: a valid description and out-pointer.
            unsafe { self.device.CreateTexture2D(&wanted, None, Some(&mut made)) }.ok()?;
            *staging = made;
        }
        let target = staging.as_ref()?;
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        // SAFETY: both textures are live and the same size; the map is
        // released below before the slice is dropped.
        unsafe {
            self.context.CopyResource(target, &texture);
            self.context
                .Map(target, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .ok()?;
        }
        let width = (desc.Width as usize).min(usize::try_from(content.Width).unwrap_or(0));
        let height = (desc.Height as usize).min(usize::try_from(content.Height).unwrap_or(0));
        let stride = mapped.RowPitch as usize;
        let result = (!mapped.pData.is_null() && width > 0 && height > 0)
            .then(|| {
                // SAFETY: a mapped staging texture holds `RowPitch * Height`
                // readable bytes until `Unmap`.
                let bytes = unsafe {
                    std::slice::from_raw_parts(
                        mapped.pData.cast::<u8>(),
                        stride.checked_mul(desc.Height as usize)?,
                    )
                };
                Some(read(bytes, stride, width, height))
            })
            .flatten();
        // SAFETY: balances the map above.
        unsafe { self.context.Unmap(target, 0) };
        result
    }
}

/// What the frame callback needs, behind one lock.
struct StreamState {
    gpu: Gpu,
    staging: Option<ID3D11Texture2D>,
    pacer: FramePacer,
    /// The pool's buffer size; a resized window remakes the pool.
    size: SizeInt32,
    options: ScreenOptions,
}

struct Shared {
    sink: FrameSink<VideoFrame>,
    preview: Arc<PreviewSlot>,
    stopped: AtomicBool,
    stream: Mutex<StreamState>,
}

impl Shared {
    fn on_frame(&self, pool: &Direct3D11CaptureFramePool) {
        // Always take the frame, so the pool's one buffer comes back.
        let Ok(frame) = pool.TryGetNextFrame() else {
            return;
        };
        if self.stopped.load(Ordering::Acquire) {
            return;
        }
        let now = Instant::now();
        let mut state = self.stream.lock_or_recover();
        if let Ok(content) = frame.ContentSize()
            && (content.Width != state.size.Width || content.Height != state.size.Height)
            && content.Width > 0
            && content.Height > 0
        {
            state.size = content;
            let _ = pool.Recreate(
                &state.gpu.winrt,
                DirectXPixelFormat::B8G8R8A8UIntNormalized,
                1,
                content,
            );
            return;
        }
        if !state.pacer.due(now) {
            return;
        }
        let options = state.options;
        let StreamState { gpu, staging, .. } = &mut *state;
        let converted = gpu
            .read(&frame, staging, |bgra, stride, width, height| {
                let (w, h) = fit_capture_size(
                    u32::try_from(width).ok()?,
                    u32::try_from(height).ok()?,
                    options.max_width,
                    options.max_height,
                );
                let i420 = bgra_to_i420(bgra, stride, width, height, w as usize, h as usize)?;
                Some(VideoFrame {
                    i420,
                    width: u16::try_from(w).ok()?,
                    height: u16::try_from(h).ok()?,
                    captured_at: now,
                })
            })
            .flatten();
        drop(state);
        if let Some(frame) = converted {
            self.preview.offer(&frame);
            self.sink.push(frame);
        }
    }

    /// The source closed: `Ended`, once.
    fn end(&self) {
        if !self.stopped.swap(true, Ordering::AcqRel) {
            self.sink.push_event(CaptureEvent::Ended);
        }
    }
}

/// A running capture session.
struct WinCapture {
    shared: Arc<Shared>,
    item: GraphicsCaptureItem,
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    frame_token: i64,
    closed_token: i64,
    monitor: Option<WindowMonitor>,
    closed: bool,
}

impl ScreenCapture for WinCapture {
    fn stop(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;
        self.shared.stopped.store(true, Ordering::Release);
        self.monitor = None;
        let _ = self.pool.RemoveFrameArrived(self.frame_token);
        let _ = self.item.RemoveClosed(self.closed_token);
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}

impl Drop for WinCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Watches a shared window: minimised is `Paused`, restored `Resumed`,
/// gone `Ended`.
struct WindowMonitor {
    stop: mpsc::Sender<()>,
    worker: Option<std::thread::JoinHandle<Option<()>>>,
}

impl WindowMonitor {
    fn start(
        hwnd: isize,
        sink: FrameSink<VideoFrame>,
        shared: Arc<Shared>,
    ) -> std::io::Result<Self> {
        let (stop, stopped) = mpsc::channel::<()>();
        let worker = crate::thread_guard::spawn_guarded("quill-screen-pause", move || {
            let window = HWND(hwnd as *mut core::ffi::c_void);
            let mut paused = false;
            while let Err(mpsc::RecvTimeoutError::Timeout) = stopped.recv_timeout(PAUSE_POLL) {
                if shared.stopped.load(Ordering::Acquire) {
                    return;
                }
                // SAFETY: plain queries on a window handle; a stale
                // handle reads as gone.
                let (exists, minimised) =
                    unsafe { (IsWindow(Some(window)).as_bool(), IsIconic(window).as_bool()) };
                if !exists {
                    shared.end();
                    return;
                }
                if minimised != paused {
                    paused = minimised;
                    sink.push_event(if paused {
                        CaptureEvent::Paused
                    } else {
                        CaptureEvent::Resumed
                    });
                }
            }
        })?;
        Ok(Self {
            stop,
            worker: Some(worker),
        })
    }
}

impl Drop for WindowMonitor {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// The capture item for a `display:<HMONITOR>` or `window:<HWND>` id.
fn capture_item(id: &SourceId) -> windows::core::Result<GraphicsCaptureItem> {
    let interop = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
    if let Some(handle) =
        id.0.strip_prefix("display:")
            .and_then(|raw| raw.parse::<isize>().ok())
    {
        // SAFETY: a monitor handle from `EnumDisplayMonitors`; a stale one
        // fails cleanly.
        return unsafe { interop.CreateForMonitor(HMONITOR(handle as *mut core::ffi::c_void)) };
    }
    let handle = window_handle(id).ok_or_else(windows::core::Error::empty)?;
    // SAFETY: a window handle from `EnumWindows`; a stale one fails cleanly.
    unsafe { interop.CreateForWindow(HWND(handle as *mut core::ffi::c_void)) }
}

fn window_handle(id: &SourceId) -> Option<isize> {
    id.0.strip_prefix("window:")?.parse().ok()
}

/// Every monitor, the primary first.
fn monitors() -> Vec<ScreenSource> {
    let mut found: Vec<(bool, ScreenSource)> = Vec::new();
    // SAFETY: the callback only pushes into `found`, which outlives the
    // synchronous enumeration.
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(monitor_proc),
            LPARAM(std::ptr::addr_of_mut!(found) as isize),
        );
    }
    found.sort_by_key(|(primary, _)| !*primary);
    found.into_iter().map(|(_, source)| source).collect()
}

unsafe extern "system" fn monitor_proc(
    monitor: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    // SAFETY: `data` is the `Vec` `monitors` passed, alive for the call.
    let Some(found) = (unsafe { (data.0 as *mut Vec<(bool, ScreenSource)>).as_mut() }) else {
        return BOOL(0);
    };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: a monitor from the enumeration and a sized MONITORINFO.
    if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        let rect = info.rcMonitor;
        found.push((
            info.dwFlags & MONITORINFOF_PRIMARY != 0,
            ScreenSource {
                id: SourceId(format!("display:{}", monitor.0 as isize)),
                kind: SourceKind::Screen,
                title: String::new(),
                size: (
                    u32::try_from(rect.right - rect.left).unwrap_or(0),
                    u32::try_from(rect.bottom - rect.top).unwrap_or(0),
                ),
            },
        ));
    }
    BOOL(1)
}

/// Top-level windows worth sharing, in Z order.
fn windows() -> Vec<ScreenSource> {
    let mut found: Vec<ScreenSource> = Vec::new();
    // SAFETY: the callback only pushes into `found`, which outlives the
    // synchronous enumeration.
    unsafe {
        let _ = EnumWindows(
            Some(window_proc),
            LPARAM(std::ptr::addr_of_mut!(found) as isize),
        );
    }
    found
}

unsafe extern "system" fn window_proc(hwnd: HWND, data: LPARAM) -> BOOL {
    // SAFETY: `data` is the `Vec` `windows` passed, alive for the call.
    let Some(found) = (unsafe { (data.0 as *mut Vec<ScreenSource>).as_mut() }) else {
        return BOOL(0);
    };
    // SAFETY: queries on a window handle from the enumeration.
    if let Some(source) = unsafe { shareable_window(hwnd) } {
        found.push(source);
    }
    BOOL(1)
}

/// The window as a source, when it is one tdesktop's list would show.
///
/// # Safety
/// `hwnd` must come from `EnumWindows`.
unsafe fn shareable_window(hwnd: HWND) -> Option<ScreenSource> {
    // SAFETY: the caller passes an enumerated window.
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return None;
        }
        if GetWindow(hwnd, GW_OWNER).is_ok_and(|owner| !owner.is_invalid()) {
            return None;
        }
        if GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0 {
            return None;
        }
        let mut cloaked = 0u32;
        let _ = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            std::ptr::addr_of_mut!(cloaked).cast(),
            std::mem::size_of::<u32>() as u32,
        );
        if cloaked != 0 {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == std::process::id() {
            return None;
        }
        let mut rect = RECT::default();
        GetWindowRect(hwnd, &mut rect).ok()?;
        let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
        if width < MIN_WINDOW_SIDE || height < MIN_WINDOW_SIDE {
            return None;
        }
        let mut title = [0u16; 512];
        let len = usize::try_from(GetWindowTextW(hwnd, &mut title)).unwrap_or(0);
        let title = String::from_utf16_lossy(title.get(..len)?);
        if title.trim().is_empty() {
            return None;
        }
        Some(ScreenSource {
            id: SourceId(format!("window:{}", hwnd.0 as isize)),
            kind: SourceKind::Window,
            title,
            size: (u32::try_from(width).ok()?, u32::try_from(height).ok()?),
        })
    }
}
