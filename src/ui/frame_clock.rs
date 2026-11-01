//! One redraw clock for everything that animates in the window: custom
//! emoji, animated stickers and inline video. GPUI's own animated images
//! redraw the whole window on every display refresh (120 Hz on ProMotion),
//! relaying out the full tree each time; this clock redraws only as fast
//! as the content needs and stops when nothing animated rendered.
//!
//! Each request is attributed to the cached slice being drawn
//! (`app_slice`); a tick then redraws only the slices that asked, and the
//! rest of the window replays its last frame. Requests made outside any
//! slice redraw the whole app, as before.
//!
//! On a display faster than 60 Hz, while a clip also plays there, the
//! clock ticks on the main window's content grid (`frame_grid`), the
//! vsyncs the clip draws on too, so they share window frames. An animation
//! layer whose inline clip redraws it more often than its content needs
//! asks for no tick at all ([`layer_redrawn_by_clip`]).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::frame_grid::{self, Source, Vsync};
use super::video_player::FrameRequested;
use super::video_player::pacing::{FrameRequest, request_policy};
use gpui_kit::*;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};

/// The main window's display and its refresh period, and how long before
/// it is shown one of its frames starts drawing, as of its frames drawn
/// while the display link ran.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ClockTiming {
    pub(super) display: u64,
    pub(super) period: Duration,
    pub(super) lead: Duration,
}

impl ClockTiming {
    /// `timing` after a frame of `display` starting at `now`, shown on
    /// `vsync` (the display link's latest present time); as it was when
    /// that is past (the frame was drawn while the display link was
    /// stopped). A frame starts drawing some time after the display link
    /// asks for it, a busy main thread's well after, so the lead is the
    /// longest lately seen: one frame that started late must not make
    /// every later tick miss its vsync. It shrinks a sixteenth of a frame
    /// at each frame, so a real change still shows.
    pub(super) fn sample(
        timing: Option<Self>,
        display: u64,
        vsync: Vsync,
        now: Instant,
    ) -> Option<Self> {
        let Some(lead) = vsync.at.checked_duration_since(now) else {
            return timing;
        };
        let kept = timing
            .filter(|timing| timing.display == display && timing.period == vsync.period)
            .map_or(Duration::ZERO, |timing| {
                timing.lead.saturating_sub(vsync.period / 16)
            });
        Some(Self {
            display,
            period: vsync.period,
            lead: lead.max(kept).clamp(vsync.period / 2, vsync.period * 3),
        })
    }
}

/// The frame clock between wakes: the rate it ticks at, the vsync its
/// content last drew on, and a tick put off until after a layer's draw.
pub(super) struct ClockRun {
    pub(super) fps: u32,
    last: LastDraw,
    later: Option<(Duration, Option<Instant>)>,
}

/// The vsync the frame clock's content last drew on.
#[derive(Clone, Copy, Debug, PartialEq)]
enum LastDraw {
    /// No tick yet: the draw that started the clock, on the latest vsync.
    Start,
    /// A tick off the grid: its draw comes on the vsync after the latest
    /// one when it woke (the clock plans right after).
    OffGrid,
    /// A tick on this grid vsync.
    On(Instant),
}

impl ClockRun {
    pub(super) fn new() -> Self {
        Self {
            fps: 30,
            last: LastDraw::Start,
            later: None,
        }
    }

    /// How long to sleep for the next tick at `now`, and the grid vsync it
    /// draws on (`None`: off the grid), for a main window drawn as
    /// `display` says.
    pub(super) fn plan(
        &mut self,
        display: Option<(ClockTiming, Vsync)>,
        now: Instant,
    ) -> (Duration, Option<Instant>) {
        if let Some(plan) = self.later.take() {
            return plan;
        }
        let last = display.map(|(_, vsync)| match self.last {
            LastDraw::Start => vsync.at,
            LastDraw::OffGrid => vsync.at + vsync.period,
            LastDraw::On(at) => at,
        });
        tick_wait(display, now, self.fps, last)
    }

