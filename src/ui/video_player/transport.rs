//! The player's transport: what state playback is in, where it is, and
//! what is buffered. Pure (no backend, no GPUI): the player feeds it what
//! the backend reported on each pull ([`BackendStatus`]) and the user's
//! commands, and reads back a [`Transport`] snapshot for the UI.
//!
//! The position is cached with the time it was read; between pulls the UI
//! extrapolates `position + (now - pulled_at) * rate` while playing, so
//! the elapsed label and the seek bar move without a timer.
#![deny(clippy::indexing_slicing)]

use std::ops::Range;
use std::time::{Duration, Instant};

/// Where playback is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlayState {
    /// The backend is still opening the file. (Mirrors the backend's
    /// state, which the player polls; nothing is held here.)
    NotOpen,
    /// Open and showing its first picture, not playing yet.
    Ready,
    Playing,
    /// Playing, but the picture stalled (no picture due and none decoded).
    Buffering,
    Paused,
    /// Ran to the end and holds the last picture (looping players never
    /// get here).
    Ended,
    /// The file can't play; the last picture (if any) stays.
    Failed,
}

/// What the user wants of the clock: it survives Opening and Buffering, so
/// a Play pressed before the file opens still starts it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Intent {
    Pause,
    Play,
}

/// Where the machine is: a [`PlayState`] with what the user wants, or, for
/// a looping player playing or buffering, that state with the loop's seek
/// back to the start still landing. `Landing`: the seek was sent ahead of
/// the end and the backend still reports a position near it; it ends when
/// the position leaves the end, and any other move of the state (sleep,
/// pause, a failure) replaces the value, so nothing resets it by hand. The
/// intent lives in the value, so a move sets state and intent together.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    At(PlayState, Intent),
    Landing(PlayState, Intent, Instant),
}

impl Stage {
    fn play_state(self) -> PlayState {
        match self {
            Self::At(state, _) | Self::Landing(state, _, _) => state,
        }
    }

    fn intent(self) -> Intent {
        match self {
            Self::At(_, intent) | Self::Landing(_, intent, _) => intent,
        }
    }

    fn landing(self) -> bool {
        matches!(self, Self::Landing(..))
    }

    /// The same stage in `state`; a loop seek still landing carries over
    /// only while the player stays playing or buffering.
    fn in_state(self, state: PlayState) -> Self {
        let playing = matches!(state, PlayState::Playing | PlayState::Buffering);
        if playing && let Self::Landing(_, _, since) = self {
            Self::Landing(state, self.intent(), since)
        } else {
            Self::At(state, self.intent())
        }
    }

    /// The same stage wanting `intent`.
    fn wanting(self, intent: Intent) -> Self {
        match self {
            Self::At(state, _) => Self::At(state, intent),
            Self::Landing(state, _, since) => Self::Landing(state, intent, since),
        }
    }
}

/// A loop's seek that has not left the end by then is sent again (a backend
/// that dropped it would otherwise stay at the end for good).
const LANDING_WAIT: Duration = Duration::from_secs(2);
/// A paused seek or step waits this long for its picture at most.
const AWAIT_PICTURE: Duration = Duration::from_secs(2);

/// Whether a picture awaited for `waited` is still worth a frame request.
pub(crate) fn awaits_picture(waited: Duration) -> bool {
    waited < AWAIT_PICTURE
}

impl PlayState {
    /// The picture moves (or should): frames are requested every vsync.
    pub(crate) fn animates(self) -> bool {
        matches!(self, PlayState::Playing | PlayState::Buffering)
    }
}

/// A snapshot of the transport for the UI.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Transport {
    pub(crate) state: PlayState,
    /// Seconds, extrapolated to when the snapshot was taken.
    pub(crate) position: f64,
    /// Seconds, once known (the message's duration until then).
    pub(crate) duration: Option<f64>,
    /// Time ranges the backend has data for, merged and sorted.
    pub(crate) buffered: Vec<Range<f64>>,
    pub(crate) rate: f64,
    pub(crate) volume: f32,
    /// The clip has a soundtrack that is playing out.
    pub(crate) has_sound: bool,
    /// A live feed (a stream, a live story, a video chat track): no
    /// position, duration or rate, nothing to seek or pause.
    pub(crate) live: bool,
}

impl Transport {
    /// Playing or about to (Space pauses).
    pub(crate) fn is_playing(&self) -> bool {
        self.state.animates()
    }

