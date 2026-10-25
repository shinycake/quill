//! The takeover state: what Quill's window shows while an agent drives it
//! (`docs/decisions/agent-access.md`, phase 5 step 2).
//!
//! One process-wide record of facts ([`Facts`]: the drive session, the
//! pause, Stop) and one pure function, [`phase`], that turns the facts, the
//! switches and who is connected into what the window shows: [`Phase::Idle`],
//! [`Phase::Driving`], [`Phase::Paused`] or [`Phase::FadingOut`]. The UI,
//! the driver's refusals and `quill_status` all read [`current`]; nothing
//! keeps a second copy. Facts change only at events (a command, an aimed
//! step, real input, Esc, the pill's buttons, a disconnect seen by
//! [`observe`]); time does the rest.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Driving lasts this long after the agent's last command or step.
pub const HOLD: Duration = Duration::from_secs(4);
/// The glow fades in over this when an agent starts driving.
pub const FADE_IN: Duration = Duration::from_millis(240);
/// And out over this when it stops, disconnects or is stopped.
pub const FADE_OUT: Duration = Duration::from_millis(600);
/// The ghost cursor's glide to the next target; a pointer step waits for it.
pub const GLIDE: Duration = Duration::from_millis(220);
/// The ring that pulses out of the ghost cursor on a click.
pub const PULSE: Duration = Duration::from_millis(380);
/// After "Let it continue", pointer moves do not pause again for this long
/// (the user is moving the pointer away from the button).
pub const RESUME_GRACE: Duration = Duration::from_millis(1500);
/// A pointer move shorter than this (logical pixels) is jitter, not the
/// user reaching for the mouse.
pub const JITTER_PX: f32 = 4.0;

/// What the agent is about to do at its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aim {
    Click,
    Menu,
    Hover,
    Scroll,
    /// Typing into the focused text box; the point is its caret.
    Type,
}

/// The agent's latest step, for the ghost cursor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Action {
    pub aim: Aim,
    /// Where it lands, in window logical pixels.
    pub to: (f32, f32),
    /// Where the ghost glides from; `None` appears at `to`.
    pub from: Option<(f32, f32)>,
    pub at_ms: u64,
    /// When the click went in, for the press pulse.
    pub pressed_ms: Option<u64>,
}

/// Why the driver is paused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseReason {
    /// The user pressed Esc.
    Escape,
    /// The user moved the mouse, clicked, scrolled or typed in the window.
    Input,
}

/// Input that came from the user (never the driver's own).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RealInput {
    Move { x: f32, y: f32 },
    Press,
    Scroll,
    Key,
}

/// One agent's turn at the window, from its first command until it ends.
#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    /// The MCP client's name.
    pub agent: String,
    pub started_ms: u64,
    /// The last command or step.
    pub last_ms: u64,
    /// Set when the session was cut short: Stop, a disconnect, a switch
    /// turned off. Otherwise it ends [`HOLD`] after `last_ms`.
    pub ended_ms: Option<u64>,
    pub action: Option<Action>,
}

/// Everything the phase is derived from, besides the switches and who is
/// connected ([`Live`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Facts {
    pub session: Option<Session>,
    pub paused: Option<PauseReason>,
    /// The user pressed Stop: UI commands are refused until the agents
    /// disconnect or the driving switch goes off.
    pub stopped: bool,
    /// When the user last pressed "Let it continue" (0: never).
    pub resumed_ms: u64,
    /// The user's real pointer, for the jitter check.
    pub last_pointer: Option<(f32, f32)>,
}

/// The parts of the phase read live from elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Live {
    /// At least one agent is connected.
    pub connected: bool,
    /// Agent access and "Let agents drive Quill's window" are on.
    pub allowed: bool,
}

/// What the window shows.
#[derive(Debug, Clone, PartialEq)]
pub enum Phase {
    Idle,
    Driving {
        agent: String,
        since_ms: u64,
        action: Option<Action>,
    },
    Paused {
        agent: String,
        since_ms: u64,
        reason: PauseReason,
    },
    /// `since_ms` is when the session started, so a session that ends
    /// while still fading in fades out from where it got to.
    FadingOut {
        agent: String,
        since_ms: u64,
        ended_ms: u64,
        action: Option<Action>,
    },
}

fn ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

impl Phase {
    /// How much of the takeover is on screen, 0 to 1: fading in, full,
    /// fading out. A pause or an end during the fade-in carries on from
    /// the value reached, never jumping to full first.
    pub fn presence(&self, now: u64) -> f32 {
        match self {
            Phase::Idle => 0.0,
            Phase::Driving { since_ms, .. } | Phase::Paused { since_ms, .. } => {
                progress(*since_ms, now, FADE_IN)
            }
            Phase::FadingOut {
                since_ms, ended_ms, ..
            } => {
                let reached = progress(*since_ms, *ended_ms, FADE_IN);
                reached * (1.0 - progress(*ended_ms, now, FADE_OUT))
            }
        }
    }

