//! Screen sharing on Wayland through the xdg-desktop-portal ScreenCast
//! interface (`ashpd`, already linked by GPUI), as WebRTC's PipeWire
//! capturer that tdesktop uses there (`BaseCapturerPipeWire`,
//! `ScreenCastPortal`):
//! - one source of any kind (`MONITOR | WINDOW`, whichever the portal
//!   offers), not persisted, so the portal asks every time;
//! - the cursor embedded in the picture when the portal offers it
//!   (tgcalls captures the mouse);
//! - the portal's own dialog picks it. Until the first picture arrives the
//!   share counts as paused, as tgcalls reports a capture without frames;
//! - cancelling the dialog, or a portal or PipeWire failure, stops the
//!   share with tdesktop's "An error occurred. Screencast has stopped."
//!   (`lng_group_call_failed_screen`); the stream's node going away (the
//!   user stopped the cast from the desktop) ends it.
//!
//! Everything runs on one thread per share: the portal calls (blocking on
//! zbus' async-io executor), then the PipeWire stream until the share
//! stops. Stopping never waits for it; the thread closes the stream and
//! the portal session on its way out.
#![deny(clippy::indexing_slicing)]

use super::pipewire::{PipeWireStream, StreamHandler, StreamState};
use super::screen::{PreviewSlot, ScreenCapture, ScreenOptions, fit_capture_size};
use super::screen_bgra::FramePacer;
use super::screen_linux::PauseTracker;
use super::screen_raw::{RawPicture, raw_to_i420};
use super::{CaptureError, CaptureEvent, FrameSink, VideoFrame};
use ashpd::desktop::Session;
use ashpd::desktop::screencast::{CursorMode, Screencast, SelectSourcesOptions, SourceType};
use futures_lite::future;
use std::os::fd::OwnedFd;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// How often the portal thread looks at the stop flag.
const POLL: Duration = Duration::from_millis(50);
/// How long closing the portal session may take on the way out.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(2);

/// tdesktop's toast for any screencast failure.
fn failed_screen() -> CaptureError {
    CaptureError::Device(crate::lang::tr(crate::lang::keys::GROUP_CALL_FAILED_SCREEN).to_string())
}

/// What the portal handed out.
struct Opened {
    session: Session<Screencast>,
    remote: OwnedFd,
    node: u32,
    size: Option<(u32, u32)>,
}

enum PortalEnd {
    /// The share was stopped while the portal was asking.
    Stopped,
    Failed(String),
}

/// Start sharing: the portal dialog first, then the stream.
pub fn start(
    options: ScreenOptions,
    sink: FrameSink<VideoFrame>,
    preview: Arc<PreviewSlot>,
) -> Result<Box<dyn ScreenCapture>, CaptureError> {
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = stop.clone();
    // No picture until the user picks (tgcalls: a capture without frames
    // is paused).
    sink.push_event(CaptureEvent::Paused);
    crate::thread_guard::spawn_guarded("quill-screen-portal", move || {
        run(options, &sink, preview, &worker_stop);
    })
    .map_err(|err| CaptureError::Device(format!("Couldn't start screen sharing: {err}")))?;
    Ok(Box::new(PortalCapture { stop }))
}

fn run(
    options: ScreenOptions,
    sink: &FrameSink<VideoFrame>,
    preview: Arc<PreviewSlot>,
    stop: &Arc<AtomicBool>,
) {
    let opened = async_io::block_on(open(stop));
    let opened = match opened {
        Ok(opened) => opened,
        Err(PortalEnd::Stopped) => return,
        Err(PortalEnd::Failed(why)) => {
            eprintln!("quill: screen portal: {why}");
            if !stop.load(Ordering::Acquire) {
                sink.push_event(CaptureEvent::Failed(failed_screen()));
            }
            return;
        }
    };
    let Opened {
        session,
        remote,
        node,
        size,
    } = opened;
    let done = Arc::new(AtomicBool::new(false));
    let handler = PortalFrames {
        sink: sink.clone(),
        preview,
        options,
        pacer: FramePacer::new(options.fps),
        tracker: paused_tracker(),
        streamed: false,
        stop: stop.clone(),
        done: done.clone(),
    };
    let default_size = size.unwrap_or((options.max_width.max(1), options.max_height.max(1)));
    match PipeWireStream::connect(Some(remote), node, default_size, Box::new(handler)) {
        Ok(stream) => {
            while !stop.load(Ordering::Acquire) && !done.load(Ordering::Acquire) {
                std::thread::sleep(POLL);
            }
            drop(stream);
        }
        Err(err) => {
            eprintln!("quill: screen portal: {err}");
            if !stop.load(Ordering::Acquire) {
                sink.push_event(CaptureEvent::Failed(failed_screen()));
            }
        }
    }
    async_io::block_on(close(&session));
}

/// A tracker that starts paused (the share was reported paused at start).
fn paused_tracker() -> PauseTracker {
    let mut tracker = PauseTracker::default();
    let _ = tracker.no_picture();
    tracker
}