    /// The wake for `planned` did `woke`. `false`: the clock stops.
    pub(super) fn woke(&mut self, planned: Option<Instant>, woke: Woke) -> bool {
        match woke {
            Woke::Later(plan) => self.later = Some(plan),
            Woke::Ticked(0) => return false,
            Woke::Ticked(fps) => {
                self.fps = fps;
                self.last = planned.map_or(LastDraw::OffGrid, LastDraw::On);
            }
        }
        true
    }
}

/// What each animation layer of the main window drew since the clock last
/// ticked it: a layer a clip's picture redrew (with its emoji) needs no
/// tick of its own until one content frame after that draw.
#[derive(Debug, Default)]
pub(super) struct LayerDraws(Vec<LayerDraw>);

#[derive(Debug)]
struct LayerDraw {
    painter: EntityId,
    /// The clock ticked it and it has not drawn since: its next draw is
    /// the tick's own.
    ticked: bool,
    /// The vsync it last drew on after its tick's own draw.
    redrawn: Option<Instant>,
}

impl LayerDraws {
    /// The clock ticked `painter` (any other view is no layer: ignored).
    pub(super) fn ticked(&mut self, painter: EntityId) {
        if let Some(draw) = self.0.iter_mut().find(|draw| draw.painter == painter) {
            draw.ticked = true;
            draw.redrawn = None;
        }
    }

    /// `painter` drew a frame shown on the vsync at `at`.
    pub(super) fn drawn(&mut self, painter: EntityId, at: Instant) {
        if let Some(draw) = self.0.iter_mut().find(|draw| draw.painter == painter) {
            if std::mem::take(&mut draw.ticked) {
                draw.redrawn = None;
            } else {
                draw.redrawn = Some(at);
            }
            return;
        }
        // The chat list's and the conversation's; a replaced one ages out.
        if self.0.len() >= 4 {
            self.0.remove(0);
        }
        self.0.push(LayerDraw {
            painter,
            ticked: false,
            redrawn: Some(at),
        });
    }

    /// The oldest of the vsyncs `targets` were last redrawn on since their
    /// ticks: `None` unless every target is a layer redrawn since.
    pub(super) fn redrawn(
        &self,
        targets: impl IntoIterator<Item = Option<EntityId>>,
    ) -> Option<Instant> {
        let mut oldest: Option<Instant> = None;
        for target in targets {
            let painter = target?;
            let at = self
                .0
                .iter()
                .find(|draw| draw.painter == painter)?
                .redrawn?;
            oldest = Some(oldest.map_or(at, |oldest| oldest.min(at)));
        }
        oldest
    }
}

impl QuillApp {
    /// Ask for another frame within `1 / fps` seconds. Animated content
    /// calls this each time it renders; the clock stops after a tick in
    /// which nothing asked.
    #[track_caller]
    pub(super) fn request_animation_tick(&self, fps: u32, cx: &mut Context<Self>) {
        let target = self.frame.tick_view.get().or(self.slices.current());
        self.request_animation_tick_for(target, fps, cx);
    }

    /// [`Self::request_animation_tick`] for what `QuillApp` builds itself
    /// around the slices (the tap effect, the sent sticker's flight): it
    /// renders, and every slice replays.
    #[track_caller]
    pub(super) fn request_app_tick(&self, fps: u32, cx: &mut Context<Self>) {
        let target = self.frame.tick_view.get().or(self.slices.app_only_id());
        self.request_animation_tick_for(target, fps, cx);
    }

    /// [`Self::request_animation_tick`] for an explicit target: a slice or
    /// the chat list's animation layer by entity id, `None` for the app.
    #[track_caller]
    pub(super) fn request_animation_tick_for(
        &self,
        target: Option<EntityId>,
        fps: u32,
        cx: &mut Context<Self>,
    ) {
        self.request_tick(target, fps, TickGate::App, cx);
    }