    /// The ghost cursor's action, while it shows (driving or fading out).
    pub fn action(&self) -> Option<&Action> {
        match self {
            Phase::Driving { action, .. } | Phase::FadingOut { action, .. } => action.as_ref(),
            Phase::Idle | Phase::Paused { .. } => None,
        }
    }

    pub fn agent(&self) -> Option<&str> {
        match self {
            Phase::Idle => None,
            Phase::Driving { agent, .. }
            | Phase::Paused { agent, .. }
            | Phase::FadingOut { agent, .. } => Some(agent),
        }
    }
}

/// How far `now` is into a span of `length` that started at `start`, 0 to 1.
fn progress(start: u64, now: u64, length: Duration) -> f32 {
    let length = ms(length).max(1);
    (now.saturating_sub(start) as f32 / length as f32).clamp(0.0, 1.0)
}

/// What the window shows for `facts` at `now`.
pub fn phase(facts: &Facts, live: Live, now: u64) -> Phase {
    let Some(session) = &facts.session else {
        return Phase::Idle;
    };
    let on = live.connected && live.allowed && session.ended_ms.is_none();
    if on && let Some(reason) = facts.paused {
        return Phase::Paused {
            agent: session.agent.clone(),
            since_ms: session.started_ms,
            reason,
        };
    }
    let end = session
        .ended_ms
        .unwrap_or_else(|| session.last_ms.saturating_add(ms(HOLD)));
    if on && now < end {
        return Phase::Driving {
            agent: session.agent.clone(),
            since_ms: session.started_ms,
            action: session.action,
        };
    }
    if now < end.saturating_add(ms(FADE_OUT)) {
        return Phase::FadingOut {
            agent: session.agent.clone(),
            since_ms: session.started_ms,
            ended_ms: end.min(now),
            action: session.action,
        };
    }
    Phase::Idle
}

/// Record what [`Live`] says: with no agent connected or the switches off,
/// a running session ends now (and fades), the pause is dropped, and Stop
/// is forgotten (a new connection, or the switch turned back on, may drive).
pub fn observe(facts: &mut Facts, live: Live, now: u64) {
    if live.connected && live.allowed {
        return;
    }
    facts.paused = None;
    facts.stopped = false;
    if let Some(session) = facts.session.as_mut()
        && session.ended_ms.is_none()
        && now < session.last_ms.saturating_add(ms(HOLD))
    {
        session.ended_ms = Some(now);
    }
}

/// A command reached the window: start a session, or extend the running one.
pub fn command(facts: &mut Facts, agent: &str, now: u64) {
    let running = facts.session.as_ref().is_some_and(|session| {
        session.ended_ms.is_none() && now < session.last_ms.saturating_add(ms(HOLD))
    });
    match facts.session.as_mut() {
        Some(session) if running => {
            session.last_ms = now;
            agent.clone_into(&mut session.agent);
        }
        _ => {
            facts.session = Some(Session {
                agent: agent.to_string(),
                started_ms: now,
                last_ms: now,
                ended_ms: None,
                action: None,
            });
        }
    }
}

/// Where the ghost cursor is at `now`: gliding from `from` to `to`.
pub fn ghost_point(action: &Action, now: u64) -> (f32, f32) {
    let (tx, ty) = action.to;
    let Some((fx, fy)) = action.from else {
        return action.to;
    };
    let t = ease_out_cubic(progress(action.at_ms, now, GLIDE));
    (fx + (tx - fx) * t, fy + (ty - fy) * t)
}

/// How far a ghost that appears (no `from`) has faded in, 0 to 1.
pub fn ghost_arrival(action: &Action, now: u64) -> f32 {
    if action.from.is_some() {
        1.0
    } else {
        progress(action.at_ms, now, GLIDE)
    }
}

/// The press pulse's progress, 0 to 1, while it runs.
pub fn press_pulse(action: &Action, now: u64) -> Option<f32> {
    let pressed = action.pressed_ms?;
    let t = progress(pressed, now, PULSE);
    (t < 1.0).then_some(t)
}

pub fn ease_out_cubic(t: f32) -> f32 {
    let inv = 1.0 - t.clamp(0.0, 1.0);
    1.0 - inv * inv * inv
}

