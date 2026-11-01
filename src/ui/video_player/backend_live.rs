//! The pushed-frames backend: a channel's live stream, a live story and
//! each video chat track play through the same player as files
//! (`docs/decisions/video-player-live.md`).
//!
//! The engine pushes I420 planes into a [`LiveSource`]; the player holds
//! the [`LiveSink`]. A waker task arms the sink after each wake and, when
//! a frame lands, redraws the one view the element draws in, so nothing
//! polls and an idle feed costs nothing. A frame is converted once, into
//! the platform picture (`live_picture.rs`): on the UI thread for small
//! tiles (NV12, a few microseconds) and on the background executor above
//! [`INLINE_MAX_EDGE`]; a frame that becomes BGRA (about 0.5 ms for a
//! 640 × 360 tile: the pause after a refused surface, `live_surface.rs`)
//! always on the background executor. One conversion at a time; the newest frame wins.
//!
//! The transport is live ([`TransportMachine::live`]): Opening until the
//! first frame, Playing while frames come, Buffering once none came for
//! the stall time while the sender is active (the picture stays), Paused
//! while the sender paused, Ended when the feed ends, Failed on an engine
//! error. There is no position, duration or rate; play, pause, seek and
//! frame steps do nothing.
//!
//! The element asks the frame clock only to notice a stall: `next_due` is
//! when the feed would turn Buffering (the last arrival plus the stall
//! time), then every 250 ms while it is. Pictures themselves come with
//! the waker, on the first display frame after they arrive.
//!
//! The off-screen policy runs before a frame is converted: a picture
//! painted out of sight, or not painted at all for 500 ms (a minimized or
//! hidden window), for two wakes stops redrawing and converting on each
//! frame, and looks again once a second (a tile) or four times a second
//! (a stream or a story). The frame stays in the feed for that look or
//! for the next draw. A live player never hibernates.
//!
//! A picture of an older frame never replaces a newer one: a large frame
//! still converting in the background when a smaller, newer one was
//! drawn (a quality switch) is dropped when it lands.
#![deny(clippy::indexing_slicing)]

use super::live_picture::{self, Prepared, Surfaces, Target};
use super::transport::{BackendStatus, LiveStatus, Want};
use super::{Backend, ErrorKind, Picture, PlayerError, Present, Pulled, Purpose, VideoPlayer};
use crate::ui::inline_video::INLINE_MAX_EDGE;
use gpui_kit::*;
use quill::calls::live_video::convert::{mirror_raster, paused_raster};
use quill::calls::live_video::{LiveEnd, LiveFrame, LiveSink, LiveSnapshot, LiveState};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// No frame for this long while the sender is active: Buffering.
pub(crate) const STALL_AFTER: Duration = Duration::from_millis(1500);
/// While Buffering, the element looks again this often.
const BUFFERING_RECHECK: Duration = Duration::from_millis(250);
/// Arrival gaps kept for the median (as `PresentEstimator` keeps draws).
const GAPS: usize = 8;
/// The stall time is at least this many median gaps: a screen share of a
/// still window sends a frame a second or less.
const STALL_GAPS: u32 = 4;
/// A picture woken this many times without being painted in view stops
/// redrawing on each frame...
const UNSEEN_WAKES: u32 = 2;
/// ...a wake counting when the element painted out of sight since the
/// last one, or did not paint at all for this long after a redraw (its
/// window is not drawing); a slow window that draws a few times a second
/// keeps its pictures...
const UNSEEN_STALE: Duration = Duration::from_millis(500);
/// ...and a tile looks again this often, so a window back from behind
/// another (whose cached views replay without painting) catches up...
const UNSEEN_PROBE: Duration = Duration::from_secs(1);
/// ...a stream or a story sooner: it is the window's whole content.
const UNSEEN_PROBE_PICTURE: Duration = Duration::from_millis(250);
/// Pixel buffers kept after they were shown, so the pool does not refill
/// one a frame still in flight may read: GPUI's Metal renderer drops the
/// plane textures once encoded (`metal_renderer.rs`, `draw_surfaces`) and
/// keeps up to three drawables (`set_maximum_drawable_count(3)`). A
/// Windows or Linux NV12 frame is never written again once made (its pool
/// buffer is reused only when no frame holds it), so the one on screen is
/// enough.
const SURFACES_HELD: usize = if cfg!(target_os = "macos") { 3 } else { 1 };
/// The paused picture (and the live story's backdrop) is made again at
/// most this often.
const PAUSED_REFRESH: Duration = Duration::from_millis(500);

