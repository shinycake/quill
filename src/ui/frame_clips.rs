//! A clip's requests for window frames, and what the frame clock learns
//! from them: which draws a clip's pictures make anyway.
#![deny(clippy::indexing_slicing)]

use super::super::frame_grid;
use super::super::frame_grid::{ContentGrid, Vsync};
use super::super::video_player::FrameRequested;
use super::super::video_player::pacing::{FrameRequest, request_policy};
use super::{AppGate, ClockTiming, assume_active, content_frame, tick_wanted};
use gpui_kit::*;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};

/// What [`request_video_frame`] needs from the playing clip.
pub(crate) struct VideoFrameAsk {
    /// The display it shows on, while that reports vsync times.
    pub(crate) display: Option<u64>,
    /// When the next picture becomes due (`None`: unknown, ask every frame).
    pub(crate) next_due: Option<Instant>,
    /// The display's frame interval as measured by the element.
    pub(crate) interval: Duration,
    /// How long before it is shown a frame starts drawing.
    pub(crate) lead: Duration,
    /// Its pictures' spacing, once they come a whole number of grid
    /// vsyncs apart (`frame_grid::Cadence::even`).
    pub(crate) even_spacing: Option<Duration>,
    /// The clip plays with sound (keeps drawing behind another app).
    pub(crate) plays_sound: bool,
    /// Keep drawing while the window is inactive (a Picture-in-Picture
    /// window rarely has focus).
    pub(crate) keep_alive: bool,
    /// Shared per player: the request in flight (the view it redraws and
    /// when it counts as lost), so several draws in one frame ask once.
    pub(crate) requested: FrameRequested,
}

/// A request not answered this long after its frame was due is treated as
/// lost (the window closed or the frame never came) and asked again.
const VIDEO_REQUEST_LOST: Duration = Duration::from_millis(250);
/// A sleeping request counts as lost this long after its wake.
const VIDEO_WAKE_SLACK: Duration = Duration::from_millis(50);

/// Whether a request for `target` is already in flight at `now`. One left
/// by another view (a closed Picture-in-Picture window) does not hold this
/// one back.
pub(super) fn video_request_in_flight(
    requested: Option<(EntityId, Instant)>,
    target: EntityId,
    now: Instant,
) -> bool {
    requested.is_some_and(|(view, lost_at)| view == target && now < lost_at)
}

/// What to ask for a clip drawn in `target` at `now` whose next picture
/// is due at `next_due`, noted in `requested` (the player's) as in flight
/// until it counts as lost. `None` while a request for `target` is in
/// flight.
pub(in crate::ui) fn video_request(
    requested: &FrameRequested,
    target: EntityId,
    now: Instant,
    display: Option<u64>,
    next_due: Option<Instant>,
    interval: Duration,
    lead: Duration,
    even_spacing: Option<Duration>,
) -> Option<FrameRequest> {
    if video_request_in_flight(requested.get(), target, now) {
        return None;
    }
    let request = request_policy(now, next_due, interval, lead);
    let lost_after = match request {
        FrameRequest::NextFrame => VIDEO_REQUEST_LOST,
        // A long sleep (a low-rate clip) must not look lost before it
        // wakes, or a second timer would stack on it.
        FrameRequest::SleepUntil(wake) => VIDEO_REQUEST_LOST.max(
            wake.saturating_duration_since(now)
                .saturating_add(VIDEO_WAKE_SLACK),
        ),
    };
    requested.set(Some((target, now.checked_add(lost_after).unwrap_or(now))));
    VIDEO_ASKS.with(|asks| {
        let mut asks = asks.borrow_mut();
        asks.retain(|ask| !ask.asked_by(requested) && ask.live(now));
        asks.push(VideoAsk {
            requested: Rc::downgrade(requested),
            display,
            due: next_due.unwrap_or(now),
            even_spacing,
        });
    });
    Some(request)
}

thread_local! {
    static VIDEO_ASKS: RefCell<Vec<VideoAsk>> = const { RefCell::new(Vec::new()) };
}

/// A player's clip frame request ([`video_request`]).
struct VideoAsk {
    requested: Weak<Cell<Option<(EntityId, Instant)>>>,
    /// The display it shows on.
    display: Option<u64>,
    /// When the picture it asked for is due.
    due: Instant,
    /// Its pictures' spacing, once they come a whole number of grid
    /// vsyncs apart.
    even_spacing: Option<Duration>,
}

impl VideoAsk {
    fn asked_by(&self, requested: &FrameRequested) -> bool {
        self.requested
            .upgrade()
            .is_some_and(|other| Rc::ptr_eq(&other, requested))
    }