/// An agent is about to act at `to`.
pub fn aim(facts: &mut Facts, what: Aim, to: (f32, f32), now: u64) {
    let Some(session) = facts.session.as_mut() else {
        return;
    };
    let from = session.action.map(|before| ghost_point(&before, now));
    session.action = Some(Action {
        aim: what,
        to,
        from,
        at_ms: now,
        pressed_ms: None,
    });
    session.last_ms = session.last_ms.max(now);
}

/// The aimed click went in.
pub fn press(facts: &mut Facts, now: u64) {
    if let Some(session) = facts.session.as_mut() {
        if let Some(action) = session.action.as_mut() {
            action.pressed_ms = Some(now);
        }
        session.last_ms = session.last_ms.max(now);
    }
}

/// Whether `input` from the user pauses the driver now. Clicks, scrolls and
/// keys always do while an agent drives; a pointer move does only in the
/// active window, past the grace after "Let it continue", and further than
/// jitter from where the user's pointer last was.
pub fn pauses_on(facts: &Facts, phase: &Phase, input: RealInput, active: bool, now: u64) -> bool {
    if !matches!(phase, Phase::Driving { .. }) {
        return false;
    }
    match input {
        RealInput::Press | RealInput::Scroll | RealInput::Key => true,
        RealInput::Move { x, y } => {
            let settled = facts.resumed_ms == 0
                || now >= facts.resumed_ms.saturating_add(ms(RESUME_GRACE));
            let moved = facts
                .last_pointer
                .is_some_and(|(px, py)| ((x - px).powi(2) + (y - py).powi(2)).sqrt() >= JITTER_PX);
            active && settled && moved
        }
    }
}

/// The user handed control back to the agent.
pub fn resume(facts: &mut Facts, now: u64) {
    facts.paused = None;
    facts.resumed_ms = now;
    if let Some(session) = facts.session.as_mut()
        && session.ended_ms.is_none()
    {
        session.last_ms = session.last_ms.max(now);
    }
}

/// The user pressed Stop: the session ends now and UI commands are refused.
pub fn stop(facts: &mut Facts, now: u64) {
    facts.stopped = true;
    facts.paused = None;
    if let Some(session) = facts.session.as_mut()
        && session.ended_ms.is_none()
    {
        let end = session.last_ms.saturating_add(ms(HOLD));
        session.ended_ms = Some(now.min(end));
    }
}

/// When the phase next changes by itself (Driving ends, the fade ends),
/// for a window that is not animating to wake at.
pub fn next_change_ms(phase: &Phase, facts: &Facts) -> Option<u64> {
    let session = facts.session.as_ref()?;
    match phase {
        Phase::Driving { .. } => Some(
            session
                .ended_ms
                .unwrap_or_else(|| session.last_ms.saturating_add(ms(HOLD))),
        ),
        Phase::FadingOut { ended_ms, .. } => Some(ended_ms.saturating_add(ms(FADE_OUT))),
        Phase::Idle | Phase::Paused { .. } => None,
    }
}

/// Whether something on screen moves at `now` by itself: a fade, a glide
/// or a press pulse (the glow's own motion is the UI's choice).
pub fn moving(phase: &Phase, now: u64) -> bool {
    let fading = match phase {
        Phase::Driving { since_ms, .. } | Phase::Paused { since_ms, .. } => {
            progress(*since_ms, now, FADE_IN) < 1.0
        }
        Phase::FadingOut { .. } => true,
        Phase::Idle => false,
    };
    fading
        || phase.action().is_some_and(|action| {
            progress(action.at_ms, now, GLIDE) < 1.0 || press_pulse(action, now).is_some()
        })
}

// --- the process-wide record ---

static FACTS: Mutex<Facts> = Mutex::new(Facts {
    session: None,
    paused: None,
    stopped: false,
    resumed_ms: 0,
    last_pointer: None,
});

/// Demos and tests pin the clock here (0: the real clock).
static CLOCK: AtomicU64 = AtomicU64::new(0);

fn epoch() -> std::time::Instant {
    static EPOCH: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    *EPOCH.get_or_init(std::time::Instant::now)
}

/// Milliseconds on the takeover clock (starts at 1, so 0 means "never").
pub fn now_ms() -> u64 {
    match CLOCK.load(Ordering::Relaxed) {
        0 => u64::try_from(epoch().elapsed().as_millis())
            .unwrap_or(u64::MAX)
            .saturating_add(1),
        pinned => pinned,
    }
}

/// Pin the clock to `at` milliseconds, or `None` for the real clock. For
/// screenshot demos and tests: the glow and the ghost cursor then hold
/// still at a chosen moment.
pub fn pin_clock(at: Option<u64>) {
    CLOCK.store(at.map_or(0, |at| at.max(1)), Ordering::Relaxed);
}

