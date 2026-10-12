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
const EXPAND_MS: u64 = 250;
const RELEASE_SHOW_MS: u64 = 250;
const RELEASE_HIDE_MS: u64 = 250;
const PANEL_MS: u64 = 320;
const BOUNCE_MS: u64 = 400;

/// `kPreloadMessagesCount` (`support/support_preload.cpp`): the next
/// channel's history page loaded while pulling.
pub const PRELOAD_MESSAGES: i32 = 50;
/// `jumpWhenReady`: poll interval, longest wait for the history and for
/// the pinned bar.
pub const JUMP_POLL_MS: u64 = 100;
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

/// What the UI must do after feeding the machine.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Step {
    /// The wheel event was taken by the edge: the list must not scroll.
    pub consumed: bool,
    /// A pull began: find the next channel or topic (and preload it), then
    /// call [`PullMachine::set_has_next`].
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

/// The edge and the controller (`ElasticScroll` bottom edge +
/// `PullToNextChannel`).
#[derive(Debug, Clone)]
pub struct PullMachine {
    // ElasticScroll's bottom edge.
    finger_down: bool,
    last_event: u64,
    accumulated: f32,
    overscroll: f32,
    spring: Option<(f32, u64)>,
    // PullToNextChannel.
    pulling: bool,
    holding: bool,
    committed: bool,
    jumping: bool,
    reached: bool,
    expand_to: bool,
    pull: f32,
    peak_pull: f32,
    effective: f32,
    expand: Tween,
    /// The expand animation still owes the indicator a frame (its last
    /// one included).
    expand_pending: bool,
    dwell_due: Option<u64>,
    has_next: bool,
    // Indicator.
    shown: bool,
    ready: bool,
    release: Tween,
    bounce: Tween,
    // HintOverlay.
    hint_on: bool,
    hint_ready: bool,
    panel: Tween,
    hint_release: Tween,
}

impl Default for PullMachine {
    fn default() -> Self {
        Self {
            finger_down: false,
            last_event: 0,
            accumulated: 0.,
            overscroll: 0.,
            spring: None,
            pulling: false,
            holding: false,
            committed: false,
            jumping: false,
            reached: false,
            expand_to: false,
            pull: 0.,
            peak_pull: 0.,
            effective: 0.,
            expand: Tween::still(0.),
            expand_pending: false,
            dwell_due: None,
            has_next: false,
            shown: false,
            ready: false,
            release: Tween::still(0.),
            bounce: Tween::still(1.),
            hint_on: false,
            hint_ready: false,
            panel: Tween::still(0.),
            hint_release: Tween::still(0.),
        }
    }
}

impl PullMachine {
    pub fn new() -> Self {
        Self::default()
    }

    fn movement(&self) -> Movement {
        if self.finger_down {
            Movement::Progress
        } else if self.spring.is_some() {
            Movement::Returning
        } else {
            Movement::None
        }
    }

    /// The edge's visual stretch, px.
    pub fn overscroll(&self) -> f32 {
        self.overscroll
    }

    pub fn pulling(&self) -> bool {
        self.pulling
    }

    pub fn jumping(&self) -> bool {
        self.jumping
    }

    pub fn reached(&self) -> bool {
        self.reached
    }

    /// How far the history content is lifted: the stretch plus the content
    /// inset (`setContentBottomInset`), which together are the indicator's
    /// height while it shows.
    pub fn content_shift(&self) -> f32 {
        if self.pulling || self.jumping {
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
                self.finger_down = true;
                // A new gesture during the bounce stops it where it is.
                if self.spring.take().is_some() {
                    self.accumulated = accumulated_from_overscroll(self.overscroll, PULL_THRESHOLD);
                }
            }
            Phase::Ended | Phase::Cancelled => {
                self.finger_down = false;
                if self.overscroll > 0. {
                    self.spring = Some((self.overscroll, now));
                }
                step.consumed = self.overscroll > 0.;
                step.merge(self.handle(active, now));
                return step;
            }
            Phase::Moved => {}
        }
        if !self.finger_down {
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
        if self.finger_down && now.saturating_sub(self.last_event) > STALE_GESTURE_MS {
            self.finger_down = false;
            if self.overscroll > 0. {
                self.spring = Some((self.overscroll, now));
            }
            step.merge(self.handle(active, now));
        }
        if let Some(due) = self.dwell_due
            && now >= due
        {
            self.dwell_due = None;
            if self.pulling && self.pull >= PULL_THRESHOLD {
                self.reached = true;
                self.peak_pull = self.pull;
                step.haptic = true;
                self.start_expand(true, now);
                self.push_indicator(now);
                step.redraw = true;
            }
        }
        if let Some((from, start)) = self.spring {
            let t_ms = now.saturating_sub(start);
            let t = t_ms as f32 / 1000.;
            let value = from * (-RUBBER_BAND_STIFFNESS * t / RUBBER_BAND_PERIOD).exp();
            if value < 0.5 || t_ms >= RUBBER_BAND_MAX_RETURN_MS {
                self.spring = None;
                self.overscroll = 0.;
                self.accumulated = 0.;
            } else {
                self.overscroll = value;
                self.accumulated = accumulated_from_overscroll(value, PULL_THRESHOLD);
            }
            step.merge(self.handle(active, now));
            step.redraw = true;
        }
        if self.expand_pending {
            // `_expand.start([=] { pushIndicator(); }, …)`.
            self.expand_pending = self.expand.animating(now);
            if self.pulling || self.jumping {
                self.push_indicator(now);
            }
            step.redraw = true;
        }
        step
    }