    /// The duration in seconds, 0 while unknown.
    pub(crate) fn total(&self) -> f64 {
        self.duration.unwrap_or(0.0)
    }
}

/// What a backend's engine reported: one stage, never two at once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum EngineHealth {
    /// The file is still opening.
    #[default]
    NotOpen,
    /// The file can't play.
    Failed,
    /// Open; how the clock and the picture are doing.
    Open(Flow),
}

/// How an open engine runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Flow {
    #[default]
    Steady,
    /// The picture is stalled while the clock should run.
    Stalled,
    /// The backend stopped at the end of the clip.
    AtEnd,
}

impl EngineHealth {
    /// The one stage an engine's readings amount to. A failure wins, then
    /// not being open yet, then the end of the clip, then a stall.
    pub(crate) fn read(ready: bool, failed: bool, stalled: bool, at_end: bool) -> Self {
        if failed {
            Self::Failed
        } else if !ready {
            Self::NotOpen
        } else if at_end {
            Self::Open(Flow::AtEnd)
        } else if stalled {
            Self::Open(Flow::Stalled)
        } else {
            Self::Open(Flow::Steady)
        }
    }
}

/// What a backend reported when it was polled.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct BackendStatus {
    /// How the engine stands: opening, failed, or open and how it runs.
    pub(crate) health: EngineHealth,
    /// The playback position (seconds).
    pub(crate) position: f64,
    pub(crate) duration: Option<f64>,
    pub(crate) buffered: Vec<Range<f64>>,
    pub(crate) has_sound: bool,
}

/// What a live feed reported when it was polled
/// (`backend_live.rs`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LiveStatus {
    /// A frame has arrived (the last picture is kept from then on).
    pub(crate) has_frame: bool,
    /// No frame for the stall time while the sender is active.
    pub(crate) stalled: bool,
    /// What the sender's video is doing, once known.
    pub(crate) sender: Option<quill::calls::live_video::LiveState>,
    /// Whether the feed is still running, ended or failed.
    pub(crate) end: FeedEnd,
}

/// How a live feed stands at its end.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum FeedEnd {
    #[default]
    Live,
    /// The feed ended.
    Ended,
    /// The engine failed.
    Failed,
}

/// What the player must do to the backend after a status update, besides
/// syncing it to the [`Want`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Command {
    /// The clip ended on a looping player: seek to 0 (the sync plays on).
    Loop,
}

/// The transport's play intent, which a backend's engine follows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Want {
    pub(crate) playing: bool,
    pub(crate) rate: f64,
}

impl Default for Want {
    fn default() -> Self {
        Self {
            playing: false,
            rate: 1.0,
        }
    }
}

/// What a backend's engine is doing, as the backend reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Engine {
    /// Open and able to play.
    pub(crate) ready: bool,
    /// The clock runs.
    pub(crate) running: bool,
    pub(crate) rate: f64,
    /// Stopped at the end of the clip.
    pub(crate) at_end: bool,
    /// The buffer holds enough to play on. AVPlayer drops its rate to 0 on
    /// a stall and, not waiting to minimize stalls, never raises it again.
    pub(crate) keeps_up: bool,
}

/// What a backend does to its engine to follow the [`Want`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Correction {
    Start,
    Pause,
    Rate,
}

/// Rates closer than this are the same rate (an engine's own rate is an
/// `f32`).
pub(crate) const RATE_TOLERANCE: f64 = 1e-3;

/// The decision behind every backend's sync. Idempotent: a sync on every
/// poll corrects drift (a stall, the end) and otherwise changes nothing.
pub(crate) fn reconcile(want: Want, engine: Engine) -> Option<Correction> {
    match (want.playing, engine.running) {
        (true, false) if engine.ready && !engine.at_end && engine.keeps_up => {
            Some(Correction::Start)
        }
        (true, true) if (engine.rate - want.rate).abs() > RATE_TOLERANCE => Some(Correction::Rate),
        (false, true) => Some(Correction::Pause),
        _ => None,
    }
}

/// A looping player starts over this close to the end (seconds), without
/// waiting for the backend to stop there: the seek back then overlaps the
/// last picture instead of following it. The old inline player looped the
/// same way (a 1 s round video loops with a 100 to 130 ms seam otherwise).
const LOOP_LEAD: f64 = 0.05;

/// The transport's state machine.
#[derive(Clone, Debug)]
pub(crate) struct TransportMachine {
    stage: Stage,
    looping: bool,
    position: f64,
    pulled_at: Instant,
    duration: Option<f64>,
    /// The message's duration, shown until the backend knows better.
    fallback_duration: Option<f64>,
    buffered: Vec<Range<f64>>,
    rate: f64,
    volume: f32,
    has_sound: bool,
    /// A live feed: driven by [`Self::observe_live`] only.
    live: bool,
}

