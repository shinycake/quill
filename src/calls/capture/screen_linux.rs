//! Linux screen capture, as tdesktop does it (tgcalls' WebRTC capturers):
//! - **Wayland**: no app may read the screen, so the screen cast portal
//!   (`org.freedesktop.portal.ScreenCast`, through `ashpd`) shows the
//!   desktop's own picker. tdesktop then skips its chooser
//!   (`uniqueDesktopCaptureSource`): the share button starts sharing, or
//!   stops a running share. The portal hands back a PipeWire node, which
//!   `pipewire_dl` streams. The portal session closing (the desktop's
//!   "Stop sharing", the source going away) ends the share, and the
//!   stream pausing pauses it.
//! - **X11**: Quill's chooser lists screens and windows and captures them
//!   itself (`screen_x11`).
//!
//! GPUI picks Wayland when `WAYLAND_DISPLAY` is set; so does this.
#![deny(clippy::indexing_slicing)]

use super::pipewire_dl::{self, PlaneData, PwStream, StreamHandler, StreamState};
use super::pipewire_frames::buffer_to_i420;
use super::screen::{
    PreviewSlot, ScreenAccess, ScreenBackend, ScreenCapture, ScreenOptions, ScreenSource, SourceId,
    Thumbnail, fit_capture_size,
};
use super::screen_bgra::FramePacer;
use super::spa_pod::NegotiatedFormat;
use super::{CaptureError, CaptureEvent, FrameSink, VideoFrame};
use ashpd::desktop::screencast::{
    CursorMode, Screencast, SelectSourcesOptions, SourceType, StartCastOptions,
};
use ashpd::desktop::{CreateSessionOptions, ResponseError, Session};
use futures_channel::oneshot;
use futures_lite::StreamExt;
use std::sync::Arc;
use std::time::Instant;

/// The Linux backend for this session.
pub enum LinuxScreen {
    /// Wayland: the screen cast portal picks and PipeWire streams.
    Portal,
    /// X11: Quill lists and captures.
    X11,
}

impl LinuxScreen {
    pub fn detect() -> Self {
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some_and(|value| !value.is_empty());
        if wayland { Self::Portal } else { Self::X11 }
    }
}

impl ScreenBackend for LinuxScreen {
    fn system_picker(&self) -> bool {
        matches!(self, Self::Portal)
    }

    fn access(&self) -> ScreenAccess {
        ScreenAccess::Granted
    }

    fn ask_access(&self) -> Option<&'static str> {
        None
    }

    fn sources(&self) -> Result<Vec<ScreenSource>, CaptureError> {
        match self {
            Self::Portal => Ok(Vec::new()),
            Self::X11 => super::screen_x11::sources(),
        }
    }

    fn thumbnail(&self, source: &ScreenSource, max: (u32, u32)) -> Option<Thumbnail> {
        match self {
            Self::Portal => None,
            Self::X11 => super::screen_x11::thumbnail(&source.id, max),
        }
    }

    fn start(
        &self,
        source: &SourceId,
        options: ScreenOptions,
        sink: FrameSink<VideoFrame>,
        preview: Arc<PreviewSlot>,
    ) -> Result<Box<dyn ScreenCapture>, CaptureError> {
        match self {
            Self::Portal => start_portal(options, sink, preview),
            Self::X11 => super::screen_x11::start(source, options, sink, preview),
        }
    }
}

fn unsupported() -> CaptureError {
    CaptureError::Unsupported(
        crate::lang::tr(crate::lang::keys::Q_OPS_SCREEN_PORTAL_MISSING).to_string(),
    )
}

/// A portal screen cast: the picker, then the stream, on one thread.
struct PortalCapture {
    stop: Option<oneshot::Sender<()>>,
}