    #[track_caller]
    pub(super) fn request_window_view_tick(
        &self,
        view: EntityId,
        window: &Window,
        fps: u32,
        cx: &mut Context<Self>,
    ) {
        self.frame.window_views.borrow_mut().insert(view);
        let gate = TickGate::OwnWindow {
            active: window.is_window_active(),
        };
        self.request_tick(Some(view), fps, gate, cx);
    }

    /// [`Self::request_animation_tick`] for the whole app that keeps
    /// ticking behind another app: the agent takeover, which the user
    /// watches from the app the agent runs in.
    #[track_caller]
    pub(super) fn request_background_animation_tick(&self, fps: u32, cx: &mut Context<Self>) {
        self.request_tick(None, fps, TickGate::Sound, cx);
    }

    #[track_caller]
    fn request_tick(
        &self,
        target: Option<EntityId>,
        fps: u32,
        gate: TickGate,
        cx: &mut Context<Self>,
    ) {
        // Like tdesktop (`isGifPausedAtLeastFor` → `!widget()->isActive()`),
        // nothing animates behind another app; activation redraws and the
        // content asks again (`observe_window_activation`).
        let own_window_active = gate == TickGate::OwnWindow { active: true };
        let plays_sound = gate == TickGate::Sound;
        if !tick_wanted(self.frame.window_active.get(), plays_sound) && !own_window_active {
            return;
        }
        // Nor in a main window the active chat window hides whole
        // (`chat_window::cover`): its slices and their layers get no tick.
        // A tick for the whole app still comes, whichever window asks; it
        // redraws every window but that one then (`tick_app`).
        if self.frame.main_covered.get() && target.is_some_and(|id| self.slices.owns(id)) {
            return;
        }
        // `QUILL_TRACE_TICKS=1`: log who keeps the clock running (once a
        // second per call site), to hunt idle redraws.
        if trace_ticks() {
            trace_caller(
                std::panic::Location::caller(),
                fps,
                target,
                self.frame.window_active.get(),
            );
        }
        self.frame.animation_demand.set(
            self.frame
                .animation_demand
                .get()
                .max(fps.clamp(1, MAX_TICK_FPS)),
        );
        self.frame.animation_targets.borrow_mut().insert(target);
        if plays_sound {
            self.frame.animation_sound.set(true);
        }
        if own_window_active {
            self.frame.own_window_tick.set(true);
        }
        if self.frame.frame_clock_running.get() {
            return;
        }
        self.frame.frame_clock_running.set(true);
        cx.spawn(async move |this, cx| {
            let mut run = ClockRun::new();
            loop {
                let (wait, vsync) = this
                    .update(cx, |this, _| run.plan(this.clock_display(), Instant::now()))
                    .unwrap_or((tick_period(run.fps), None));
                cx.background_executor().timer(wait).await;
                let woke = this
                    .update(cx, |this, cx| {
                        if let Some(plan) = this.tick_after_draws(run.fps, vsync) {
                            return Woke::Later(plan);
                        }
                        let demand = this.frame.animation_demand.replace(0);
                        let targets =
                            std::mem::take(&mut *this.frame.animation_targets.borrow_mut());
                        let window_views =
                            std::mem::take(&mut *this.frame.window_views.borrow_mut());
                        let sound = this.frame.animation_sound.replace(false);
                        let own_window = this.frame.own_window_tick.replace(false);
                        let app_ticks = tick_wanted(this.frame.window_active.get(), sound);
                        if demand > 0 && (app_ticks || own_window) {
                            note_clock_playing(
                                this.frame.clock_timing.get(),
                                Instant::now(),
                                demand,
                            );
                            if app_ticks {
                                for id in targets.iter().flatten() {
                                    this.frame.layers_drawn.borrow_mut().ticked(*id);
                                }
                                if targets.contains(&None) {
                                    this.tick_app(cx);
                                } else {
                                    let covered = this.frame.main_covered.get();
                                    let views: Vec<_> = targets
                                        .into_iter()
                                        .flatten()
                                        .filter(|id| !covered || !this.slices.owns(*id))
                                        .collect();
                                    let app: &mut App = cx;
                                    for id in views {
                                        app.notify(id);
                                    }
                                }
                            } else {
                                // Only the window views' own window is
                                // active: the rest holds still.
                                for id in &window_views {
                                    this.frame.layers_drawn.borrow_mut().ticked(*id);
                                }
                            }
                            let app: &mut App = cx;
                            for id in window_views {
                                app.notify(id);
                            }
                        }
                        Woke::Ticked(demand)
                    })
                    .unwrap_or(Woke::Ticked(0));
                if !run.woke(vsync, woke) {
                    break;
                }
            }
            let _ = this.update(cx, |this, _| this.frame.frame_clock_running.set(false));
        })
        .detach();
    }