impl TransportMachine {
    pub(crate) fn new(fallback_duration: Option<f64>, now: Instant) -> Self {
        Self {
            stage: Stage::At(PlayState::NotOpen, Intent::Pause),
            looping: false,
            position: 0.0,
            pulled_at: now,
            duration: None,
            fallback_duration: fallback_duration.filter(|d| d.is_finite() && *d > 0.0),
            buffered: Vec::new(),
            rate: 1.0,
            volume: 1.0,
            has_sound: false,
            live: false,
        }
    }

    /// A machine for a live feed: Opening until its first frame; play,
    /// pause, seek and rate do nothing.
    pub(crate) fn live(now: Instant) -> Self {
        let mut machine = Self::new(None, now);
        machine.live = true;
        machine.stage = machine.stage.wanting(Intent::Play);
        machine
    }

    /// Move to `state`, keeping what the user wants.
    fn set_state(&mut self, state: PlayState) {
        self.stage = self.stage.in_state(state);
    }

    /// Move to `state` wanting `intent`: one assignment, so the two agree.
    fn move_to(&mut self, state: PlayState, intent: Intent) {
        self.stage = self.stage.in_state(state).wanting(intent);
    }

    pub(crate) fn state(&self) -> PlayState {
        self.stage.play_state()
    }

    pub(crate) fn set_looping(&mut self, looping: bool) {
        self.looping = looping;
    }

    pub(crate) fn rate(&self) -> f64 {
        self.rate
    }

    pub(crate) fn wants_play(&self) -> bool {
        self.stage.intent() == Intent::Play
    }

    /// What the backend's engine must do now.
    pub(crate) fn want(&self) -> Want {
        Want {
            playing: self.wants_play() && self.state() != PlayState::Failed,
            rate: self.rate,
        }
    }

    /// The duration to show: the backend's, else the message's.
    pub(crate) fn duration(&self) -> Option<f64> {
        self.duration.or(self.fallback_duration)
    }

    /// The position at `now`, extrapolated from the last pull while the
    /// clock runs, never past the end.
    pub(crate) fn position_at(&self, now: Instant) -> f64 {
        // A live feed has no position.
        let running = self.state() == PlayState::Playing && !self.live;
        let mut position = self.position;
        if running {
            position += now.saturating_duration_since(self.pulled_at).as_secs_f64() * self.rate;
        }
        match self.duration() {
            Some(total) => position.clamp(0.0, total),
            None => position.max(0.0),
        }
    }

    pub(crate) fn snapshot(&self, now: Instant) -> Transport {
        Transport {
            state: self.state(),
            position: self.position_at(now),
            duration: self.duration(),
            buffered: self.buffered.clone(),
            rate: self.rate,
            volume: self.volume,
            has_sound: self.has_sound,
            live: self.live,
        }
    }

    /// Play was asked for. Returns whether the backend should start now
    /// (it is open), and whether it must first go back to the start (it
    /// had ended).
    pub(crate) fn play(&mut self, now: Instant) -> PlayStart {
        if self.live {
            return PlayStart::Already;
        }
        self.stage = self.stage.wanting(Intent::Play);
        match self.state() {
            PlayState::NotOpen | PlayState::Failed => PlayStart::Later,
            PlayState::Ended => {
                self.position = 0.0;
                self.pulled_at = now;
                self.set_state(PlayState::Playing);
                PlayStart::FromStart
            }
            PlayState::Playing | PlayState::Buffering => PlayStart::Already,
            PlayState::Ready | PlayState::Paused => {
                self.pulled_at = now;
                self.set_state(PlayState::Playing);
                PlayStart::Now
            }
        }
    }

    pub(crate) fn pause(&mut self, now: Instant) {
        if self.live {
            return;
        }
        if self.state().animates() {
            self.position = self.position_at(now);
            self.pulled_at = now;
            self.move_to(PlayState::Paused, Intent::Pause);
        } else {
            self.stage = self.stage.wanting(Intent::Pause);
        }
    }

    /// The backend is gone for now (an inline tile released off screen):
    /// paused where it was, no play wanted, until a new backend opens
    /// through a new machine. A failed clip stays failed.
    pub(crate) fn sleep(&mut self, now: Instant) {
        if self.state() != PlayState::Failed {
            self.position = self.position_at(now);
            self.pulled_at = now;
            self.move_to(PlayState::Paused, Intent::Pause);
        } else {
            self.stage = self.stage.wanting(Intent::Pause);
        }
    }

