//! The pushed-frames backend: a channel's live stream, a live story and
//! each video chat track play through the same player as files
//! (`docs/decisions/video-player-live.md`).
//!
//! The engine pushes I420 planes into a [`LiveSource`]; the player holds
//! the [`LiveSink`]. A waker task arms the sink after each wake and, when
//! a frame lands, redraws the one view the element draws in, so nothing
//! polls and an idle feed costs nothing. A frame is converted once, into
//! the platform picture (`live_picture.rs`): on the UI thread for small
//! tiles, on the background executor above [`INLINE_MAX_EDGE`], one
//! conversion at a time with the newest frame winning.
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
//! An off-screen tile (`Purpose::Live { tile: true }` painted outside the
//! visible area for two wakes) stops redrawing on each frame, and looks
//! again once a second. A live player never hibernates.
#![deny(clippy::indexing_slicing)]

use super::live_picture::{self, Prepared, Surfaces, Target};
use super::transport::{BackendStatus, LiveStatus};
use super::{Backend, ErrorKind, Picture, PlayerError, Present, Pulled, Purpose, VideoPlayer};
use crate::ui::inline_video::INLINE_MAX_EDGE;
use gpui_kit::*;
use quill::calls::live_video::convert::paused_raster;
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
/// A tile woken this many times without being painted in view stops
/// redrawing on each frame...
const UNSEEN_WAKES: u32 = 2;
/// ...counting only wakes this long after the last redraw, so frames
/// that arrive before the window drew once do not count.
const UNSEEN_GRACE: Duration = Duration::from_millis(100);
/// ...and looks again this often, so a window back from behind another
/// (whose cached views replay without painting) catches up.
const UNSEEN_PROBE: Duration = Duration::from_secs(1);

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

/// Whether a frame converts on the background executor.
fn converts_in_background(frame: &LiveFrame) -> bool {
    u32::from(frame.width.max(frame.height)) > INLINE_MAX_EDGE
}

/// The live state shared by the backend, the player and the waker task.
pub(super) struct LiveCore {
    sink: LiveSink,
    /// A video chat tile: the off-screen policy applies.
    tile: bool,
    /// Every frame converts on the UI thread: a screenshot capture shows
    /// the first frame in its first draw.
    inline_always: bool,
    track: RefCell<Track>,
    /// The view the element draws in, redrawn when a frame lands.
    view: Cell<Option<EntityId>>,
    /// How many times a frame redrew the view.
    asks: Cell<u64>,
}

#[derive(Default)]
struct Track {
    gaps: ArrivalGaps,
    surfaces: Surfaces,
    /// A large frame converted off the UI thread, not drawn yet.
    ready: Option<Prepared>,
    /// BGRA images: the one shown and the one before it, which the last
    /// drawn frame may still use.
    shown_image: Option<Arc<RenderImage>>,
    previous_image: Option<Arc<RenderImage>>,
    /// Images to drop from every window's atlas (`App::drop_image`).
    retired: Vec<Arc<RenderImage>>,
    /// The paused picture, made from frame `seq`.
    paused: Option<(u64, Arc<RenderImage>)>,
    /// The off-screen policy: the shown count at the last wake, wakes
    /// since it last grew, and when the last look went out.
    seen: u64,
    unseen: u32,
    probed: Option<Instant>,
    notified: Option<Instant>,
}

impl LiveCore {
    fn new(sink: LiveSink, tile: bool) -> Self {
        Self {
            sink,
            tile,
            inline_always: capture_mode(),
            track: RefCell::new(Track::default()),
            view: Cell::new(None),
            asks: Cell::new(0),
        }
    }

    /// The feed's status at `now` and when to look again.
    fn status(&self, now: Instant) -> (LiveStatus, Option<Instant>) {
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
        let inline = self.inline_always;
        let frame = self
            .sink
            .take_if(|frame| inline || !converts_in_background(frame))?;
        let (width, height) = frame.shown_size();
        let target = track.surfaces.target(width, height);
        Some((frame, target))
    }

    /// The newest frame to convert off the UI thread, with its target.
    fn take_background(&self) -> Option<(Arc<LiveFrame>, Target)> {
        if self.inline_always {
            return None;
        }
        let mut track = self.track.try_borrow_mut().ok()?;
        let frame = self.sink.take_if(converts_in_background)?;
        let (width, height) = frame.shown_size();
        let target = track.surfaces.target(width, height);
        Some((frame, target))
    }

    /// A frame converted off the UI thread, drawn on the next pull (an
    /// older one not drawn yet is replaced: its buffer goes back to the
    /// pool).
    fn landed(&self, prepared: Option<Prepared>) {
        if let (Some(prepared), Ok(mut track)) = (prepared, self.track.try_borrow_mut()) {
            track.ready = Some(prepared);
        }
    }

    fn take_ready(&self) -> Option<Prepared> {
        self.track.try_borrow_mut().ok()?.ready.take()
    }

    /// Make `prepared` the shown picture; the image from two pictures
    /// back is retired.
    fn show(&self, prepared: Prepared) -> Option<Picture> {
        let (picture, image) = prepared.into_picture()?;
        if let Ok(mut track) = self.track.try_borrow_mut() {
            let track = &mut *track;
            let older = match image {
                Some(image) => {
                    let shown = track.shown_image.replace(image);
                    std::mem::replace(&mut track.previous_image, shown)
                }
                // A surface after images (a pool that failed earlier):
                // the images go.
                None => track.shown_image.take().or(track.previous_image.take()),
            };
            track.retired.extend(older);
        }
        Some(picture)
    }

