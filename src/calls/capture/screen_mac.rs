//! macOS screen capture through ScreenCaptureKit (macOS 14+, Quill's
//! minimum):
//! - `SCShareableContent` lists the displays and on-screen windows (no
//!   desktop, menu bar or tiny windows, none of Quill's own).
//! - `SCScreenshotManager` takes one thumbnail per source, as tdesktop's
//!   chooser shows one captured frame per source.
//! - An `SCStream` delivers NV12 (`420v`) frames at the call's size and
//!   rate. They are converted to I420 on ScreenCaptureKit's queue and
//!   pushed into the sink; nothing touches GPUI.
//! - For a window, a monitor thread checks every 500 ms whether the window
//!   is on screen: minimised or hidden is `Paused`, back is `Resumed`,
//!   gone is `Ended` (tdesktop's capturer `setOnPause`).
//! - `CGPreflightScreenCaptureAccess` is the permission pre-check.
#![deny(clippy::indexing_slicing)]

use super::screen::{
    PreviewSlot, ScreenAccess, ScreenBackend, ScreenCapture, ScreenOptions, ScreenSource, SourceId,
    SourceKind, Thumbnail, fit_capture_size, nv12_to_i420,
};
use super::{CaptureError, CaptureEvent, FrameSink, VideoFrame};
use block2::RcBlock;
use dispatch2::{DispatchQueue, DispatchRetained};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{AnyThread, DefinedClass, define_class, msg_send};
use objc2_core_foundation::{CFRetained, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGImage, CGImageAlphaInfo,
    CGImageByteOrderInfo, CGPreflightScreenCaptureAccess, CGRequestScreenCaptureAccess,
    kCGColorSpaceSRGB,
};
use objc2_core_media::{CMSampleBuffer, CMTime};
use objc2_core_video::{
    CVPixelBuffer, CVPixelBufferGetBaseAddressOfPlane, CVPixelBufferGetBytesPerRowOfPlane,
    CVPixelBufferGetHeight, CVPixelBufferGetHeightOfPlane, CVPixelBufferGetPixelFormatType,
    CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
    CVPixelBufferUnlockBaseAddress, kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
};
use objc2_foundation::{NSArray, NSError, NSObject, NSObjectProtocol};
use objc2_screen_capture_kit::{
    SCContentFilter, SCScreenshotManager, SCShareableContent, SCStream, SCStreamConfiguration,
    SCStreamDelegate, SCStreamErrorCode, SCStreamOutput, SCStreamOutputType,
};
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// How long to wait for a ScreenCaptureKit completion handler.
const REPLY_TIMEOUT: Duration = Duration::from_secs(5);
/// How often a shared window's visibility is checked (the design's 500 ms).
const PAUSE_POLL: Duration = Duration::from_millis(500);
/// Windows smaller than this are toolbars and helpers, not shareable.
const MIN_WINDOW_SIDE: f64 = 100.0;

/// The ScreenCaptureKit backend. Stateless: every call asks the system.
pub struct MacScreen;

impl ScreenBackend for MacScreen {
    fn access(&self) -> ScreenAccess {
        if CGPreflightScreenCaptureAccess() {
            ScreenAccess::Granted
        } else {
            ScreenAccess::Denied
        }
    }

    fn open_privacy_settings(&self) {
        if crate::agent::ui_driver::guard::refuse_outside_if_injecting("open System Settings") {
            return;
        }
        // Asking first adds Quill to the Screen Recording list, so the
        // switch is there when the settings open.
        let _ = CGRequestScreenCaptureAccess();
        if let Ok(child) = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
            .spawn()
        {
            crate::child_reap::wait_in_background(child);
        }
    }

    fn sources(&self) -> Result<Vec<ScreenSource>, CaptureError> {
        let content = shareable_content(true)?;
        let mut sources = Vec::new();
        // SAFETY: plain property reads on a live SCShareableContent.
        unsafe {
            for display in content.displays().iter() {
                sources.push(ScreenSource {
                    id: display_id(display.displayID()),
                    kind: SourceKind::Screen,
                    title: String::new(),
                    size: (
                        u32::try_from(display.width()).unwrap_or(0),
                        u32::try_from(display.height()).unwrap_or(0),
                    ),
                });
            }
            for window in content.windows().iter() {
                if !shareable_window(&window) {
                    continue;
                }
                let frame = window.frame();
                sources.push(ScreenSource {
                    id: window_id(window.windowID()),
                    kind: SourceKind::Window,
                    title: window.title().map(|t| t.to_string()).unwrap_or_default(),
                    size: (frame.size.width as u32, frame.size.height as u32),
                });
            }
        }
        Ok(sources)
    }