/// The last arrivals of a feed's frames.
#[derive(Clone, Debug, Default)]
pub(super) struct ArrivalGaps {
    last: Option<(u64, Instant)>,
    gaps: VecDeque<Duration>,
}

impl ArrivalGaps {
    /// Frame `seq` arrived `at` (the same frame again changes nothing).
    pub(super) fn note(&mut self, seq: u64, at: Instant) {
        if let Some((last_seq, last_at)) = self.last {
            if last_seq == seq {
                return;
            }
            if let Some(gap) = at.checked_duration_since(last_at) {
                if self.gaps.len() == GAPS {
                    self.gaps.pop_front();
                }
                self.gaps.push_back(gap);
            }
        }
        self.last = Some((seq, at));
    }

    /// The median of the last gaps, once there is one.
    pub(super) fn median(&self) -> Option<Duration> {
        let mut gaps: Vec<Duration> = self.gaps.iter().copied().collect();
        gaps.sort_unstable();
        gaps.get(gaps.len() / 2).copied()
    }

    /// How long without a frame counts as a stall.
    pub(super) fn stall_after(&self) -> Duration {
        self.median().map_or(STALL_AFTER, |median| {
            STALL_AFTER.max(median.saturating_mul(STALL_GAPS))
        })
    }
}

/// The feed's status at `now`, from what the sink reported.
pub(super) fn live_status(snapshot: &LiveSnapshot, gaps: &ArrivalGaps, now: Instant) -> LiveStatus {
    let active = snapshot
        .state
        .is_none_or(|state| state == LiveState::Active);
    LiveStatus {
        has_frame: snapshot.latest.is_some(),
        stalled: active
            && snapshot
                .latest
                .is_some_and(|(_, at)| now.saturating_duration_since(at) >= gaps.stall_after()),
        sender: snapshot.state,
        ended: snapshot
            .end
            .as_ref()
            .map(|end| matches!(end, LiveEnd::Failed(_))),
    }
}

/// When the element looks again: when the feed would turn Buffering,
/// every 250 ms while it is, never while it waits for its first frame
/// (the waker brings it), is paused, has no video or has ended.
pub(super) fn stall_check_at(
    status: &LiveStatus,
    latest_at: Option<Instant>,
    gaps: &ArrivalGaps,
    now: Instant,
) -> Option<Instant> {
    let waiting = matches!(status.sender, Some(LiveState::Paused | LiveState::Inactive));
    if status.ended.is_some() || waiting {
        return None;
    }
    if status.stalled {
        return now.checked_add(BUFFERING_RECHECK);
    }
    latest_at?.checked_add(gaps.stall_after())
}

/// A frame too large to convert during the draw, whatever it becomes.
fn large(frame: &LiveFrame) -> bool {
    u32::from(frame.width.max(frame.height)) > INLINE_MAX_EDGE
}

/// A converted frame, its sequence number and what converting it took
/// (the `QUILL_TRACE_VIDEO` `VF` line).
pub(super) struct Landed {
    pub(super) seq: u64,
    pub(super) prepared: Prepared,
    pub(super) convert_us: u32,
}

impl Landed {
    fn stats(&self) -> super::PictureStats {
        let (format, width, height, bytes) = self.prepared.trace();
        super::PictureStats {
            format,
            width,
            height,
            bytes,
            convert_us: self.convert_us,
        }
    }
}