fn live() -> Live {
    Live {
        connected: !crate::agent::activity::connected().is_empty(),
        allowed: crate::agent::access::enabled() && crate::agent::access::ui_driver(),
    }
}

fn with_facts<T>(f: impl FnOnce(&mut Facts, Live, u64) -> T) -> T {
    let live = live();
    let now = now_ms();
    let mut facts = crate::locks::lock_or_recover(&FACTS);
    observe(&mut facts, live, now);
    f(&mut facts, live, now)
}

/// The phase now, with the facts it came from.
pub fn snapshot() -> (Phase, Facts, u64) {
    with_facts(|facts, live, now| (phase(facts, live, now), facts.clone(), now))
}

/// The phase now.
pub fn current() -> Phase {
    snapshot().0
}

/// A command from `agent` reached the window.
pub fn note_command(agent: &str) {
    with_facts(|facts, _, now| command(facts, agent, now));
}

/// The driver is about to act at `to` (window pixels).
pub fn note_aim(what: Aim, to: (f32, f32)) {
    with_facts(|facts, _, now| aim(facts, what, to, now));
}

/// The aimed click went in.
pub fn note_press() {
    with_facts(|facts, _, now| press(facts, now));
}

/// Real input reached the window; pauses the driver when it should.
/// `on_controls`: the pointer is on the takeover's own pill, so the user
/// is answering it (Stop, "Let it continue"), not taking the window back.
/// Returns whether it paused.
pub fn note_real_input(input: RealInput, window_active: bool, on_controls: bool) -> bool {
    // Every pointer move lands here: without a session there is nothing
    // to pause, so skip reading who is connected.
    {
        let mut facts = crate::locks::lock_or_recover(&FACTS);
        if facts.session.is_none() {
            if let RealInput::Move { x, y } = input {
                facts.last_pointer = Some((x, y));
            }
            return false;
        }
    }
    let paused = with_facts(|facts, live, now| {
        let shown = phase(facts, live, now);
        let pause = !on_controls && pauses_on(facts, &shown, input, window_active, now);
        if let RealInput::Move { x, y } = input {
            facts.last_pointer = Some((x, y));
        }
        if pause {
            facts.paused = Some(PauseReason::Input);
        }
        pause
    });
    if paused {
        crate::wake::AGENT.poke();
    }
    paused
}

/// Esc from the user: pauses the driver while it drives. Returns whether
/// it did (the key is then the pause's, not the window's).
pub fn note_escape() -> bool {
    let paused = with_facts(|facts, live, now| {
        let driving = matches!(phase(facts, live, now), Phase::Driving { .. });
        if driving {
            facts.paused = Some(PauseReason::Escape);
        }
        driving
    });
    if paused {
        crate::wake::AGENT.poke();
    }
    paused
}

/// Pause for `reason` (kept when already paused).
pub fn pause(reason: PauseReason) {
    with_facts(|facts, _, _| {
        facts.paused.get_or_insert(reason);
    });
    crate::wake::AGENT.poke();
}

/// "Let it continue".
pub fn let_continue() {
    with_facts(|facts, _, now| resume(facts, now));
    crate::wake::AGENT.poke();
}

/// Stop: end the session, refuse every UI command waiting and to come.
pub fn stop_driving() {
    with_facts(|facts, _, now| stop(facts, now));
    super::bridge::refuse_all(&stopped_refusal());
    crate::wake::AGENT.poke();
}

pub fn paused() -> bool {
    crate::locks::lock_or_recover(&FACTS).paused.is_some()
}

pub fn stopped() -> bool {
    crate::locks::lock_or_recover(&FACTS).stopped
}

/// Why the driver refuses every command right now: the user took control
/// or pressed Stop.
pub fn refusal() -> Option<String> {
    let facts = crate::locks::lock_or_recover(&FACTS);
    if facts.stopped {
        Some(stopped_refusal())
    } else if facts.paused.is_some() {
        Some(paused_refusal())
    } else {
        None
    }
}

pub fn paused_refusal() -> String {
    "The user took control of Quill, so the UI driver is paused. Wait, then call quill_status to see when you can drive again (ui_driver.paused turns false)."
        .into()
}

pub fn stopped_refusal() -> String {
    "The user pressed Stop in Quill: don't drive its window again in this session. Tell the user; the chat tools (list_chats, read_history, send_message...) still work."
        .into()
}

/// Back to no session, no pause, no Stop and the real clock.
#[cfg(any(test, feature = "test-support"))]
pub fn reset_for_test() {
    *crate::locks::lock_or_recover(&FACTS) = Facts::default();
    pin_clock(None);
}

#[cfg(test)]
#[path = "takeover_tests.rs"]
mod tests;
