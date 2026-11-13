//! "Swipe to the next unread channel" (pure core, no GPUI).
//!
//! Telegram Desktop's `HistoryView::PullToNextChannel`
//! (`history/history_view_pull_to_next_channel.cpp`) on top of lib_ui's
//! `Ui::ElasticScroll` bottom edge. In a channel scrolled to its newest
//! message, a trackpad scroll past the bottom stretches the edge: a capsule
//! with an arrow grows under the last message. Held past the threshold for
//! a moment, it turns into the next unread channel's photo and name;
//! letting go there opens that channel. In a forum topic the same gesture
//! goes to the next unread topic.
//!
//! [`PullMachine`] is the edge and the controller together. Time is passed
//! in as milliseconds, so every step is deterministic. The UI feeds it
//! precise wheel events and frames, reads [`PullMachine::indicator`] and
//! [`PullMachine::hint`], and does what the returned [`Step`] asks.
#![deny(clippy::indexing_slicing)]

// Metrics (`chat_helpers/chat_helpers.style`, `historyPullNext*`).

/// `historyPullNextThreshold`.
pub const THRESHOLD: f32 = 48.;
/// `historyPullNextExpand`: the card the indicator grows to once ready.
pub const EXPAND: f32 = 110.;
/// `historyPullNextMaxHeight`: the indicator never gets taller.
pub const MAX_HEIGHT: f32 = 150.;
/// `historyPullNextAvatar`.
pub const AVATAR: f32 = 56.;
/// `historyPullNextSkip`.
pub const SKIP: f32 = 8.;
/// `historyPullNextArrowStroke`.
pub const ARROW_STROKE: f32 = 2.;
/// `historyPullNextBadge`.
pub const BADGE: f32 = 20.;
/// `historyPullNextBadgeRing`.
pub const BADGE_RING: f32 = 2.;
/// `historyPullNextInset`.
pub const INSET: f32 = 16.;
/// `historyPullNextPadding`.
pub const PADDING: f32 = 4.;
/// `historyPullNextArrowSize`.
pub const ARROW_SIZE: f32 = 24.;
/// `historyPullNextArrowRaise`.
pub const ARROW_RAISE: f32 = 20.;
/// `historyPullNextRise`.
pub const RISE: f32 = 35.;
/// `historyPullNextNameTop`.
pub const NAME_TOP: f32 = 20.;
/// `historyPullNextNameRadius`.
pub const NAME_RADIUS: f32 = 15.;
/// `historyPullNextHintSlide`.
pub const HINT_SLIDE: f32 = 10.;
/// `historyPullNextBounce`.
pub const BOUNCE: f32 = 4.;
/// `historyPullNextBadgeFont`, `historyPullNextNameFont`,
/// `historyPullNextHintFont` sizes.
pub const BADGE_FONT: f32 = 12.;
pub const NAME_FONT: f32 = 13.;
pub const HINT_FONT: f32 = 14.;

/// `PullThreshold()`: the soft pull stretches about twice as easily as the
/// old log curve, so the threshold doubles.
pub const PULL_THRESHOLD: f32 = THRESHOLD * 2.;

const RESET_REACHED_ON: f32 = 0.95;
const REVERSE_EASE: f32 = 1.5;
/// `kReadyDwell`: how long the pull must stay past the threshold.
pub const READY_DWELL_MS: u64 = 120;

thread_local! {
    static DWELL_OVERRIDE: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) };
}

/// Hold time before a pull is ready. UI tests on a loaded machine can't
/// rely on real time staying under 120 ms, so they may stretch it for
/// their own thread.
pub fn ready_dwell_ms() -> u64 {
    DWELL_OVERRIDE.with(|d| d.get()).unwrap_or(READY_DWELL_MS)
}

/// Test hook: set this thread's dwell (`None` restores the default).
pub fn set_ready_dwell_for_tests(ms: Option<u64>) {
    DWELL_OVERRIDE.with(|d| d.set(ms));
}
const EXPAND_MS: u64 = 250;
const RELEASE_SHOW_MS: u64 = 250;
const RELEASE_HIDE_MS: u64 = 250;
const PANEL_MS: u64 = 320;
const BOUNCE_MS: u64 = 400;