/// Microseconds since `since`, for the trace.
fn elapsed_us(since: Instant) -> u32 {
    u32::try_from(since.elapsed().as_micros()).unwrap_or(u32::MAX)
}

/// The live state shared by the backend, the player and the waker task.
pub(super) struct LiveCore {
    sink: LiveSink,
    /// A video chat tile: looks again once a second while unseen.
    tile: bool,
    /// Every frame converts on the UI thread: a screenshot capture shows
    /// the first frame in its first draw.
    inline_always: bool,
    /// Shown mirrored left to right (your camera preview), in the plane
    /// copy each frame gets anyway.
    mirrored: Cell<bool>,
    /// Every picture is an image, never a pixel buffer: a small picture
    /// over another one (your camera's corner preview) rounds its own
    /// corners, which a surface can't do over a picture.
    images_only: Cell<bool>,
    /// Pictures are NV12 surface frames (Windows, Linux: `live_surface.rs`).
    #[cfg(any(not(target_os = "macos"), test))]
    nv12: Cell<bool>,
    track: RefCell<Track>,
    /// The view the element draws in, redrawn when a frame lands. A live
    /// player is drawn in one view: a tile or the stage only in the call
    /// window's panel, the stream picture only in its own view, the live
    /// story only in the story viewer. A later view (the window opened
    /// again) takes over from the old one, which is gone.
    view: Cell<Option<EntityId>>,
    /// Paints of the element, in sight or not.
    painted: Cell<u64>,
    /// How many times a frame redrew the view.
    asks: Cell<u64>,
    /// How many frames were converted.
    conversions: Cell<u64>,
    /// Tests: the time the core reads instead of the clock.
    #[cfg(test)]
    clock: Cell<Option<Instant>>,
}

#[derive(Default)]
struct Track {
    gaps: ArrivalGaps,
    surfaces: Surfaces,
    /// NV12 buffers, the pause after a refused frame and the NV12 frame
    /// on screen (Windows).
    #[cfg(any(not(target_os = "macos"), test))]
    nv12: super::live_surface::LiveNv12,
    /// A large frame converted off the UI thread, not drawn yet.
    ready: Option<Landed>,
    /// The newest frame shown; pictures of older ones are dropped.
    shown_seq: Option<u64>,
    /// BGRA images: the one shown and the one before it, which the last
    /// drawn frame may still use.
    shown_image: Option<Arc<RenderImage>>,
    previous_image: Option<Arc<RenderImage>>,
    /// Pixel buffers: the last [`SURFACES_HELD`] shown.
    held_surfaces: VecDeque<Picture>,
    /// Images to drop from every window's atlas (`App::drop_image`).
    retired: Vec<Arc<RenderImage>>,
    /// The paused picture, made from frame `seq` at a time, and the one
    /// before it (kept as the shown images are).
    paused: Option<(u64, Instant, Arc<RenderImage>)>,
    paused_previous: Option<Arc<RenderImage>>,
    /// The off-screen policy: the in-sight and all paint counts at the
    /// last wake, wakes counted unseen since, when the last look went
    /// out, since when no paint came, and whether the last wake was let
    /// go (a redraw for another reason then converts nothing either).
    seen: u64,
    painted: u64,
    unseen: u32,
    probed: Option<Instant>,
    unpainted_since: Option<Instant>,
    quiet: bool,
}

impl Track {
    /// `image` is shown now; the one before it stays (the last drawn
    /// frame may still use it) and the one before that is retired.
    fn keep_image(&mut self, image: Arc<RenderImage>) {
        let shown = self.shown_image.replace(image);
        let older = std::mem::replace(&mut self.previous_image, shown);
        self.retired.extend(older);
    }
}

