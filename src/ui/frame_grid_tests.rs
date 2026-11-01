use super::super::frame_clock::{
    ClockRun, ClockTiming, Woke, clip_redraws_off_grid, later_tick, layer_redrawn_by_clip,
    note_clock_playing, video_request, video_request_done,
};
use super::super::layer_draws::LayerDraws;
use super::super::video_player::FrameRequested;
use super::super::video_player::pacing::{FrameRequest, PresentEstimator};
use super::{Cadence, ContentGrid, Source, Vsync, clock_tick, display_grid, note_playing, shared};
use gpui_kit::EntityId;
use quill::sticker_playback::player::head::pick;
use quill::sticker_playback::player::{PlayMode, Timeline};
use std::collections::HashSet;
use std::time::{Duration, Instant};

fn hz(rate: f64) -> Duration {
    Duration::from_secs_f64(1.0 / rate)
}

fn close(a: Instant, b: Instant) -> bool {
    a.max(b).duration_since(a.min(b)) < Duration::from_micros(50)
}

fn before(at: Instant, by: Duration) -> Instant {
    at.checked_sub(by).unwrap()
}

#[test]
fn only_displays_at_whole_multiples_of_60_hz_have_a_grid() {
    let at = Instant::now();
    let step = |rate: f64| {
        ContentGrid::new(Vsync {
            at,
            period: hz(rate),
        })
        .map(|grid| grid.step)
    };
    assert_eq!(step(60.0), None);
    assert_eq!(step(59.94), None);
    assert_eq!(step(119.88), Some(2));
    assert_eq!(step(120.0), Some(2));
    assert_eq!(step(240.0), Some(4));
    // No whole number of vsyncs makes 60 a second: no grid.
    assert_eq!(step(90.0), None);
    assert_eq!(step(144.0), None);
    assert_eq!(step(165.0), None);
}

#[test]
fn a_picture_draws_on_the_first_grid_vsync_it_is_ready_for() {
    let t0 = Instant::now();
    let period = hz(120.0);
    let grid = ContentGrid::new(Vsync { at: t0, period }).unwrap();
    // On the grid (give or take noise): stays.
    assert!(close(grid.snap(t0), t0));
    assert!(close(grid.snap(t0 + period * 2), t0 + period * 2));
    assert!(close(
        grid.snap(t0 + period * 2 + Duration::from_micros(100)),
        t0 + period * 2
    ));
    // Due a little after a grid vsync (FFmpeg's own picture times): not
    // ready on it, so the next one.
    assert!(close(
        grid.snap(t0 + period * 2 + Duration::from_millis(1)),
        t0 + period * 4
    ));
    // Between grid vsyncs: the next one, one vsync later at most.
    assert!(close(grid.snap(t0 + period), t0 + period * 2));
    assert!(close(grid.snap(t0 + period * 3), t0 + period * 4));
    // Before the anchor too.
    assert!(close(
        grid.snap(before(t0, period * 3)),
        before(t0, period * 2)
    ));
}

#[test]
fn the_grid_keeps_its_phase_as_vsyncs_come_in() {
    let t0 = Instant::now();
    let period = hz(120.0);
    let grid = ContentGrid::new(Vsync { at: t0, period }).unwrap();
    // An odd vsync a while later, the display a little slower than it
    // says: anchored on the even one before it, from the real vsync time.
    let real = hz(119.99);
    let odd = t0 + real * 1001;
    let followed = grid.follow(Vsync { at: odd, period }).unwrap();
    assert!(close(followed.anchor, before(odd, period)));
    assert!(close(followed.snap(odd), odd + period));
    // A vsync off this phase (another display, a jump): a new grid on it.
    let off = t0 + period * 10 + period / 2;
    assert!(close(
        grid.follow(Vsync { at: off, period }).unwrap().anchor,
        off
    ));
    // Down to 60 Hz: no grid.
    assert_eq!(
        grid.follow(Vsync {
            at: t0,
            period: hz(60.0)
        }),
        None
    );
}

#[test]
fn content_ticks_on_the_grid_only_at_rates_it_divides() {
    let at = Instant::now();
    for (rate, cases) in [
        (
            120.0,
            [
                (60, Some(1)),
                (30, Some(2)),
                (20, Some(3)),
                (12, Some(5)),
                (25, None),
                (24, None),
                (40, None),
                (120, None),
            ],
        ),
        (
            119.88,
            [
                (60, Some(1)),
                (30, Some(2)),
                (20, Some(3)),
                (12, Some(5)),
                (25, None),
                (24, None),
                (50, None),
                (90, None),
            ],
        ),
    ] {
        let grid = ContentGrid::new(Vsync {
            at,
            period: hz(rate),
        })
        .unwrap();
        for (fps, every) in cases {
            assert_eq!(grid.every(fps), every, "{rate} Hz, {fps} fps");
        }
    }
}