    /// Seek to `secs` (clamped to the clip). Playing stays playing; an
    /// ended clip becomes paused there.
    pub(crate) fn seek(&mut self, secs: f64, now: Instant) -> f64 {
        if self.live {
            return 0.0;
        }
        let secs = if secs.is_finite() { secs } else { 0.0 };
        let target = match self.duration() {
            Some(total) => secs.clamp(0.0, total),
            None => secs.max(0.0),
        };
        self.position = target;
        self.pulled_at = now;
        if self.state() == PlayState::Ended {
            self.move_to(PlayState::Paused, Intent::Pause);
        }
        target
    }

    /// A new rate keeps the position where it is.
    pub(crate) fn set_rate(&mut self, rate: f64, now: Instant) -> f64 {
        if self.live {
            return 1.0;
        }
        self.position = self.position_at(now);
        self.pulled_at = now;
        self.rate = if rate.is_finite() && rate > 0.0 {
            rate
        } else {
            1.0
        };
        self.rate
    }

    pub(crate) fn set_volume(&mut self, volume: f32) -> f32 {
        self.volume = if volume.is_finite() {
            volume.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.volume
    }

    /// The backend failed: hold the last position.
    pub(crate) fn fail(&mut self, now: Instant) {
        if self.state() != PlayState::Failed {
            self.position = self.position_at(now);
            self.pulled_at = now;
            self.set_state(PlayState::Failed);
        }
    }

    /// Take in what the backend reported at `now`.
    pub(crate) fn observe(&mut self, status: &BackendStatus, now: Instant) -> Option<Command> {
        if self.state() == PlayState::Failed {
            return None;
        }
        let flow = match status.health {
            EngineHealth::Failed => {
                self.fail(now);
                return None;
            }
            EngineHealth::NotOpen => None,
            EngineHealth::Open(flow) => Some(flow),
        };
        if let Some(duration) = status.duration.filter(|d| d.is_finite() && *d > 0.0) {
            self.duration = Some(duration);
        }
        self.buffered = merge_ranges(status.buffered.clone());
        self.has_sound = status.has_sound;
        if self.state() == PlayState::NotOpen {
            // Still not open: nothing more to take in.
            flow?;
            self.position = status.position.max(0.0);
            self.pulled_at = now;
            self.set_state(if self.wants_play() {
                PlayState::Playing
            } else {
                PlayState::Ready
            });
            return None;
        }
        // Only the backend's own duration (`self.duration`), never the
        // message's whole-second label: a 12.4 s file labelled 12 s would
        // otherwise loop 0.45 s early.
        let near_end = self.looping
            && self
                .duration
                .is_some_and(|total| status.position >= total - LOOP_LEAD);
        if !near_end && self.stage.landing() {
            self.stage = Stage::At(self.state(), self.stage.intent());
        }
        // While a loop's seek has not landed the backend still reports the
        // end: keep the position at the start (no flash of the end in the
        // remaining-time pill).
        if status.position.is_finite() && !self.stage.landing() {
            self.position = status.position.max(0.0);
            self.pulled_at = now;
        }
        // The seek was sent a while ago and the backend still reports the
        // end: it was lost, so it goes again (and the wait restarts).
        if let Stage::Landing(state, intent, since) = self.stage
            && (near_end || flow == Some(Flow::AtEnd))
            && now.saturating_duration_since(since) >= LANDING_WAIT
        {
            self.stage = Stage::Landing(state, intent, now);
            return Some(Command::Loop);
        }
        let loops_early = near_end && !self.stage.landing();
        let flow = flow.unwrap_or_default();
        let ended = flow == Flow::AtEnd && !self.stage.landing();
        match self.state() {
            PlayState::Playing | PlayState::Buffering if ended || loops_early => {
                if self.looping {
                    self.position = 0.0;
                    self.stage = Stage::Landing(PlayState::Playing, self.stage.intent(), now);
                    return Some(Command::Loop);
                }
                self.move_to(PlayState::Ended, Intent::Pause);
                if let Some(total) = self.duration() {
                    self.position = total;
                }
            }
            PlayState::Playing if flow == Flow::Stalled => self.set_state(PlayState::Buffering),
            PlayState::Buffering if flow != Flow::Stalled => self.set_state(PlayState::Playing),
            _ => {}
        }
        None
    }
}

impl TransportMachine {
    /// Take in what a live feed reported: Opening until the first frame,
    /// Playing while frames come, Buffering on a stall (the picture
    /// stays), Paused while the sender paused, Ready while the sender has
    /// no video, Ended once the feed ended, Failed on an engine error.
    pub(crate) fn observe_live(&mut self, status: &LiveStatus, now: Instant) {
        use quill::calls::live_video::LiveState;
        if self.state() == PlayState::Failed {
            return;
        }
        self.pulled_at = now;
        let state = match (status.end, status.sender) {
            (FeedEnd::Failed, _) => PlayState::Failed,
            (FeedEnd::Ended, _) => PlayState::Ended,
            (FeedEnd::Live, Some(LiveState::Paused)) => PlayState::Paused,
            (FeedEnd::Live, Some(LiveState::Inactive)) => PlayState::Ready,
            (FeedEnd::Live, _) if !status.has_frame => PlayState::NotOpen,
            (FeedEnd::Live, _) if status.stalled => PlayState::Buffering,
            (FeedEnd::Live, _) => PlayState::Playing,
        };
        self.move_to(
            state,
            if matches!(state, PlayState::Ended | PlayState::Failed) {
                Intent::Pause
            } else {
                Intent::Play
            },
        );
    }
}

/// How a play request lands on the backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlayStart {
    /// Start the backend now.
    Now,
    /// It had ended: seek to 0, then start.
    FromStart,
    /// Already playing.
    Already,
    /// Not open yet (or failed): it starts once ready.
    Later,
}