impl LiveCore {
    fn new(sink: LiveSink, tile: bool) -> Self {
        Self {
            sink,
            tile,
            inline_always: capture_mode(),
            mirrored: Cell::new(false),
            images_only: Cell::new(false),
            #[cfg(any(not(target_os = "macos"), test))]
            nv12: Cell::new(super::live_surface::nv12_live()),
            track: RefCell::new(Track::default()),
            view: Cell::new(None),
            painted: Cell::new(0),
            asks: Cell::new(0),
            conversions: Cell::new(0),
            #[cfg(test)]
            clock: Cell::new(None),
        }
    }

    /// The time: `now`, or in tests the injected clock.
    fn time(&self, now: Instant) -> Instant {
        #[cfg(test)]
        if let Some(clock) = self.clock.get() {
            return clock;
        }
        now
    }

    /// The feed's status at `now` and when to look again.
    fn status(&self, now: Instant) -> (LiveStatus, Option<Instant>) {
        let now = self.time(now);
        let snapshot = self.sink.snapshot();
        let latest_at = snapshot.latest.map(|(_, at)| at);
        let decide = |gaps: &ArrivalGaps| {
            let status = live_status(&snapshot, gaps, now);
            let check = stall_check_at(&status, latest_at, gaps, now);
            (status, check)
        };
        match self.track.try_borrow_mut() {
            Ok(mut track) => {
                if let Some((seq, at)) = snapshot.latest {
                    track.gaps.note(seq, at);
                }
                decide(&track.gaps)
            }
            // Busy (never expected): the plain stall time.
            Err(_) => decide(&ArrivalGaps::default()),
        }
    }

    /// The newest frame for the UI thread to convert now, with its
    /// target; large frames are left to the waker task.
    fn take_inline(&self) -> Option<(Arc<LiveFrame>, Target)> {
        let mut track = self.track.try_borrow_mut().ok()?;
        if track.quiet {
            return None;
        }
        let inline = self.inline_always;
        let frame = self
            .sink
            .take_if(|frame| inline || !(self.makes_image(&track, frame) || large(frame)))?;
        let target = self.target(&mut track, &frame);
        self.conversions.set(self.conversions.get().wrapping_add(1));
        Some((frame, target))
    }

    /// The newest frame to convert off the UI thread, with its target.
    fn take_background(&self) -> Option<(Arc<LiveFrame>, Target)> {
        if self.inline_always {
            return None;
        }
        let mut track = self.track.try_borrow_mut().ok()?;
        let frame = self
            .sink
            .take_if(|frame| self.makes_image(&track, frame) || large(frame))?;
        let target = self.target(&mut track, &frame);
        self.conversions.set(self.conversions.get().wrapping_add(1));
        Some((frame, target))
    }

    /// Whether `frame` becomes a BGRA image (converted off the UI thread
    /// whatever its size): images only, the BGRA platforms, and on
    /// Windows an odd size or the pause after a refused surface. A macOS
    /// pool that failed converts its BGRA during the draw, as before.
    fn makes_image(&self, track: &Track, frame: &LiveFrame) -> bool {
        if self.images_only.get() {
            return true;
        }
        // As `target` decides: NV12 first (off unless the platform draws
        // it, `live_surface::nv12_live`).
        #[cfg(any(not(target_os = "macos"), test))]
        if self.nv12.get() {
            let (width, height) = frame.shown_size();
            return !track.nv12.takes(width, height, self.time(Instant::now()));
        }
        #[cfg(all(target_os = "macos", not(test)))]
        let _ = (track, frame);
        live_picture::bgra_only()
    }

    /// What `frame` converts into.
    fn target(&self, track: &mut Track, frame: &LiveFrame) -> Target {
        if self.images_only.get() {
            return Target::Bgra;
        }
        let (width, height) = frame.shown_size();
        #[cfg(any(not(target_os = "macos"), test))]
        if self.nv12.get() {
            let now = self.time(Instant::now());
            return track
                .nv12
                .buffer(width, height, now)
                .map_or(Target::Bgra, Target::Nv12);
        }
        track.surfaces.target(width, height)
    }