#[test]
fn a_clip_draws_on_the_grid_only_when_that_holds_it_as_evenly() {
    let t0 = Instant::now();
    let period = hz(120.0);
    let grid = ContentGrid::new(Vsync { at: t0, period }).unwrap();
    // Pictures `vsyncs` apart, due on odd vsyncs (AVPlayer's due times).
    let drawn = |vsyncs: &[u32]| {
        let mut cadence = Cadence::default();
        let mut at = 1;
        let mut out = Vec::new();
        for gap in vsyncs {
            at += gap;
            let due = t0 + period * at;
            cadence.note(due, Some(&grid));
            out.push(cadence.on_grid(due, Some(&grid)) > due + period / 2);
        }
        out
    };
    // 30 fps (4 vsyncs, two grid ticks): on the grid, a vsync later, once
    // three gaps in a row were whole grid ticks.
    assert_eq!(drawn(&[4, 4, 4, 4, 4]), [false, false, false, true, true]);
    // 24 fps (5 vsyncs), 25 fps (5, 5, 5, 5, 4), 50 fps (2, 3, 2, 2, 3) and
    // 120 fps: the grid would hold them unevenly, so as due.
    assert!(drawn(&[5; 8]).iter().all(|on| !on));
    assert!(drawn(&[5, 5, 5, 5, 4, 5, 5, 5, 5, 4]).iter().all(|on| !on));
    assert!(drawn(&[2, 3, 2, 2, 3, 2, 3, 2, 2, 3]).iter().all(|on| !on));
    assert!(drawn(&[1; 8]).iter().all(|on| !on));
    // No grid (alone on the display, 60 Hz, no vsync times): as due.
    let mut thirty = Cadence::default();
    for k in 0..5u32 {
        thirty.note(t0 + period * (4 * k + 1), Some(&grid));
    }
    let due = t0 + period * 21;
    assert!(close(thirty.on_grid(due, None), due));
}

#[test]
fn the_clock_wakes_three_quarters_of_a_frame_before_its_frame_starts() {
    let t0 = Instant::now();
    let period = hz(120.0);
    let lead = period * 3 / 2;
    let grid = ContentGrid::new(Vsync { at: t0, period }).unwrap();
    // 30 fps after a tick on `t0`: two grid ticks on.
    let now = before(t0, lead) + Duration::from_millis(1);
    let (wake, at) = clock_tick(&grid, lead, t0, now, 30).unwrap();
    assert!(close(at, t0 + period * 4));
    assert!(close(wake, before(at, lead + period * 3 / 4)));
    // Woken late, past that frame's start: the next grid vsync, at once.
    let late = before(at, lead) + Duration::from_micros(200);
    let (wake, next) = clock_tick(&grid, lead, t0, late, 30).unwrap();
    assert!(close(next, at + period * 2));
    assert!(wake >= late);
    // Faster than the grid, or a rate it does not divide: not on it.
    assert_eq!(clock_tick(&grid, lead, t0, now, 120), None);
    assert_eq!(clock_tick(&grid, lead, t0, now, 25), None);
}

#[test]
fn content_shares_the_grid_only_with_something_else_on_its_display() {
    let now = Instant::now();
    let soon = now + Duration::from_millis(30);
    note_playing(900, Source::Clock, now, soon);
    assert!(!shared(900, Source::Clock, now));
    assert!(shared(900, Source::Clip(1), now));
    note_playing(900, Source::Clip(1), now, soon);
    assert!(shared(900, Source::Clock, now));
    // Another display: alone.
    assert!(!shared(901, Source::Clip(2), now));
    // Past its next draw and some slack: no longer animating.
    let later = soon + Duration::from_millis(150);
    assert!(!shared(900, Source::Clock, later));
}

#[test]
fn a_late_tick_is_never_put_off() {
    let t0 = Instant::now();
    let period = hz(120.0);
    let lead = period * 3 / 2;
    // The display link's latest present time at `now`: vsyncs on `t0` and
    // every `period`, each asked for `lead` before it.
    let display = |now: Instant| {
        let since = (now + lead).saturating_duration_since(t0);
        let frames = (since.as_secs_f64() / period.as_secs_f64()).floor() as u32;
        let at = t0 + period * frames;
        let timing = ClockTiming {
            display: 910,
            period,
            lead,
        };
        Some((timing, Vsync { at, period }))
    };
    let painter = EntityId::from(1u64);
    // A clip plays on the display too.
    note_playing(910, Source::Clip(1), t0, t0 + Duration::from_secs(1));
    // The last tick drew its layer on `t0`; the next is due two grid
    // ticks on, and its wake came late, past that frame's start.
    let mut layers = LayerDraws::default();
    layers.drawn(painter, before(t0, period * 4), true);
    layers.ticked(painter);
    layers.drawn(painter, t0, true);
    let planned = t0 + period * 4;
    let late = before(planned, lead) + Duration::from_millis(2);
    let redrawn = layers.redrawn([Some(painter)]);
    assert_eq!(redrawn, None);
    assert_eq!(
        later_tick(display(late), late, 30, Some(planned), redrawn),
        None
    );
    // A late picture redrew the layer a display frame after that tick: the
    // frame after it is still the planned one, so a late wake is not put
    // off either.
    layers.drawn(painter, t0 + period, true);
    let redrawn = layers.redrawn([Some(painter)]);
    assert_eq!(redrawn, Some(t0 + period));
    assert_eq!(
        later_tick(display(late), late, 30, Some(planned), redrawn),
        None
    );
    // A clip's picture redrew the layer one grid tick after the tick: the
    // next tick waits for the frame one content frame after that draw.
    layers.ticked(painter);
    layers.drawn(painter, t0, true);
    layers.drawn(painter, t0 + period * 2, true);
    let redrawn = layers.redrawn([Some(painter)]);
    assert_eq!(redrawn, Some(t0 + period * 2));
    let wake = t0 + period * 7 / 4;
    let (_, next) = later_tick(display(wake), wake, 30, Some(planned), redrawn).unwrap();
    assert!(close(next.unwrap(), t0 + period * 6));
    // A view that is not a layer asked too: due as planned.
    assert_eq!(layers.redrawn([Some(painter), None]), None);
}