/// `kPreloadMessagesCount` (`support/support_preload.cpp`): the next
/// channel's history page loaded while pulling.
pub const PRELOAD_MESSAGES: i32 = 50;
/// `jumpWhenReady`: longest wait for the history and for the pinned bar.
pub const JUMP_MAX_WAIT_MS: u64 = 1500;
pub const JUMP_PINNED_MAX_WAIT_MS: u64 = 600;

// lib_ui `ElasticScroll`: the pull side follows the finger with a soft
// linear stiffness within the pull distance and the stiff rubber band past
// it.
const PULL_BAND_STIFFNESS: f32 = 2.5;
const RUBBER_BAND_STIFFNESS: f32 = 20.;
const RUBBER_BAND_PERIOD: f32 = 1.6;
const RUBBER_BAND_MAX_RETURN_MS: u64 = 1000;
/// A gesture whose end event never arrived is let go after this long, as
/// `chat_swipe` does.
const STALE_GESTURE_MS: u64 = 600;

/// `PullAccumulatedFromOverscroll`.
pub fn accumulated_from_overscroll(value: f32, distance: f32) -> f32 {
    if value <= distance {
        value * PULL_BAND_STIFFNESS
    } else {
        distance * PULL_BAND_STIFFNESS + (value - distance) * RUBBER_BAND_STIFFNESS
    }
}

/// `PullOverscrollFromAccumulated`.
pub fn overscroll_from_accumulated(value: f32, distance: f32) -> f32 {
    let soft = distance * PULL_BAND_STIFFNESS;
    if value <= soft {
        value / PULL_BAND_STIFFNESS
    } else {
        distance + (value - soft) / RUBBER_BAND_STIFFNESS
    }
}

/// `anim::easeOutQuint`.
pub fn ease_out_quint(t: f32) -> f32 {
    1. - (1. - t).powi(5)
}

/// `anim::sineInOut`.
pub fn sine_in_out(t: f32) -> f32 {
    -((std::f32::consts::PI * t).cos() - 1.) / 2.
}

/// `BounceProgress`: up, overshoot down, settle.
pub fn bounce_progress(t: f32) -> f32 {
    let ease = |x: f32| {
        if x < 0.5 {
            4. * x * x * x
        } else {
            let u = -2. * x + 2.;
            1. - u * u * u / 2.
        }
    };
    if t < 0.45 {
        ease(t / 0.45)
    } else if t < 0.75 {
        1. - ease((t - 0.45) / 0.3) * 1.5
    } else {
        -0.5 + ease((t - 0.75) / 0.25) * 0.5
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Ease {
    OutQuint,
    SineInOut,
    Linear,
}

/// `Ui::Animations::Simple`, evaluated at a given time.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Tween {
    from: f32,
    to: f32,
    start: u64,
    duration: u64,
    ease: Ease,
}

impl Tween {
    fn still(value: f32) -> Self {
        Self {
            from: value,
            to: value,
            start: 0,
            duration: 0,
            ease: Ease::Linear,
        }
    }

    fn start(from: f32, to: f32, now: u64, duration: u64, ease: Ease) -> Self {
        Self {
            from,
            to,
            start: now,
            duration,
            ease,
        }
    }

    fn animating(&self, now: u64) -> bool {
        self.duration > 0 && now < self.start + self.duration
    }

    fn value(&self, now: u64) -> f32 {
        if !self.animating(now) {
            return self.to;
        }
        let t = (now - self.start) as f32 / self.duration as f32;
        let eased = match self.ease {
            Ease::OutQuint => ease_out_quint(t),
            Ease::SineInOut => sine_in_out(t),
            Ease::Linear => t,
        };
        self.from + (self.to - self.from) * eased
    }
}

/// Phase of a precise (trackpad) scroll event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Started,
    Moved,
    Ended,
    Cancelled,
}

/// `Ui::ElasticScrollMovement`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Movement {
    None,
    Progress,
    Returning,
}

/// What the pull leads to (`PullToNextChannel::Mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    History,
    Topic,
}