/// Resolves once `stop` is set.
async fn stopped(stop: &AtomicBool) -> PortalEnd {
    while !stop.load(Ordering::Acquire) {
        async_io::Timer::after(POLL).await;
    }
    PortalEnd::Stopped
}

fn portal_error(err: ashpd::Error) -> PortalEnd {
    PortalEnd::Failed(err.to_string())
}

/// The portal steps, each given up as soon as the share stops.
async fn open(stop: &AtomicBool) -> Result<Opened, PortalEnd> {
    let setup = async {
        let proxy = Screencast::new().await.map_err(portal_error)?;
        let session = proxy
            .create_session(Default::default())
            .await
            .map_err(portal_error)?;
        Ok((proxy, session))
    };
    let (proxy, session) = future::or(setup, async { Err(stopped(stop).await) }).await?;
    let picked = async {
        let cursor = proxy
            .available_cursor_modes()
            .await
            .unwrap_or_default()
            .contains(CursorMode::Embedded)
            .then_some(CursorMode::Embedded);
        let offered = proxy
            .available_source_types()
            .await
            .unwrap_or(SourceType::Monitor.into());
        let wanted = SourceType::Monitor | SourceType::Window;
        let types = if offered.intersects(wanted) {
            offered & wanted
        } else {
            SourceType::Monitor.into()
        };
        let select = SelectSourcesOptions::default()
            .set_sources(types)
            .set_multiple(false)
            .set_cursor_mode(cursor);
        proxy
            .select_sources(&session, select)
            .await
            .map_err(portal_error)?
            .response()
            .map_err(portal_error)?;
        let streams = proxy
            .start(&session, None, Default::default())
            .await
            .map_err(portal_error)?
            .response()
            .map_err(portal_error)?;
        let stream = streams
            .streams()
            .first()
            .ok_or_else(|| PortalEnd::Failed("the portal returned no stream".into()))?;
        let node = stream.pipe_wire_node_id();
        let size = stream.size().and_then(|(w, h)| {
            Some((u32::try_from(w).ok()?, u32::try_from(h).ok()?)).filter(|(w, h)| *w > 0 && *h > 0)
        });
        let remote = proxy
            .open_pipe_wire_remote(&session, Default::default())
            .await
            .map_err(portal_error)?;
        Ok((node, size, remote))
    };
    match future::or(picked, async { Err(stopped(stop).await) }).await {
        Ok((node, size, remote)) => Ok(Opened {
            session,
            remote,
            node,
            size,
        }),
        Err(end) => {
            // Closing the session also closes an open dialog.
            close(&session).await;
            Err(end)
        }
    }
}

async fn close(session: &Session<Screencast>) {
    let closing = async {
        let _ = session.close().await;
    };
    future::or(closing, async {
        async_io::Timer::after(CLOSE_TIMEOUT).await;
    })
    .await;
}

/// The stream's pictures, on PipeWire's thread.
struct PortalFrames {
    sink: FrameSink<VideoFrame>,
    preview: Arc<PreviewSlot>,
    options: ScreenOptions,
    pacer: FramePacer,
    tracker: PauseTracker,
    /// Whether the stream ever ran (a later stop is the cast ending).
    streamed: bool,
    stop: Arc<AtomicBool>,
    done: Arc<AtomicBool>,
}

impl PortalFrames {
    fn finish(&mut self, event: CaptureEvent) {
        if !self.stop.load(Ordering::Acquire) && !self.done.swap(true, Ordering::AcqRel) {
            self.sink.push_event(event);
        }
    }
}

impl StreamHandler for PortalFrames {
    fn frame(&mut self, picture: &RawPicture<'_>) {
        if self.stop.load(Ordering::Acquire) || self.done.load(Ordering::Acquire) {
            return;
        }
        let now = Instant::now();
        if !self.pacer.due(now) {
            return;
        }
        let Some(frame) = convert(picture, self.options, now) else {
            return;
        };
        self.preview.offer(&frame);
        self.sink.push(frame);
        if let Some(event) = self.tracker.picture() {
            self.sink.push_event(event);
        }
    }

    fn state(&mut self, state: StreamState) {
        match state {
            StreamState::Streaming => self.streamed = true,
            // A suspended node (the producer stopped sending).
            StreamState::Paused if self.streamed => {
                if let Some(event) = self.tracker.no_picture() {
                    self.sink.push_event(event);
                }
            }
            StreamState::Unconnected if self.streamed => self.finish(CaptureEvent::Ended),
            StreamState::Error(why) => {
                eprintln!("quill: screen stream: {why}");
                self.finish(CaptureEvent::Failed(failed_screen()));
            }
            _ => {}
        }
    }
}

