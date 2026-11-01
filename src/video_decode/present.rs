//! Picking pictures for the moment they reach the screen: the UI asks for
//! the picture due at the predicted present time of the frame it draws
//! (the next vsync), not at the time it happens to ask, and for when the
//! next picture becomes due so it can sleep until just before then
//! (`docs/decisions/video-player-core.md`). Frame stepping and the
//! "starved" (buffering) state live here too.
#![deny(clippy::indexing_slicing)]

use super::{DUE_SLACK, Pixels, Player, VideoFrame, lock, synced_position, take_due};
use std::time::{Duration, Instant};

/// The longest the UI is told to sleep before the next picture, so a wrong
/// guess (a clock jump, a slow decoder) costs at most this.
const MAX_DUE_AHEAD: Duration = Duration::from_millis(250);

impl Player {
    /// The picture to show now, when it changed since the last call.
    pub fn take_frame(&mut self) -> Option<VideoFrame> {
        self.take_frame_at(Instant::now())
    }

    /// The picture due on screen at `present` (the predicted present time
    /// of the frame being drawn), when it changed since the last call.
    /// Older due pictures are dropped: they would be late.
    pub fn take_frame_at(&mut self, present: Instant) -> Option<VideoFrame> {
        self.take_frame_for(present, Instant::now())
    }

    pub(super) fn take_frame_for(&mut self, present: Instant, now: Instant) -> Option<VideoFrame> {
        // With sound, pull the wall clock back to it when they drifted
        // apart (the sound is the master; the wall clock only smooths it).
        let clock = self.clock();
        if !clock.held && clock.is_playing() {
            let wall = clock.position(now);
            let synced = synced_position(wall, self.heard());
            if synced != wall {
                self.with_clock(|clock| clock.follow(synced, now));
            }
        }
        let position = self.position_at(present.max(now));
        let picked = {
            let mut state = lock(&self.shared.state);
            take_due(&mut state.video, position, self.first)
        };
        if let Some(frame) = &picked {
            self.note_handed_out(frame);
            self.shared.wake.notify_all();
            if self.first {
                self.first = false;
                self.with_clock(|clock| clock.release(now));
            }
            self.last_pts = Some(frame.pts);
        }
        picked
    }

    fn note_handed_out(&self, frame: &VideoFrame) {
        if matches!(frame.pixels, Pixels::Nv12 { .. }) {
            self.bgra.release();
        }
    }

    /// When the front queued picture becomes due on the wall clock, while
    /// the clock runs. `None` when paused, held for a first picture, ended,
    /// or nothing is queued yet (the caller then asks every frame).
    pub fn next_due(&self, now: Instant) -> Option<Instant> {
        let clock = self.clock();
        if clock.held || !clock.is_playing() || self.ended {
            return None;
        }
        let front = lock(&self.shared.state)
            .video
            .front()
            .map(|frame| frame.pts)?;
        let ahead = (front - DUE_SLACK - self.position_at(now)) / clock.rate.max(0.01);
        if !ahead.is_finite() || ahead <= 0.0 {
            return Some(now);
        }
        let ahead = Duration::try_from_secs_f64(ahead)
            .unwrap_or(MAX_DUE_AHEAD)
            .min(MAX_DUE_AHEAD);
        now.checked_add(ahead)
    }

    /// Playing, but the decoder has no picture ready and is not at the end
    /// of the file: the picture is stalled (shown as buffering).
    pub fn starved(&self) -> bool {
        let clock = self.clock();
        if clock.held || !clock.is_playing() || self.ended {
            return false;
        }
        let state = lock(&self.shared.state);
        let has_video = state.info.as_ref().is_some_and(|info| info.has_video);
        has_video && state.video.is_empty() && !state.eof
    }

    /// Paused: show the next queued picture and move the clock to it.
    pub fn step_forward(&mut self) -> Option<VideoFrame> {
        if self.clock().is_playing() {
            return None;
        }
        let frame = lock(&self.shared.state).video.pop_front()?;
        self.note_handed_out(&frame);
        self.shared.wake.notify_all();
        let now = Instant::now();
        self.with_clock(|clock| {
            clock.follow(frame.pts, now);
            clock.pause(now);
        });
        self.first = false;
        self.ended = false;
        self.last_pts = Some(frame.pts);
        Some(frame)
    }

    /// Paused: go back one picture. Seeks to just before the one shown, so
    /// the next [`Player::take_frame_at`] returns the picture before it.
    pub fn step_back(&mut self) {
        if self.clock().is_playing() {
            return;
        }
        let fps = self
            .info()
            .map(|info| info.frame_rate)
            .filter(|fps| fps.is_finite() && *fps > 0.0)
            .unwrap_or(30.0);
        let shown = self.last_pts.unwrap_or_else(|| self.position());
        self.seek((shown - 1.5 / fps).max(0.0));
    }
}