    /// The main window's display timing and that display's latest vsync,
    /// while its display link reports them.
    fn clock_display(&self) -> Option<(ClockTiming, Vsync)> {
        let timing = self.frame.clock_timing.get()?;
        Some((timing, display_vsync(timing.display)?))
    }

    /// The tick planned for the grid vsync `planned`, put off (see
    /// [`later_tick`]) when every view that asked is an animation layer a
    /// clip's picture redrew since the last tick. `None` when it is due.
    fn tick_after_draws(
        &self,
        fps: u32,
        planned: Option<Instant>,
    ) -> Option<(Duration, Option<Instant>)> {
        let redrawn = self
            .frame
            .layers_drawn
            .borrow()
            .redrawn(self.frame.animation_targets.borrow().iter().copied());
        let fps = fps.max(self.frame.animation_demand.get());
        later_tick(self.clock_display(), Instant::now(), fps, planned, redrawn)
    }

    /// Whether animation layer `painter`, just drawn with content at `fps`
    /// (`tick_animation_layer`), needs a tick: not while a clip's picture
    /// redraws it in time ([`layer_redrawn_by_clip`]). The clock then also
    /// drops what the layer asked before, and stops if nothing else asked:
    /// the picture's draw asks again.
    pub(super) fn layer_wants_tick(&self, painter: EntityId, fps: u32) -> bool {
        if !layer_redrawn_by_clip(self.clock_display(), painter, fps, Instant::now()) {
            return true;
        }
        let mut targets = self.frame.animation_targets.borrow_mut();
        targets.remove(&Some(painter));
        if targets.is_empty() {
            self.frame.animation_demand.set(0);
        }
        false
    }

    /// Note that animation layer `painter` drew on this frame of the main
    /// window (`tick_animation_layer`), for [`Self::tick_after_draws`].
    pub(super) fn note_layer_drawn(&self, painter: EntityId) {
        if let Some((_, vsync)) = self.clock_display() {
            self.frame
                .layers_drawn
                .borrow_mut()
                .drawn(painter, vsync.at);
        }
    }

    /// Note the main window's display and how far ahead of its present
    /// time this frame started (`QuillApp::render`), for the clock's
    /// content grid ([`ClockTiming::sample`]).
    pub(super) fn note_frame_timing(&self, window: &Window, cx: &App) {
        let Some(display) = window.display(cx).map(|display| u64::from(display.id())) else {
            return;
        };
        if let Some(vsync) = display_vsync(display) {
            let timing = self.frame.clock_timing.get();
            self.frame.clock_timing.set(ClockTiming::sample(
                timing,
                display,
                vsync,
                Instant::now(),
            ));
        }
    }
}