/// The chat or topic a pull leads to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    Chat(crate::ids::ChatId),
    Topic(i32),
}

/// What a pull found when it started: where it leads, and `via` (the UI's
/// own handle for the gesture that found it: its window, where the release
/// opens it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Found<W> {
    pub next: Next,
    pub via: W,
}

/// What the UI must do after feeding the machine.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Step {
    /// The wheel event was taken by the edge: the list must not scroll.
    pub consumed: bool,
    /// A pull began: find the next channel or topic (and preload it), then
    /// call [`PullMachine::set_found`].
    pub started: bool,
    /// The pull just became ready (`base::Platform::Haptic`).
    pub haptic: bool,
    /// The finger lifted on a pull that was ready: if the next chat is
    /// still unread and the pull still allowed, call
    /// [`PullMachine::begin_jump`] and open it.
    pub release_ready: bool,
    /// Something visible changed.
    pub redraw: bool,
}

impl Step {
    fn merge(&mut self, other: Step) {
        self.consumed |= other.consumed;
        self.started |= other.started;
        self.haptic |= other.haptic;
        self.release_ready |= other.release_ready;
        self.redraw |= other.redraw;
    }
}

/// The indicator under the history (`PullToNextChannel::Indicator`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IndicatorState {
    /// Height the indicator takes (`_effective`); the content rises by it.
    pub offset: f32,
    /// The pull is ready (`_reached`).
    pub ready: bool,
    /// 0 → 1 as the arrow turns into the photo and name.
    pub release: f32,
    /// Vertical bounce of the photo and name, px.
    pub bounce_offset: f32,
}

/// The note over the composer (`PullToNextChannel::HintOverlay`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HintState {
    /// Panel opacity.
    pub panel: f32,
    /// 0: "Pull up…", 1: "Release…".
    pub release: f32,
}

/// How far a pull has got (`_reached`). The dwell timer lives and dies
/// with the stretching stage; the peak with the ready one.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Reach {
    /// Not ready. `dwell_due` (ms): armed when the threshold was crossed
    /// with the finger down; at that time the pull becomes ready.
    Stretching { dwell_due: Option<u64> },
    /// The pull is ready; `peak` is the largest stretch since.
    Ready { peak: f32 },
}

impl Default for Reach {
    fn default() -> Self {
        Self::Stretching { dwell_due: None }
    }
}

/// The pull in progress (`PullToNextChannel`'s `_pull`, `_reached`, ...).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Pull<W> {
    /// How far the edge is stretched.
    amount: f32,
    reach: Reach,
    /// The next channel or topic the UI found, if any. It lives with the
    /// pull, so every way the pull ends takes it along.
    found: Option<Found<W>>,
}

impl<W> Default for Pull<W> {
    fn default() -> Self {
        Self {
            amount: 0.,
            reach: Reach::default(),
            found: None,
        }
    }
}

impl<W> Pull<W> {
    fn reached(&self) -> bool {
        matches!(self.reach, Reach::Ready { .. })
    }

    fn dwell_due(&self) -> Option<u64> {
        match self.reach {
            Reach::Stretching { dwell_due } => dwell_due,
            Reach::Ready { .. } => None,
        }
    }
}

/// Where the controller is. The pull's data lives in the stages that have
/// a pull.
#[derive(Debug, Clone, Default, PartialEq)]
enum Stage<W> {
    /// No pull: the edge may stretch, nothing is shown.
    #[default]
    Idle,
    Pulling(Pull<W>),
    /// The finger let go and the release was decided (`_committed`): the
    /// edge springs back; the decision is not taken twice.
    Released(Pull<W>),
    /// The release opens the next chat: the indicator holds at its full
    /// height until the chat changes, or `until` (ms) passes, so a jump
    /// that never lands cannot hold it for good. `job` is the jump itself,
    /// owned here: leaving the stage (the chat changed, the hold ran out,
    /// a restart) stops it.
    Jumping {
        pull: Pull<W>,
        until: u64,
        job: crate::task_hold::TaskHold,
    },
}