/// A window on a display refreshing every `period`, whose frames are shown
/// `lead` after the display link asks for them, with custom emoji on an
/// animation layer the frame clock ticks, and a clip on that layer or a
/// view of its own. It runs the app's scheduling (`ClockRun`, `later_tick`
/// and `LayerDraws`, `layer_redrawn_by_clip`, `note_clock_playing`,
/// `ClockTiming::sample`, `PresentEstimator`, `video_request`, the
/// display's grid and what shares it) on a simulated clock, with the GPUI
/// glue around them: timers fire late (`Late`), and a notified view draws
/// on the next display frame.
struct Sim {
    display: u64,
    t0: Instant,
    period: Duration,
    lead: Duration,
    /// The content grid is in use (else: what Quill did before it).
    grid: bool,
    late: Late,
    now: Instant,
    seed: u64,
    timers: Vec<(Instant, Wake)>,
    /// Views to redraw on the next display frame.
    dirty: Vec<EntityId>,
    /// Clip views whose frame request asked for the next display frame.
    next_frame: Vec<EntityId>,
    /// The clip in a view of its own has started drawing.
    clip_started: bool,
    /// Frames on which the whole app redraws (`QuillApp::render`), from
    /// the first display link call at or after each time, with how long
    /// after that call each starts drawing.
    app_draws: Vec<(Instant, Duration)>,
    emoji_fps: u32,
    clip: Option<Clip>,
    clock: Option<ClockRun>,
    /// Times the frame clock started.
    clock_starts: u32,
    /// When the frame clock woke.
    clock_wakes: Vec<Instant>,
    demand: u32,
    targets: HashSet<Option<EntityId>>,
    timing: Option<ClockTiming>,
    layers: LayerDraws,
    estimator: PresentEstimator,
    requested: FrameRequested,
    /// When each window frame started drawing.
    draws: Vec<Instant>,
    /// The emoji's frame at each of its draws, with its present time.
    emoji_frames: Vec<(Instant, u64)>,
    /// The clip's picture at each of its draws, with its present time.
    pictures: Vec<(Instant, u64)>,
}

/// How late timers fire.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Late {
    /// Up to a millisecond.
    Little,
    /// Up to a tenth of the wait, plus up to 2 ms (macOS timer leeway and
    /// the hop to the main thread).
    Realistic,
    /// Always a tenth of the wait plus 2 ms: a run of late wakes.
    Always,
}

#[derive(Clone, Copy, Debug)]
enum Wake {
    Clock(Option<Instant>),
    Clip(EntityId),
}

#[derive(Clone, Copy, Debug)]
struct Clip {
    fps: f64,
    /// When its first picture is due.
    start: Instant,
    /// When it starts drawing, and stops.
    from: Instant,
    until: Option<Instant>,
    /// Drawn by the emoji's layer (an inline tile), not a view of its own.
    on_layer: bool,
    /// Due times are its pictures' own (FFmpeg), not the first vsync each
    /// is ready on (AVPlayer).
    own_times: bool,
}

impl Clip {
    fn picture_at(&self, at: Instant) -> u64 {
        let along = at.saturating_duration_since(self.start).as_secs_f64();
        (along * self.fps).floor() as u64
    }

    fn due(&self, picture: u64) -> Instant {
        self.start + Duration::from_secs_f64(picture as f64 / self.fps)
    }
}

/// How long after the display link's call a frame starts drawing.
const DRAW_START: Duration = Duration::from_micros(300);
const PAINTER: u64 = 1;
const CLIP_VIEW: u64 = 2;
const PLAYER: u64 = 7;

impl Sim {
    fn new(display: u64, rate: f64, grid: bool) -> Self {
        let t0 = Instant::now();
        let period = hz(rate);
        Self {
            display,
            t0,
            period,
            lead: period * 3 / 2,
            grid,
            late: Late::Little,
            now: t0,
            seed: 0x2545_f491_4f6c_dd1d,
            timers: Vec::new(),
            dirty: Vec::new(),
            next_frame: Vec::new(),
            clip_started: false,
            app_draws: vec![(t0, DRAW_START)],
            emoji_fps: 0,
            clip: None,
            clock: None,
            clock_starts: 0,
            clock_wakes: Vec::new(),
            demand: 0,
            targets: HashSet::new(),
            timing: None,
            layers: LayerDraws::default(),
            estimator: PresentEstimator::default(),
            requested: FrameRequested::default(),
            draws: Vec::new(),
            emoji_frames: Vec::new(),
            pictures: Vec::new(),
        }
    }