    /// Whether frames are still needed.
    pub fn animating(&self, now: u64) -> bool {
        self.finger_down
            || self.spring.is_some()
            || self.dwell_due.is_some()
            || self.expand_pending
            || self.release.animating(now)
            || self.bounce.animating(now)
            || self.panel.animating(now)
            || self.hint_release.animating(now)
    }

    /// The UI found (or did not find) a next channel or topic.
    pub fn set_has_next(&mut self, has_next: bool, now: u64) {
        self.has_next = has_next;
        if self.pulling {
            self.push_indicator(now);
        }
    }

    pub fn has_next(&self) -> bool {
        self.has_next
    }

    /// The release opens the next chat: the indicator holds at its full
    /// height until the chat changes (`_jumping`).
    pub fn begin_jump(&mut self) {
        self.pulling = false;
        self.jumping = true;
    }

    /// `PullToNextChannel::handleOverscroll`.
    fn handle(&mut self, active: bool, now: u64) -> Step {
        let mut step = Step {
            redraw: true,
            ..Step::default()
        };
        let movement = self.movement();
        let pull = self.overscroll.max(0.);
        if !self.pulling {
            if self.jumping || movement != Movement::Progress || pull <= 0. || !active {
                return step;
            }
            self.pulling = true;
            self.committed = false;
            self.has_next = false;
            step.started = true;
        }
        self.pull = pull;
        self.holding = movement == Movement::Progress;
        let reached = pull >= PULL_THRESHOLD;
        if self.reached {
            // Collapse on a peak-relative reverse, not down to the
            // threshold.
            self.peak_pull = self.peak_pull.max(pull);
            let floor = PULL_THRESHOLD * RESET_REACHED_ON;
            let reset_at = self.peak_pull - (self.peak_pull - floor) / REVERSE_EASE;
            if pull < reset_at {
                self.reached = false;
                self.peak_pull = 0.;
                self.start_expand(false, now);
            }
        } else if reached {
            // Arm the dwell on finger-down; a flick releases before it
            // fires.
            if self.dwell_due.is_none() && movement == Movement::Progress {
                self.dwell_due = Some(now + READY_DWELL_MS);
            }
        } else if pull < PULL_THRESHOLD * RESET_REACHED_ON {
            self.dwell_due = None;
        }
        self.push_indicator(now);
        if movement == Movement::Progress {
            return step;
        }
        self.dwell_due = None;
        if !self.committed {
            self.committed = true;
            if self.reached && active {
                step.release_ready = true;
                return step;
            }
        }
        if pull <= 0. {
            self.reset(true, now);
        }
        step
    }

    /// `PullToNextChannel::reset`: drop the pull; the hint fades when
    /// `animated`.
    pub fn reset(&mut self, animated: bool, now: u64) {
        self.expand = Tween::still(0.);
        self.expand_pending = false;
        self.dwell_due = None;
        self.pulling = false;
        self.holding = false;
        self.committed = false;
        self.jumping = false;
        self.reached = false;
        self.peak_pull = 0.;
        self.pull = 0.;
        self.expand_to = false;
        self.has_next = false;
        self.effective = 0.;
        // Indicator::hideNow.
        self.shown = false;
        self.ready = false;
        self.release = Tween::still(0.);
        self.bounce = Tween::still(1.);
        if animated {
            self.toggle_hint(false, now);
        } else {
            self.hint_on = false;
            self.hint_ready = false;
            self.panel = Tween::still(0.);
            self.hint_release = Tween::still(0.);
        }
    }

    /// The chat or topic changed: forget everything, the edge included.
    pub fn reset_all(&mut self, now: u64) {
        self.reset(false, now);
        self.finger_down = false;
        self.spring = None;
        self.overscroll = 0.;
        self.accumulated = 0.;
    }

    fn start_expand(&mut self, ready: bool, now: u64) {
        let from = self.expand.value(now);
        self.expand_to = ready;
        self.expand = Tween::start(
            from,
            if ready { 1. } else { 0. },
            now,
            EXPAND_MS,
            if ready {
                Ease::OutQuint
            } else {
                Ease::SineInOut
            },
        );
        self.expand_pending = true;
    }

    /// `PullToNextChannel::pushIndicator`.
    fn push_indicator(&mut self, now: u64) {
        let expand = self.expand.value(now);
        self.effective = MAX_HEIGHT.min(self.pull + expand * (EXPAND - PULL_THRESHOLD));
        // Indicator::setProgress.
        let ready = self.reached;
        if self.ready != ready {
            let from = self.release.value(now);
            self.ready = ready;
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
        self.shown = self.effective > 0.;
        // HintOverlay::setData.
        let visible = self.holding && self.pull > 0.;
        self.toggle_hint(visible && self.has_next, now);
        if self.hint_ready != ready {
            let from = self.hint_release.value(now);
            self.hint_ready = ready;
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
        if self.hint_on == visible {
            return;
        }
        let from = self.panel.value(now);
        self.hint_on = visible;
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
        if !self.shown || self.effective <= 1. {
            return None;
        }
        Some(IndicatorState {
            offset: self.effective,
            ready: self.ready,
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