    /// A frame converted off the UI thread, drawn on the next pull. One
    /// older than the shown frame or the one already waiting is dropped
    /// (a pixel buffer goes back to the pool).
    fn landed(&self, landed: Option<Landed>) {
        let (Some(landed), Ok(mut track)) = (landed, self.track.try_borrow_mut()) else {
            return;
        };
        let stale = track.shown_seq.is_some_and(|shown| landed.seq <= shown)
            || track
                .ready
                .as_ref()
                .is_some_and(|ready| landed.seq <= ready.seq);
        if !stale {
            track.ready = Some(landed);
        }
    }

    fn take_ready(&self) -> Option<Landed> {
        self.track.try_borrow_mut().ok()?.ready.take()
    }

    /// Make `landed` the shown picture, unless a newer frame is shown
    /// already; the image from two pictures back is retired.
    fn show(&self, landed: Landed) -> Option<Picture> {
        let mut track = self.track.try_borrow_mut().ok()?;
        let track = &mut *track;
        if track.shown_seq.is_some_and(|shown| landed.seq <= shown) {
            return None;
        }
        let shown = landed.prepared.into_picture()?;
        track.shown_seq = Some(landed.seq);
        #[cfg(any(not(target_os = "macos"), test))]
        track.nv12.shown(shown.nv12);
        let picture = shown.picture;
        match shown.image {
            Some(image) => track.keep_image(image),
            // A surface after images (a pool that failed earlier, or the
            // pause after a refused NV12 frame ended): the images go.
            None => {
                track.held_surfaces.push_back(picture.clone());
                while track.held_surfaces.len() > SURFACES_HELD {
                    track.held_surfaces.pop_front();
                }
                let images = track.shown_image.take().into_iter();
                let images = images.chain(track.previous_image.take());
                track.retired.extend(images);
            }
        }
        Some(picture)
    }

    /// The feed ended (no frame converts any more) or the player goes
    /// (`gone`): its NV12 buffers go.
    fn release_buffers(&self, gone: bool) {
        #[cfg(any(not(target_os = "macos"), test))]
        if let Ok(mut track) = self.track.try_borrow_mut() {
            if gone {
                track.nv12.release();
            } else {
                track.nv12.feed_ended();
            }
        }
        #[cfg(all(target_os = "macos", not(test)))]
        let _ = gone;
    }

    fn take_retired(&self) -> Vec<Arc<RenderImage>> {
        self.track
            .try_borrow_mut()
            .map(|mut track| std::mem::take(&mut track.retired))
            .unwrap_or_default()
    }

    /// Every image the player holds, for its owner to drop from the
    /// windows' atlases as the player goes.
    fn take_all_images(&self) -> Vec<Arc<RenderImage>> {
        let Ok(mut track) = self.track.try_borrow_mut() else {
            return Vec::new();
        };
        let track = &mut *track;
        std::mem::take(&mut track.retired)
            .into_iter()
            .chain(track.shown_image.take())
            .chain(track.previous_image.take())
            .chain(track.paused.take().map(|(_, _, image)| image))
            .chain(track.paused_previous.take())
            .collect()
    }

    /// The view to redraw for a wake, after the off-screen policy:
    /// `shown` is the player's count of paints in view. `None`: nothing
    /// to redraw, and nothing to convert.
    fn wake_view(&self, shown: u64, now: Instant) -> Option<EntityId> {
        let now = self.time(now);
        let view = self.view.get()?;
        let painted = self.painted.get();
        let mut track = self.track.try_borrow_mut().ok()?;
        if shown != track.seen {
            // Painted in sight since the last wake.
            track.seen = shown;
            track.painted = painted;
            track.unseen = 0;
            track.probed = None;
            track.unpainted_since = None;
        } else if painted != track.painted {
            // Painted, but out of sight.
            track.painted = painted;
            track.unseen = track.unseen.saturating_add(1);
            track.unpainted_since = None;
        } else {
            // Not painted since the last wake: a slow window that has not
            // drawn yet, or, past 500 ms, one that is not drawing
            // (minimized, or behind another).
            let since = *track.unpainted_since.get_or_insert(now);
            if now.saturating_duration_since(since) >= UNSEEN_STALE {
                track.unseen = track.unseen.saturating_add(1);
            }
        }
        if track.unseen >= UNSEEN_WAKES {
            let probe = if self.tile {
                UNSEEN_PROBE
            } else {
                UNSEEN_PROBE_PICTURE
            };
            let quiet = match track.probed {
                None => true,
                Some(at) => now.saturating_duration_since(at) < probe,
            };
            if quiet {
                track.probed.get_or_insert(now);
                track.quiet = true;
                return None;
            }
            track.probed = Some(now);
        }
        track.quiet = false;
        self.asks.set(self.asks.get().wrapping_add(1));
        Some(view)
    }