    /// Custom emoji at `fps` on the layer.
    fn emoji(mut self, fps: u32) -> Self {
        self.emoji_fps = fps;
        self
    }

    /// A clip at `fps` in a view of its own.
    fn clip(mut self, fps: f64) -> Self {
        self.clip = Some(Clip {
            fps,
            start: self.t0,
            from: self.t0,
            until: None,
            on_layer: false,
            own_times: false,
        });
        self
    }

    /// The clip drawn by the emoji's layer, its pictures `offset` after
    /// the emoji's frames.
    fn on_layer(mut self, offset: Duration) -> Self {
        if let Some(clip) = self.clip.as_mut() {
            clip.on_layer = true;
            clip.start = self.t0 + offset;
        }
        self
    }

    /// The clip plays from `from` on (until `until`), not from the start.
    fn playing(mut self, from: Duration, until: Option<Duration>) -> Self {
        let t0 = self.t0;
        if let Some(clip) = self.clip.as_mut() {
            clip.from = t0 + from;
            clip.until = until.map(|until| t0 + until);
        }
        self
    }

    /// The whole app redraws at `at`, starting `late` after the display
    /// link's call (a busy main thread).
    fn app_draw(mut self, at: Duration, late: Duration) -> Self {
        self.app_draws.push((self.t0 + at, late));
        self
    }

    fn own_times(mut self) -> Self {
        if let Some(clip) = self.clip.as_mut() {
            clip.own_times = true;
        }
        self
    }

    fn late(mut self, late: Late) -> Self {
        self.late = late;
        self
    }

    fn lateness(&mut self, wait: Duration) -> Duration {
        self.seed = self
            .seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let unit = ((self.seed >> 33) % 1000) as u32;
        let ms = Duration::from_millis(1);
        match self.late {
            Late::Little => ms * unit / 1000,
            Late::Realistic => wait / 10 * unit / 1000 + ms * 2 * ((unit * 7) % 1000) / 1000,
            Late::Always => wait / 10 + ms * 2,
        }
    }

    fn run(mut self, secs: f64) -> Self {
        let end = self.t0 + Duration::from_secs_f64(secs);
        let mut n = 0u32;
        loop {
            let call = self.t0 + self.period * n;
            if call >= end {
                break;
            }
            while let Some(index) = self
                .timers
                .iter()
                .enumerate()
                .filter(|(_, (at, _))| *at < call)
                .min_by_key(|(_, (at, _))| *at)
                .map(|(index, _)| index)
            {
                let (at, wake) = self.timers.swap_remove(index);
                self.now = at;
                match wake {
                    // The timer's `on_next_frame`.
                    Wake::Clip(target) => self.next_frame.push(target),
                    Wake::Clock(planned) => self.clock_woke(planned),
                }
            }
            self.frame(call);
            n += 1;
        }
        self
    }

    /// What the frame clock knows of the main window's display: its timing
    /// and the display link's latest present time.
    fn display(&self) -> Option<(ClockTiming, Vsync)> {
        let since = self.now.duration_since(self.t0).as_secs_f64();
        let frames = (since / self.period.as_secs_f64()).floor() as u32;
        let vsync = Vsync {
            at: self.t0 + self.period * frames + self.lead,
            period: self.period,
        };
        self.timing.map(|timing| (timing, vsync))
    }

    /// `request_tick`: content asked for a tick of `target` at `fps`.
    fn ask_tick(&mut self, target: EntityId, fps: u32) {
        self.demand = self.demand.max(fps);
        self.targets.insert(Some(target));
        if self.clock.is_none() {
            self.clock = Some(ClockRun::new());
            self.clock_starts += 1;
            self.plan_tick();
        }
    }

    fn plan_tick(&mut self) {
        let display = self.display();
        let Some(run) = self.clock.as_mut() else {
            return;
        };
        let (wait, vsync) = run.plan(display, self.now);
        let at = self.now + wait + self.lateness(wait);
        self.timers.push((at, Wake::Clock(vsync)));
    }

    /// A wake of `request_tick`'s loop.
    fn clock_woke(&mut self, planned: Option<Instant>) {
        let Some(mut run) = self.clock.take() else {
            return;
        };
        self.clock_wakes.push(self.now);
        let redrawn = self.layers.redrawn(self.targets.iter().copied());
        let fps = run.fps.max(self.demand);
        let woke = match later_tick(self.display(), self.now, fps, planned, redrawn) {
            Some(plan) => Woke::Later(plan),
            None => {
                let demand = std::mem::take(&mut self.demand);
                let targets = std::mem::take(&mut self.targets);
                if demand > 0 {
                    note_clock_playing(self.timing, self.now, demand);
                    for id in targets.into_iter().flatten() {
                        self.layers.ticked(id);
                        self.dirty.push(id);
                    }
                }
                Woke::Ticked(demand)
            }
        };
        if run.woke(planned, woke) {
            self.clock = Some(run);
            self.plan_tick();
        }
    }

