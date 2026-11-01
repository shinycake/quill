#![deny(clippy::indexing_slicing)]

use super::{
    AUDIO_HIGH_SECS, AUDIO_LOW_SECS, AudioQueue, AudioShared, AudioTap, Clock, Demuxer, IDLE_WAIT,
    Item, PixelOutput, SYNC_TOLERANCE, Shared, State, VideoFrame, lock,
};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Step {
    Wait,
    Decode,
    ReachForSound,
}

pub(super) fn next_step(
    video_queued: Option<(usize, usize)>,
    audio_buffered_secs: Option<f64>,
) -> Step {
    if audio_buffered_secs.is_some_and(|secs| secs >= AUDIO_HIGH_SECS) {
        return Step::Wait;
    }
    let audio_low = audio_buffered_secs.is_some_and(|secs| secs < AUDIO_LOW_SECS);
    match video_queued {
        Some((queued, cap)) if queued < cap.max(1) => Step::Decode,
        Some(_) if audio_low => Step::ReachForSound,
        None if audio_low => Step::Decode,
        _ => Step::Wait,
    }
}

/// Nothing else moves the queue while nothing paints, and the sound filed
/// behind these pictures has to keep coming.
fn drop_passed_pictures(queue: &mut VecDeque<VideoFrame>, floor: f64) {
    while queue.get(1).is_some_and(|next| next.pts <= floor) {
        queue.pop_front();
    }
}

/// The lowest position paint can reach: what is heard while the sound
/// drives the clock, else the running clock itself, as `Player` paces.
/// `None` while the clock stands still.
fn paint_floor(clock: &Clock, sound: Option<f64>, now: Instant) -> Option<f64> {
    let position = match sound {
        Some(heard) => heard,
        None if clock.is_playing() && !clock.held => clock.position(now),
        None => return None,
    };
    Some(position - SYNC_TOLERANCE)
}

/// How long the thread may sleep. With no sound to wake it as it plays,
/// until the clock passes the second queued picture, which it then drops.
fn idle_wait(state: &State, sound: Option<f64>, now: Instant) -> Duration {
    let clock = &state.clock;
    if sound.is_some() || !clock.is_playing() || clock.held {
        return IDLE_WAIT;
    }
    let Some(next) = state.video.get(1) else {
        return IDLE_WAIT;
    };
    let ahead = (next.pts + SYNC_TOLERANCE - clock.position(now)) / clock.rate.max(0.01);
    // A whole millisecond past it, so the wake finds the picture passed.
    Duration::try_from_secs_f64(ahead.max(0.0))
        .map_or(IDLE_WAIT, |wait| wait.min(IDLE_WAIT))
        .saturating_add(Duration::from_millis(1))
}

pub(super) enum Turn<'s> {
    Stop,
    Worked,
    Idle(MutexGuard<'s, State>, Duration),
}

pub(super) struct Decoder {
    demuxer: Box<dyn Demuxer>,
    has_video: bool,
    applied_output: Option<PixelOutput>,
    decoded_a_picture: bool,
}

pub(super) fn run(demuxer: Box<dyn Demuxer>, shared: Arc<Shared>, want_audio: bool) {
    let mut decoder = Decoder::start(demuxer, &shared, want_audio);
    loop {
        match decoder.turn(&shared, Instant::now()) {
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
    pub(super) fn start(demuxer: Box<dyn Demuxer>, shared: &Arc<Shared>, want_audio: bool) -> Self {
        let info = demuxer.info();
        let has_video = info.has_video;
        let mut state = lock(&shared.state);
        if want_audio && info.has_audio {
            let audio = Arc::new(AudioShared {
                rate: info.sample_rate,
                channels: info.channels.max(1),
                queue: Mutex::new(AudioQueue::default()),
                generation: AtomicU64::new(0),
                origin: AtomicU64::new(0f64.to_bits()),
                played: AtomicU64::new(0f64.to_bits()),
                played_generation: AtomicU64::new(u64::MAX),
                detached: AtomicBool::new(false),
                starved: AtomicBool::new(true),
            });
            state.tap = Some(AudioTap {
                shared: audio.clone(),
                wake: shared.clone(),
                chunk: Vec::new(),
                at: 0,
                chunk_pts: 0.0,
                end_pts: 0.0,
                pts_shift: 0.0,
                gap: 0,
                gap_at: 0,
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
            applied_output: None,
            decoded_a_picture: false,
        }
    }

    pub(super) fn turn<'s>(&mut self, shared: &'s Shared, now: Instant) -> Turn<'s> {
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
        let sound = audio
            .as_ref()
            .filter(|audio| !audio.starved.load(Ordering::Acquire))
            .and_then(|audio| audio.heard());
        if let Some(floor) = paint_floor(&state.clock, sound, now) {
            drop_passed_pictures(&mut state.video, floor);
        }
        let video_queued = self
            .has_video
            .then_some((state.video.len(), state.video_cap));
        let audio_buffered = audio.as_ref().map(|audio| audio.buffered_secs());
        let step = next_step(video_queued, audio_buffered);
        if state.eof || step == Step::Wait {
            let wait = idle_wait(&state, sound, now);
            return Turn::Idle(state, wait);
        }
        let epoch = state.epoch;
        let output = state.output;
        drop(state);
        if self.applied_output != Some(output) {
            self.demuxer.set_output(output);
            self.applied_output = Some(output);
        }
        let reaching = step == Step::ReachForSound;
        let item = if reaching {
            let sound = self.demuxer.next_sound();
            sound.map(|chunk| chunk.map(Item::Audio))
        } else {
            self.demuxer.next()
        };
        self.decoded_a_picture |= matches!(item, Ok(Some(Item::Video(_))));
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
            Ok(None) if reaching => return Turn::Idle(state, IDLE_WAIT),
            Ok(None) => {
                if self.has_video && !self.decoded_a_picture {
                    state.error = Some("The video has no pictures".to_string());
                }
                state.eof = true;
            }
            Err(err) => {
                state.error = Some(err);
                state.eof = true;
            }
        }
        Turn::Worked
    }
}