    /// The newest frame shrunk to 90 px and blurred (tdesktop's paused
    /// tile), made again for a new frame at most every 500 ms.
    fn paused_picture(&self) -> Option<Arc<RenderImage>> {
        let frame = self.sink.latest()?;
        let now = self.time(Instant::now());
        let mut track = self.track.try_borrow_mut().ok()?;
        if let Some((seq, made, image)) = track.paused.as_ref()
            && (*seq == frame.seq || now.saturating_duration_since(*made) < PAUSED_REFRESH)
        {
            return Some(image.clone());
        }
        let mut raster = paused_raster(&frame.i420()?, frame.rotation)?;
        if self.mirrored.get() {
            mirror_raster(&mut raster);
        }
        let image = live_picture::render_image(raster.bgra, raster.width, raster.height)?;
        if let Some((_, _, old)) = track.paused.replace((frame.seq, now, image.clone())) {
            let older = track.paused_previous.replace(old);
            track.retired.extend(older);
        }
        Some(image)
    }
}

impl Drop for LiveCore {
    /// What the owner did not release ([`VideoPlayer::release_live`])
    /// leaves the atlas at the next sweep.
    fn drop(&mut self) {
        let track = self.track.get_mut();
        let images = std::mem::take(&mut track.retired)
            .into_iter()
            .chain(track.shown_image.take())
            .chain(track.previous_image.take())
            .chain(track.paused.take().map(|(_, _, image)| image))
            .chain(track.paused_previous.take());
        super::super::image_budget::retire_all(images);
    }
}

/// Screenshot captures convert every frame at once.
fn capture_mode() -> bool {
    #[cfg(feature = "demo-capture")]
    if std::env::var_os("QUILL_DEMO_CAPTURE").is_some() {
        return true;
    }
    false
}

struct LiveBackend {
    core: Rc<LiveCore>,
}

impl Backend for LiveBackend {
    fn status(&mut self) -> BackendStatus {
        BackendStatus::default()
    }

    fn live_status(&mut self, now: Instant) -> Option<LiveStatus> {
        Some(self.core.status(now).0)
    }

    // A live feed has no transport to drive.
    fn sync(&mut self, _want: Want) {}
    fn seek(&mut self, _secs: f64) {}
    fn set_volume(&mut self, _volume: f32) {}
    fn step_frame(&mut self, _forward: bool) {}

    fn take_picture(&mut self, _present: &Present) -> Pulled {
        let ready = self.core.take_ready();
        let mirrored = self.core.mirrored.get();
        let inline = self.core.take_inline().and_then(|(frame, target)| {
            let started = Instant::now();
            live_picture::convert(&frame, target, mirrored).map(|prepared| Landed {
                seq: frame.seq,
                prepared,
                convert_us: elapsed_us(started),
            })
        });
        // The newer of the two; the other's buffer goes back to the pool.
        let newest = match (ready, inline) {
            (Some(ready), Some(inline)) if ready.seq > inline.seq => Some(ready),
            (ready, inline) => inline.or(ready),
        };
        let stats = newest.as_ref().map(Landed::stats);
        let picture = newest.and_then(|landed| self.core.show(landed));
        Pulled {
            stats: stats.filter(|_| picture.is_some()),
            picture,
            item_secs: 0.0,
            display_secs: None,
        }
    }