    /// The display link's call at `call`: a window frame if a view is dirty.
    fn frame(&mut self, call: Instant) {
        self.now = call;
        for target in std::mem::take(&mut self.next_frame) {
            video_request_done(&self.requested, target);
            self.dirty.push(target);
        }
        let painter = EntityId::from(PAINTER);
        let view = EntityId::from(CLIP_VIEW);
        // A clip in a view of its own draws from when it starts.
        if !self.clip_started
            && self
                .clip
                .is_some_and(|clip| !clip.on_layer && clip.from <= call)
        {
            self.clip_started = true;
            self.dirty.push(view);
        }
        let app = self
            .app_draws
            .first()
            .copied()
            .filter(|(at, _)| *at <= call);
        if app.is_some() {
            self.app_draws.remove(0);
            // Every view redraws.
            if self.emoji_fps > 0 || self.clip.is_some_and(|clip| clip.on_layer) {
                self.dirty.push(painter);
            }
            if self.clip_started {
                self.dirty.push(view);
            }
        }
        if self.dirty.is_empty() {
            return;
        }
        let dirty = std::mem::take(&mut self.dirty);
        self.draws.push(call);
        self.now = call + app.map_or(DRAW_START, |(_, late)| late);
        let present = call + self.lead;
        if self.grid && app.is_some() {
            // `QuillApp::render`: `note_frame_timing`.
            let vsync = Vsync {
                at: present,
                period: self.period,
            };
            self.timing = ClockTiming::sample(self.timing, self.display, vsync, self.now);
        }
        if dirty.contains(&painter) {
            if self.clip.is_some_and(|clip| clip.on_layer) {
                self.draw_clip(painter, present);
            }
            if self.emoji_fps > 0 {
                // An emoji shows the frame its paint time reached.
                let emoji = Timeline {
                    frames: u32::MAX,
                    file_fps: f64::from(self.emoji_fps),
                    step: 1,
                };
                let along = self.now.duration_since(self.t0);
                let display_hz = 1.0 / self.period.as_secs_f64();
                let frame = pick(along, PlayMode::Loop, emoji, false, display_hz)
                    .map_or(0, |pick| u64::from(pick.index));
                self.emoji_frames.push((present, frame));
            }
            // `tick_animation_layer`.
            if let Some((_, vsync)) = self.display() {
                let counts = !clip_redraws_off_grid(painter, self.now);
                self.layers.drawn(painter, vsync.at, counts);
            }
            if self.emoji_fps > 0 {
                let fps = self.emoji_fps;
                // `QuillApp::layer_wants_tick`.
                if layer_redrawn_by_clip(self.display(), painter, fps, self.now) {
                    self.targets.remove(&Some(painter));
                    if self.targets.is_empty() {
                        self.demand = 0;
                    }
                } else {
                    self.ask_tick(painter, fps);
                }
            }
        }
        if dirty.contains(&view) {
            self.draw_clip(view, present);
        }
    }

    /// `VideoFrame`: pull the picture for the present time, then ask for
    /// the frame of the next one.
    fn draw_clip(&mut self, target: EntityId, present: Instant) {
        let Some(clip) = self.clip else {
            return;
        };
        let now = self.now;
        if now < clip.from || clip.until.is_some_and(|until| now >= until) {
            return;
        }
        let source = Source::Clip(PLAYER);
        // `present_hint` and `set_content_grid`.
        let vsync = Vsync {
            at: present,
            period: self.period,
        };
        let grid = self
            .grid
            .then(|| display_grid(self.display, vsync, now))
            .flatten();
        let shares = grid.is_some() && shared(self.display, source, now);
        self.estimator.set_period(Some(self.period));
        self.estimator.set_grid(grid, shares);
        // `pull`.
        self.estimator.record_draw(now);
        self.estimator.set_hint(Some(present));
        let shown = self.estimator.predict(now);
        self.estimator.note_present(shown);
        let picture = clip.picture_at(shown);
        self.pictures.push((shown, picture));
        // `frame_demand`: the backend's next due time, on the grid.
        let interval = self.estimator.interval();
        let due = if clip.own_times {
            clip.due(picture + 1)
        } else {
            // AVPlayer: the first vsync after this one with a new picture.
            (1..=8u32)
                .map(|frames| shown + interval * frames)
                .find(|at| clip.picture_at(*at) > picture)
                .unwrap_or(shown + interval * 8)
        };
        let due = self.estimator.on_grid(due);
        // The pump.
        if grid.is_some() {
            note_playing(self.display, source, now, due);
        }
        let lead = self.estimator.request_lead();
        let on_grid = self.estimator.draws_on_grid();
        let ask = video_request(
            &self.requested,
            target,
            now,
            Some(due),
            interval,
            lead,
            on_grid,
        );
        if let Some(request) = ask {
            match request {
                FrameRequest::NextFrame => self.next_frame.push(target),
                FrameRequest::SleepUntil(wake) => {
                    let wait = wake.saturating_duration_since(now);
                    let at = now + wait + self.lateness(wait);
                    self.timers.push((at, Wake::Clip(target)));
                }
            }
        }
    }