    /// Still in flight for any view.
    fn live(&self, now: Instant) -> bool {
        let lost_at = self.requested.upgrade().and_then(|asked| asked.get());
        lost_at.is_some_and(|(_, lost_at)| now < lost_at)
    }

    fn in_flight(&self, target: EntityId, now: Instant) -> bool {
        self.requested
            .upgrade()
            .is_some_and(|requested| video_request_in_flight(requested.get(), target, now))
    }
}

/// Whether a clip whose pictures come `spacing` apart keeps time with
/// content whose frames come `frame` apart: one is a whole number of the
/// other. Only then does each of its pictures that shows content's next
/// frame come one content frame after the last. A 12 fps clip's, ten
/// refreshes apart at 120 Hz, would show 30 fps content's frames on every
/// other grid vsync, one refresh early or late.
fn keeps_time(spacing: Option<Duration>, frame: Duration) -> bool {
    let whole = |a: f64, b: f64| {
        let ratio = a / b;
        ratio.is_finite() && ratio.round() >= 1.0 && (ratio - ratio.round()).abs() <= 0.05
    };
    spacing.is_some_and(|spacing| {
        let (spacing, frame) = (spacing.as_secs_f64(), frame.as_secs_f64());
        whole(spacing, frame) || whole(frame, spacing)
    })
}

/// A clip picture whose frame request was answered: the view it redraws,
/// when it is due, and its clip's [`VideoAsk::even_spacing`].
#[derive(Clone, Copy)]
struct Answered {
    target: EntityId,
    due: Instant,
    even_spacing: Option<Duration>,
}

thread_local! {
    static ANSWERED: RefCell<Vec<Answered>> = const { RefCell::new(Vec::new()) };
}

/// Forget every clip's asks and answers: a simulated clock starts again at
/// a real `Instant`, which an earlier simulation on this thread ran past.
#[cfg(test)]
pub(in crate::ui) fn forget_video_requests() {
    VIDEO_ASKS.with(|asks| asks.borrow_mut().clear());
    ANSWERED.with(|answered| answered.borrow_mut().clear());
}

/// Whether the frame shown on `vsync` draws a picture of a clip that keeps
/// time with content whose frames come `frame` apart ([`keeps_time`]), in
/// `target` (`None`: any view). Content shows its own time on that frame
/// ([`content_now`]), and a layer it redraws needs no tick until a content
/// frame later (`LayerDraws`).
pub(in crate::ui) fn clip_frame_keeps_time(
    target: Option<EntityId>,
    vsync: Vsync,
    frame: Duration,
) -> bool {
    let shows_now =
        |due: Instant| due <= vsync.at + vsync.period / 4 && vsync.at < due + vsync.period;
    ANSWERED.with(|answered| {
        answered.borrow().iter().any(|picture| {
            target.is_none_or(|target| target == picture.target)
                && shows_now(picture.due)
                && keeps_time(picture.even_spacing, frame)
        })
    })
}

/// Whether a clip frame asked for `target` and still in flight at `now`
/// shows a picture by `by` and keeps time with content at `fps`: its draw
/// redraws `target` with that content's next frame.
fn video_redraws(target: EntityId, fps: u32, by: Instant, now: Instant) -> bool {
    VIDEO_ASKS.with(|asks| {
        asks.borrow().iter().any(|ask| {
            ask.due <= by
                && keeps_time(ask.even_spacing, content_frame(fps))
                && ask.in_flight(target, now)
        })
    })
}

/// The vsync of the last picture before `tick`, a planned tick of content at
/// `fps` on `display`, of a clip keeping time with it, when the tick shares
/// no clip's window frame. On any one clip's pictures it stays there, so two
/// clips in opposite phases never pull it back and forth.
pub(super) fn clip_beat(
    display: u64,
    grid: &ContentGrid,
    fps: u32,
    tick: Instant,
    now: Instant,
) -> Option<Instant> {
    let frame = content_frame(fps);
    VIDEO_ASKS.with(|asks| {
        let mut beat = None;
        let asks = asks.borrow();
        let clips = asks
            .iter()
            .filter(|ask| ask.display == Some(display) && ask.live(now))
            .filter(|ask| keeps_time(ask.even_spacing, frame));
        for ask in clips {
            if ask.even_spacing? + grid.period() / 4 < frame {
                return None;
            }
            let at = frame_grid::beat_before(grid, ask.due, frame, tick)?;
            beat = beat.or(Some(at));
        }
        beat
    })
}