    fn next_due(&self, now: Instant, _interval: Duration) -> Option<Instant> {
        // The renderer has not answered for the NV12 frame on screen yet:
        // the next display frame looks (and replaces it if refused).
        #[cfg(any(not(target_os = "macos"), test))]
        if self.core.nv12_unsettled() {
            return None;
        }
        self.core.status(now).1
    }

    #[cfg(any(not(target_os = "macos"), test))]
    fn replace_refused(&mut self, now: Instant) -> Option<Picture> {
        self.core.replace_refused(now)
    }

    fn error(&self) -> Option<PlayerError> {
        match self.core.sink.snapshot().end {
            Some(LiveEnd::Failed(detail)) => Some(PlayerError::new(ErrorKind::Decode, detail)),
            _ => None,
        }
    }
}

impl VideoPlayer {
    /// A player over a live feed: Opening until the first frame, then the
    /// newest frame on each draw. Wakes the view it is drawn in when a
    /// frame lands (`Purpose::Live { tile }`; any other purpose is taken
    /// as a full-size picture).
    pub(crate) fn live(sink: LiveSink, purpose: Purpose, cx: &mut App) -> Self {
        let tile = matches!(purpose, Purpose::Live { tile: true });
        let core = Rc::new(LiveCore::new(sink.clone(), tile));
        let backend = Box::new(LiveBackend { core: core.clone() });
        let player = Self::from_live(backend, core.clone());
        let task = spawn_waker(Rc::downgrade(&core), Rc::downgrade(&player.0), sink, cx);
        player.with_inner(|inner| inner._poll = Some(task));
        player
    }

    /// The live feed this player shows, if it is a live player.
    pub(crate) fn live_sink(&self) -> Option<LiveSink> {
        self.0.live.as_ref().map(|core| core.sink.clone())
    }

    /// A frame has arrived (drawn or not): the tile has a picture to show.
    pub(crate) fn has_live_frame(&self) -> bool {
        self.0
            .live
            .as_ref()
            .is_some_and(|core| core.sink.latest().is_some())
    }

    /// Show the feed mirrored left to right (your camera preview, as
    /// tdesktop's `setMirrored`), from the next frame on. The mirror is
    /// made in the plane copy, not by turning the drawn picture.
    pub(crate) fn set_live_mirrored(&self, mirrored: bool) {
        if let Some(core) = self.0.live.as_ref() {
            core.mirrored.set(mirrored);
        }
    }

    /// Draw every picture as an image (see `LiveCore::images_only`), from
    /// the next frame on.
    pub(crate) fn set_live_images_only(&self, images_only: bool) {
        if let Some(core) = self.0.live.as_ref() {
            core.images_only.set(images_only);
        }
    }

    /// Whether the feed is shown mirrored (tests).
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(crate) fn live_mirrored(&self) -> bool {
        self.0.live.as_ref().is_some_and(|core| core.mirrored.get())
    }

    /// The newest frame shrunk to 90 px and blurred, as tdesktop draws a
    /// paused track (and the live story's backdrop), made from the frame
    /// the feed keeps: nothing is read back from the GPU.
    pub(crate) fn paused_picture(&self) -> Option<Arc<RenderImage>> {
        self.0.live.as_ref()?.paused_picture()
    }

    /// How many times a frame redrew the view (the off-screen policy's
    /// count, for tests).
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(crate) fn live_asks(&self) -> u64 {
        self.0.live.as_ref().map_or(0, |core| core.asks.get())
    }

    /// The element draws the picture in `view`.
    pub(super) fn note_live_view(&self, view: EntityId) {
        if let Some(core) = self.0.live.as_ref() {
            core.view.set(Some(view));
        }
    }