    fn warm(&self) -> Instant {
        self.t0 + Duration::from_millis(500)
    }

    /// Window frames drawn in the second after warming up for half of one.
    fn draws_in_a_second(&self) -> usize {
        let to = self.warm() + Duration::from_secs(1);
        let warm = self.warm();
        self.draws
            .iter()
            .filter(|at| **at >= warm && **at < to)
            .count()
    }

    /// Frame clock wakes in the second after warming up for half of one.
    fn clock_wakes_in_a_second(&self) -> usize {
        let (warm, to) = (self.warm(), self.warm() + Duration::from_secs(1));
        self.clock_wakes
            .iter()
            .filter(|at| **at >= warm && **at < to)
            .count()
    }

    /// The emoji's frames from each draw to the next, after warming up.
    fn emoji_steps(&self) -> Vec<u64> {
        let frames: Vec<u64> = self
            .emoji_frames
            .iter()
            .filter(|(at, _)| *at >= self.warm())
            .map(|(_, frame)| *frame)
            .collect();
        frames
            .windows(2)
            .filter_map(|pair| match pair {
                [a, b] => Some(b - a),
                _ => None,
            })
            .collect()
    }

    /// How long each emoji frame that came up after warming up stayed on
    /// screen, in display frames.
    fn emoji_holds(&self) -> Vec<u32> {
        let mut firsts: Vec<Instant> = Vec::new();
        let mut last = None;
        for (at, frame) in &self.emoji_frames {
            if last.is_some_and(|last| last != *frame) && *at >= self.warm() {
                firsts.push(*at);
            }
            last = Some(*frame);
        }
        let period = self.period.as_secs_f64();
        firsts
            .windows(2)
            .filter_map(|pair| match pair {
                [a, b] => Some((b.duration_since(*a).as_secs_f64() / period).round() as u32),
                _ => None,
            })
            .collect()
    }

    /// The emoji drew each of its frames once, in order.
    fn emoji_plays_every_frame_once(&self) -> bool {
        let steps = self.emoji_steps();
        !steps.is_empty() && steps.iter().all(|step| *step == 1)
    }

    /// Every picture of the clip showed, within `late` of its own time.
    fn clip_shows_every_picture_within(&self, late: Duration) -> bool {
        let Some(clip) = self.clip else {
            return false;
        };
        let shown: Vec<&(Instant, u64)> = self
            .pictures
            .iter()
            .filter(|(at, _)| *at >= self.warm())
            .collect();
        let (Some(first), Some(last)) = (shown.first(), shown.last()) else {
            return false;
        };
        (first.1 + 1..=last.1).all(|picture| {
            let due = clip.due(picture);
            shown
                .iter()
                .find(|(_, shown)| *shown == picture)
                .is_some_and(|(at, _)| at.saturating_duration_since(due) <= late)
        })
    }

    /// How long each picture stayed on screen, in display frames, after
    /// warming up.
    fn holds(&self) -> Vec<u32> {
        let mut firsts: Vec<Instant> = Vec::new();
        let mut last = None;
        for (at, picture) in self.pictures.iter().filter(|(at, _)| *at >= self.warm()) {
            if last != Some(*picture) {
                firsts.push(*at);
                last = Some(*picture);
            }
        }
        let period = self.period.as_secs_f64();
        firsts
            .windows(2)
            .filter_map(|pair| match pair {
                [a, b] => Some((b.duration_since(*a).as_secs_f64() / period).round() as u32),
                _ => None,
            })
            .collect()
    }

    /// Every window frame and every picture shown, as times since `t0`.
    fn timeline(&self) -> (Vec<Duration>, Vec<(Duration, u64)>) {
        let draws = self.draws.iter().map(|at| at.duration_since(self.t0));
        let pictures = self
            .pictures
            .iter()
            .map(|(at, picture)| (at.duration_since(self.t0), *picture));
        (draws.collect(), pictures.collect())
    }
}

/// A picture shows on the first vsync after its time, or on the grid a
/// display frame later.
fn on_time(sim: &Sim) -> Duration {
    sim.period * 2 + Duration::from_millis(1)
}

#[test]
fn the_clock_and_a_clip_share_window_frames() {
    // 30 fps emoji and a 30 fps clip in a view of its own on a 120 Hz
    // display: before the grid, about 50 window frames a second.
    let sim = Sim::new(1, 120.0, true).emoji(30).clip(30.0).run(1.6);
    let draws = sim.draws_in_a_second();
    assert!(draws <= 31, "{draws} window frames in a second");
    assert!(sim.emoji_plays_every_frame_once(), "{:?}", sim.emoji_frames);
    assert!(sim.clip_shows_every_picture_within(on_time(&sim)));
    assert!(
        sim.holds().iter().all(|hold| *hold == 4),
        "{:?}",
        sim.holds()
    );

    // A 60 fps sticker and a 30 fps clip: before, about 76.
    let sim = Sim::new(2, 120.0, true).emoji(60).clip(30.0).run(1.6);
    let draws = sim.draws_in_a_second();
    assert!(draws <= 60, "{draws} window frames in a second");
    assert!(sim.emoji_plays_every_frame_once());
    assert!(sim.clip_shows_every_picture_within(on_time(&sim)));
}

