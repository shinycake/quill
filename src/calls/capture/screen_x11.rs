//! Screen sharing on X11, as tdesktop's WebRTC does it there
//! (`ScreenCapturerX11`, `WindowCapturerX11`, `window_list_utils.cc`):
//! - Screens are the RandR 1.5 monitors in the server's order (the primary
//!   first), or the whole root window without RandR.
//! - Windows are the top-level application windows (the one with `WM_STATE`
//!   under each root child), top first, that are in the normal state and
//!   are "normal" windows (`_NET_WM_WINDOW_TYPE_NORMAL`; without the hint,
//!   not a `gnome-panel` / `desktop_window`). As on macOS and Windows, Quill
//!   also leaves out its own windows and ones under 100 x 100.
//! - A screen is read from the root window, a window from its Composite
//!   pixmap (so covered windows still show); the cursor is drawn in from
//!   XFixes, as tgcalls' `DesktopAndCursorComposer` does.
//! - A minimised window (`WM_STATE` iconic) pauses the share, a closed one
//!   ends it.
//!
//! x11rb is pure Rust (already linked by GPUI); nothing extra is loaded.
#![deny(clippy::indexing_slicing)]

use super::screen::{
    PreviewSlot, ScreenCapture, ScreenOptions, ScreenSource, SourceId, SourceKind, Thumbnail,
    fit_capture_size,
};
use super::screen_bgra::{FramePacer, PackedOrder, packed_thumbnail, packed_to_i420};
use super::screen_linux::PauseTracker;
use super::{CaptureError, CaptureEvent, FrameSink, VideoFrame};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol::composite::{ConnectionExt as _, Redirect};
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::xfixes::ConnectionExt as _;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _, ImageFormat, ImageOrder, Window};
use x11rb::rust_connection::RustConnection;

/// How often a shared window's state is checked (the design's 500 ms).
const PAUSE_POLL: Duration = Duration::from_millis(500);
/// Windows smaller than this are toolbars and helpers, not shareable.
const MIN_WINDOW_SIDE: u16 = 100;
/// `WM_STATE` values (ICCCM 4.1.3.1).
const NORMAL_STATE: u32 = 1;
const ICONIC_STATE: u32 = 3;
/// How deep under a root child the application window is looked for.
const MAX_DEPTH: u8 = 3;

const SCREEN_PREFIX: &str = "x11-screen:";
const WINDOW_PREFIX: &str = "x11-window:";

fn unavailable() -> CaptureError {
    CaptureError::Unsupported("Screen sharing needs an X11 or Wayland session.".into())
}

fn failed(what: &str) -> CaptureError {
    CaptureError::Device(format!("Screen capture failed: {what}"))
}

struct Atoms {
    wm_state: u32,
    window_type: u32,
    window_type_normal: u32,
    net_wm_name: u32,
    utf8_string: u32,
    net_wm_pid: u32,
}

/// One connection to the X server, with what Quill checked about it.
struct X11 {
    conn: RustConnection,
    root: Window,
    atoms: Atoms,
    composite: bool,
    xfixes: bool,
}

impl X11 {
    fn connect() -> Result<Self, CaptureError> {
        let (conn, screen) = x11rb::connect(None).map_err(|_| unavailable())?;
        let setup = conn.setup();
        let root_screen = setup.roots.get(screen).ok_or_else(unavailable)?;
        // Quill reads 32-bit little-endian pixels (BGRx), what every
        // depth-24 and depth-32 desktop serves.
        let packed_32 = setup
            .pixmap_formats
            .iter()
            .any(|f| f.depth == root_screen.root_depth && f.bits_per_pixel == 32);
        if !packed_32 || setup.image_byte_order != ImageOrder::LSB_FIRST {
            return Err(CaptureError::Unsupported(
                "Screen sharing needs a 24-bit or 32-bit X display.".into(),
            ));
        }
        let root = root_screen.root;
        let intern = |name: &[u8]| -> Result<u32, CaptureError> {
            conn.intern_atom(false, name)
                .ok()
                .and_then(|cookie| cookie.reply().ok())
                .map(|reply| reply.atom)
                .ok_or_else(|| failed("X11 atoms"))
        };
        let atoms = Atoms {
            wm_state: intern(b"WM_STATE")?,
            window_type: intern(b"_NET_WM_WINDOW_TYPE")?,
            window_type_normal: intern(b"_NET_WM_WINDOW_TYPE_NORMAL")?,
            net_wm_name: intern(b"_NET_WM_NAME")?,
            utf8_string: intern(b"UTF8_STRING")?,
            net_wm_pid: intern(b"_NET_WM_PID")?,
        };
        let composite = conn
            .composite_query_version(0, 4)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .is_some_and(|v| v.major_version > 0 || v.minor_version >= 2);
        let xfixes = conn
            .xfixes_query_version(4, 0)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .is_some_and(|v| v.major_version >= 1);
        Ok(Self {
            conn,
            root,
            atoms,
            composite,
            xfixes,
        })
    }