/// The card the indicator grows to: at rest at a value, or a tween that
/// owes the indicator frames (the one that settles it included).
#[derive(Debug, Clone, Copy, PartialEq)]
enum Expand {
    Settled(f32),
    Running(Tween),
}

impl Expand {
    fn value(&self, now: u64) -> f32 {
        match self {
            Self::Settled(value) => *value,
            Self::Running(tween) => tween.value(now),
        }
    }
}

/// The longest a jump holds the indicator without the chat changing.
const JUMP_HOLD_MS: u64 = JUMP_MAX_WAIT_MS + JUMP_PINNED_MAX_WAIT_MS + 1000;

/// Where the edge is in its gesture (`Ui::ElasticScrollMovement` with the
/// spring's data): at rest, held by a finger, or springing back from the
/// stretch it was let go at. A finger down stops the spring where it is.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Edge {
    Rest,
    Held,
    /// Let go at `from` px of stretch at `since` ms.
    Returning {
        from: f32,
        since: u64,
    },
}

/// The edge and the controller (`ElasticScroll` bottom edge +
/// `PullToNextChannel`).
#[derive(Debug, Clone)]
pub struct PullMachine<W = ()> {
    // ElasticScroll's bottom edge.
    edge: Edge,
    last_event: u64,
    accumulated: f32,
    overscroll: f32,
    // PullToNextChannel.
    stage: Stage<W>,
    effective: f32,
    expand: Expand,
    // Indicator: shown while `effective > 0`; ready while `release` heads
    // to 1.
    release: Tween,
    bounce: Tween,
    // HintOverlay: on while `panel` heads to 1; ready while `hint_release`
    // heads to 1.
    panel: Tween,
    hint_release: Tween,
}

impl<W> Default for PullMachine<W> {
    fn default() -> Self {
        Self {
            edge: Edge::Rest,
            last_event: 0,
            accumulated: 0.,
            overscroll: 0.,
            stage: Stage::Idle,
            effective: 0.,
            expand: Expand::Settled(0.),
            release: Tween::still(0.),
            bounce: Tween::still(1.),
            panel: Tween::still(0.),
            hint_release: Tween::still(0.),
        }
    }
}

impl<W: Copy> PullMachine<W> {
    pub fn new() -> Self {
        Self::default()
    }

    fn movement(&self) -> Movement {
        match self.edge {
            Edge::Held => Movement::Progress,
            Edge::Returning { .. } => Movement::Returning,
            Edge::Rest => Movement::None,
        }
    }

    /// The edge's visual stretch, px.
    pub fn overscroll(&self) -> f32 {
        self.overscroll
    }

    pub fn pulling(&self) -> bool {
        matches!(self.stage, Stage::Pulling(_) | Stage::Released(_))
    }

    pub fn jumping(&self) -> bool {
        matches!(self.stage, Stage::Jumping { .. })
    }

    pub fn reached(&self) -> bool {
        self.pull().reached()
    }

    /// The pull of a stage that has one; nothing (all zero) when idle.
    fn pull(&self) -> Pull<W> {
        match &self.stage {
            Stage::Idle => Pull::default(),
            Stage::Pulling(pull) | Stage::Released(pull) | Stage::Jumping { pull, .. } => *pull,
        }
    }

    fn pull_mut(&mut self) -> Option<&mut Pull<W>> {
        match &mut self.stage {
            Stage::Idle => None,
            Stage::Pulling(pull) | Stage::Released(pull) | Stage::Jumping { pull, .. } => {
                Some(pull)
            }
        }
    }

    /// The indicator is ready (heading to its full state).
    fn indicator_ready(&self) -> bool {
        self.release.to > 0.5
    }

    fn hint_ready(&self) -> bool {
        self.hint_release.to > 0.5
    }

    fn hint_on(&self) -> bool {
        self.panel.to > 0.5
    }

    /// How far the history content is lifted: the stretch plus the content
    /// inset (`setContentBottomInset`), which together are the indicator's
    /// height while it shows.
    pub fn content_shift(&self) -> f32 {
        if !matches!(self.stage, Stage::Idle) {
            self.effective
        } else {
            self.overscroll
        }
    }

