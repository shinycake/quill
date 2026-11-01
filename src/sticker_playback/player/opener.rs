//! The opener threads, one per worker: they run through clips whose frames
//! all come up front (WebM video stickers), packing each frame into the
//! clip's store as it comes. A decode takes tens to hundreds of milliseconds,
//! so on a worker it would hold back the frames of every clip that worker
//! owns. A decoded clip holds no canvas, so nothing ties the decode to the
//! worker that later unpacks its frames.
#![deny(clippy::indexing_slicing)]

use super::engine::Shared;
use super::source::Packer;
use super::store::Scratch;
use super::{Cancel, ClipInfo, ClipKey, ClipState, DecodeAll};
use crate::locks::lock_or_recover;

pub(super) fn run(shared: &Shared) {
    let mut scratch = Scratch::default();
    loop {
        let (key, id, decode, cancel) = {
            let mut state = lock_or_recover(&shared.state);
            loop {
                if state.stop {
                    return;
                }
                if let Some(job) = state.next_decode() {
                    break job;
                }
                state = shared
                    .work
                    .wait(state)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        };
        open(shared, &mut scratch, key, id, decode, &cancel);
        shared.wake();
        shared.work.notify_all();
    }
}

pub(super) fn open(
    shared: &Shared,
    scratch: &mut Scratch,
    key: ClipKey,
    id: u64,
    decode: DecodeAll,
    cancel: &Cancel,
) {
    let mut packer = Packer::new(key, scratch);
    let opened = decode(cancel, &mut packer).and_then(|fps| packer.timeline(fps));
    let packed = packer.store;
    let mut state = lock_or_recover(&shared.state);
    let Some(clip) = state.clip_mut(&key, id) else {
        state.freed.push(packed);
        return;
    };
    clip.busy = false;
    match opened {
        Ok(timeline) => {
            clip.state = ClipState::Ready(ClipInfo { timeline });
            state.install(&key, packed);
        }
        Err(error) => {
            clip.wanted.clear();
            clip.state = ClipState::Failed(error);
            state.freed.push(packed);
        }
    }
}