    fn property(&self, window: Window, atom: u32, kind: u32) -> Option<Vec<u8>> {
        let reply = self
            .conn
            .get_property(false, window, atom, kind, 0, 1024)
            .ok()?
            .reply()
            .ok()?;
        (reply.type_ != u32::from(AtomEnum::NONE)).then_some(reply.value)
    }

    fn property32(&self, window: Window, atom: u32, kind: u32) -> Option<Vec<u32>> {
        let value = self.property(window, atom, kind)?;
        Some(
            value
                .as_chunks::<4>()
                .0
                .iter()
                .map(|bytes| u32::from_ne_bytes(*bytes))
                .collect(),
        )
    }

    /// `WM_STATE`, or `None` when the window has none (or is gone).
    fn wm_state(&self, window: Window) -> Option<u32> {
        self.property32(window, self.atoms.wm_state, self.atoms.wm_state)?
            .first()
            .copied()
    }

    /// The application window at or under `window` (webrtc
    /// `GetApplicationWindow`): one in the normal state; a minimised one is
    /// skipped.
    fn application_window(&self, window: Window, depth: u8) -> Option<Window> {
        match self.wm_state(window) {
            Some(NORMAL_STATE) => return Some(window),
            Some(ICONIC_STATE) => return None,
            _ => {}
        }
        if depth == 0 {
            return None;
        }
        let tree = self.conn.query_tree(window).ok()?.reply().ok()?;
        tree.children
            .iter()
            .rev()
            .find_map(|child| self.application_window(*child, depth - 1))
    }

    /// webrtc `IsDesktopElement`: panels, docks and desktops.
    fn is_desktop_element(&self, window: Window) -> bool {
        let types = self
            .property32(window, self.atoms.window_type, AtomEnum::ATOM.into())
            .unwrap_or_default();
        if !types.is_empty() {
            return !types.contains(&self.atoms.window_type_normal);
        }
        let class = self
            .property(window, AtomEnum::WM_CLASS.into(), AtomEnum::STRING.into())
            .unwrap_or_default();
        let name = class.split(|byte| *byte == 0).next().unwrap_or_default();
        name == b"gnome-panel" || name == b"desktop_window"
    }

    fn title(&self, window: Window) -> String {
        if let Some(name) = self.property(window, self.atoms.net_wm_name, self.atoms.utf8_string) {
            return String::from_utf8_lossy(&name).into_owned();
        }
        self.property(window, AtomEnum::WM_NAME.into(), AtomEnum::ANY.into())
            .map(|name| name.iter().map(|byte| char::from(*byte)).collect())
            .unwrap_or_default()
    }

    fn is_own(&self, window: Window) -> bool {
        self.property32(window, self.atoms.net_wm_pid, AtomEnum::CARDINAL.into())
            .and_then(|pid| pid.first().copied())
            .is_some_and(|pid| pid == std::process::id())
    }

    fn monitors(&self) -> Vec<ScreenSource> {
        let randr = self
            .conn
            .randr_query_version(1, 5)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .is_some_and(|v| v.major_version > 1 || v.minor_version >= 5);
        let monitors = randr
            .then(|| {
                self.conn
                    .randr_get_monitors(self.root, true)
                    .ok()
                    .and_then(|cookie| cookie.reply().ok())
            })
            .flatten()
            .map(|reply| reply.monitors)
            .unwrap_or_default();
        let rects: Vec<Rect> = if monitors.is_empty() {
            self.geometry(self.root).into_iter().collect()
        } else {
            monitors
                .iter()
                .map(|m| Rect {
                    x: m.x,
                    y: m.y,
                    width: m.width,
                    height: m.height,
                })
                .collect()
        };
        rects
            .into_iter()
            .filter(|rect| rect.width > 0 && rect.height > 0)
            .map(|rect| ScreenSource {
                id: SourceId(format!(
                    "{SCREEN_PREFIX}{},{},{},{}",
                    rect.x, rect.y, rect.width, rect.height
                )),
                kind: SourceKind::Screen,
                title: String::new(),
                size: (u32::from(rect.width), u32::from(rect.height)),
            })
            .collect()
    }