    /// One precise wheel event over the history. `delta_down` is how far
    /// the finger moved toward newer messages (negative: back up).
    /// `at_bottom`: the list shows its newest loaded message and nothing
    /// newer is left to load. `active`: `PullToNextChannel::active`.
    pub fn wheel(
        &mut self,
        phase: Phase,
        delta_down: f32,
        at_bottom: bool,
        active: bool,
        now: u64,
    ) -> Step {
        self.last_event = now;
        let mut step = Step::default();
        match phase {
            Phase::Started => {
                // A new gesture during the bounce stops it where it is.
                let was_returning = matches!(self.edge, Edge::Returning { .. });
                self.edge = Edge::Held;
                if was_returning {
                    self.accumulated = accumulated_from_overscroll(self.overscroll, PULL_THRESHOLD);
                }
            }
            Phase::Ended | Phase::Cancelled => {
                self.let_go(now);
                step.consumed = self.overscroll > 0.;
                step.merge(self.handle(active, now));
                return step;
            }
            Phase::Moved => {}
        }
        if self.edge != Edge::Held {
            // Momentum after the finger lifted never moves the edge, and
            // the list holds still while the edge springs back.
            step.consumed = self.overscroll > 0.;
            return step;
        }
        if self.overscroll > 0. {
            if delta_down < 0. {
                // Scrolling away from the stretch takes it back 1:1.
                self.overscroll = (self.overscroll + delta_down).max(0.);
                self.accumulated = accumulated_from_overscroll(self.overscroll, PULL_THRESHOLD);
            } else {
                self.accumulated += delta_down;
                self.overscroll = overscroll_from_accumulated(self.accumulated, PULL_THRESHOLD);
            }
            step.consumed = true;
        } else if delta_down > 0. && at_bottom && active {
            self.accumulated = delta_down;
            self.overscroll = overscroll_from_accumulated(self.accumulated, PULL_THRESHOLD);
            step.consumed = true;
        }
        step.merge(self.handle(active, now));
        step
    }

    /// Advance the edge's spring, the dwell timer and the expand animation.
    /// Call on every frame while [`Self::animating`].
    pub fn frame(&mut self, active: bool, now: u64) -> Step {
        let mut step = Step::default();
        if self.edge == Edge::Held && now.saturating_sub(self.last_event) > STALE_GESTURE_MS {
            self.let_go(now);
            step.merge(self.handle(active, now));
        }
        if let Some(due) = self.pull().dwell_due()
            && now >= due
        {
            let mut fired = false;
            if let Stage::Pulling(pull) | Stage::Released(pull) = &mut self.stage {
                if pull.amount >= PULL_THRESHOLD {
                    pull.reach = Reach::Ready { peak: pull.amount };
                    fired = true;
                } else {
                    pull.reach = Reach::default();
                }
            }
            if fired {
                step.haptic = true;
                self.start_expand(true, now);
                self.push_indicator(now);
                step.redraw = true;
            }
        }
        if let Edge::Returning { from, since } = self.edge {
            let t_ms = now.saturating_sub(since);
            let t = t_ms as f32 / 1000.;
            let value = from * (-RUBBER_BAND_STIFFNESS * t / RUBBER_BAND_PERIOD).exp();
            if value < 0.5 || t_ms >= RUBBER_BAND_MAX_RETURN_MS {
                self.edge = Edge::Rest;
                self.overscroll = 0.;
                self.accumulated = 0.;
            } else {
                self.overscroll = value;
                self.accumulated = accumulated_from_overscroll(value, PULL_THRESHOLD);
            }
            step.merge(self.handle(active, now));
            step.redraw = true;
        }
        if let Expand::Running(tween) = self.expand {
            // `_expand.start([=] { pushIndicator(); }, …)`.
            if !tween.animating(now) {
                self.expand = Expand::Settled(tween.to);
            }
            if !matches!(self.stage, Stage::Idle) {
                self.push_indicator(now);
            }
            step.redraw = true;
        }
        // A jump that never landed lets go of the indicator.
        if let Stage::Jumping { until, .. } = &self.stage
            && now >= *until
        {
            self.reset(true, now);
            step.redraw = true;
        }
        step
    }