#[test]
fn emoji_on_a_clips_layer_draw_with_its_pictures() {
    // The measured chat: 30 fps custom emoji and a 30 fps clip on the
    // conversation's animation layer, the clip's pictures a grid tick and
    // a bit after the emoji's frames. Each picture redraws the layer with
    // its emoji, so the emoji's own tick waits for the next picture
    // instead of drawing the window on a grid vsync of its own.
    let sim = Sim::new(3, 120.0, true)
        .emoji(30)
        .clip(30.0)
        .on_layer(Duration::from_millis(20))
        .run(1.6);
    let draws = sim.draws_in_a_second();
    assert!(draws <= 31, "{draws} window frames in a second");
    assert!(sim.emoji_plays_every_frame_once(), "{:?}", sim.emoji_frames);
    assert!(sim.clip_shows_every_picture_within(on_time(&sim)));
}

#[test]
fn late_wakes_skip_no_emoji_frame() {
    for late in [Late::Realistic, Late::Always] {
        let setups = [
            Sim::new(4, 120.0, true).emoji(30).clip(30.0),
            Sim::new(5, 120.0, true)
                .emoji(30)
                .clip(30.0)
                .on_layer(Duration::from_millis(20)),
            Sim::new(6, 120.0, true).emoji(60).clip(30.0),
        ];
        for sim in setups {
            let sim = sim.late(late).run(3.0);
            let steps = sim.emoji_steps();
            assert!(
                !steps.is_empty() && steps.iter().all(|step| *step <= 1),
                "{late:?}, {} fps: {steps:?}",
                sim.emoji_fps
            );
            // A late wake may draw a picture a display frame later.
            let late_by = on_time(&sim) + sim.period;
            assert!(sim.clip_shows_every_picture_within(late_by));
            assert!(sim.draws_in_a_second() <= 60);
        }
    }
}

#[test]
fn a_lone_clip_keeps_its_own_cadence() {
    // 24 fps at 120 Hz: every picture held five display frames.
    let sim = Sim::new(7, 120.0, true).clip(24.0).run(2.0);
    let holds = sim.holds();
    assert!(
        holds.len() > 20 && holds.iter().all(|hold| *hold == 5),
        "{holds:?}"
    );
    // Any rate, alone: exactly as before.
    for fps in [24.0, 25.0, 30.0, 50.0] {
        for late in [Late::Little, Late::Realistic] {
            let before = Sim::new(8, 120.0, false).clip(fps).late(late).run(1.6);
            let after = Sim::new(9, 120.0, true).clip(fps).late(late).run(1.6);
            assert_eq!(after.timeline(), before.timeline(), "{fps} fps, {late:?}");
        }
    }
}

#[test]
fn a_clip_the_grid_would_hold_unevenly_keeps_its_vsyncs_beside_emoji() {
    // A 24 fps clip beside 30 fps emoji: still five display frames each,
    // not two and three grid ticks.
    let sim = Sim::new(10, 120.0, true).emoji(30).clip(24.0).run(2.0);
    let holds = sim.holds();
    assert!(
        holds.len() > 20 && holds.iter().all(|hold| *hold == 5),
        "{holds:?}"
    );
    assert!(sim.emoji_plays_every_frame_once());
    // 25 fps: four and five display frames, as alone.
    let sim = Sim::new(11, 120.0, true).emoji(30).clip(25.0).run(2.0);
    assert!(sim.holds().iter().all(|hold| *hold == 4 || *hold == 5));
}

#[test]
fn a_clip_off_the_grid_on_the_emojis_layer_never_puts_their_tick_off() {
    let mut display = 160;
    for fps in [24.0, 25.0] {
        for offset in [0, 5, 10, 20] {
            display += 1;
            let sim = Sim::new(display, 120.0, true)
                .emoji(30)
                .clip(fps)
                .on_layer(Duration::from_millis(offset))
                .run(2.0);
            let holds = sim.emoji_holds();
            let case = format!("{fps} fps, {offset} ms");
            assert!(
                holds.len() > 20 && holds.iter().all(|hold| *hold == 4),
                "{case}: {holds:?}"
            );
            let draws = sim.draws_in_a_second();
            assert!(draws <= 56, "{case}: {draws} window frames in a second");
        }
    }
}

#[test]
fn one_animation_alone_draws_exactly_as_before() {
    for fps in [24, 25, 30, 60] {
        for late in [Late::Little, Late::Realistic] {
            let before = Sim::new(12, 120.0, false).emoji(fps).late(late).run(1.6);
            let after = Sim::new(13, 120.0, true).emoji(fps).late(late).run(1.6);
            assert_eq!(after.timeline(), before.timeline(), "{fps} fps, {late:?}");
        }
    }
}