    fn take_retired(&self) -> Vec<Arc<RenderImage>> {
        self.track
            .try_borrow_mut()
            .map(|mut track| std::mem::take(&mut track.retired))
            .unwrap_or_default()
    }

    /// The view to redraw for a wake, after the off-screen policy:
    /// `shown` is the player's count of paints in view.
    fn wake_view(&self, shown: u64, now: Instant) -> Option<EntityId> {
        let view = self.view.get()?;
        if self.tile {
            let mut track = self.track.try_borrow_mut().ok()?;
            if shown != track.seen {
                track.seen = shown;
                track.unseen = 0;
                track.probed = None;
            } else if track
                .notified
                .is_none_or(|at| now.saturating_duration_since(at) >= UNSEEN_GRACE)
            {
                track.unseen = track.unseen.saturating_add(1);
            }
            if track.unseen >= UNSEEN_WAKES {
                let due = track
                    .probed
                    .is_some_and(|at| now.saturating_duration_since(at) >= UNSEEN_PROBE);
                if track.probed.is_some() && !due {
                    return None;
                }
                let first = track.probed.is_none();
                track.probed = Some(now);
                if first {
                    return None;
                }
            }
            track.notified = Some(now);
        }
        self.asks.set(self.asks.get().wrapping_add(1));
        Some(view)
    }

    /// The newest frame shrunk to 90 px and blurred (tdesktop's paused
    /// tile), made again only for a new frame.
    fn paused_picture(&self) -> Option<Arc<RenderImage>> {
        let frame = self.sink.latest()?;
        let mut track = self.track.try_borrow_mut().ok()?;
        if let Some((seq, image)) = track.paused.as_ref()
            && *seq == frame.seq
        {
            return Some(image.clone());
        }
        let raster = paused_raster(&frame.i420()?, frame.rotation)?;
        let image = live_picture::render_image(raster.bgra, raster.width, raster.height)?;
        if let Some((_, old)) = track.paused.replace((frame.seq, image.clone())) {
            track.retired.push(old);
        }
        Some(image)
    }
}

impl Drop for LiveCore {
    fn drop(&mut self) {
        let track = self.track.get_mut();
        let images = std::mem::take(&mut track.retired)
            .into_iter()
            .chain(track.shown_image.take())
            .chain(track.previous_image.take())
            .chain(track.paused.take().map(|(_, image)| image));
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
    fn play(&mut self, _rate: f64) {}
    fn pause(&mut self) {}
    fn seek(&mut self, _secs: f64) {}
    fn set_volume(&mut self, _volume: f32) {}
    fn set_rate(&mut self, _rate: f64, _playing: bool) {}
    fn step_frame(&mut self, _forward: bool) {}

    fn take_picture(&mut self, _present: &Present) -> Pulled {
        let prepared = self.core.take_ready().or_else(|| {
            let (frame, target) = self.core.take_inline()?;
            live_picture::convert(&frame, target)
        });
        Pulled {
            picture: prepared.and_then(|prepared| self.core.show(prepared)),
            item_secs: 0.0,
            display_secs: None,
        }
    }

    fn next_due(&self, now: Instant, _interval: Duration) -> Option<Instant> {
        self.core.status(now).1
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

    /// The newest frame shrunk to 90 px and blurred, as tdesktop draws a
    /// paused track (and the live story's backdrop), made from the frame
    /// the feed keeps: nothing is read back from the GPU.
    pub(crate) fn paused_picture(&self) -> Option<Arc<RenderImage>> {
        self.0.live.as_ref()?.paused_picture()
    }

    /// How many times a frame redrew the view (the off-screen policy's
    /// count, for tests).
    #[cfg(all(test, feature = "demo-capture"))]
    pub(crate) fn live_asks(&self) -> u64 {
        self.0.live.as_ref().map_or(0, |core| core.asks.get())
    }

    /// The element draws the picture in `view`.
    pub(super) fn note_live_view(&self, view: EntityId) {
        if let Some(core) = self.0.live.as_ref() {
            core.view.set(Some(view));
        }
    }

    pub(super) fn is_live(&self) -> bool {
        self.0.live.is_some()
    }
}

/// Arm the sink, wait for a frame (or a state change, or the end),
/// convert a large frame off the UI thread, then redraw the view and drop
/// retired images from every window. Ends with the feed or the player.
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
            let job = match core.upgrade() {
                Some(core) => core.take_background(),
                None => return,
            };
            if let Some((frame, target)) = job {
                let prepared = cx
                    .background_spawn(async move { live_picture::convert(&frame, target) })
                    .await;
                match core.upgrade() {
                    Some(core) => core.landed(prepared),
                    None => return,
                }
            }
            let (Some(core), Some(player)) = (core.upgrade(), player.upgrade()) else {
                return;
            };
            let shown = VideoPlayer(player).shown_count();
            let view = core.wake_view(shown, Instant::now());
            let retired = core.take_retired();
            let ended = core.sink.snapshot().end.is_some();
            drop(core);
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

#[cfg(test)]
#[path = "live_tests.rs"]
mod tests;