    /// Whether frames are still needed.
    pub fn animating(&self, now: u64) -> bool {
        self.edge != Edge::Rest
            || self.pull().dwell_due().is_some()
            || matches!(self.expand, Expand::Running(_))
            || self.jumping()
            || self.release.animating(now)
            || self.bounce.animating(now)
            || self.panel.animating(now)
            || self.hint_release.animating(now)
    }

    /// The UI found (or did not find) a next channel or topic. Without a
    /// pull there is nothing to attach it to.
    pub fn set_found(&mut self, found: Option<Found<W>>, now: u64) {
        if let Some(pull) = self.pull_mut() {
            pull.found = found;
        }
        if self.pulling() {
            self.push_indicator(now);
        }
    }

    /// Where the pull leads, in any stage that has a pull.
    pub fn found(&self) -> Option<Found<W>> {
        self.pull().found
    }

    /// Where the pull leads and the gesture's handle, while the finger
    /// pulls or has just let go (the release opens it from there).
    pub fn found_for_release(&self) -> Option<Found<W>> {
        match &self.stage {
            Stage::Pulling(pull) | Stage::Released(pull) => pull.found,
            Stage::Idle | Stage::Jumping { .. } => None,
        }
    }

    /// The release opens the next chat, with `job` running the jump: the
    /// indicator holds at its full height until the chat changes
    /// (`_jumping`), or [`JUMP_HOLD_MS`] pass. The job goes when the stage
    /// does.
    pub fn begin_jump(&mut self, now: u64, job: crate::task_hold::TaskHold) {
        self.stage = Stage::Jumping {
            pull: self.pull(),
            until: now.saturating_add(JUMP_HOLD_MS),
            job,
        };
    }

    /// `PullToNextChannel::handleOverscroll`.
    fn handle(&mut self, active: bool, now: u64) -> Step {
        let mut step = Step {
            redraw: true,
            ..Step::default()
        };
        let movement = self.movement();
        let amount = self.overscroll.max(0.);
        let (mut pull, released) = match &self.stage {
            Stage::Jumping { .. } => return step,
            Stage::Pulling(pull) => (*pull, false),
            Stage::Released(pull) => (*pull, true),
            Stage::Idle => {
                if movement != Movement::Progress || amount <= 0. || !active {
                    return step;
                }
                step.started = true;
                (Pull::default(), false)
            }
        };
        pull.amount = amount;
        let past = amount >= PULL_THRESHOLD;
        match pull.reach {
            Reach::Ready { peak } => {
                // Collapse on a peak-relative reverse, not down to the
                // threshold.
                let peak = peak.max(amount);
                let floor = PULL_THRESHOLD * RESET_REACHED_ON;
                let reset_at = peak - (peak - floor) / REVERSE_EASE;
                if amount < reset_at {
                    pull.reach = Reach::default();
                    self.start_expand(false, now);
                } else {
                    pull.reach = Reach::Ready { peak };
                }
            }
            Reach::Stretching { dwell_due } => {
                if past {
                    // Arm the dwell on finger-down; a flick releases before
                    // it fires.
                    if dwell_due.is_none() && movement == Movement::Progress {
                        pull.reach = Reach::Stretching {
                            dwell_due: Some(now + ready_dwell_ms()),
                        };
                    }
                } else if amount < PULL_THRESHOLD * RESET_REACHED_ON {
                    pull.reach = Reach::default();
                }
            }
        }
        let letting_go = movement != Movement::Progress;
        if letting_go && let Reach::Stretching { .. } = pull.reach {
            pull.reach = Reach::default();
        }
        let decide_release = letting_go && !released;
        self.stage = if released || decide_release {
            Stage::Released(pull)
        } else {
            Stage::Pulling(pull)
        };
        self.push_indicator(now);
        if !letting_go {
            return step;
        }
        if decide_release && pull.reached() && active {
            step.release_ready = true;
            return step;
        }
        if amount <= 0. {
            self.reset(true, now);
        }
        step
    }