#[test]
fn a_60_hz_display_draws_as_before() {
    for (emoji, clip) in [(30, 30.0), (60, 25.0)] {
        let before = Sim::new(14, 60.0, false).emoji(emoji).clip(clip).run(1.6);
        let after = Sim::new(15, 60.0, true).emoji(emoji).clip(clip).run(1.6);
        assert_eq!(after.timeline(), before.timeline(), "{emoji} + {clip} fps");
    }
}

#[test]
fn pictures_due_just_after_a_grid_vsync_draw_once() {
    // FFmpeg asks for each picture at its own time, here a millisecond
    // after a grid vsync: drawn on the next grid vsync, once, not on the
    // one before (not ready yet) and again a display frame later.
    let sim = Sim::new(16, 120.0, true)
        .emoji(30)
        .clip(30.0)
        // The window's vsyncs (and the grid's) are `lead` after `t0`.
        .on_layer(hz(120.0) * 3 / 2 + Duration::from_millis(1))
        .own_times()
        .run(1.6);
    let draws = sim.draws_in_a_second();
    assert!(draws <= 31, "{draws} window frames in a second");
    assert!(sim.clip_shows_every_picture_within(hz(60.0) + Duration::from_millis(1)));
    assert!(sim.emoji_plays_every_frame_once());
}

#[test]
fn emoji_skip_no_frame_as_a_clip_starts_and_stops_beside_them() {
    // Emoji animating alone tick off the grid; a clip starting beside them
    // moves their ticks onto it, and one stopping moves them off again.
    // Neither may hold an emoji frame too long, nor stop the clock (which
    // restarts at 30 fps, holding a 60 fps sticker's frame too long).
    let mut display = 100;
    for emoji in [60, 30] {
        for on_layer in [false, true] {
            // Started at any phase of the emoji's ticks and the vsyncs.
            for step in 0..12u32 {
                display += 1;
                let from = Duration::from_millis(600) + Duration::from_millis(23) * step;
                let mut sim = Sim::new(display, 120.0, true).emoji(emoji).clip(30.0);
                if on_layer {
                    sim = sim.on_layer(Duration::ZERO);
                }
                let sim = sim
                    .playing(from, Some(Duration::from_millis(1200)))
                    .run(1.6);
                let steps = sim.emoji_steps();
                let case = format!("{emoji} fps, on layer: {on_layer}, from {from:?}");
                assert!(steps.iter().all(|step| *step <= 1), "{case}: {steps:?}");
                assert_eq!(sim.clock_starts, 1, "{case}");
                assert!(sim.clip_shows_every_picture_within(on_time(&sim)), "{case}");
            }
        }
    }
}

#[test]
fn a_frame_that_starts_late_keeps_ticks_on_the_grid() {
    // A whole-app redraw starting most of a display frame after the display
    // link's call (a busy main thread) makes frames look like they start
    // later than they do. Ticks planned on that alone would miss their
    // vsyncs from then on, each drawing a window frame after the clip's.
    for (display, emoji, most) in [(130, 30, 31), (131, 60, 60)] {
        let sim = Sim::new(display, 120.0, true)
            .emoji(emoji)
            .clip(30.0)
            .app_draw(Duration::from_millis(300), hz(120.0) * 9 / 10)
            .run(1.6);
        let draws = sim.draws_in_a_second();
        assert!(
            draws <= most,
            "{emoji} fps: {draws} window frames in a second"
        );
        assert!(sim.emoji_plays_every_frame_once(), "{emoji} fps");
        assert!(sim.clip_shows_every_picture_within(on_time(&sim)));
    }
}

#[test]
fn a_clip_faster_than_the_emoji_on_its_layer_leaves_the_clock_idle() {
    // A 60 fps clip on the layer of 30 fps emoji redraws them every other
    // vsync: the clock has nothing to tick, so it stops instead of waking
    // about 60 times a second to put its tick off again.
    let mut display = 140;
    for offset in [0, 5, 10] {
        for late in [Late::Little, Late::Realistic] {
            display += 1;
            let sim = Sim::new(display, 120.0, true)
                .emoji(30)
                .clip(60.0)
                .on_layer(Duration::from_millis(offset))
                .late(late)
                .run(1.6);
            let wakes = sim.clock_wakes_in_a_second();
            assert!(wakes <= 1, "{offset} ms, {late:?}: {wakes} clock wakes");
            let steps = sim.emoji_steps();
            assert!(steps.iter().all(|step| *step <= 1), "{steps:?}");
            assert!(sim.clip_shows_every_picture_within(on_time(&sim)));
        }
    }
    // Once the clip stops, the clock ticks the emoji again, none skipped.
    let stop = Duration::from_millis(1000);
    let sim = Sim::new(150, 120.0, true)
        .emoji(30)
        .clip(60.0)
        .on_layer(Duration::ZERO)
        .playing(Duration::ZERO, Some(stop))
        .run(1.6);
    let steps = sim.emoji_steps();
    assert!(steps.iter().all(|step| *step <= 1), "{steps:?}");
    let after = sim
        .emoji_frames
        .iter()
        .filter(|(at, _)| *at > sim.t0 + stop + sim.lead)
        .count();
    assert!(
        after >= 17,
        "{after} emoji draws in the 0.6 s after the clip"
    );
}
