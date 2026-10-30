use super::super::frame_clock::{ClockTiming, later_tick, tick_wait};
use super::super::video_player::pacing::{FrameRequest, PresentEstimator, request_policy};
use super::{Cadence, ContentGrid, Vsync, clock_tick, display_grid};
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
fn a_due_time_snaps_to_the_next_grid_vsync() {
    let t0 = Instant::now();
    let period = hz(120.0);
    let grid = ContentGrid::new(Vsync { at: t0, period }).unwrap();
    // On the grid (give or take noise): stays.
    assert!(close(grid.snap(t0), t0));
    assert!(close(grid.snap(t0 + period * 2), t0 + period * 2));
    assert!(close(
        grid.snap(t0 + period * 2 + Duration::from_micros(300)),
        t0 + period * 2
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
fn content_ticks_as_many_grid_vsyncs_as_fit_in_its_frame() {
    let at = Instant::now();
    for (rate, cases) in [
        (
            120.0,
            [
                (60, Some(1)),
                (30, Some(2)),
                (25, Some(2)),
                (12, Some(5)),
                (120, None),
            ],
        ),
        (
            119.88,
            [
                (60, Some(1)),
                (30, Some(2)),
                (25, Some(2)),
                (12, Some(5)),
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
fn a_clip_faster_than_the_grid_keeps_its_own_vsyncs() {
    let t0 = Instant::now();
    let period = hz(120.0);
    let grid = ContentGrid::new(Vsync { at: t0, period }).unwrap();
    // 60 fps pictures due on odd vsyncs: each a vsync later, on the grid.
    let mut sixty = Cadence::default();
    for k in 0..4u32 {
        let due = t0 + period * (2 * k + 1);
        sixty.note(due);
        assert!(close(sixty.on_grid(due, Some(&grid)), due + period));
    }
    // 120 fps pictures: every vsync, as due.
    let mut fast = Cadence::default();
    fast.note(t0 + period);
    for k in 2..5u32 {
        let due = t0 + period * k;
        fast.note(due);
        assert!(close(fast.on_grid(due, Some(&grid)), due));
    }
    // No grid (60 Hz, no vsync times): as due.
    assert!(close(sixty.on_grid(t0 + period, None), t0 + period));
}

#[test]
fn the_clock_wakes_half_a_vsync_before_its_frame_starts() {
    let t0 = Instant::now();
    let period = hz(120.0);
    let lead = period * 3 / 2;
    let grid = ContentGrid::new(Vsync { at: t0, period }).unwrap();
    // 30 fps after a tick on `t0`: two grid ticks on.
    let now = before(t0, lead) + Duration::from_millis(1);
    let (wake, at) = clock_tick(&grid, lead, t0, now, 30).unwrap();
    assert!(close(at, t0 + period * 4));
    assert!(close(wake, before(at, lead + period / 2)));
    // Woken late, past that frame's start: the next grid vsync, at once.
    let late = before(at, lead) + Duration::from_micros(200);
    let (wake, next) = clock_tick(&grid, lead, t0, late, 30).unwrap();
    assert!(close(next, at + period * 2));
    assert!(wake >= late);
    // Faster than the grid: not on it.
    assert_eq!(clock_tick(&grid, lead, t0, now, 120), None);
}

/// A window on a display refreshing every `period`, whose frames are shown
/// `lead` after the display link asks for them, with two animated sources:
/// custom emoji (or a sticker) on an animation layer the frame clock ticks
/// at `sticker_fps`, and an inline clip at `clip_fps`, on the same layer or
/// a view of its own. It runs the decisions the app makes
/// (`frame_clock::tick_wait` and `later_tick`, `PresentEstimator`,
/// `pacing::request_policy`, AVPlayer's `next_due` probing) on a simulated
/// clock: timers fire up to a millisecond late, and a notified view draws
/// on the next display frame.
struct Sim {
    display: u64,
    t0: Instant,
    period: Duration,
    lead: Duration,
    /// The content grid is in use (else: what Quill did before it).
    grid: bool,
    now: Instant,
    timers: Vec<(Instant, Wake)>,
    seed: u64,
    /// A video `on_next_frame` callback is pending.
    next_frame: bool,
    sticker_dirty: bool,
    clip_dirty: bool,
    sticker_fps: u32,
    clip_fps: f64,
    /// When the clip's first picture is due.
    clip_start: Instant,
    /// The clip is on the emoji's layer: drawing either draws both.
    same_layer: bool,
    /// The running clock's rate and the vsync its last tick drew on.
    clock: Option<(u32, Option<Instant>)>,
    demand: u32,
    timing: Option<ClockTiming>,
    /// The vsync the emoji's layer last drew on (`note_layer_drawn`).
    layer_drawn: Option<Instant>,
    estimator: PresentEstimator,
    requested: Option<Instant>,
    /// When each window frame started drawing.
    draws: Vec<Instant>,
    /// The sticker's frame at each of its draws.
    sticker_frames: Vec<(Instant, u64)>,
    /// The clip's picture at each of its draws, with its present time.
    pictures: Vec<(Instant, u64)>,
}

#[derive(Clone, Copy, Debug)]
enum Wake {
    Clock(Option<Instant>),
    Clip,
}

/// How long after the display link's call a frame starts drawing.
const DRAW_START: Duration = Duration::from_micros(300);

impl Sim {
    fn new(display: u64, rate: f64, grid: bool, sticker_fps: u32, clip_fps: f64) -> Self {
        let t0 = Instant::now();
        let period = hz(rate);
        Self {
            display,
            t0,
            period,
            lead: period * 3 / 2,
            grid,
            now: t0,
            timers: Vec::new(),
            seed: 0x2545_f491_4f6c_dd1d,
            next_frame: false,
            sticker_dirty: sticker_fps > 0,
            clip_dirty: clip_fps > 0.0,
            sticker_fps,
            clip_fps,
            clip_start: t0,
            same_layer: false,
            clock: None,
            demand: 0,
            timing: None,
            layer_drawn: None,
            estimator: PresentEstimator::default(),
            requested: None,
            draws: Vec::new(),
            sticker_frames: Vec::new(),
            pictures: Vec::new(),
        }
    }

    /// The clip on the emoji's layer, its pictures `offset` after theirs.
    fn on_one_layer(mut self, offset: Duration) -> Self {
        self.same_layer = true;
        self.clip_start = self.t0 + offset;
        self
    }

    /// A timer's lateness: up to a millisecond.
    fn jitter(&mut self) -> Duration {
        self.seed = self
            .seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        Duration::from_micros((self.seed >> 33) % 1000)
    }

    fn run(&mut self, secs: f64) {
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
                self.fire(wake);
            }
            self.frame(call);
            n += 1;
        }
    }

    /// The display link's latest present time: the frame it last asked for.
    fn latest_vsync(&self) -> Vsync {
        let since = self.now.duration_since(self.t0).as_secs_f64();
        let frames = (since / self.period.as_secs_f64()).floor() as u32;
        Vsync {
            at: self.t0 + self.period * frames + self.lead,
            period: self.period,
        }
    }

    fn display(&self) -> Option<(ClockTiming, Vsync)> {
        self.timing
            .filter(|_| self.grid)
            .map(|timing| (timing, self.latest_vsync()))
    }

    fn fire(&mut self, wake: Wake) {
        match wake {
            Wake::Clip => self.next_frame = true,
            // `request_tick`'s loop: a tick redraws what asked since the
            // last one, and the clock stops after a tick nothing asked for;
            // a tick whose layer a clip redrew since waits (`later_tick`).
            Wake::Clock(planned) => {
                let Some((fps, _)) = self.clock else {
                    return;
                };
                if self.demand > 0
                    && let (Some(planned), Some(drawn)) = (planned, self.layer_drawn)
                    && let Some((wait, next)) = later_tick(
                        self.display(),
                        self.now,
                        fps.max(self.demand),
                        planned,
                        drawn,
                    )
                {
                    let at = self.now + wait + self.jitter();
                    self.timers.push((at, Wake::Clock(next)));
                    return;
                }
                let demand = std::mem::take(&mut self.demand);
                if demand == 0 {
                    self.clock = None;
                    return;
                }
                self.sticker_dirty = true;
                self.clock = Some((demand, planned));
                self.schedule_tick();
            }
        }
    }

    fn schedule_tick(&mut self) {
        let Some((fps, last)) = self.clock else {
            return;
        };
        let (wait, vsync) = tick_wait(self.display(), self.now, fps, last);
        let at = self.now + wait + self.jitter();
        self.timers.push((at, Wake::Clock(vsync)));
    }

    fn frame(&mut self, call: Instant) {
        self.now = call;
        if std::mem::take(&mut self.next_frame) {
            self.requested = None;
            self.clip_dirty = true;
        }
        if self.same_layer && (self.sticker_dirty || self.clip_dirty) {
            self.sticker_dirty = self.sticker_fps > 0;
            self.clip_dirty = self.clip_fps > 0.0;
        }
        if !(self.sticker_dirty || self.clip_dirty) {
            return;
        }
        self.draws.push(call);
        self.now = call + DRAW_START;
        let present = call + self.lead;
        // `QuillApp::render`: `note_frame_timing`.
        self.timing = Some(ClockTiming {
            display: self.display,
            lead: (present - self.now).clamp(self.period / 2, self.period * 3),
        });
        if std::mem::take(&mut self.clip_dirty) {
            self.draw_clip(present);
        }
        if std::mem::take(&mut self.sticker_dirty) {
            // A sticker head shows the frame its paint time reached.
            let along = self.now.duration_since(self.t0).as_secs_f64();
            let frame = (along * f64::from(self.sticker_fps) + 0.25).floor() as u64;
            self.sticker_frames.push((present, frame));
            // `tick_animation_layer`.
            self.layer_drawn = Some(present);
            self.demand = self.demand.max(self.sticker_fps);
            if self.clock.is_none() {
                self.clock = Some((30, None));
                self.schedule_tick();
            }
        }
    }

    fn picture_at(&self, at: Instant) -> u64 {
        let along = at.saturating_duration_since(self.clip_start).as_secs_f64();
        (along * self.clip_fps).floor() as u64
    }

    fn picture_due(&self, picture: u64) -> Instant {
        self.clip_start + Duration::from_secs_f64(picture as f64 / self.clip_fps)
    }

    /// `video_frame`: pull the picture for the present time, then ask for
    /// the frame of the next one (`request_video_frame`).
    fn draw_clip(&mut self, hint: Instant) {
        let now = self.now;
        let grid = self
            .grid
            .then(|| {
                display_grid(
                    self.display,
                    Vsync {
                        at: hint,
                        period: self.period,
                    },
                    now,
                )
            })
            .flatten();
        self.estimator.set_period(Some(self.period));
        self.estimator.set_grid(grid);
        self.estimator.record_draw(now);
        self.estimator.set_hint(Some(hint));
        let present = self.estimator.predict(now);
        self.estimator.note_present(present);
        let picture = self.picture_at(present);
        self.pictures.push((present, picture));
        // AVPlayer: the first vsync after the last pull with a new picture.
        let interval = self.estimator.interval();
        let due = (1..=8u32)
            .map(|frames| present + interval * frames)
            .find(|at| self.picture_at(*at) > picture)
            .unwrap_or(present + interval * 8);
        let due = self.estimator.on_grid(due);
        if self.requested.is_some_and(|lost| now < lost) {
            return;
        }
        match request_policy(now, Some(due), interval, self.estimator.lead()) {
            FrameRequest::NextFrame => {
                self.requested = Some(now + Duration::from_millis(250));
                self.next_frame = true;
            }
            FrameRequest::SleepUntil(wake) => {
                let sleep = wake.saturating_duration_since(now);
                self.requested =
                    Some(now + (sleep + Duration::from_millis(50)).max(Duration::from_millis(250)));
                let at = wake + self.jitter();
                self.timers.push((at, Wake::Clip));
            }
        }
    }

    /// Window frames drawn in the second after warming up for half of one.
    fn draws_in_a_second(&self) -> usize {
        let from = self.t0 + Duration::from_millis(500);
        let to = from + Duration::from_secs(1);
        self.draws
            .iter()
            .filter(|at| **at >= from && **at < to)
            .count()
    }

    /// The sticker drew each of its frames once, in order, after warming up.
    fn sticker_plays_every_frame(&self) -> bool {
        let from = self.t0 + Duration::from_millis(500);
        let frames: Vec<u64> = self
            .sticker_frames
            .iter()
            .filter(|(at, _)| *at >= from)
            .map(|(_, frame)| *frame)
            .collect();
        !frames.is_empty()
            && frames
                .windows(2)
                .all(|pair| matches!(pair, [a, b] if *b == a + 1))
    }

    /// Every picture of the clip showed, within `late` of its own time.
    fn clip_shows_every_picture_within(&self, late: Duration) -> bool {
        let from = self.t0 + Duration::from_millis(500);
        let shown: Vec<&(Instant, u64)> =
            self.pictures.iter().filter(|(at, _)| *at >= from).collect();
        let (Some(first), Some(last)) = (shown.first(), shown.last()) else {
            return false;
        };
        (first.1 + 1..=last.1).all(|picture| {
            let due = self.picture_due(picture);
            shown
                .iter()
                .find(|(_, shown)| *shown == picture)
                .is_some_and(|(at, _)| at.saturating_duration_since(due) <= late)
        })
    }
}

#[test]
fn two_animations_at_different_rates_share_60_window_frames_a_second() {
    // A 60 fps sticker and a 50 fps clip on a 120 Hz display: before the
    // grid, the clock's ticks and the clip's pictures landed on different
    // vsyncs and the window drew their union (about 90 frames a second).
    let mut sim = Sim::new(1, 120.0, true, 60, 50.0);
    sim.run(1.6);
    let draws = sim.draws_in_a_second();
    assert!(draws <= 60, "{draws} window frames in a second");
    assert!(sim.sticker_plays_every_frame(), "{:?}", sim.sticker_frames);
    // Each picture on time, or one grid tick (two vsyncs) late at most.
    let tick = sim.period * 2 + Duration::from_millis(1);
    assert!(
        sim.clip_shows_every_picture_within(tick),
        "{:?}",
        sim.pictures
    );

    // A 30 fps clip and 30 fps custom emoji (the measured chat).
    let mut sim = Sim::new(2, 120.0, true, 30, 30.0);
    sim.run(1.6);
    let draws = sim.draws_in_a_second();
    assert!(draws <= 60, "{draws} window frames in a second");
    assert!(sim.sticker_plays_every_frame());
    assert!(sim.clip_shows_every_picture_within(tick));
}

#[test]
fn emoji_on_a_clips_layer_draw_with_its_pictures() {
    // The measured chat: 30 fps custom emoji and a 30 fps clip on the
    // conversation's animation layer, the clip's pictures a grid tick and
    // a bit after the emoji's frames. Each picture redraws the layer with
    // its emoji, so the emoji's own tick waits for the next picture
    // instead of drawing the window on a grid vsync of its own.
    let mut sim = Sim::new(30, 120.0, true, 30, 30.0).on_one_layer(Duration::from_millis(20));
    sim.run(1.6);
    let draws = sim.draws_in_a_second();
    assert!(draws <= 31, "{draws} window frames in a second");
    assert!(sim.sticker_plays_every_frame(), "{:?}", sim.sticker_frames);
    let tick = sim.period * 2 + Duration::from_millis(1);
    assert!(
        sim.clip_shows_every_picture_within(tick),
        "{:?}",
        sim.pictures
    );
}

#[test]
fn one_animation_alone_draws_as_often_as_before() {
    for (sticker, clip) in [(0, 50.0), (0, 30.0), (0, 25.0), (30, 0.0)] {
        let mut before = Sim::new(10, 120.0, false, sticker, clip);
        before.run(1.6);
        let mut after = Sim::new(11, 120.0, true, sticker, clip);
        after.run(1.6);
        // (The clock's timer drifted later by its own lateness before.)
        let (before, after) = (before.draws_in_a_second(), after.draws_in_a_second());
        assert!(
            after.abs_diff(before) <= 1,
            "{sticker} fps sticker, {clip} fps clip: {before} -> {after}"
        );
    }
}

#[test]
fn a_60_hz_display_draws_as_before() {
    let mut before = Sim::new(20, 60.0, false, 30, 50.0);
    before.run(1.6);
    let mut after = Sim::new(21, 60.0, true, 30, 50.0);
    after.run(1.6);
    let since = |sim: &Sim| -> Vec<Duration> {
        sim.draws
            .iter()
            .map(|at| at.duration_since(sim.t0))
            .collect()
    };
    assert_eq!(since(&after), since(&before));
    let pictures = |sim: &Sim| -> Vec<(Duration, u64)> {
        let t0 = sim.t0;
        sim.pictures
            .iter()
            .map(|(at, picture)| (at.duration_since(t0), *picture))
            .collect()
    };
    assert_eq!(pictures(&after), pictures(&before));
}
