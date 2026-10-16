//! X11 screen capture (best effort, as the design says), behind Quill's
//! chooser like tdesktop's X11 capturer:
//! - Screens are the RandR monitors, the primary first; windows are the
//!   window manager's `_NET_CLIENT_LIST`, without docks, desktops, hidden
//!   windows, Quill's own and anything under 100 x 100.
//! - Pictures come from `GetImage`: a screen's area of the root window,
//!   or a window's Composite pixmap (so overlapping windows do not show
//!   through), falling back to the window itself without Composite.
//! - A capture grabs at 15 fps on its own thread and connection. A shared
//!   window is checked every 500 ms: unmapped or `_NET_WM_STATE_HIDDEN`
//!   pauses, back resumes, gone ends the share.
//!
//! Only 24- and 32-bit little-endian TrueColor displays (every current
//! X server) are read; others get `Unsupported`.
#![deny(clippy::indexing_slicing)]

use super::screen::{
    PreviewSlot, ScreenCapture, ScreenOptions, ScreenSource, SourceId, SourceKind, Thumbnail,
    fit_capture_size,
};
use super::screen_bgra::{bgra_thumbnail, bgra_to_i420};
use super::{CaptureError, CaptureEvent, FrameSink, VideoFrame};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol::composite::{ConnectionExt as _, Redirect};
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ConnectionExt as _, ImageFormat, ImageOrder, MapState, Window,
};
use x11rb::rust_connection::RustConnection;

/// How often a shared window's state is checked.
const STATE_POLL: Duration = Duration::from_millis(500);
/// Windows smaller than this are toolbars and helpers.
const MIN_WINDOW_SIDE: u16 = 100;

/// One open display and what reading it needs.
struct Display {
    conn: RustConnection,
    root: Window,
    /// Whether the Composite extension can name window pixmaps.
    composite: bool,
}

impl Display {
    fn open() -> Result<Self, CaptureError> {
        let failed = |err: &dyn std::fmt::Display| {
            CaptureError::Device(format!("Couldn't open the X display: {err}"))
        };
        let (conn, screen) = x11rb::connect(None).map_err(|err| failed(&err))?;
        let setup = conn.setup();
        let root = setup
            .roots
            .get(screen)
            .map(|screen| screen.root)
            .ok_or_else(|| failed(&"no screen"))?;
        let depth_ok = setup
            .pixmap_formats
            .iter()
            .any(|format| matches!(format.depth, 24 | 32) && format.bits_per_pixel == 32);
        if setup.image_byte_order != ImageOrder::LSB_FIRST || !depth_ok {
            return Err(CaptureError::Unsupported(
                "This X display's pixel format can't be shared.".into(),
            ));
        }
        let composite = conn
            .composite_query_version(0, 4)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .is_some();
        Ok(Self {
            conn,
            root,
            composite,
        })
    }

    fn atom(&self, name: &str) -> Option<Atom> {
        Some(
            self.conn
                .intern_atom(false, name.as_bytes())
                .ok()?
                .reply()
                .ok()?
                .atom,
        )
    }

    fn property32(&self, window: Window, property: Atom, kind: impl Into<Atom>) -> Vec<u32> {
        self.conn
            .get_property(false, window, property, kind, 0, u32::MAX / 4)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .and_then(|reply| reply.value32().map(Iterator::collect))
            .unwrap_or_default()
    }

    fn title(&self, window: Window) -> String {
        let utf8 = self.atom("UTF8_STRING");
        let net_name = self.atom("_NET_WM_NAME");
        let read = |property: Atom, kind: Atom| {
            self.conn
                .get_property(false, window, property, kind, 0, 1024)
                .ok()?
                .reply()
                .ok()
                .map(|reply| String::from_utf8_lossy(&reply.value).into_owned())
                .filter(|title| !title.trim().is_empty())
        };
        net_name
            .zip(utf8)
            .and_then(|(name, utf8)| read(name, utf8))
            .or_else(|| read(AtomEnum::WM_NAME.into(), AtomEnum::STRING.into()))
            .unwrap_or_default()
    }

    /// Mapped and not hidden by the window manager; `None` when gone.
    fn visible(&self, window: Window, state: Option<Atom>, hidden: Option<Atom>) -> Option<bool> {
        let attributes = self.conn.get_window_attributes(window).ok()?.reply().ok()?;
        let hidden = state.zip(hidden).is_some_and(|(state, hidden)| {
            self.property32(window, state, AtomEnum::ATOM)
                .contains(&hidden)
        });
        Some(attributes.map_state == MapState::VIEWABLE && !hidden)
    }