/// Sort ranges and merge the ones that touch or overlap; drop empty and
/// non-finite ones.
pub(crate) fn merge_ranges(mut ranges: Vec<Range<f64>>) -> Vec<Range<f64>> {
    ranges.retain(|range| {
        range.start.is_finite() && range.end.is_finite() && range.end > range.start
    });
    ranges.sort_by(|a, b| a.start.total_cmp(&b.start));
    let mut merged: Vec<Range<f64>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        match merged.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => merged.push(range),
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::{
        BackendStatus, Command, Correction, Engine, EngineHealth, Flow, LANDING_WAIT, PlayStart,
        PlayState, TransportMachine, Want, merge_ranges, reconcile,
    };
    use std::time::{Duration, Instant};

    fn ready(position: f64) -> BackendStatus {
        BackendStatus {
            health: EngineHealth::Open(Flow::Steady),
            position,
            duration: Some(10.0),
            ..BackendStatus::default()
        }
    }

    #[test]
    fn sleep_holds_the_position_and_wants_no_play_from_any_state() {
        let t0 = Instant::now();
        // Opening, play wanted: asleep it is paused and wants nothing.
        let mut machine = TransportMachine::new(None, t0);
        machine.play(t0);
        machine.sleep(t0);
        assert_eq!(machine.state(), PlayState::Paused);
        assert!(!machine.snapshot(t0).is_playing());
        // Playing: the position freezes where it was.
        let mut machine = TransportMachine::new(Some(10.0), t0);
        machine.play(t0);
        machine.observe(&ready(0.0), t0);
        let later = t0 + Duration::from_secs(2);
        machine.sleep(later);
        assert_eq!(machine.state(), PlayState::Paused);
        let at = machine.snapshot(later + Duration::from_secs(5)).position;
        assert!((at - 2.0).abs() < 0.01, "{at}");
        // Failed stays failed.
        let mut machine = TransportMachine::new(None, t0);
        machine.fail(t0);
        machine.sleep(t0);
        assert_eq!(machine.state(), PlayState::Failed);
    }

    #[test]
    fn opens_then_plays_buffers_and_ends() {
        let t0 = Instant::now();
        let mut machine = TransportMachine::new(Some(9.0), t0);
        assert_eq!(machine.state(), PlayState::NotOpen);
        assert_eq!(machine.duration(), Some(9.0));
        // Play before the file is open: it starts once ready.
        assert_eq!(machine.play(t0), PlayStart::Later);
        assert_eq!(machine.observe(&BackendStatus::default(), t0), None);
        assert_eq!(machine.observe(&ready(0.0), t0), None);
        assert!(machine.want().playing);
        assert_eq!(machine.state(), PlayState::Playing);
        assert_eq!(machine.duration(), Some(10.0));
        let stalled = BackendStatus {
            health: EngineHealth::Open(Flow::Stalled),
            ..ready(1.0)
        };
        machine.observe(&stalled, t0);
        assert_eq!(machine.state(), PlayState::Buffering);
        machine.observe(&ready(1.2), t0);
        assert_eq!(machine.state(), PlayState::Playing);
        let end = BackendStatus {
            health: EngineHealth::Open(Flow::AtEnd),
            ..ready(9.98)
        };
        machine.observe(&end, t0);
        assert_eq!(machine.state(), PlayState::Ended);
        assert_eq!(machine.snapshot(t0).position, 10.0);
        // Play after the end starts over.
        assert_eq!(machine.play(t0), PlayStart::FromStart);
        assert_eq!(machine.snapshot(t0).position, 0.0);
    }

    #[test]
    fn opening_without_play_waits_ready() {
        let t0 = Instant::now();
        let mut machine = TransportMachine::new(None, t0);
        assert_eq!(machine.observe(&ready(0.0), t0), None);
        assert_eq!(machine.state(), PlayState::Ready);
        assert_eq!(machine.play(t0), PlayStart::Now);
        assert_eq!(machine.play(t0), PlayStart::Already);
    }

    #[test]
    fn a_looping_player_never_ends() {
        let t0 = Instant::now();
        let mut machine = TransportMachine::new(None, t0);
        machine.set_looping(true);
        machine.play(t0);
        machine.observe(&ready(0.0), t0);
        let end = BackendStatus {
            health: EngineHealth::Open(Flow::AtEnd),
            ..ready(10.0)
        };
        assert_eq!(machine.observe(&end, t0), Some(Command::Loop));
        assert_eq!(machine.state(), PlayState::Playing);
        assert_eq!(machine.snapshot(t0).position, 0.0);
    }

    #[test]
    fn a_looping_player_starts_over_just_before_the_end_once() {
        let t0 = Instant::now();
        let mut machine = TransportMachine::new(None, t0);
        machine.set_looping(true);
        machine.play(t0);
        machine.observe(&ready(0.0), t0);
        assert_eq!(machine.observe(&ready(9.9), t0), None);
        assert_eq!(machine.observe(&ready(9.97), t0), Some(Command::Loop));
        // The seek has not landed: no second loop while it is still there.
        assert_eq!(machine.observe(&ready(9.98), t0), None);
        assert_eq!(machine.observe(&ready(0.02), t0), None);
        assert_eq!(machine.observe(&ready(9.96), t0), Some(Command::Loop));
        // A clip that is not looping plays its last picture to the end.
        let mut once = TransportMachine::new(None, t0);
        once.play(t0);
        once.observe(&ready(0.0), t0);
        assert_eq!(once.observe(&ready(9.99), t0), None);
        assert_eq!(once.state(), PlayState::Playing);
    }

    /// A loop's seek the backend never acted on is sent again after the
    /// wait, not waited for forever.
    #[test]
    fn a_loop_seek_that_never_lands_is_sent_again() {
        let t0 = Instant::now();
        let mut machine = TransportMachine::new(None, t0);
        machine.set_looping(true);
        machine.play(t0);
        machine.observe(&ready(0.0), t0);
        let end = BackendStatus {
            health: EngineHealth::Open(Flow::AtEnd),
            ..ready(10.0)
        };
        assert_eq!(machine.observe(&end, t0), Some(Command::Loop));
        let soon = t0 + LANDING_WAIT / 2;
        assert_eq!(machine.observe(&end, soon), None, "still landing");
        let late = t0 + LANDING_WAIT;
        assert_eq!(machine.observe(&end, late), Some(Command::Loop), "again");
        assert_eq!(machine.observe(&end, late), None, "the wait restarts");
        assert_eq!(machine.observe(&ready(0.02), late), None);
        assert_eq!(machine.state(), PlayState::Playing);
    }

    #[test]
    fn an_early_loop_waits_for_its_seek_and_keeps_the_position_at_the_start() {
        let t0 = Instant::now();
        let mut machine = TransportMachine::new(None, t0);
        machine.set_looping(true);
        machine.play(t0);
        machine.observe(&ready(0.0), t0);
        assert_eq!(machine.observe(&ready(9.97), t0), Some(Command::Loop));
        // The backend reaches the end before the seek lands: no second loop,
        // and the position stays at the start.
        let end = BackendStatus {
            health: EngineHealth::Open(Flow::AtEnd),
            ..ready(10.0)
        };
        assert_eq!(machine.observe(&end, t0), None);
        assert_eq!(machine.snapshot(t0).position, 0.0);
        assert_eq!(machine.state(), PlayState::Playing);
        // Landed: the position follows the backend again.
        assert_eq!(machine.observe(&ready(0.1), t0), None);
        assert_eq!(machine.snapshot(t0).position, 0.1);
    }

    #[test]
    fn the_message_duration_never_loops_early() {
        let t0 = Instant::now();
        // The message says 9 s; the backend has not reported its own yet.
        let mut machine = TransportMachine::new(Some(9.0), t0);
        machine.set_looping(true);
        machine.play(t0);
        let unknown = BackendStatus {
            health: EngineHealth::Open(Flow::Steady),
            ..BackendStatus::default()
        };
        machine.observe(&unknown, t0);
        let late = BackendStatus {
            position: 8.97,
            ..unknown.clone()
        };
        assert_eq!(machine.observe(&late, t0), None);
        assert_eq!(machine.state(), PlayState::Playing);
    }

    #[test]
    fn seeking_while_paused_playing_and_ended() {
        let t0 = Instant::now();
        let mut machine = TransportMachine::new(None, t0);
        machine.observe(&ready(0.0), t0);
        // Ready (paused): moves, stays put.
        assert_eq!(machine.seek(4.0, t0), 4.0);
        assert_eq!(machine.state(), PlayState::Ready);
        assert_eq!(machine.snapshot(t0 + Duration::from_secs(1)).position, 4.0);
        // Playing: keeps playing from there.
        machine.play(t0);
        machine.seek(6.0, t0);
        assert_eq!(machine.state(), PlayState::Playing);
        let later = machine.snapshot(t0 + Duration::from_millis(500)).position;
        assert!((later - 6.5).abs() < 1e-6, "{later}");
        // Clamped to the clip, and NaN goes to the start.
        assert_eq!(machine.seek(99.0, t0), 10.0);
        assert_eq!(machine.seek(f64::NAN, t0), 0.0);
        // Ended: a seek pauses there.
        machine.observe(
            &BackendStatus {
                health: EngineHealth::Open(Flow::AtEnd),
                ..ready(10.0)
            },
            t0,
        );
        assert_eq!(machine.state(), PlayState::Ended);
        machine.seek(3.0, t0);
        assert_eq!(machine.state(), PlayState::Paused);
        assert!(!machine.wants_play());
    }

    #[test]
    fn extrapolates_at_the_rate_and_a_rate_change_keeps_position() {
        let t0 = Instant::now();
        for rate in [1.0, 1.5, 0.5] {
            let mut machine = TransportMachine::new(None, t0);
            machine.observe(&ready(2.0), t0);
            machine.play(t0);
            machine.set_rate(rate, t0);
            let at = machine.snapshot(t0 + Duration::from_secs(2)).position;
            assert!((at - (2.0 + 2.0 * rate)).abs() < 1e-6, "rate {rate}: {at}");
        }
        let mut machine = TransportMachine::new(None, t0);
        machine.observe(&ready(0.0), t0);
        machine.play(t0);
        let t1 = t0 + Duration::from_secs(2);
        machine.set_rate(2.0, t1);
        assert!((machine.snapshot(t1).position - 2.0).abs() < 1e-6);
        let t2 = t1 + Duration::from_secs(1);
        assert!((machine.snapshot(t2).position - 4.0).abs() < 1e-6);
        // A bad rate falls back to normal speed.
        assert_eq!(machine.set_rate(0.0, t2), 1.0);
    }

    #[test]
    fn paused_and_failed_hold_the_position() {
        let t0 = Instant::now();
        let mut machine = TransportMachine::new(None, t0);
        machine.observe(&ready(1.0), t0);
        machine.play(t0);
        let t1 = t0 + Duration::from_secs(1);
        machine.pause(t1);
        assert_eq!(machine.state(), PlayState::Paused);
        let held = machine.snapshot(t1 + Duration::from_secs(3)).position;
        assert!((held - 2.0).abs() < 1e-6, "{held}");
        machine.play(t1);
        let t2 = t1 + Duration::from_secs(1);
        machine.observe(
            &BackendStatus {
                health: EngineHealth::Failed,
                ..BackendStatus::default()
            },
            t2,
        );
        assert_eq!(machine.state(), PlayState::Failed);
        let failed_at = machine.snapshot(t2 + Duration::from_secs(5)).position;
        assert!((failed_at - 3.0).abs() < 1e-6, "{failed_at}");
        // Nothing revives a failed player.
        assert_eq!(machine.observe(&ready(0.0), t2), None);
        assert_eq!(machine.play(t2), PlayStart::Later);
    }

    fn idle() -> Engine {
        Engine {
            ready: true,
            running: false,
            rate: 1.0,
            at_end: false,
            keeps_up: true,
        }
    }

    fn play(rate: f64) -> Want {
        Want {
            playing: true,
            rate,
        }
    }

    #[test]
    fn a_stalled_clip_is_told_to_play_again_once_the_buffer_keeps_up() {
        // Play wanted, rate dropped by a stall, buffer caught up.
        assert_eq!(reconcile(play(1.0), idle()), Some(Correction::Start));
        let starved = Engine {
            keeps_up: false,
            ..idle()
        };
        assert_eq!(reconcile(play(1.0), starved), None);
        // Paused on purpose, already playing, ended, or not open yet.
        assert_eq!(reconcile(Want::default(), idle()), None);
        let running = Engine {
            running: true,
            ..idle()
        };
        assert_eq!(reconcile(play(1.0), running), None);
        let ended = Engine {
            at_end: true,
            ..idle()
        };
        assert_eq!(reconcile(play(1.0), ended), None);
        let opening = Engine {
            ready: false,
            ..idle()
        };
        assert_eq!(reconcile(play(1.0), opening), None);
    }

    #[test]
    fn a_running_engine_pauses_and_follows_the_rate() {
        let running = Engine {
            running: true,
            ..idle()
        };
        assert_eq!(reconcile(Want::default(), running), Some(Correction::Pause));
        assert_eq!(reconcile(play(1.5), running), Some(Correction::Rate));
        let at_speed = Engine {
            rate: 1.5,
            ..running
        };
        assert_eq!(reconcile(play(1.5), at_speed), None);
    }

    #[test]
    fn the_end_and_a_seek_back_want_no_play() {
        let t0 = Instant::now();
        let mut machine = TransportMachine::new(None, t0);
        machine.observe(&ready(0.0), t0);
        machine.play(t0);
        machine.set_rate(1.5, t0);
        assert_eq!(machine.want(), play(1.5));
        let end = BackendStatus {
            health: EngineHealth::Open(Flow::AtEnd),
            ..ready(10.0)
        };
        machine.observe(&end, t0);
        assert_eq!(machine.state(), PlayState::Ended);
        assert!(!machine.want().playing);
        machine.seek(3.0, t0);
        assert!(!machine.want().playing);
        assert_eq!(reconcile(machine.want(), idle()), None);
        // Failed: nothing plays, whatever was wanted.
        let mut failed = TransportMachine::new(None, t0);
        failed.play(t0);
        failed.fail(t0);
        assert!(!failed.want().playing);
    }

    #[test]
    fn buffered_ranges_merge_and_sort() {
        let merged = merge_ranges(vec![
            5.0..6.0,
            0.0..2.0,
            1.5..3.0,
            3.0..4.0,
            7.0..7.0,
            f64::NAN..9.0,
        ]);
        assert_eq!(merged, vec![0.0..4.0, 5.0..6.0]);
        assert!(merge_ranges(Vec::new()).is_empty());
    }

    #[test]
    fn an_engine_reading_is_one_stage_with_failure_first() {
        use super::EngineHealth as H;
        assert_eq!(H::read(true, true, true, true), H::Failed);
        assert_eq!(H::read(false, false, true, true), H::NotOpen);
        assert_eq!(H::read(true, false, true, true), H::Open(Flow::AtEnd));
        assert_eq!(H::read(true, false, true, false), H::Open(Flow::Stalled));
        assert_eq!(H::read(true, false, false, false), H::Open(Flow::Steady));
    }

    #[test]
    fn what_the_user_wants_moves_with_the_state() {
        use super::{FeedEnd, LiveStatus};
        let t0 = Instant::now();
        let mut machine = TransportMachine::new(Some(10.0), t0);
        machine.play(t0);
        machine.observe(&ready(0.0), t0);
        assert!(machine.wants_play());
        machine.pause(t0);
        assert!(!machine.wants_play());
        machine.play(t0);
        machine.fail(t0);
        assert!(
            !machine.want().playing,
            "a failed clip is never asked to play"
        );
        let mut live = TransportMachine::live(t0);
        assert!(live.wants_play());
        live.observe_live(
            &LiveStatus {
                end: FeedEnd::Ended,
                ..LiveStatus::default()
            },
            t0,
        );
        assert!(!live.wants_play(), "an ended feed wants nothing");
    }
}