    fn thumbnail(&self, source: &ScreenSource, max: (u32, u32)) -> Option<Thumbnail> {
        let content = shareable_content(true).ok()?;
        let filter = content_filter(&content, &source.id)?;
        let (width, height) = pixel_size(&filter, max);
        // SAFETY: a fresh configuration, configured before use.
        let config = unsafe {
            let config = SCStreamConfiguration::new();
            config.setWidth(width as usize);
            config.setHeight(height as usize);
            config.setShowsCursor(false);
            config
        };
        let (tx, rx) = mpsc::sync_channel(1);
        let block = RcBlock::new(move |image: *mut CGImage, _error: *mut NSError| {
            let picture = NonNull::new(image).and_then(|image| {
                // SAFETY: the handler's image is valid for the call; we
                // retain it for the conversion.
                let image = unsafe { CFRetained::retain(image) };
                draw_rgba(&image)
            });
            let _ = tx.try_send(picture);
        });
        // SAFETY: filter, config and block outlive the call; the handler
        // only sends on a channel.
        unsafe {
            SCScreenshotManager::captureImageWithFilter_configuration_completionHandler(
                &filter,
                &config,
                Some(&block),
            );
        }
        rx.recv_timeout(REPLY_TIMEOUT).ok().flatten()
    }

    fn start(
        &self,
        source: &SourceId,
        options: ScreenOptions,
        sink: FrameSink<VideoFrame>,
        preview: Arc<PreviewSlot>,
    ) -> Result<Box<dyn ScreenCapture>, CaptureError> {
        let content = shareable_content(false)?;
        let filter = content_filter(&content, source)
            .ok_or_else(|| CaptureError::Device("The shared window or screen is gone.".into()))?;
        let (width, height) = pixel_size(&filter, (options.max_width, options.max_height));
        // SAFETY: a fresh configuration, configured before use.
        let config = unsafe {
            let config = SCStreamConfiguration::new();
            config.setWidth(width as usize);
            config.setHeight(height as usize);
            config.setPixelFormat(kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange);
            config.setMinimumFrameInterval(CMTime::new(1, i32::from(options.fps.max(1))));
            config.setShowsCursor(true);
            config.setQueueDepth(5);
            config
        };
        let shared = Arc::new(StreamShared {
            sink: sink.clone(),
            preview,
            stopped: AtomicBool::new(false),
        });
        let output = StreamOutput::new(shared.clone());
        let protocol_output = ProtocolObject::<dyn SCStreamOutput>::from_ref(&*output);
        let protocol_delegate = ProtocolObject::<dyn SCStreamDelegate>::from_ref(&*output);
        // SAFETY: filter, config and the delegate are live; the stream
        // keeps what it needs and `MacCapture` keeps the output alive.
        let stream = unsafe {
            SCStream::initWithFilter_configuration_delegate(
                SCStream::alloc(),
                &filter,
                &config,
                Some(protocol_delegate),
            )
        };
        let queue = DispatchQueue::new("app.quill.screen-capture", None);
        // SAFETY: a serial queue owned by `MacCapture` for the stream's life.
        unsafe {
            stream.addStreamOutput_type_sampleHandlerQueue_error(
                protocol_output,
                SCStreamOutputType::Screen,
                Some(&*queue),
            )
        }
        .map_err(|err| describe(&err))?;
        let (tx, rx) = mpsc::sync_channel(1);
        let block = RcBlock::new(move |error: *mut NSError| {
            // SAFETY: the handler's error, if any, is valid for the call.
            let result = match unsafe { error.as_ref() } {
                Some(error) => Err(describe(error)),
                None => Ok(()),
            };
            let _ = tx.try_send(result);
        });
        // SAFETY: the stream is configured; the block only sends.
        unsafe { stream.startCaptureWithCompletionHandler(Some(&block)) };
        rx.recv_timeout(REPLY_TIMEOUT)
            .map_err(|_| CaptureError::Device("Screen capture did not start.".into()))??;
        let monitor = window_number(source)
            .and_then(|window| WindowMonitor::start(window, sink.clone(), shared.clone()).ok());
        Ok(Box::new(MacCapture {
            stream: Some(stream),
            _output: output,
            _queue: queue,
            shared,
            monitor,
        }))
    }
}

