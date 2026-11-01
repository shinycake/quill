//! The opener threads, one per worker: they decode clips whose frames all
//! come up front (WebM video stickers) and fill their stores. A decode takes
//! tens to hundreds of milliseconds, so on a worker it would hold back the
//! frames of every clip that worker owns. A decoded clip holds no canvas, so
//! nothing ties the decode to the worker that later unpacks its frames.
#![deny(clippy::indexing_slicing)]

use super::engine::Shared;
use super::store::{self, Scratch, Store};
use super::{Cancel, ClipInfo, ClipKey, ClipState, DecodeAll, source};
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
    let mut packed = Store::default();
    let opened = decode(cancel)
        .and_then(|decoded| source::decoded(key, decoded))
        .and_then(|(timeline, frames)| {
            for (index, frame) in (0u32..).zip(&frames) {
                let frame = store::encode(frame, scratch)
                    .ok_or_else(|| "Could not keep animated sticker frames".to_string())?;
                packed.insert(index, frame);
            }
            Ok(timeline)
        });
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
