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
use std::time::Instant;

/// Where playback is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlayState {
    /// The backend is still opening the file.
    Opening,
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

/// What a backend reported when it was polled.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct BackendStatus {
    /// The file is open and can play.
    pub(crate) ready: bool,
    /// The file can't play.
    pub(crate) failed: bool,
    /// The playback position (seconds).
    pub(crate) position: f64,
    pub(crate) duration: Option<f64>,
    /// The picture is stalled while the clock should run.
    pub(crate) stalled: bool,
    /// The backend stopped at the end of the clip.
    pub(crate) at_end: bool,
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
    /// The feed ended: `Some(true)` when the engine failed.
    pub(crate) ended: Option<bool>,
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

/// The decision behind every backend's sync. Idempotent: a sync on every
/// poll corrects drift (a stall, the end) and otherwise changes nothing.
pub(crate) fn reconcile(want: Want, engine: Engine) -> Option<Correction> {
    match (want.playing, engine.running) {
        (true, false) if engine.ready && !engine.at_end && engine.keeps_up => {
            Some(Correction::Start)
        }
        (true, true) if (engine.rate - want.rate).abs() > 1e-3 => Some(Correction::Rate),
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
    state: PlayState,
    /// The user wants it playing (survives Opening and Buffering).
    wants_play: bool,
    looping: bool,
    /// A loop was sent ahead of the end and the backend still reports a
    /// position near it (its seek has not landed yet).
    loop_sent: bool,
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
            state: PlayState::Opening,
            wants_play: false,
            looping: false,
            loop_sent: false,
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
        Self {
            live: true,
            wants_play: true,
            ..Self::new(None, now)
        }
    }

    pub(crate) fn state(&self) -> PlayState {
        self.state
    }

    pub(crate) fn set_looping(&mut self, looping: bool) {
        self.looping = looping;
    }

    pub(crate) fn rate(&self) -> f64 {
        self.rate
    }

    pub(crate) fn wants_play(&self) -> bool {
        self.wants_play
    }

    /// What the backend's engine must do now.
    pub(crate) fn want(&self) -> Want {
        Want {
            playing: self.wants_play && self.state != PlayState::Failed,
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
        let running = self.state == PlayState::Playing && !self.live;
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
            state: self.state,
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
        self.wants_play = true;
        match self.state {
            PlayState::Opening | PlayState::Failed => PlayStart::Later,
            PlayState::Ended => {
                self.position = 0.0;
                self.pulled_at = now;
                self.state = PlayState::Playing;
                PlayStart::FromStart
            }
            PlayState::Playing | PlayState::Buffering => PlayStart::Already,
            PlayState::Ready | PlayState::Paused => {
                self.pulled_at = now;
                self.state = PlayState::Playing;
                PlayStart::Now
            }
        }
    }

    pub(crate) fn pause(&mut self, now: Instant) {
        if self.live {
            return;
        }
        self.wants_play = false;
        if self.state.animates() {
            self.position = self.position_at(now);
            self.pulled_at = now;
            self.state = PlayState::Paused;
        }
    }

    /// The backend is gone for now (an inline tile released off screen):
    /// paused where it was, no play wanted, until a new backend opens
    /// through a new machine. A failed clip stays failed.
    pub(crate) fn sleep(&mut self, now: Instant) {
        self.wants_play = false;
        self.loop_sent = false;
        if self.state != PlayState::Failed {
            self.position = self.position_at(now);
            self.pulled_at = now;
            self.state = PlayState::Paused;
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
        if self.state == PlayState::Ended {
            self.state = PlayState::Paused;
            self.wants_play = false;
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
        if self.state != PlayState::Failed {
            self.position = self.position_at(now);
            self.pulled_at = now;
            self.state = PlayState::Failed;
        }
    }

    /// Take in what the backend reported at `now`.
    pub(crate) fn observe(&mut self, status: &BackendStatus, now: Instant) -> Option<Command> {
        if self.state == PlayState::Failed {
            return None;
        }
        if status.failed {
            self.fail(now);
            return None;
        }
        if let Some(duration) = status.duration.filter(|d| d.is_finite() && *d > 0.0) {
            self.duration = Some(duration);
        }
        self.buffered = merge_ranges(status.buffered.clone());
        self.has_sound = status.has_sound;
        if self.state == PlayState::Opening {
            if !status.ready {
                return None;
            }
            self.position = status.position.max(0.0);
            self.pulled_at = now;
            self.state = if self.wants_play {
                PlayState::Playing
            } else {
                PlayState::Ready
            };
            return None;
        }
        // Only the backend's own duration (`self.duration`), never the
        // message's whole-second label: a 12.4 s file labelled 12 s would
        // otherwise loop 0.45 s early.
        let near_end = self.looping
            && self
                .duration
                .is_some_and(|total| status.position >= total - LOOP_LEAD);
        if !near_end {
            self.loop_sent = false;
        }
        // While a loop's seek has not landed the backend still reports the
        // end: keep the position at the start (no flash of the end in the
        // remaining-time pill).
        if status.position.is_finite() && !self.loop_sent {
            self.position = status.position.max(0.0);
            self.pulled_at = now;
        }
        let loops_early = near_end && !self.loop_sent;
        let ended = status.at_end && !self.loop_sent;
        match self.state {
            PlayState::Playing | PlayState::Buffering if ended || loops_early => {
                if self.looping {
                    self.position = 0.0;
                    self.state = PlayState::Playing;
                    self.loop_sent = true;
                    return Some(Command::Loop);
                }
                self.state = PlayState::Ended;
                self.wants_play = false;
                if let Some(total) = self.duration() {
                    self.position = total;
                }
            }
            PlayState::Playing if status.stalled => self.state = PlayState::Buffering,
            PlayState::Buffering if !status.stalled => self.state = PlayState::Playing,
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
        if self.state == PlayState::Failed {
            return;
        }
        self.pulled_at = now;
        self.state = match (status.ended, status.sender) {
            (Some(true), _) => PlayState::Failed,
            (Some(false), _) => PlayState::Ended,
            (None, Some(LiveState::Paused)) => PlayState::Paused,
            (None, Some(LiveState::Inactive)) => PlayState::Ready,
            (None, _) if !status.has_frame => PlayState::Opening,
            (None, _) if status.stalled => PlayState::Buffering,
            (None, _) => PlayState::Playing,
        };
        self.wants_play = !matches!(self.state, PlayState::Ended | PlayState::Failed);
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
        BackendStatus, Command, Correction, Engine, PlayStart, PlayState, TransportMachine, Want,
        merge_ranges, reconcile,
    };
    use std::time::{Duration, Instant};

    fn ready(position: f64) -> BackendStatus {
        BackendStatus {
            ready: true,
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
        assert_eq!(machine.state(), PlayState::Opening);
        assert_eq!(machine.duration(), Some(9.0));
        // Play before the file is open: it starts once ready.
        assert_eq!(machine.play(t0), PlayStart::Later);
        assert_eq!(machine.observe(&BackendStatus::default(), t0), None);
        assert_eq!(machine.observe(&ready(0.0), t0), None);
        assert!(machine.want().playing);
        assert_eq!(machine.state(), PlayState::Playing);
        assert_eq!(machine.duration(), Some(10.0));
        let stalled = BackendStatus {
            stalled: true,
            ..ready(1.0)
        };
        machine.observe(&stalled, t0);
        assert_eq!(machine.state(), PlayState::Buffering);
        machine.observe(&ready(1.2), t0);
        assert_eq!(machine.state(), PlayState::Playing);
        let end = BackendStatus {
            at_end: true,
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
            at_end: true,
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
            at_end: true,
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
            ready: true,
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
                at_end: true,
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
                failed: true,
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
            at_end: true,
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
}