/// How long until the next tick at `now` of content at `fps`, and the grid
/// vsync it draws on, for a main window drawn as `display` says with that
/// display's latest vsync, if known: one content frame after `last` (the
/// vsync the content last drew on) on the content grid while a clip also
/// plays on the display (`frame_grid`); else one period at `fps` from
/// now, as before.
fn tick_wait(
    display: Option<(ClockTiming, Vsync)>,
    now: Instant,
    fps: u32,
    last: Option<Instant>,
) -> (Duration, Option<Instant>) {
    let on_grid = display.and_then(|(timing, vsync)| {
        let grid = frame_grid::display_grid(timing.display, vsync, now)?;
        // Alone, content keeps its own timing.
        if !frame_grid::shared(timing.display, Source::Clock, now) {
            return None;
        }
        frame_grid::clock_tick(&grid, timing.lead, last?, now, fps)
    });
    match on_grid {
        Some((wake, vsync)) => (wake.saturating_duration_since(now), Some(vsync)),
        None => (tick_period(fps), None),
    }
}

/// The clock ticked content at `fps` on the main window's display at `now`
/// (`timing`), so a clip there shares its grid.
pub(super) fn note_clock_playing(timing: Option<ClockTiming>, now: Instant, fps: u32) {
    if let Some(timing) = timing {
        let next = now.checked_add(tick_period(fps)).unwrap_or(now);
        frame_grid::note_playing(timing.display, Source::Clock, now, next);
    }
}

/// What a wake of the frame clock did.
pub(super) enum Woke {
    /// Ticked what asked at this rate (0: nothing asked, the clock stops).
    Ticked(u32),
    /// Nothing was due yet: wait this long, for that vsync.
    Later((Duration, Option<Instant>)),
}

/// The tick planned for the grid vsync `planned` waits when every view
/// that asked is an animation layer drawn again after the tick before it
/// (a clip's picture redrew it, with its emoji), the oldest on `redrawn`:
/// the tick then comes one content frame after that draw, sharing the
/// clip's next window frame. A tick is never put off for being late: the
/// frame after `redrawn` is counted from that draw, not from `now`, and a
/// tick's own draw is no redraw ([`LayerDraws`]). `None` when it is due.
pub(super) fn later_tick(
    display: Option<(ClockTiming, Vsync)>,
    now: Instant,
    fps: u32,
    planned: Option<Instant>,
    redrawn: Option<Instant>,
) -> Option<(Duration, Option<Instant>)> {
    let (planned, redrawn) = (planned?, redrawn?);
    let (timing, vsync) = display?;
    let grid = frame_grid::display_grid(timing.display, vsync, now)?;
    if !frame_grid::shared(timing.display, Source::Clock, now) {
        return None;
    }
    let due = frame_grid::content_due(&grid, redrawn, fps)?;
    if due <= planned + Duration::from_millis(1) {
        return None;
    }
    let (wake, at) = frame_grid::clock_tick(&grid, timing.lead, redrawn, now, fps)?;
    Some((wake.saturating_duration_since(now), Some(at)))
}

/// `display`'s latest vsync as its display link reported it (macOS).
#[cfg(target_os = "macos")]
fn display_vsync(display: u64) -> Option<Vsync> {
    let (hint, period) = gpui_macos::present_timing(display);
    let ((at, _), period) = hint.zip(period)?;
    Some(Vsync { at, period })
}

/// No display link: no vsync times, so no content grid.
#[cfg(not(target_os = "macos"))]
fn display_vsync(_: u64) -> Option<Vsync> {
    None
}

/// The fastest the clock ticks: animated stickers on a 120 Hz display
/// (`sticker_player`).
pub(super) const MAX_TICK_FPS: u32 = 120;

/// The wait between two ticks at `fps`. A tick only marks the window for
/// its next display frame, so at display-like rates the wait is a little
/// shorter than a frame: a tick is then pending at every display frame
/// instead of now and then landing just after one (a skipped frame).
fn tick_period(fps: u32) -> Duration {
    let frame = 1.0 / f64::from(fps.max(1));
    if fps >= 48 {
        Duration::from_secs_f64(frame * 0.92)
    } else {
        Duration::from_secs_f64(frame)
    }
}

