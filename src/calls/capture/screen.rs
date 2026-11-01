//! Screen sharing captured by Quill: the sources the chooser lists, their
//! thumbnails, and a running capture that turns the chosen screen or
//! window into I420 frames for the call's external screen source.
//!
//! [`ScreenBackend`] is the per-OS part: ScreenCaptureKit on macOS
//! (`screen_mac.rs`), Windows.Graphics.Capture on Windows
//! (`screen_win.rs`), and on Linux the screen cast portal (Wayland) or
//! X11 (`screen_linux.rs`). The conversions here are plain Rust and
//! shared.
//!
//! Design: `docs/decisions/call-capture.md`.
#![deny(clippy::indexing_slicing)]

use super::{CaptureError, FrameSink, VideoFrame};
use crate::locks::LockExt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A screen (display) or a single window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceKind {
    Screen,
    Window,
}

/// A backend's id for one source, stable while the source exists.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceId(pub String);

/// One entry of the chooser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenSource {
    pub id: SourceId,
    pub kind: SourceKind,
    /// The window's title; empty for screens and untitled windows.
    pub title: String,
    /// Pixel size of the source.
    pub size: (u32, u32),
}

/// A small RGBA picture of a source, for the chooser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thumbnail {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// How a capture runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenOptions {
    pub max_width: u32,
    pub max_height: u32,
    pub fps: u8,
}

impl Default for ScreenOptions {
    /// The engine's screen description: 1920 x 1080 at 15 fps.
    fn default() -> Self {
        Self {
            max_width: 1920,
            max_height: 1080,
            fps: 15,
        }
    }
}

/// Whether the OS lets Quill record the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenAccess {
    Granted,
    Denied,
}

/// The source id of a backend whose OS shows its own picker when the
/// capture starts (see [`ScreenBackend::system_picker`]).
pub const SYSTEM_PICKER_SOURCE: &str = "system-picker";

/// Whether calls can share a screen here: Quill's own capture (on Linux
/// the portal or X11) feeds the call's external screen source.
pub fn can_share() -> bool {
    crate::calls::engine::external_screen_capture_enabled()
}

/// The per-OS screen capture. Listing and thumbnails run on a background
/// thread of the UI; captures are started by the connect driver.
pub trait ScreenBackend: Send + Sync {
    /// Whether the OS asks what to share itself (the Wayland screen cast
    /// portal; tdesktop `uniqueDesktopCaptureSource`). Then Quill shows
    /// no chooser: the share button starts [`SYSTEM_PICKER_SOURCE`], or
    /// stops a running share.
    fn system_picker(&self) -> bool {
        false
    }
    /// Checked before the chooser opens (tdesktop
    /// `desktopCaptureAllowed`).
    fn access(&self) -> ScreenAccess;
    /// Ask the OS for access. Returns the system settings pane where the
    /// user grants it, for the UI to open (`QuillApp::open_system_settings`).
    fn ask_access(&self) -> Option<&'static str>;
    /// Screens first, then windows, in the OS order.
    fn sources(&self) -> Result<Vec<ScreenSource>, CaptureError>;
    /// One picture of `source`, fitted into `max` pixels.
    fn thumbnail(&self, source: &ScreenSource, max: (u32, u32)) -> Option<Thumbnail>;
    /// Start capturing `source` into `sink`. Pause, resume, the source
    /// closing and failures arrive as sink events.
    fn start(
        &self,
        source: &SourceId,
        options: ScreenOptions,
        sink: FrameSink<VideoFrame>,
        preview: Arc<PreviewSlot>,
    ) -> Result<Box<dyn ScreenCapture>, CaptureError>;
}

/// A running capture. Dropping it stops it too.
pub trait ScreenCapture {
    /// Stop capturing. Idempotent.
    fn stop(&mut self);
}

/// The screen backend of this build, if it has one.
pub fn native_backend() -> Option<Arc<dyn ScreenBackend>> {
    #[cfg(all(feature = "ui", target_os = "macos"))]
    {
        Some(Arc::new(super::screen_mac::MacScreen))
    }
    #[cfg(all(feature = "ui", windows))]
    {
        Some(Arc::new(super::screen_win::WinScreen))
    }
    #[cfg(all(feature = "ui", target_os = "linux"))]
    {
        Some(Arc::new(super::screen_linux::LinuxScreen::detect()))
    }
    #[cfg(not(all(feature = "ui", any(target_os = "macos", windows, target_os = "linux"))))]
    {
        None
    }
}

/// What the chooser calls a source (tdesktop
/// `desktop_capture_choose_source.cpp`: "Screen {index}" for screens,
/// the title for windows, "Window {index}" for untitled windows).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceLabel {
    Screen(usize),
    Window(String),
    UntitledWindow(usize),
}

/// Labels for `sources`, numbered the way tdesktop numbers them.
pub fn source_labels(sources: &[ScreenSource]) -> Vec<SourceLabel> {
    let (mut screens, mut untitled) = (0, 0);
    sources
        .iter()
        .map(|source| match source.kind {
            SourceKind::Screen => {
                screens += 1;
                SourceLabel::Screen(screens)
            }
            SourceKind::Window if !source.title.trim().is_empty() => {
                SourceLabel::Window(source.title.clone())
            }
            SourceKind::Window => {
                untitled += 1;
                SourceLabel::UntitledWindow(untitled)
            }
        })
        .collect()
}

