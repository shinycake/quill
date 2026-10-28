//! A worker thread: opens its clips and draws or unpacks their frames.
#![deny(clippy::indexing_slicing)]

use super::engine::{Job, Shared, State};
use super::source::{Kind, Source, frame_len};
use super::store::{self, Scratch, StoredFrame};
use super::{ClipInfo, ClipKey, ClipState, Origin, Rendered};
use crate::locks::lock_or_recover;
use std::collections::HashMap;
use std::sync::MutexGuard;
use std::time::{Duration, Instant};

pub(super) fn run(me: usize, shared: &Shared) {
    // The clips this worker opened: ThorVG canvases stay on the thread
    // that created them (`player` module docs).
    let mut sources: HashMap<ClipKey, Source> = HashMap::new();
    let mut scratch = Scratch::default();
    loop {
        let job = {
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
                if let Some(job) = state.next_job(me) {
                    break job;
                }
                state = wait(shared, state);
            }
        };
        // The UI is woken when a clip opened or failed and for a clip's
        // first frame; later frames are picked up as views paint.
        let wake = match job {
            Job::Open {
                key,
                origin,
                display_hz,
            } => {
                open(shared, &mut sources, &mut scratch, key, origin, display_hz);
                true
            }
            Job::Frame { key, index, stored } => {
                frame(shared, &mut sources, &mut scratch, key, index, stored)
            }
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

fn open(
    shared: &Shared,
    sources: &mut HashMap<ClipKey, Source>,
    scratch: &mut Scratch,
    key: ClipKey,
    origin: Origin,
    display_hz: f64,
) {
    let cancel = lock_or_recover(&shared.state)
        .clips
        .get(&key)
        .map(|clip| clip.cancel.clone())
        .unwrap_or_default();
    let mut packed: Vec<StoredFrame> = Vec::new();
    let opened = match origin {
        Origin::Lottie(path) => Source::lottie(&path, key, display_hz),
        Origin::Decoded(decode) => decode(&cancel)
            .and_then(|decoded| Source::stored(key, decoded))
            .and_then(|(source, frames)| {
                for frame in &frames {
                    packed.push(
                        store::encode(frame, scratch)
                            .ok_or_else(|| "Could not keep animated sticker frames".to_string())?,
                    );
                }
                Ok(source)
            }),
    };
    let mut state = lock_or_recover(&shared.state);
    let Some(clip) = state.clips.get_mut(&key) else {
        // Closed meanwhile.
        return;
    };
    clip.busy = false;
    match opened {
        Ok(source) => {
            clip.state = ClipState::Ready(ClipInfo {
                timeline: source.timeline,
            });
            // A drawing source holds its canvas.
            clip.source_bytes = match source.kind {
                Kind::Lottie(_) => frame_len(key).unwrap_or(0),
                Kind::Stored => 0,
            };
            for (index, stored) in (0u32..).zip(packed) {
                state.store(&key, index, stored);
            }
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
    key: ClipKey,
    index: u32,
    stored: Option<StoredFrame>,
) -> bool {
    // The pixels, and what to store (a frame just drawn).
    let mut drawn: Option<(Vec<u8>, Option<StoredFrame>)> = None;
    let mut timing = Timing::default();
    if let Some(source) = sources.get_mut(&key)
        && let Ok(len) = frame_len(key)
    {
        let started = Instant::now();
        let unpacked = stored.as_ref().and_then(|stored| {
            let mut out = vec![0u8; len];
            store::decode(stored, &mut out).then_some(out)
        });
        if let Some(pixels) = unpacked {
            timing.unpack = Some(started.elapsed());
            drawn = Some((pixels, None));
        } else if let Some(pixels) = source.draw(index) {
            timing.draw = Some(started.elapsed());
            let packing = Instant::now();
            let packed = store::encode(&pixels, scratch);
            timing.pack = packing.elapsed();
            drawn = Some((pixels, packed));
        }
    }
    let mut state = lock_or_recover(&shared.state);
    let Some(clip) = state.clips.get_mut(&key) else {
        return false;
    };
    clip.busy = false;
    clip.drawing = None;
    let first = drawn.is_some() && !std::mem::replace(&mut clip.delivered, true);
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