/// What [`request_video_frame`] needs from the playing clip.
pub(crate) struct VideoFrameAsk {
    /// When the next picture becomes due (`None`: unknown, ask every frame).
    pub(crate) next_due: Option<Instant>,
    /// The display's frame interval as measured by the element.
    pub(crate) interval: Duration,
    /// How long before it is shown a frame starts drawing.
    pub(crate) lead: Duration,
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
fn video_request_in_flight(
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
pub(super) fn video_request(
    requested: &FrameRequested,
    target: EntityId,
    now: Instant,
    next_due: Option<Instant>,
    interval: Duration,
    lead: Duration,
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
        asks.retain(|(other, _)| {
            other.upgrade().is_some_and(|other| {
                !Rc::ptr_eq(&other, requested)
                    && other.get().is_some_and(|(_, lost_at)| now < lost_at)
            })
        });
        asks.push((Rc::downgrade(requested), next_due.unwrap_or(now)));
    });
    Some(request)
}

thread_local! {
    /// Each player's clip frame request ([`video_request`]), with when the
    /// picture it asked for is due.
    static VIDEO_ASKS: RefCell<Vec<(Weak<Cell<Option<(EntityId, Instant)>>>, Instant)>> =
        const { RefCell::new(Vec::new()) };
}

/// Whether a clip frame asked for `target` and still in flight at `now`
/// shows a picture by `by`: its draw redraws `target` by then.
fn video_redraws(target: EntityId, by: Instant, now: Instant) -> bool {
    VIDEO_ASKS.with(|asks| {
        asks.borrow().iter().any(|(requested, due)| {
            *due <= by
                && requested
                    .upgrade()
                    .is_some_and(|requested| video_request_in_flight(requested.get(), target, now))
        })
    })
}

/// Whether animation layer `painter`, drawn on the main window's latest
/// vsync (`display`) with content at `fps`, is drawn again by a clip's
/// picture (an inline tile's) at least a display frame before that content
/// needs its next frame: the clip's frame, already asked for, redraws the
/// layer with it. The layer then needs no tick, which would only wait for
/// that draw (`later_tick`). Only on a display with a content grid.
pub(super) fn layer_redrawn_by_clip(
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
    by.is_some_and(|by| video_redraws(painter, by, now))
}

/// The request for `target` was answered: forget it, unless another view's
/// replaced it meanwhile.
pub(super) fn video_request_done(requested: &FrameRequested, target: EntityId) {
    if requested.get().is_some_and(|(view, _)| view == target) {
        requested.set(None);
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
    VIDEO_TARGETS.with(|targets| targets.borrow_mut().push(target));
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
        ask.next_due,
        ask.interval,
        ask.lead,
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

/// A frame-clock tick for the whole app that leaves the main window out:
/// sent while the active chat window covers it (`QuillApp::tick_app`), to
/// the views that follow the app ([`follow_app`]).
pub(in crate::ui) struct AppTick;

impl EventEmitter<AppTick> for QuillApp {}

/// Follow the app from a view in a window other than the main one: redraw
/// on each change to the app and on each [`AppTick`], so content there that
/// asks for whole-app ticks keeps animating while the main window does not
/// redraw. A cached view that follows the app redraws with it.
pub(in crate::ui) fn follow_app<V: 'static>(
    app: &Entity<QuillApp>,
    cx: &mut Context<V>,
) -> Subscription {
    Subscription::join(
        cx.observe(app, |_, _, cx| cx.notify()),
        cx.subscribe(app, |_, _, _: &AppTick, cx| cx.notify()),
    )
}

/// The app's animation gate as `QuillApp::follow_windows` last set it, for
/// the clips' frame requests, which only have their own window at hand.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct AppGate {
    /// A Quill window is active (`FrameState::window_active`).
    pub(super) active: bool,
    /// The windows whose own content holds still meanwhile: those the
    /// active window hides whole (`chat_window::cover`).
    pub(super) held: Vec<WindowId>,
}

impl Global for AppGate {}

impl AppGate {
    /// Whether content in `window` animates under this gate.
    fn animates(&self, window: WindowId) -> bool {
        self.active && !self.held.contains(&window)
    }