    /// BGRx pixels of `window` (its Composite pixmap when possible), or of
    /// the root window's `area`: (bytes, stride, width, height).
    fn grab(&self, target: Target) -> Option<(Vec<u8>, usize, usize, usize)> {
        let (drawable, x, y, width, height, pixmap) = match target {
            Target::Area {
                x,
                y,
                width,
                height,
            } => (self.root, x, y, width, height, None),
            Target::Window(window) => {
                let geometry = self.conn.get_geometry(window).ok()?.reply().ok()?;
                let pixmap = self
                    .composite
                    .then(|| self.conn.generate_id().ok())
                    .flatten();
                let named = pixmap.filter(|&pixmap| {
                    self.conn
                        .composite_name_window_pixmap(window, pixmap)
                        .ok()
                        .and_then(|cookie| cookie.check().ok())
                        .is_some()
                });
                let drawable = named.unwrap_or(window);
                (drawable, 0, 0, geometry.width, geometry.height, named)
            }
        };
        let image = self
            .conn
            .get_image(
                ImageFormat::Z_PIXMAP,
                drawable,
                x,
                y,
                width,
                height,
                u32::MAX,
            )
            .ok()
            .and_then(|cookie| cookie.reply().ok());
        if let Some(pixmap) = pixmap {
            let _ = self.conn.free_pixmap(pixmap);
        }
        let image = image?;
        let (w, h) = (usize::from(width), usize::from(height));
        if !matches!(image.depth, 24 | 32) || w == 0 || h == 0 {
            return None;
        }
        let stride = image.data.len() / h;
        (stride >= w * 4).then_some((image.data, stride, w, h))
    }
}

/// What a source id names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Area {
        x: i16,
        y: i16,
        width: u16,
        height: u16,
    },
    Window(Window),
}

/// `display:<x>,<y>,<width>,<height>` or `window:<xid>`.
fn parse_id(id: &SourceId) -> Option<Target> {
    if let Some(window) = id.0.strip_prefix("window:") {
        return window.parse().ok().map(Target::Window);
    }
    let mut parts = id.0.strip_prefix("display:")?.split(',');
    let mut next = || parts.next()?.parse::<i32>().ok();
    let (x, y, width, height) = (next()?, next()?, next()?, next()?);
    Some(Target::Area {
        x: i16::try_from(x).ok()?,
        y: i16::try_from(y).ok()?,
        width: u16::try_from(width).ok().filter(|w| *w > 0)?,
        height: u16::try_from(height).ok().filter(|h| *h > 0)?,
    })
}

/// Screens (primary first), then the window manager's windows.
pub fn sources() -> Result<Vec<ScreenSource>, CaptureError> {
    let display = Display::open()?;
    let mut monitors = display
        .conn
        .randr_get_monitors(display.root, true)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .map(|reply| reply.monitors)
        .unwrap_or_default();
    monitors.sort_by_key(|monitor| !monitor.primary);
    let mut sources: Vec<ScreenSource> = monitors
        .iter()
        .map(|m| ScreenSource {
            id: SourceId(format!("display:{},{},{},{}", m.x, m.y, m.width, m.height)),
            kind: SourceKind::Screen,
            title: String::new(),
            size: (u32::from(m.width), u32::from(m.height)),
        })
        .collect();
    if sources.is_empty() {
        // No RandR: the whole root window is the one screen.
        if let Ok(geometry) = display
            .conn
            .get_geometry(display.root)
            .map_err(|_| ())
            .and_then(|cookie| cookie.reply().map_err(|_| ()))
        {
            sources.push(ScreenSource {
                id: SourceId(format!(
                    "display:0,0,{},{}",
                    geometry.width, geometry.height
                )),
                kind: SourceKind::Screen,
                title: String::new(),
                size: (u32::from(geometry.width), u32::from(geometry.height)),
            });
        }
    }
    sources.extend(windows(&display));
    Ok(sources)
}

fn windows(display: &Display) -> Vec<ScreenSource> {
    let Some(list) = display.atom("_NET_CLIENT_LIST") else {
        return Vec::new();
    };
    let pid_atom = display.atom("_NET_WM_PID");
    let type_atom = display.atom("_NET_WM_WINDOW_TYPE");
    let skipped_types: Vec<Atom> = ["_NET_WM_WINDOW_TYPE_DOCK", "_NET_WM_WINDOW_TYPE_DESKTOP"]
        .iter()
        .filter_map(|name| display.atom(name))
        .collect();
    let state = display.atom("_NET_WM_STATE");
    let hidden = display.atom("_NET_WM_STATE_HIDDEN");
    let own_pid = std::process::id();
    let mut found = Vec::new();
    for window in display.property32(display.root, list, AtomEnum::WINDOW) {
        if display.visible(window, state, hidden) != Some(true) {
            continue;
        }
        let pid = pid_atom.and_then(|atom| {
            display
                .property32(window, atom, AtomEnum::CARDINAL)
                .first()
                .copied()
        });
        if pid == Some(own_pid) {
            continue;
        }
        let types = type_atom
            .map(|atom| display.property32(window, atom, AtomEnum::ATOM))
            .unwrap_or_default();
        if types.iter().any(|kind| skipped_types.contains(kind)) {
            continue;
        }
        let Some(geometry) = display
            .conn
            .get_geometry(window)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
        else {
            continue;
        };
        if geometry.width < MIN_WINDOW_SIDE || geometry.height < MIN_WINDOW_SIDE {
            continue;
        }
        found.push(ScreenSource {
            id: SourceId(format!("window:{window}")),
            kind: SourceKind::Window,
            title: display.title(window),
            size: (u32::from(geometry.width), u32::from(geometry.height)),
        });
    }
    found
}