    /// `PullToNextChannel::reset`: drop the pull; the hint fades when
    /// `animated`.
    pub fn reset(&mut self, animated: bool, now: u64) {
        self.expand = Expand::Settled(0.);
        self.stage = Stage::Idle;
        self.effective = 0.;
        // Indicator::hideNow.
        self.release = Tween::still(0.);
        self.bounce = Tween::still(1.);
        if animated {
            self.toggle_hint(false, now);
        } else {
            self.panel = Tween::still(0.);
            self.hint_release = Tween::still(0.);
        }
    }

    /// The chat or topic changed: forget everything, the edge included.
    pub fn reset_all(&mut self, now: u64) {
        self.reset(false, now);
        self.edge = Edge::Rest;
        self.overscroll = 0.;
        self.accumulated = 0.;
    }

    /// The finger lifted (or its end never came): a stretched edge springs
    /// back from where it is, a plain one is at rest; a spring already
    /// running goes on.
    fn let_go(&mut self, now: u64) {
        if self.overscroll > 0. {
            self.edge = Edge::Returning {
                from: self.overscroll,
                since: now,
            };
        } else if self.edge == Edge::Held {
            self.edge = Edge::Rest;
        }
    }

    fn start_expand(&mut self, ready: bool, now: u64) {
        let from = self.expand.value(now);
        self.expand = Expand::Running(Tween::start(
            from,
            if ready { 1. } else { 0. },
            now,
            EXPAND_MS,
            if ready {
                Ease::OutQuint
            } else {
                Ease::SineInOut
            },
        ));
    }

    /// `PullToNextChannel::pushIndicator`.
    fn push_indicator(&mut self, now: u64) {
        let expand = self.expand.value(now);
        let pull = self.pull();
        self.effective = MAX_HEIGHT.min(pull.amount + expand * (EXPAND - PULL_THRESHOLD));
        // Indicator::setProgress.
        let ready = pull.reached();
        if self.indicator_ready() != ready {
            let from = self.release.value(now);
            self.release = Tween::start(
                from,
                if ready { 1. } else { 0. },
                now,
                if ready {
                    RELEASE_SHOW_MS
                } else {
                    RELEASE_HIDE_MS
                },
                if ready {
                    Ease::OutQuint
                } else {
                    Ease::SineInOut
                },
            );
            if ready {
                self.bounce = Tween::start(0., 1., now, BOUNCE_MS, Ease::Linear);
            }
        }
        // HintOverlay::setData.
        let visible = self.edge == Edge::Held && pull.amount > 0.;
        self.toggle_hint(visible && pull.found.is_some(), now);
        if self.hint_ready() != ready {
            let from = self.hint_release.value(now);
            self.hint_release = Tween::start(
                from,
                if ready { 1. } else { 0. },
                now,
                if ready {
                    RELEASE_SHOW_MS
                } else {
                    RELEASE_HIDE_MS
                },
                if ready {
                    Ease::OutQuint
                } else {
                    Ease::SineInOut
                },
            );
        }
    }

    /// `HintOverlay::toggle`.
    fn toggle_hint(&mut self, visible: bool, now: u64) {
        if self.hint_on() == visible {
            return;
        }
        let from = self.panel.value(now);
        self.panel = Tween::start(
            from,
            if visible { 1. } else { 0. },
            now,
            PANEL_MS,
            Ease::OutQuint,
        );
    }

    /// The indicator to draw, if any (`Indicator::paintEvent`).
    pub fn indicator(&self, now: u64) -> Option<IndicatorState> {
        if self.effective <= 1. {
            return None;
        }
        Some(IndicatorState {
            offset: self.effective,
            ready: self.indicator_ready(),
            release: self.release.value(now),
            bounce_offset: -bounce_progress(self.bounce.value(now)) * BOUNCE,
        })
    }

    /// The hint over the composer, if any (`HintOverlay::paintEvent`).
    pub fn hint(&self, now: u64) -> Option<HintState> {
        let panel = self.panel.value(now);
        (panel > 0.).then(|| HintState {
            panel,
            release: self.hint_release.value(now),
        })
    }
}

mod find;
mod layout;
pub use find::*;
pub use layout::*;

#[cfg(test)]
mod tests;