/// A picture as the call's I420 frame, within the call's size.
pub(super) fn convert(
    picture: &RawPicture<'_>,
    options: ScreenOptions,
    at: Instant,
) -> Option<VideoFrame> {
    let (w, h) = fit_capture_size(
        u32::try_from(picture.width).ok()?,
        u32::try_from(picture.height).ok()?,
        options.max_width,
        options.max_height,
    );
    let i420 = raw_to_i420(picture, (w as usize, h as usize))?;
    Some(VideoFrame {
        i420,
        width: u16::try_from(w).ok()?,
        height: u16::try_from(h).ok()?,
        captured_at: at,
    })
}

/// A running portal share. Stopping sets the flag; the thread winds down
/// on its own.
struct PortalCapture {
    stop: Arc<AtomicBool>,
}

impl ScreenCapture for PortalCapture {
    fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

impl Drop for PortalCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calls::capture::spa_pod::VideoFormat;

    fn frames() -> (PortalFrames, FrameSink<VideoFrame>, Arc<AtomicBool>) {
        let sink = FrameSink::new(4);
        let stop = Arc::new(AtomicBool::new(false));
        let handler = PortalFrames {
            sink: sink.clone(),
            preview: Arc::default(),
            options: ScreenOptions::default(),
            pacer: FramePacer::new(15),
            tracker: paused_tracker(),
            streamed: false,
            stop: stop.clone(),
            done: Arc::new(AtomicBool::new(false)),
        };
        (handler, sink, stop)
    }

    fn picture(data: &[u8]) -> RawPicture<'_> {
        RawPicture::from_single_buffer(VideoFormat::Bgrx, 4, 2, data, 16).unwrap()
    }

    #[test]
    fn the_first_picture_resumes_and_frames_flow() {
        let (mut handler, sink, _) = frames();
        let data = [200u8; 32];
        handler.state(StreamState::Connecting);
        handler.state(StreamState::Paused);
        assert!(sink.take_events().is_empty(), "negotiating is not a pause");
        handler.state(StreamState::Streaming);
        handler.frame(&picture(&data));
        assert_eq!(sink.take_events(), vec![CaptureEvent::Resumed]);
        let frame = sink.pop_timeout(Duration::ZERO).expect("a frame");
        assert_eq!((frame.width, frame.height), (4, 2));
        assert_eq!(frame.i420.len(), 12);
        // Paced: a second picture at once is dropped.
        handler.frame(&picture(&data));
        assert!(sink.pop_timeout(Duration::ZERO).is_none());
    }

    #[test]
    fn a_suspended_stream_pauses_and_the_node_going_away_ends() {
        let (mut handler, sink, _) = frames();
        let data = [0u8; 32];
        handler.state(StreamState::Streaming);
        handler.frame(&picture(&data));
        let _ = sink.take_events();
        handler.state(StreamState::Paused);
        assert_eq!(sink.take_events(), vec![CaptureEvent::Paused]);
        handler.state(StreamState::Unconnected);
        handler.state(StreamState::Error("late".into()));
        assert_eq!(sink.take_events(), vec![CaptureEvent::Ended], "once");
        assert!(handler.done.load(Ordering::Acquire));
    }

    #[test]
    fn a_stream_error_fails_with_tdesktops_text_and_stop_silences() {
        let (mut handler, sink, stop) = frames();
        handler.state(StreamState::Error("no node".into()));
        let events = sink.take_events();
        assert!(matches!(
            events.as_slice(),
            [CaptureEvent::Failed(CaptureError::Device(_))]
        ));
        let (mut handler, sink, stop2) = frames();
        stop2.store(true, Ordering::Release);
        handler.state(StreamState::Streaming);
        handler.state(StreamState::Unconnected);
        handler.frame(&picture(&[0u8; 32]));
        assert!(sink.take_events().is_empty());
        assert!(sink.pop_timeout(Duration::ZERO).is_none());
        drop(stop);
    }

    #[test]
    fn large_pictures_fit_the_call_size() {
        let data = vec![0u8; 3840 * 2160 * 4];
        let picture =
            RawPicture::from_single_buffer(VideoFormat::Bgrx, 3840, 2160, &data, 3840 * 4).unwrap();
        let frame = convert(&picture, ScreenOptions::default(), Instant::now()).unwrap();
        assert_eq!((frame.width, frame.height), (1920, 1080));
        assert_eq!(frame.i420.len(), 1920 * 1080 * 3 / 2);
    }

    /// Needs a desktop session with xdg-desktop-portal and a ScreenCast
    /// backend (and someone to pick in its dialog, unless the backend
    /// picks by itself, as xdg-desktop-portal-wlr with `chooser_type=none`).
    #[test]
    #[ignore]
    fn shares_through_the_real_portal() {
        let sink = FrameSink::new(4);
        let mut capture =
            start(ScreenOptions::default(), sink.clone(), Arc::default()).expect("start");
        let frame = sink.pop_timeout(Duration::from_secs(30));
        let events = sink.take_events();
        capture.stop();
        eprintln!("events: {events:?}");
        let frame = frame.expect("a frame from the portal");
        assert!(frame.width > 0 && frame.height > 0);
        assert_eq!(events.first(), Some(&CaptureEvent::Paused));
        assert!(events.contains(&CaptureEvent::Resumed));
    }
}