/// One picture of a source.
pub fn thumbnail(id: &SourceId, max: (u32, u32)) -> Option<Thumbnail> {
    let display = Display::open().ok()?;
    let (bytes, stride, w, h) = display.grab(parse_id(id)?)?;
    bgra_thumbnail(&bytes, stride, w, h, max)
}

/// A running X11 capture.
struct X11Capture {
    stop: Option<mpsc::Sender<()>>,
    worker: Option<std::thread::JoinHandle<Option<()>>>,
}

impl ScreenCapture for X11Capture {
    fn stop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for X11Capture {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn start(
    source: &SourceId,
    options: ScreenOptions,
    sink: FrameSink<VideoFrame>,
    preview: Arc<PreviewSlot>,
) -> Result<Box<dyn ScreenCapture>, CaptureError> {
    let target = parse_id(source)
        .ok_or_else(|| CaptureError::Device("The shared window or screen is gone.".into()))?;
    let display = Display::open()?;
    if let Target::Window(window) = target {
        if display.visible(window, None, None).is_none() {
            return Err(CaptureError::Device(
                "The shared window or screen is gone.".into(),
            ));
        }
        if display.composite {
            let _ = display
                .conn
                .composite_redirect_window(window, Redirect::AUTOMATIC);
            let _ = display.conn.flush();
        }
    }
    let (stop, stopped) = mpsc::channel::<()>();
    let worker = crate::thread_guard::spawn_guarded("quill-screen-x11", move || {
        run(display, target, options, sink, preview, stopped);
    })
    .map_err(|err| CaptureError::Device(format!("Couldn't start screen sharing: {err}")))?;
    Ok(Box::new(X11Capture {
        stop: Some(stop),
        worker: Some(worker),
    }))
}

fn run(
    display: Display,
    target: Target,
    options: ScreenOptions,
    sink: FrameSink<VideoFrame>,
    preview: Arc<PreviewSlot>,
    stopped: mpsc::Receiver<()>,
) {
    let interval = Duration::from_secs(1) / u32::from(options.fps.max(1));
    let state = display.atom("_NET_WM_STATE");
    let hidden = display.atom("_NET_WM_STATE_HIDDEN");
    let mut paused = false;
    let mut checked: Option<Instant> = None;
    while let Err(mpsc::RecvTimeoutError::Timeout) = stopped.recv_timeout(interval) {
        if let Target::Window(window) = target
            && checked.is_none_or(|at| at.elapsed() >= STATE_POLL)
        {
            checked = Some(Instant::now());
            let Some(visible) = display.visible(window, state, hidden) else {
                sink.push_event(CaptureEvent::Ended);
                break;
            };
            if visible == paused {
                paused = !visible;
                sink.push_event(if paused {
                    CaptureEvent::Paused
                } else {
                    CaptureEvent::Resumed
                });
            }
        }
        if paused {
            continue;
        }
        let now = Instant::now();
        let Some((bytes, stride, w, h)) = display.grab(target) else {
            continue;
        };
        let (Ok(src_w), Ok(src_h)) = (u32::try_from(w), u32::try_from(h)) else {
            continue;
        };
        let (fw, fh) = fit_capture_size(src_w, src_h, options.max_width, options.max_height);
        let (Some(i420), Ok(width), Ok(height)) = (
            bgra_to_i420(&bytes, stride, w, h, fw as usize, fh as usize),
            u16::try_from(fw),
            u16::try_from(fh),
        ) else {
            continue;
        };
        let frame = VideoFrame {
            i420,
            width,
            height,
            captured_at: now,
        };
        preview.offer(&frame);
        sink.push(frame);
    }
    if let Target::Window(window) = target
        && display.composite
    {
        let _ = display
            .conn
            .composite_unredirect_window(window, Redirect::AUTOMATIC);
        let _ = display.conn.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_ids_round_trip() {
        assert_eq!(
            parse_id(&SourceId("display:-1920,0,1920,1080".into())),
            Some(Target::Area {
                x: -1920,
                y: 0,
                width: 1920,
                height: 1080
            })
        );
        assert_eq!(
            parse_id(&SourceId("window:4194307".into())),
            Some(Target::Window(4_194_307))
        );
        assert_eq!(parse_id(&SourceId("display:0,0,0,10".into())), None);
        assert_eq!(parse_id(&SourceId("display:0,0".into())), None);
        assert_eq!(parse_id(&SourceId("system-picker".into())), None);
    }
}