/// The capture size for a `width` x `height` source within `max`: aspect
/// kept, never upscaled, even sides (I420 needs them), at least 2 x 2.
pub fn fit_capture_size(width: u32, height: u32, max_width: u32, max_height: u32) -> (u32, u32) {
    let (w, h) = (f64::from(width.max(1)), f64::from(height.max(1)));
    let scale = (f64::from(max_width.max(2)) / w)
        .min(f64::from(max_height.max(2)) / h)
        .min(1.0);
    let even = |value: f64| ((value * scale).floor() as u32 & !1).max(2);
    (even(w), even(h))
}

/// NV12 (a Y plane and an interleaved UV plane) to I420 (Y, U, V planes),
/// the layout the engine takes for screens. `width` and
/// `height` must be even; strides may exceed the width. `None` when the
/// planes are too short.
pub fn nv12_to_i420(
    y_plane: &[u8],
    y_stride: usize,
    uv_plane: &[u8],
    uv_stride: usize,
    width: usize,
    height: usize,
) -> Option<Vec<u8>> {
    if width == 0 || height == 0 || !width.is_multiple_of(2) || !height.is_multiple_of(2) {
        return None;
    }
    if y_stride < width || uv_stride < width {
        return None;
    }
    let (cw, ch) = (width / 2, height / 2);
    let mut out = Vec::with_capacity(width * height * 3 / 2);
    for row in 0..height {
        let start = row.checked_mul(y_stride)?;
        out.extend_from_slice(y_plane.get(start..start.checked_add(width)?)?);
    }
    let mut v = Vec::with_capacity(cw * ch);
    for row in 0..ch {
        let start = row.checked_mul(uv_stride)?;
        let line = uv_plane.get(start..start.checked_add(width)?)?;
        let (pairs, _) = line.as_chunks::<2>();
        for [u_value, v_value] in pairs {
            out.push(*u_value);
            v.push(*v_value);
        }
    }
    out.extend_from_slice(&v);
    Some(out)
}

/// A downscaled RGBA picture of an I420 frame (BT.601 video range, the
/// range ScreenCaptureKit's `420v` delivers), at most `max_width` wide.
pub fn i420_preview(i420: &[u8], width: u16, height: u16, max_width: u16) -> Option<Thumbnail> {
    let (w, h) = (usize::from(width), usize::from(height));
    if w == 0 || h == 0 || i420.len() != w.checked_mul(h)?.checked_mul(3)? / 2 {
        return None;
    }
    let (out_w, out_h) = fit_capture_size(
        u32::from(width),
        u32::from(height),
        u32::from(max_width),
        u32::MAX,
    );
    let (ow, oh) = (out_w as usize, out_h as usize);
    let (y_plane, chroma) = i420.split_at_checked(w * h)?;
    let (u_plane, v_plane) = chroma.split_at_checked((w / 2) * (h / 2))?;
    let mut rgba = Vec::with_capacity(ow * oh * 4);
    for oy in 0..oh {
        let sy = oy * h / oh;
        for ox in 0..ow {
            let sx = ox * w / ow;
            let y = f32::from(*y_plane.get(sy * w + sx)?) - 16.0;
            let ci = (sy / 2) * (w / 2) + sx / 2;
            let u = f32::from(*u_plane.get(ci)?) - 128.0;
            let v = f32::from(*v_plane.get(ci)?) - 128.0;
            let c = 1.164 * y;
            let clamp = |value: f32| value.round().clamp(0.0, 255.0) as u8;
            rgba.extend_from_slice(&[
                clamp(c + 1.596 * v),
                clamp(c - 0.392 * u - 0.813 * v),
                clamp(c + 2.017 * u),
                255,
            ]);
        }
    }
    Some(Thumbnail {
        width: out_w,
        height: out_h,
        rgba,
    })
}

/// The latest small picture of your own shared screen, for your tile in
/// the call window (and its blurred paused state). The capture thread
/// publishes at most [`PreviewSlot::INTERVAL`] apart.
#[derive(Default)]
pub struct PreviewSlot {
    inner: Mutex<PreviewInner>,
}

#[derive(Default)]
struct PreviewInner {
    latest: Option<(u64, Thumbnail)>,
    published_at: Option<Instant>,
    seq: u64,
}

impl PreviewSlot {
    /// Four pictures a second: enough for a small tile.
    pub const INTERVAL: Duration = Duration::from_millis(250);
    /// Preview width in pixels.
    pub const WIDTH: u16 = 480;

    /// Offer a captured frame; it becomes the preview when the last one
    /// is old enough.
    pub fn offer(&self, frame: &VideoFrame) {
        let due = {
            let inner = self.inner.lock_or_recover();
            inner
                .published_at
                .is_none_or(|at| frame.captured_at.saturating_duration_since(at) >= Self::INTERVAL)
        };
        if !due {
            return;
        }
        let Some(picture) = i420_preview(&frame.i420, frame.width, frame.height, Self::WIDTH)
        else {
            return;
        };
        let mut inner = self.inner.lock_or_recover();
        inner.seq = inner.seq.wrapping_add(1);
        inner.latest = Some((inner.seq, picture));
        inner.published_at = Some(frame.captured_at);
    }

    /// The newest preview and its sequence number, unless it is `seen`.
    pub fn latest_after(&self, seen: u64) -> Option<(u64, Thumbnail)> {
        let inner = self.inner.lock_or_recover();
        inner
            .latest
            .as_ref()
            .filter(|(seq, _)| *seq != seen)
            .cloned()
    }
}

#[cfg(test)]
#[path = "screen_tests.rs"]
mod tests;