    fn windows(&self) -> Vec<ScreenSource> {
        let Some(tree) = self
            .conn
            .query_tree(self.root)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
        else {
            return Vec::new();
        };
        tree.children
            .iter()
            .rev()
            .filter_map(|child| self.application_window(*child, MAX_DEPTH))
            .filter(|window| !self.is_desktop_element(*window) && !self.is_own(*window))
            .filter_map(|window| {
                let rect = self.geometry(window)?;
                if rect.width < MIN_WINDOW_SIDE || rect.height < MIN_WINDOW_SIDE {
                    return None;
                }
                Some(ScreenSource {
                    id: SourceId(format!("{WINDOW_PREFIX}{window}")),
                    kind: SourceKind::Window,
                    title: self.title(window),
                    size: (u32::from(rect.width), u32::from(rect.height)),
                })
            })
            .collect()
    }

    fn geometry(&self, drawable: u32) -> Option<Rect> {
        let reply = self.conn.get_geometry(drawable).ok()?.reply().ok()?;
        Some(Rect {
            x: reply.x,
            y: reply.y,
            width: reply.width,
            height: reply.height,
        })
    }

    /// The pixels of `source` (BGRx rows, `width * 4` bytes each) and its
    /// size, with the cursor drawn in when `cursor`.
    fn grab(&self, source: &Target, cursor: bool) -> Result<Grab, GrabError> {
        let (drawable, rect, origin, pixmap) = match source {
            Target::Screen(rect) => {
                let root = self.geometry(self.root).ok_or(GrabError::Gone)?;
                let rect = rect.clamped(&root).ok_or(GrabError::Gone)?;
                (self.root, rect, (rect.x, rect.y), None)
            }
            Target::Window(window) => {
                let geometry = self.geometry(*window).ok_or(GrabError::Gone)?;
                let at = self
                    .conn
                    .translate_coordinates(*window, self.root, 0, 0)
                    .ok()
                    .and_then(|cookie| cookie.reply().ok())
                    .map_or((0, 0), |reply| (reply.dst_x, reply.dst_y));
                let pixmap = self.conn.generate_id().map_err(|_| GrabError::Gone)?;
                self.conn
                    .composite_name_window_pixmap(*window, pixmap)
                    .map_err(|_| GrabError::Gone)?;
                let rect = Rect {
                    x: 0,
                    y: 0,
                    ..geometry
                };
                (pixmap, rect, at, Some(pixmap))
            }
        };
        let image = self
            .conn
            .get_image(
                ImageFormat::Z_PIXMAP,
                drawable,
                rect.x,
                rect.y,
                rect.width,
                rect.height,
                u32::MAX,
            )
            .ok()
            .and_then(|cookie| cookie.reply().ok());
        if let Some(pixmap) = pixmap {
            let _ = self.conn.free_pixmap(pixmap);
        }
        let mut image = image.ok_or(GrabError::Unreadable)?;
        let (width, height) = (usize::from(rect.width), usize::from(rect.height));
        if image.data.len() < width * height * 4 {
            return Err(GrabError::Unreadable);
        }
        if cursor && self.xfixes {
            let pointer = self
                .conn
                .xfixes_get_cursor_image()
                .ok()
                .and_then(|cookie| cookie.reply().ok());
            if let Some(pointer) = pointer {
                blend_cursor(
                    &mut image.data,
                    (width, height),
                    &pointer.cursor_image,
                    (usize::from(pointer.width), usize::from(pointer.height)),
                    (
                        i32::from(pointer.x) - i32::from(pointer.xhot) - i32::from(origin.0),
                        i32::from(pointer.y) - i32::from(pointer.yhot) - i32::from(origin.1),
                    ),
                );
            }
        }
        Ok(Grab {
            bgrx: image.data,
            width,
            height,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rect {
    x: i16,
    y: i16,
    width: u16,
    height: u16,
}

impl Rect {
    /// This rectangle inside `outer` (the root window's size).
    fn clamped(&self, outer: &Rect) -> Option<Rect> {
        let x = self.x.max(0);
        let y = self.y.max(0);
        let right = (i32::from(self.x) + i32::from(self.width)).min(i32::from(outer.width));
        let bottom = (i32::from(self.y) + i32::from(self.height)).min(i32::from(outer.height));
        let width = u16::try_from(right - i32::from(x))
            .ok()
            .filter(|w| *w > 0)?;
        let height = u16::try_from(bottom - i32::from(y))
            .ok()
            .filter(|h| *h > 0)?;
        Some(Rect {
            x,
            y,
            width,
            height,
        })
    }
}

/// What a source id names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Screen(Rect),
    Window(Window),
}

impl Target {
    fn parse(id: &SourceId) -> Option<Self> {
        if let Some(window) = id.0.strip_prefix(WINDOW_PREFIX) {
            return window.parse().ok().map(Target::Window);
        }
        let mut parts = id.0.strip_prefix(SCREEN_PREFIX)?.split(',');
        let mut next = || parts.next()?.parse::<i32>().ok();
        let (x, y, width, height) = (next()?, next()?, next()?, next()?);
        Some(Target::Screen(Rect {
            x: i16::try_from(x).ok()?,
            y: i16::try_from(y).ok()?,
            width: u16::try_from(width).ok()?,
            height: u16::try_from(height).ok()?,
        }))
    }
}

struct Grab {
    bgrx: Vec<u8>,
    width: usize,
    height: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GrabError {
    /// The window or screen no longer exists.
    Gone,
    /// It exists but gave no picture this time (unmapped, mid-resize).
    Unreadable,
}

/// Draw an XFixes cursor (premultiplied ARGB, `size`) with its top left at
/// `at` into a BGRx picture of `frame` size.
pub fn blend_cursor(
    bgrx: &mut [u8],
    frame: (usize, usize),
    cursor: &[u32],
    size: (usize, usize),
    at: (i32, i32),
) {
    for (row, line) in cursor.chunks(size.0.max(1)).take(size.1).enumerate() {
        let Ok(y) = usize::try_from(i64::from(at.1) + row as i64) else {
            continue;
        };
        if y >= frame.1 {
            break;
        }
        for (col, argb) in line.iter().enumerate() {
            let alpha = argb >> 24;
            let Ok(x) = usize::try_from(i64::from(at.0) + col as i64) else {
                continue;
            };
            if alpha == 0 || x >= frame.0 {
                continue;
            }
            let at = (y * frame.0 + x) * 4;
            let Some(pixel) = bgrx.get_mut(at..at + 3) else {
                continue;
            };
            for (shift, channel) in [0u32, 8, 16].into_iter().zip(pixel.iter_mut()) {
                let source = (argb >> shift) & 0xff;
                let blended = source + u32::from(*channel) * (255 - alpha) / 255;
                *channel = blended.min(255) as u8;
            }
        }
    }
}

/// Screens, then windows.
pub fn sources() -> Result<Vec<ScreenSource>, CaptureError> {
    let x11 = X11::connect()?;
    let mut sources = x11.monitors();
    sources.extend(x11.windows());
    Ok(sources)
}

/// One picture of `source`, fitted into `max`.
pub fn thumbnail(source: &ScreenSource, max: (u32, u32)) -> Option<Thumbnail> {
    let x11 = X11::connect().ok()?;
    let target = Target::parse(&source.id)?;
    let redirected = redirect(&x11, target)?;
    let grab = x11.grab(&target, false).ok();
    if redirected {
        unredirect(&x11, target);
    }
    let grab = grab?;
    packed_thumbnail(
        &grab.bgrx,
        PackedOrder::Bgra,
        grab.width * 4,
        (grab.width, grab.height),
        max,
    )
}

/// Ask Composite to keep `target`'s pixels off screen. `None` when a window
/// cannot be read without it; `Some(true)` when it must be undone.
fn redirect(x11: &X11, target: Target) -> Option<bool> {
    let Target::Window(window) = target else {
        return Some(false);
    };
    if !x11.composite {
        // webrtc refuses window capture without Composite ("No Xcomposite
        // extension detected").
        return None;
    }
    x11.conn
        .composite_redirect_window(window, Redirect::AUTOMATIC)
        .ok()?;
    Some(true)
}

fn unredirect(x11: &X11, target: Target) {
    if let Target::Window(window) = target {
        let _ = x11
            .conn
            .composite_unredirect_window(window, Redirect::AUTOMATIC);
        let _ = x11.conn.flush();
    }
}

/// Start capturing `source` on a thread of its own.
pub fn start(
    source: &SourceId,
    options: ScreenOptions,
    sink: FrameSink<VideoFrame>,
    preview: Arc<PreviewSlot>,
) -> Result<Box<dyn ScreenCapture>, CaptureError> {
    let target = Target::parse(source)
        .ok_or_else(|| CaptureError::Device("The shared window or screen is gone.".into()))?;
    let x11 = X11::connect()?;
    if redirect(&x11, target).is_none() {
        return Err(CaptureError::Unsupported(
            "Sharing a window needs the X Composite extension.".into(),
        ));
    }
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = stop.clone();
    let worker = crate::thread_guard::spawn_guarded("quill-screen-x11", move || {
        run(&x11, target, options, &sink, &preview, &worker_stop);
        unredirect(&x11, target);
    })
    .map_err(|err| CaptureError::Device(format!("Couldn't start screen sharing: {err}")))?;
    Ok(Box::new(X11Capture {
        stop,
        worker: Some(worker),
    }))
}

/// The capture loop: a frame every `1 / fps`, the window's state every
/// [`PAUSE_POLL`].
fn run(
    x11: &X11,
    target: Target,
    options: ScreenOptions,
    sink: &FrameSink<VideoFrame>,
    preview: &PreviewSlot,
    stop: &AtomicBool,
) {
    let mut pacer = FramePacer::new(options.fps);
    let mut tracker = PauseTracker::default();
    let mut checked: Option<Instant> = None;
    let mut hidden = false;
    let interval = Duration::from_secs(1) / u32::from(options.fps.max(1));
    while !stop.load(Ordering::Acquire) {
        let now = Instant::now();
        if let Target::Window(window) = target
            && checked.is_none_or(|at| now.saturating_duration_since(at) >= PAUSE_POLL)
        {
            checked = Some(now);
            if x11.geometry(window).is_none() {
                sink.push_event(CaptureEvent::Ended);
                return;
            }
            hidden = x11.wm_state(window) == Some(ICONIC_STATE);
        }
        if hidden {
            // webrtc hands tgcalls a 1 x 1 frame for a minimised window,
            // which tgcalls reports as a pause.
            if let Some(event) = tracker.no_picture() {
                sink.push_event(event);
            }
            std::thread::sleep(PAUSE_POLL.min(interval * 4));
            continue;
        }
        if pacer.due(now) {
            match x11.grab(&target, true) {
                Ok(grab) => {
                    if let Some(frame) = convert(&grab, options, now) {
                        preview.offer(&frame);
                        sink.push(frame);
                        if let Some(event) = tracker.picture() {
                            sink.push_event(event);
                        }
                    }
                }
                Err(GrabError::Gone) => {
                    sink.push_event(CaptureEvent::Ended);
                    return;
                }
                Err(GrabError::Unreadable) => {
                    if let Some(event) = tracker.no_picture() {
                        sink.push_event(event);
                    }
                }
            }
        }
        let spent = Instant::now().saturating_duration_since(now);
        std::thread::sleep(interval.saturating_sub(spent).max(Duration::from_millis(1)));
    }
}

fn convert(grab: &Grab, options: ScreenOptions, at: Instant) -> Option<VideoFrame> {
    let (w, h) = fit_capture_size(
        u32::try_from(grab.width).ok()?,
        u32::try_from(grab.height).ok()?,
        options.max_width,
        options.max_height,
    );
    let i420 = packed_to_i420(
        &grab.bgrx,
        PackedOrder::Bgra,
        grab.width * 4,
        (grab.width, grab.height),
        (w as usize, h as usize),
    )?;
    Some(VideoFrame {
        i420,
        width: u16::try_from(w).ok()?,
        height: u16::try_from(h).ok()?,
        captured_at: at,
    })
}

struct X11Capture {
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<Option<()>>>,
}

impl ScreenCapture for X11Capture {
    fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            // At most one frame's grab away.
            let _ = worker.join();
        }
    }
}

impl Drop for X11Capture {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
#[path = "screen_x11_tests.rs"]
mod tests;