/// Whether animation layer `painter`, drawn on the main window's latest
/// vsync (`display`) with content at `fps`, is drawn again by a clip's
/// picture (an inline tile's) at least a display frame before that content
/// needs its next frame: the clip's frame, already asked for, redraws the
/// layer with it. The layer then needs no tick, which would only wait for
/// that draw (`later_tick`). Only on a display with a content grid.
pub(in crate::ui) fn layer_redrawn_by_clip(
    display: Option<(ClockTiming, Vsync)>,
    painter: EntityId,
    fps: u32,
    now: Instant,
) -> bool {
    let Some((timing, vsync)) = display else {
        return false;
    };
    if frame_grid::display_grid(timing.display, vsync, now).is_none() {
        return false;
    }
    let frame = Duration::from_secs_f64(1.0 / f64::from(fps.max(1)));
    // The content's next frame, a display frame early.
    let by = vsync
        .at
        .checked_add(frame)
        .and_then(|next| next.checked_sub(vsync.period));
    by.is_some_and(|by| video_redraws(painter, fps, by, now))
}

/// The request for `target` was answered: forget it, unless another view's
/// replaced it meanwhile, and note the picture it asked for
/// ([`clip_frame_keeps_time`]).
pub(in crate::ui) fn video_request_done(requested: &FrameRequested, target: EntityId) {
    if !requested.get().is_some_and(|(view, _)| view == target) {
        return;
    }
    requested.set(None);
    let ask = VIDEO_ASKS.with(|asks| {
        asks.borrow()
            .iter()
            .find(|ask| ask.asked_by(requested))
            .map(|ask| Answered {
                target,
                due: ask.due,
                even_spacing: ask.even_spacing,
            })
    });
    if let Some(ask) = ask {
        ANSWERED.with(|answered| {
            let mut answered = answered.borrow_mut();
            answered.retain(|old| old.due + VIDEO_REQUEST_LOST > ask.due);
            answered.push(ask);
        });
    }
}

/// Ask for the frame that shows the next video picture, redrawing only
/// `target` (the view holding the picture). A frame shows the picture it
/// pulls `lead` after it starts drawing, so the frame for a picture due at
/// `next_due` starts at `next_due - lead`: a one-shot timer wakes half an
/// interval before that and asks for the next display frame. When that is
/// already now, or the due time is unknown, the next display frame is
/// asked for at once (`pacing::request_policy`).
///
/// Unlike [`QuillApp::request_animation_tick`] it runs at the display's own
/// rate, so the picture lands on the vsync it is due for. GPUI caps an
/// unfocused window's next-frame callbacks at its `inactive_frame_interval`
/// (`window_frame::INACTIVE_FRAME_INTERVAL` for Quill's main and chat
/// windows; `None` for Picture-in-Picture's).
///
/// Nothing is asked behind another app unless the clip has sound (or
/// `keep_alive`), like the animation clock: frames are asked while any
/// Quill window is active ([`AppGate`]), not only the clip's own. Asked
/// only while its own window was, a clip froze once its window lost focus
/// to another Quill window: its last asks were refused, and nothing drew
/// its window again.
pub(crate) fn request_video_frame(
    target: EntityId,
    ask: VideoFrameAsk,
    window: &mut Window,
    cx: &mut App,
) {
    #[cfg(test)]
    super::VIDEO_TARGETS.with(|targets| targets.borrow_mut().push(target));
    let active = window.is_window_active()
        || assume_active()
        || cx
            .try_global::<AppGate>()
            .is_some_and(|gate| gate.animates(window.window_handle().window_id()));
    if !tick_wanted(active, ask.plays_sound || ask.keep_alive) {
        return;
    }
    let now = Instant::now();
    let requested = ask.requested;
    let Some(request) = video_request(
        &requested,
        target,
        now,
        ask.display,
        ask.next_due,
        ask.interval,
        ask.lead,
        ask.even_spacing,
    ) else {
        return;
    };
    match request {
        FrameRequest::NextFrame => {
            window.on_next_frame(move |_, cx| {
                video_request_done(&requested, target);
                cx.notify(target);
            });
        }
        FrameRequest::SleepUntil(wake) => {
            let sleep = wake.saturating_duration_since(now);
            let handle = window.window_handle();
            cx.spawn(async move |cx| {
                cx.background_executor().timer(sleep).await;
                let asked = handle.update(cx, |_, window, _| {
                    let requested = requested.clone();
                    window.on_next_frame(move |_, cx| {
                        video_request_done(&requested, target);
                        cx.notify(target);
                    });
                });
                if asked.is_err() {
                    video_request_done(&requested, target);
                }
            })
            .detach();
        }
    }
}