    /// Whether `window` is hidden whole behind the active window.
    fn holds(cx: &App, window: WindowId) -> bool {
        cx.try_global::<AppGate>()
            .is_some_and(|gate| gate.held.contains(&window))
    }
}

/// [`follow_app`] for a chat window in `window`, which skips the redraws
/// while the active window hides it whole: nobody sees them.
/// `QuillApp::follow_windows` redraws it once it shows again.
pub(in crate::ui) fn follow_app_unless_held<V: 'static>(
    app: &Entity<QuillApp>,
    window: WindowId,
    cx: &mut Context<V>,
) -> Subscription {
    Subscription::join(
        cx.observe(app, move |_, _, cx| {
            if !AppGate::holds(cx, window) {
                cx.notify();
            }
        }),
        cx.subscribe(app, move |_, _, _: &AppTick, cx| {
            if !AppGate::holds(cx, window) {
                cx.notify();
            }
        }),
    )
}

/// What lets a tick request through while no main or chat window is active.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TickGate {
    /// Nothing: content in the main or a chat window.
    App,
    /// Media playing with sound, or the agent takeover.
    Sound,
    /// A view in a window of its own (the voice chat window), which ticks
    /// while that window is active.
    OwnWindow { active: bool },
}

/// Whether a frame-clock tick may run: always while the window is active,
/// and in the background only for media playing with sound.
fn tick_wanted(window_active: bool, plays_sound: bool) -> bool {
    window_active || plays_sound
}

/// `QUILL_ASSUME_ACTIVE=1`: animate as if the window had focus, so CPU can
/// be measured on a demo window launched from a terminal (which macOS does
/// not bring to the front).
pub(super) fn assume_active() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("QUILL_ASSUME_ACTIVE").is_some())
}

#[cfg(test)]
thread_local! {
    /// Every view a video frame was asked for on this thread, in order.
    static VIDEO_TARGETS: std::cell::RefCell<Vec<EntityId>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The views video frames were asked for since the last call, for tests.
#[cfg(test)]
#[cfg(feature = "demo-capture")]
pub(crate) fn take_video_targets() -> Vec<EntityId> {
    VIDEO_TARGETS.with(|targets| std::mem::take(&mut *targets.borrow_mut()))
}

#[cfg(test)]
thread_local! {
    /// Slice renders on this thread by kind, for tests.
    static SLICE_RENDERS: std::cell::RefCell<std::collections::BTreeMap<&'static str, u32>> =
        const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
}

/// The slice renders since the last call, by kind, for tests.
#[cfg(test)]
#[cfg(feature = "demo-capture")]
pub(crate) fn take_slice_renders() -> std::collections::BTreeMap<&'static str, u32> {
    SLICE_RENDERS.with(|counts| std::mem::take(&mut *counts.borrow_mut()))
}

/// What a window drew over `secs` of the test clock, for tests: the
/// slice renders by kind, with `(window frame)` once per frame (a test
/// window draws as soon as it is dirty), and `(next frame)` for each
/// callback that asked for a display frame (an animation frame), which
/// a test window only runs when asked.
#[cfg(test)]
#[cfg(feature = "demo-capture")]
pub(crate) fn frames_over(
    vcx: &mut gpui_kit::VisualTestContext,
    secs: u64,
) -> std::collections::BTreeMap<&'static str, u32> {
    let _ = take_slice_renders();
    let mut next_frames = 0;
    for _ in 0..secs * 10 {
        vcx.executor().advance_clock(Duration::from_millis(100));
        vcx.run_until_parked();
        next_frames += vcx.update(|window, cx| window.simulate_next_frame(cx));
        vcx.run_until_parked();
    }
    let mut drawn = take_slice_renders();
    if next_frames > 0 {
        drawn.insert(
            "(next frame)",
            u32::try_from(next_frames).unwrap_or(u32::MAX),
        );
    }
    drawn
}