/// A running ScreenCaptureKit stream.
struct MacCapture {
    stream: Option<Retained<SCStream>>,
    _output: Retained<StreamOutput>,
    _queue: DispatchRetained<DispatchQueue>,
    shared: Arc<StreamShared>,
    monitor: Option<WindowMonitor>,
}

impl ScreenCapture for MacCapture {
    fn stop(&mut self) {
        self.shared.stopped.store(true, Ordering::Release);
        self.monitor = None;
        if let Some(stream) = self.stream.take() {
            // SAFETY: stopping a started stream; no handler needed.
            unsafe { stream.stopCaptureWithCompletionHandler(None) };
        }
    }
}

impl Drop for MacCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

/// What the stream callbacks share with the capture.
struct StreamShared {
    sink: FrameSink<VideoFrame>,
    preview: Arc<PreviewSlot>,
    stopped: AtomicBool,
}

impl StreamShared {
    fn on_sample(&self, sample: &CMSampleBuffer) {
        if self.stopped.load(Ordering::Acquire) {
            return;
        }
        // Idle and blank frames carry no picture.
        // SAFETY: the sample buffer is valid for the callback.
        let Some(image) = (unsafe { sample.image_buffer() }) else {
            return;
        };
        let Some(frame) = nv12_frame(&image) else {
            return;
        };
        self.preview.offer(&frame);
        self.sink.push(frame);
    }

    fn on_stopped(&self, error: &NSError) {
        if self.stopped.swap(true, Ordering::AcqRel) {
            return;
        }
        let code = error.code();
        if code == SCStreamErrorCode::UserStopped.0 || code == SCStreamErrorCode::NoCaptureSource.0
        {
            self.sink.push_event(CaptureEvent::Ended);
        } else {
            self.sink.push_event(CaptureEvent::Failed(describe(error)));
        }
    }
}

/// One `420v` picture as an I420 frame.
fn nv12_frame(pixel: &CVPixelBuffer) -> Option<VideoFrame> {
    if CVPixelBufferGetPixelFormatType(pixel) != kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange {
        return None;
    }
    // I420 needs even sides; an odd last row or column is dropped.
    let width = CVPixelBufferGetWidth(pixel) & !1;
    let height = CVPixelBufferGetHeight(pixel) & !1;
    let (w16, h16) = (u16::try_from(width).ok()?, u16::try_from(height).ok()?);
    // SAFETY: a read-only lock on a valid pixel buffer, released below.
    if unsafe { CVPixelBufferLockBaseAddress(pixel, CVPixelBufferLockFlags::ReadOnly) } != 0 {
        return None;
    }
    let plane = |index: usize| -> Option<(&[u8], usize)> {
        let base = CVPixelBufferGetBaseAddressOfPlane(pixel, index).cast::<u8>();
        let stride = CVPixelBufferGetBytesPerRowOfPlane(pixel, index);
        let rows = CVPixelBufferGetHeightOfPlane(pixel, index);
        if base.is_null() {
            return None;
        }
        // SAFETY: while the buffer is locked, the plane holds
        // `stride * rows` readable bytes at `base`.
        Some((
            unsafe { std::slice::from_raw_parts(base, stride.checked_mul(rows)?) },
            stride,
        ))
    };
    let i420 = plane(0)
        .zip(plane(1))
        .and_then(|((y, y_stride), (uv, uv_stride))| {
            nv12_to_i420(y, y_stride, uv, uv_stride, width, height)
        });
    // SAFETY: balances the lock above; the slices are no longer used.
    unsafe { CVPixelBufferUnlockBaseAddress(pixel, CVPixelBufferLockFlags::ReadOnly) };
    Some(VideoFrame {
        i420: i420?,
        width: w16,
        height: h16,
        captured_at: Instant::now(),
    })
}