    /// The element painted the picture, in sight or not. Back in sight
    /// while the feed was let go, the waker runs once (as for a frame) to
    /// convert and draw the frame waiting in the feed.
    pub(super) fn note_live_paint(&self, visible: bool) {
        let Some(core) = self.0.live.as_ref() else {
            return;
        };
        core.painted.set(core.painted.get().wrapping_add(1));
        let back = visible
            && core.track.try_borrow_mut().is_ok_and(|mut track| {
                let quiet = std::mem::take(&mut track.quiet);
                if quiet {
                    track.unseen = 0;
                    track.probed = None;
                    track.unpainted_since = None;
                }
                quiet
            });
        if back {
            core.sink.poke();
        }
    }

    /// The player is going: drop its images from every window's atlas
    /// now, rather than at the main window's next sweep, and its NV12
    /// buffers (Windows; the renderer frees a surface's textures itself
    /// once no frame draws it).
    pub(crate) fn release_live(&self, cx: &mut App) {
        let Some(core) = self.0.live.as_ref() else {
            return;
        };
        for image in core.take_all_images() {
            cx.drop_image(image, None);
        }
        core.release_buffers(true);
    }

    /// How many frames this player converted (tests).
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(crate) fn live_conversions(&self) -> u64 {
        self.0
            .live
            .as_ref()
            .map_or(0, |core| core.conversions.get())
    }

    /// Tests: read `at` as the time from now on.
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(crate) fn set_live_clock(&self, at: Instant) {
        if let Some(core) = self.0.live.as_ref() {
            core.clock.set(Some(at));
        }
    }

    pub(super) fn is_live(&self) -> bool {
        self.0.live.is_some()
    }
}

/// Arm the sink and wait for a frame (or a state change, or the end).
/// Then the off-screen policy decides first: a picture nobody sees is
/// neither converted nor redrawn, and its frame waits in the feed. A
/// large frame is converted off the UI thread, then the view redrawn and
/// retired images dropped from every window. Ends with the feed or the
/// player.
fn spawn_waker(
    core: Weak<LiveCore>,
    player: Weak<super::PlayerCell>,
    sink: LiveSink,
    cx: &mut App,
) -> Task<()> {
    cx.spawn(async move |cx| {
        loop {
            let (tx, rx) = futures_channel::oneshot::channel::<()>();
            sink.arm(Box::new(move || {
                let _ = tx.send(());
            }));
            if rx.await.is_err() {
                return;
            }
            let (view, job) = {
                let (Some(live), Some(player)) = (core.upgrade(), player.upgrade()) else {
                    return;
                };
                let shown = VideoPlayer(player).shown_count();
                let view = live.wake_view(shown, Instant::now());
                let job = view.and_then(|_| live.take_background());
                (view, job.map(|job| (job, live.mirrored.get())))
            };
            if let Some(((frame, target), mirrored)) = job {
                let seq = frame.seq;
                let (prepared, convert_us) = cx
                    .background_spawn(async move {
                        let started = Instant::now();
                        let prepared = live_picture::convert(&frame, target, mirrored);
                        (prepared, elapsed_us(started))
                    })
                    .await;
                match core.upgrade() {
                    Some(live) => live.landed(prepared.map(|prepared| Landed {
                        seq,
                        prepared,
                        convert_us,
                    })),
                    None => return,
                }
            }
            let Some(live) = core.upgrade() else {
                return;
            };
            let retired = live.take_retired();
            let ended = live.sink.snapshot().end.is_some();
            if ended {
                live.release_buffers(false);
            }
            drop(live);
            cx.update(|cx| {
                for image in retired {
                    cx.drop_image(image, None);
                }
                if let Some(view) = view {
                    cx.notify(view);
                }
            });
            if ended {
                return;
            }
        }
    })
}

#[cfg(any(not(target_os = "macos"), test))]
#[path = "live_nv12.rs"]
mod nv12;

#[cfg(test)]
#[path = "live_tests.rs"]
mod tests;