#[path = "frame_trace.rs"]
mod trace;
pub(super) use trace::*;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "idle_frames_tests.rs"]
mod idle_frames_tests;

#[cfg(test)]
mod tests {
    use super::tick_wanted;

    #[test]
    fn ticks_at_display_rates_land_before_each_display_frame() {
        use super::tick_period;
        use std::time::Duration;
        assert_eq!(tick_period(30), Duration::from_secs_f64(1.0 / 30.0));
        assert!(tick_period(60) < Duration::from_secs_f64(1.0 / 60.0));
        assert!(tick_period(120) < Duration::from_secs_f64(1.0 / 120.0));
        assert!(tick_period(120) > Duration::from_secs_f64(1.0 / 144.0));
        assert_eq!(tick_period(0), Duration::from_secs(1));
    }

    #[test]
    fn nothing_ticks_behind_another_app() {
        assert!(tick_wanted(true, false));
        assert!(!tick_wanted(false, false));
    }

    #[test]
    fn media_with_sound_keeps_playing_in_the_background() {
        assert!(tick_wanted(false, true));
    }

    #[test]
    fn a_video_request_is_in_flight_for_its_own_view_until_it_is_lost() {
        use super::video_request_in_flight;
        use gpui_kit::EntityId;
        use std::time::{Duration, Instant};
        let now = Instant::now();
        let view = EntityId::from(1u64);
        let other = EntityId::from(2u64);
        let lost_at = now + Duration::from_millis(250);
        assert!(!video_request_in_flight(None, view, now));
        assert!(video_request_in_flight(Some((view, lost_at)), view, now));
        // Left by a closed window: not this view's.
        assert!(!video_request_in_flight(Some((other, lost_at)), view, now));
        // Past its deadline: lost, ask again.
        assert!(!video_request_in_flight(
            Some((view, lost_at)),
            view,
            lost_at + Duration::from_millis(1)
        ));
    }

    /// With the test platform: the request is recorded, held while in
    /// flight, and cleared when its frame comes.
    #[cfg(feature = "demo-capture")]
    #[gpui_kit::test]
    fn a_video_request_clears_when_its_frame_comes(cx: &mut gpui_kit::TestAppContext) {
        use super::{VideoFrameAsk, request_video_frame};
        use crate::ui::video_player::FrameRequested;
        use gpui_kit::{Context, EntityId, IntoElement, ParentElement, Render, Window, div};
        use std::cell::Cell;
        use std::rc::Rc;
        use std::time::Duration;

        struct Blank;
        impl Render for Blank {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div().child("")
            }
        }

        let ask = |requested: &FrameRequested| VideoFrameAsk {
            next_due: None,
            interval: Duration::from_millis(8),
            lead: Duration::from_millis(8),
            plays_sound: true,
            keep_alive: false,
            requested: requested.clone(),
        };
        let (view, vcx) = cx.add_window_view(|_, _| Blank);
        let target = view.entity_id();
        let requested: FrameRequested = Rc::new(Cell::new(None));
        vcx.update(|window, cx| request_video_frame(target, ask(&requested), window, cx));
        let first = requested.get();
        assert!(first.is_some_and(|(view, _)| view == target));
        // A second draw in the same frame asks nothing more.
        vcx.update(|window, cx| request_video_frame(target, ask(&requested), window, cx));
        assert_eq!(requested.get(), first);
        let ran = vcx.update(|window, cx| window.simulate_next_frame(cx));
        assert_eq!(ran, 1, "one frame callback for both draws");
        assert_eq!(requested.get(), None);
        // Another view's request in flight does not hold this one back.
        let other = EntityId::from(u64::MAX);
        requested.set(Some((
            other,
            std::time::Instant::now() + Duration::from_secs(1),
        )));
        vcx.update(|window, cx| request_video_frame(target, ask(&requested), window, cx));
        assert!(requested.get().is_some_and(|(view, _)| view == target));
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "frame_clock_window_view_tests.rs"]
mod window_view_tests;