impl ScreenCapture for PortalCapture {
    fn stop(&mut self) {
        // The thread closes the portal session and the stream itself, so
        // stopping never waits on the picker or D-Bus.
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}

impl Drop for PortalCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

fn start_portal(
    options: ScreenOptions,
    sink: FrameSink<VideoFrame>,
    preview: Arc<PreviewSlot>,
) -> Result<Box<dyn ScreenCapture>, CaptureError> {
    if !pipewire_dl::available() {
        return Err(unsupported());
    }
    let (stop, stopped) = oneshot::channel();
    crate::thread_guard::spawn_guarded("quill-screen-portal", move || {
        let events = sink.clone();
        let outcome = async_io::block_on(portal_session(options, sink, preview, stopped));
        match outcome {
            Outcome::Stopped => {}
            Outcome::Ended => events.push_event(CaptureEvent::Ended),
            Outcome::Failed(err) => events.push_event(CaptureEvent::Failed(err)),
        }
    })
    .map_err(|err| CaptureError::Device(format!("Couldn't start screen sharing: {err}")))?;
    Ok(Box::new(PortalCapture { stop: Some(stop) }))
}

enum Outcome {
    /// Quill stopped the share.
    Stopped,
    /// The picker was cancelled or the session closed.
    Ended,
    Failed(CaptureError),
}

fn portal_error(err: ashpd::Error) -> Outcome {
    match err {
        ashpd::Error::Response(ResponseError::Cancelled) => Outcome::Ended,
        ashpd::Error::PortalNotFound(_) => Outcome::Failed(unsupported()),
        other => Outcome::Failed(CaptureError::Device(format!(
            "Screen sharing failed: {other}"
        ))),
    }
}

async fn portal_session(
    options: ScreenOptions,
    sink: FrameSink<VideoFrame>,
    preview: Arc<PreviewSlot>,
    stopped: oneshot::Receiver<()>,
) -> Outcome {
    let proxy = match Screencast::new().await {
        Ok(proxy) => proxy,
        Err(err) => return portal_error(err),
    };
    let session = match proxy.create_session(CreateSessionOptions::default()).await {
        Ok(session) => session,
        Err(err) => return portal_error(err),
    };
    let stop = async {
        // A dropped sender (the capture went away) stops too.
        let _ = stopped.await;
        Outcome::Stopped
    };
    let run = async {
        match cast(&proxy, &session, options, sink, preview).await {
            Ok(outcome) => outcome,
            Err(err) => portal_error(err),
        }
    };
    let outcome = futures_lite::future::or(stop, run).await;
    // Ends the picker if it is still open, and the cast.
    let _ = session.close().await;
    outcome
}

/// The picker, then the stream until the session closes.
async fn cast(
    proxy: &Screencast,
    session: &Session<Screencast>,
    options: ScreenOptions,
    sink: FrameSink<VideoFrame>,
    preview: Arc<PreviewSlot>,
) -> Result<Outcome, ashpd::Error> {
    let offered = proxy
        .available_source_types()
        .await
        .unwrap_or(SourceType::Monitor | SourceType::Window);
    let mut types = offered & (SourceType::Monitor | SourceType::Window);
    if types.is_empty() {
        types = SourceType::Monitor.into();
    }
    let cursor = proxy
        .available_cursor_modes()
        .await
        .ok()
        .filter(|modes| modes.contains(CursorMode::Embedded))
        .map(|_| CursorMode::Embedded);
    proxy
        .select_sources(
            session,
            SelectSourcesOptions::default()
                .set_cursor_mode(cursor)
                .set_sources(types)
                .set_multiple(false),
        )
        .await?
        .response()?;
    let streams = proxy
        .start(session, None, StartCastOptions::default())
        .await?
        .response()?;
    let Some(node) = streams.streams().first().map(|s| s.pipe_wire_node_id()) else {
        return Ok(Outcome::Ended);
    };
    let fd = proxy
        .open_pipe_wire_remote(session, Default::default())
        .await?;
    let mut closed = session.receive_closed().await?;
    let handler = Frames {
        sink,
        preview,
        options,
        pacer: FramePacer::new(options.fps),
        format: None,
        streamed: false,
        paused: false,
    };
    let stream = match PwStream::connect(fd, node, Box::new(handler)) {
        Ok(stream) => stream,
        Err(err) => return Ok(Outcome::Failed(CaptureError::Device(err))),
    };
    let _ = closed.next().await;
    drop(stream);
    Ok(Outcome::Ended)
}

/// Turns PipeWire buffers into call frames, on PipeWire's thread.
struct Frames {
    sink: FrameSink<VideoFrame>,
    preview: Arc<PreviewSlot>,
    options: ScreenOptions,
    pacer: FramePacer,
    format: Option<NegotiatedFormat>,
    /// Whether frames have flowed: pausing before that is setup.
    streamed: bool,
    paused: bool,
}

impl StreamHandler for Frames {
    fn format_changed(&mut self, format: Option<NegotiatedFormat>) {
        self.format = format;
    }

    fn frame(&mut self, planes: &[PlaneData<'_>]) {
        let Some(format) = self.format else {
            return;
        };
        let now = Instant::now();
        if !self.pacer.due(now) {
            return;
        }
        let (w, h) = fit_capture_size(
            format.width,
            format.height,
            self.options.max_width,
            self.options.max_height,
        );
        let blocks: Vec<(&[u8], usize)> = planes.iter().map(|p| (p.bytes, p.stride)).collect();
        let (Some(i420), Ok(width), Ok(height)) = (
            buffer_to_i420(format, &blocks, (w as usize, h as usize)),
            u16::try_from(w),
            u16::try_from(h),
        ) else {
            return;
        };
        let frame = VideoFrame {
            i420,
            width,
            height,
            captured_at: now,
        };
        self.preview.offer(&frame);
        self.sink.push(frame);
    }

    fn state_changed(&mut self, state: StreamState, error: Option<String>) {
        match state {
            StreamState::Streaming => {
                if self.paused {
                    self.paused = false;
                    self.sink.push_event(CaptureEvent::Resumed);
                }
                self.streamed = true;
            }
            StreamState::Paused if self.streamed && !self.paused => {
                self.paused = true;
                self.sink.push_event(CaptureEvent::Paused);
            }
            StreamState::Error => self
                .sink
                .push_event(CaptureEvent::Failed(CaptureError::Device(format!(
                    "Screen sharing failed: {}",
                    error.unwrap_or_default()
                )))),
            StreamState::Unconnected if self.streamed => {
                self.sink.push_event(CaptureEvent::Ended);
            }
            _ => {}
        }
    }
}
