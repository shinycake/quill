//! The decode thread, one [`Decoder::turn`] at a time, so a test can run the
//! real loop on its own thread and see exactly when it goes idle.
#![deny(clippy::indexing_slicing)]

use super::{
    AUDIO_HIGH_SECS, AUDIO_LOW_SECS, AudioQueue, AudioShared, AudioTap, Demuxer, Item, PixelOutput,
    Shared, State, idle_wait, lock,
};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

/// Whether the decode thread should decode more now: while pictures are
/// short, or sound is below its low mark; never past the sound's cap.
pub fn wants_more(video_queued: Option<(usize, usize)>, audio_buffered_secs: Option<f64>) -> bool {
    if audio_buffered_secs.is_some_and(|secs| secs >= AUDIO_HIGH_SECS) {
        return false;
    }
    let video_short = video_queued.is_some_and(|(queued, cap)| queued < cap.max(1));
    let audio_low = audio_buffered_secs.is_some_and(|secs| secs < AUDIO_LOW_SECS);
    video_short || audio_low
}

/// How a [`Decoder::turn`] came out.
pub(super) enum Turn<'s> {
    /// The player was dropped.
    Stop,
    /// Something changed: take the next turn at once.
    Worked,
    /// Nothing to do. The lock stays held, so a wake can't slip in between
    /// this answer and the sleep on it.
    Idle(MutexGuard<'s, State>, Duration),
}

/// The decode thread's own state.
pub(super) struct Decoder {
    demuxer: Box<dyn Demuxer>,
    has_video: bool,
    /// The layout last passed to the demuxer (none yet).
    applied: Option<PixelOutput>,
}

pub(super) fn run(demuxer: Box<dyn Demuxer>, shared: Arc<Shared>, want_audio: bool) {
    let mut decoder = Decoder::start(demuxer, &shared, want_audio);
    loop {
        match decoder.turn(&shared) {
            Turn::Stop => return,
            Turn::Worked => {}
            Turn::Idle(state, wait) => {
                let (guard, _) = shared
                    .wake
                    .wait_timeout(state, wait)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                drop(guard);
            }
        }
    }
}

impl Decoder {
    /// Publish what the file holds, and the sound's two ends if it is wanted.
    pub(super) fn start(
        demuxer: Box<dyn Demuxer>,
        shared: &Arc<Shared>,
        want_audio: bool,
    ) -> Self {
        let info = demuxer.info();
        let has_video = info.has_video;
        let mut state = lock(&shared.state);
        if want_audio && info.has_audio {
            let audio = Arc::new(AudioShared {
                rate: info.sample_rate,
                channels: info.channels.max(1),
                queue: Mutex::new(AudioQueue::default()),
                generation: AtomicU64::new(0),
                played: AtomicU64::new(0f64.to_bits()),
                played_generation: AtomicU64::new(u64::MAX),
                detached: AtomicBool::new(false),
            });
            state.tap = Some(AudioTap {
                shared: audio.clone(),
                wake: shared.clone(),
                chunk: Vec::new(),
                at: 0,
                chunk_pts: 0.0,
                generation: 0,
                sounding: false,
            });
            state.audio = Some(audio);
        }
        state.info = Some(info);
        drop(state);
        Self {
            demuxer,
            has_video,
            applied: None,
        }
    }

    /// Serve a seek, decode one item, or find there is nothing to do.
    pub(super) fn turn<'s>(&mut self, shared: &'s Shared) -> Turn<'s> {
        let mut state = lock(&shared.state);
        if state.stop {
            return Turn::Stop;
        }
        if let Some(target) = state.seek.take() {
            let epoch = state.epoch;
            drop(state);
            let result = self.demuxer.seek(target);
            let mut state = lock(&shared.state);
            if state.epoch == epoch
                && let Err(err) = result
            {
                state.error = Some(err);
                state.eof = true;
            }
            return Turn::Worked;
        }
        let audio = state
            .audio
            .clone()
            .filter(|audio| !audio.detached.load(Ordering::Relaxed));
        let video_queued = self
            .has_video
            .then_some((state.video.len(), state.video_cap));
        let audio_buffered = audio.as_ref().map(|audio| audio.buffered_secs());
        if state.eof || !wants_more(video_queued, audio_buffered) {
            return Turn::Idle(state, idle_wait(audio.is_some()));
        }
        let epoch = state.epoch;
        let output = state.output;
        drop(state);
        if self.applied != Some(output) {
            self.demuxer.set_output(output);
            self.applied = Some(output);
        }
        let item = self.demuxer.next();
        let mut state = lock(&shared.state);
        if state.epoch != epoch || state.seek.is_some() || state.stop {
            return Turn::Worked;
        }
        match item {
            Ok(Some(Item::Video(frame))) => {
                if self.has_video {
                    state.video.push_back(frame);
                }
            }
            Ok(Some(Item::Audio(chunk))) => {
                drop(state);
                if let Some(audio) = audio {
                    audio.push(chunk);
                }
            }
            Ok(None) => state.eof = true,
            Err(err) => {
                state.error = Some(err);
                state.eof = true;
            }
        }
        Turn::Worked
    }
}
