//! A worker thread: opens its TGS clips and draws or unpacks the frames
//! of the clips it owns.
#![deny(clippy::indexing_slicing)]

use super::engine::{Job, Shared, State};
use super::source::{Source, frame_len};
use super::store::{self, Scratch, StoredFrame};
use super::{ClipInfo, ClipKey, ClipState, Rendered};
use crate::locks::lock_or_recover;
use std::collections::HashMap;
use std::path::Path;
use std::sync::MutexGuard;
use std::time::{Duration, Instant};

pub(super) fn run(me: usize, shared: &Shared) {
    // The clips this worker opened: ThorVG canvases stay on the thread
    // that created them (`player` module docs).
    let mut sources: HashMap<ClipKey, Source> = HashMap::new();
    let mut scratch = Scratch::default();
    loop {
        let (job, freed) = {
            let mut state = lock_or_recover(&shared.state);
            loop {
                if let Some(closed) = state.closed.get_mut(me) {
                    for key in closed.drain(..) {
                        sources.remove(&key);
                    }
                }
                if state.stop {
                    return;
                }
                if !state.freed.is_empty() {
                    break (None, std::mem::take(&mut state.freed));
                }
                if let Some(job) = state.next_job(me) {
                    break (Some(job), Vec::new());
                }
                state = wait(shared, state);
            }
        };
        // Freed outside the lock.
        drop(freed);
        let Some(job) = job else {
            continue;
        };
        // The UI is woken when a clip opened or failed and for a clip's
        // first frame; later frames are picked up as views paint.
        let wake = match job {
            Job::Open { key, id, path } => {
                open(shared, &mut sources, key, id, &path);
                true
            }
            Job::Frame {
                key,
                id,
                index,
                stored,
            } => frame(shared, &mut sources, &mut scratch, (key, id), index, stored),
        };
        if wake {
            shared.wake();
        }
    }
}

fn wait<'a>(shared: &Shared, state: MutexGuard<'a, State>) -> MutexGuard<'a, State> {
    shared
        .work
        .wait(state)
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Opens a TGS clip on this worker, which keeps its canvas.
fn open(
    shared: &Shared,
    sources: &mut HashMap<ClipKey, Source>,
    key: ClipKey,
    id: u64,
    path: &Path,
) {
    let opened = Source::lottie(path, key);
    let mut state = lock_or_recover(&shared.state);
    let Some(clip) = state.clip_mut(&key, id) else {
        // Closed meanwhile.
        return;
    };
    clip.busy = false;
    match opened {
        Ok(source) => {
            clip.state = ClipState::Ready(ClipInfo {
                timeline: source.timeline,
            });
            clip.source_bytes = frame_len(key).unwrap_or(0);
            sources.insert(key, source);
        }
        Err(error) => {
            clip.wanted.clear();
            clip.state = ClipState::Failed(error);
        }
    }
}

/// Time spent on one frame.
#[derive(Default)]
struct Timing {
    draw: Option<Duration>,
    unpack: Option<Duration>,
    pack: Duration,
}

/// Draws or unpacks one frame; true when it is the clip's first.
fn frame(
    shared: &Shared,
    sources: &mut HashMap<ClipKey, Source>,
    scratch: &mut Scratch,
    (key, id): (ClipKey, u64),
    index: u32,
    stored: Option<StoredFrame>,
) -> bool {
    // The pixels, and what to store (a frame just drawn).
    let mut drawn: Option<(Vec<u8>, Option<&[u8]>)> = None;
    let mut timing = Timing::default();
    if let Ok(len) = frame_len(key) {
        let started = Instant::now();
        let unpacked = stored.as_ref().and_then(|stored| {
            let mut out = vec![0u8; len];
            store::decode(stored, &mut out).then_some(out)
        });
        if let Some(pixels) = unpacked {
            timing.unpack = Some(started.elapsed());
            drawn = Some((pixels, None));
        } else if let Some(pixels) = sources.get_mut(&key).and_then(|source| source.draw(index)) {
            timing.draw = Some(started.elapsed());
            let packing = Instant::now();
            let packed = store::encode(&pixels, scratch);
            timing.pack = packing.elapsed();
            drawn = Some((pixels, packed));
        }
    }
    // Its block may take new frames again.
    drop(stored);
    let mut state = lock_or_recover(&shared.state);
    let Some(clip) = state.clip_mut(&key, id) else {
        return false;
    };
    clip.busy = false;
    clip.drawing = None;
    let first = drawn.is_some() && !std::mem::replace(&mut clip.delivered, true);
    if let Some(draw) = timing.draw {
        clip.note_draw(draw);
    }
    let stats = &mut state.stats;
    if let Some(draw) = timing.draw {
        stats.drawn += 1;
        stats.draw_time += draw;
        stats.pack_time += timing.pack;
    }
    if let Some(unpack) = timing.unpack {
        stats.unpacked += 1;
        stats.unpack_time += unpack;
    }
    if let Some((pixels, packed)) = drawn {
        if let Some(packed) = packed {
            state.store(&key, index, packed);
        }
        state.rendered.push(Rendered { key, index, pixels });
    }
    first
}