/// Watches a shared window: `Paused` while it is off screen (minimised,
/// hidden, on another space), `Resumed` when it is back, `Ended` once it
/// is closed.
struct WindowMonitor {
    stop: mpsc::Sender<()>,
    worker: Option<std::thread::JoinHandle<Option<()>>>,
}

impl WindowMonitor {
    fn start(
        window: u32,
        sink: FrameSink<VideoFrame>,
        shared: Arc<StreamShared>,
    ) -> std::io::Result<Self> {
        let (stop, stopped) = mpsc::channel::<()>();
        let worker = crate::thread_guard::spawn_guarded("quill-screen-pause", move || {
            let mut paused = false;
            while let Err(mpsc::RecvTimeoutError::Timeout) = stopped.recv_timeout(PAUSE_POLL) {
                if shared.stopped.load(Ordering::Acquire) {
                    return;
                }
                match window_state(window) {
                    WindowState::OnScreen if paused => {
                        paused = false;
                        sink.push_event(CaptureEvent::Resumed);
                    }
                    WindowState::OffScreen if !paused => {
                        paused = true;
                        sink.push_event(CaptureEvent::Paused);
                    }
                    WindowState::Gone => {
                        sink.push_event(CaptureEvent::Ended);
                        return;
                    }
                    WindowState::OnScreen | WindowState::OffScreen | WindowState::Unknown => {}
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

enum WindowState {
    OnScreen,
    OffScreen,
    Gone,
    /// The window list could not be read this time.
    Unknown,
}

fn window_state(window: u32) -> WindowState {
    let Ok(content) = shareable_content(false) else {
        return WindowState::Unknown;
    };
    // SAFETY: plain property reads on a live SCShareableContent.
    let on_screen = unsafe {
        content
            .windows()
            .iter()
            .find(|candidate| candidate.windowID() == window)
            .map(|found| found.isOnScreen())
    };
    match on_screen {
        Some(true) => WindowState::OnScreen,
        Some(false) => WindowState::OffScreen,
        None => WindowState::Gone,
    }
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "QuillScreenStreamOutput"]
    #[ivars = Arc<StreamShared>]
    struct StreamOutput;

    unsafe impl NSObjectProtocol for StreamOutput {}

    unsafe impl SCStreamOutput for StreamOutput {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        fn did_output(&self, _stream: &SCStream, sample: &CMSampleBuffer, kind: SCStreamOutputType) {
            if kind == SCStreamOutputType::Screen {
                self.ivars().on_sample(sample);
            }
        }
    }

    unsafe impl SCStreamDelegate for StreamOutput {
        #[unsafe(method(stream:didStopWithError:))]
        fn did_stop(&self, _stream: &SCStream, error: &NSError) {
            self.ivars().on_stopped(error);
        }
    }
);

impl StreamOutput {
    fn new(shared: Arc<StreamShared>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(shared);
        // SAFETY: NSObject's designated initializer.
        unsafe { msg_send![super(this), init] }
    }
}

/// The current shareable content. `on_screen_only: false` also lists
/// minimised and hidden windows (the pause check needs them).
fn shareable_content(on_screen_only: bool) -> Result<Retained<SCShareableContent>, CaptureError> {
    let (tx, rx) = mpsc::sync_channel(1);
    let block = RcBlock::new(
        move |content: *mut SCShareableContent, error: *mut NSError| {
            // SAFETY: the handler's pointers are valid for the call; the
            // content is retained before the handler returns.
            let result = match unsafe { Retained::retain(content) } {
                Some(content) => Ok(content),
                // SAFETY: `error` is null or a valid NSError for the call.
                None => Err(unsafe { error.as_ref() }.map_or_else(denied, describe)),
            };
            let _ = tx.try_send(result);
        },
    );
    // SAFETY: the block lives until the handler ran or we stop waiting;
    // block2 keeps it alive for ScreenCaptureKit either way.
    unsafe {
        SCShareableContent::getShareableContentExcludingDesktopWindows_onScreenWindowsOnly_completionHandler(
            true,
            on_screen_only,
            &block,
        );
    }
    rx.recv_timeout(REPLY_TIMEOUT)
        .map_err(|_| CaptureError::Device("The screen list did not arrive.".into()))?
}

/// A filter for one display (the whole screen) or one window.
fn content_filter(
    content: &SCShareableContent,
    id: &SourceId,
) -> Option<Retained<SCContentFilter>> {
    // SAFETY: property reads and filter construction on live objects.
    unsafe {
        if let Some(display) = id.0.strip_prefix("display:") {
            let display_id: u32 = display.parse().ok()?;
            let display = content
                .displays()
                .iter()
                .find(|candidate| candidate.displayID() == display_id)?;
            return Some(SCContentFilter::initWithDisplay_excludingWindows(
                SCContentFilter::alloc(),
                &display,
                &NSArray::new(),
            ));
        }
        let window_id = window_number(id)?;
        let window = content
            .windows()
            .iter()
            .find(|candidate| candidate.windowID() == window_id)?;
        Some(SCContentFilter::initWithDesktopIndependentWindow(
            SCContentFilter::alloc(),
            &window,
        ))
    }
}

/// The filter's content in pixels, fitted into `max`.
fn pixel_size(filter: &SCContentFilter, max: (u32, u32)) -> (u32, u32) {
    // SAFETY: plain reads on a live filter.
    let (rect, scale) = unsafe { (filter.contentRect(), f64::from(filter.pointPixelScale())) };
    fit_capture_size(
        (rect.size.width * scale) as u32,
        (rect.size.height * scale) as u32,
        max.0,
        max.1,
    )
}

/// Windows worth sharing: normal level, at least 100 x 100, on screen,
/// not Quill's own.
///
/// # Safety
/// `window` must be a live `SCWindow`.
unsafe fn shareable_window(window: &objc2_screen_capture_kit::SCWindow) -> bool {
    // SAFETY: the caller passes a live window.
    unsafe {
        let frame = window.frame();
        let own = window
            .owningApplication()
            .is_some_and(|app| u32::try_from(app.processID()).ok() == Some(std::process::id()));
        window.windowLayer() == 0
            && window.isOnScreen()
            && !own
            && frame.size.width >= MIN_WINDOW_SIDE
            && frame.size.height >= MIN_WINDOW_SIDE
    }
}

fn display_id(id: u32) -> SourceId {
    SourceId(format!("display:{id}"))
}

fn window_id(id: u32) -> SourceId {
    SourceId(format!("window:{id}"))
}

fn window_number(id: &SourceId) -> Option<u32> {
    id.0.strip_prefix("window:")?.parse().ok()
}

fn denied() -> CaptureError {
    CaptureError::Permission(
        "Quill needs screen recording access. Allow it in System Settings > Privacy & Security > Screen Recording."
            .into(),
    )
}

fn describe(error: &NSError) -> CaptureError {
    if error.code() == SCStreamErrorCode::UserDeclined.0 || !CGPreflightScreenCaptureAccess() {
        return denied();
    }
    CaptureError::Device(error.localizedDescription().to_string())
}

/// A screenshot as opaque RGBA.
fn draw_rgba(image: &CGImage) -> Option<Thumbnail> {
    let width = CGImage::width(Some(image));
    let height = CGImage::height(Some(image));
    let (w32, h32) = (u32::try_from(width).ok()?, u32::try_from(height).ok()?);
    let stride = width.checked_mul(4)?;
    let mut pixels = vec![0_u8; stride.checked_mul(height)?];
    // SAFETY: reading a framework constant.
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))?;
    let info = CGImageAlphaInfo::NoneSkipLast.0 | CGImageByteOrderInfo::Order32Big.0;
    {
        // SAFETY: `pixels` holds `stride * height` bytes and outlives the
        // context, which is dropped at the end of this block.
        let context = unsafe {
            CGBitmapContextCreate(
                pixels.as_mut_ptr().cast(),
                width,
                height,
                8,
                stride,
                Some(&space),
                info,
            )
        }?;
        let rect = CGRect::new(
            CGPoint::new(0.0, 0.0),
            CGSize::new(width as f64, height as f64),
        );
        CGContext::draw_image(Some(&context), rect, Some(image));
    }
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel[3] = 255;
    }
    Some(Thumbnail {
        width: w32,
        height: h32,
        rgba: pixels,
    })
}
