//! Clips played on demand (`Demand`): asked for by a click, they play
//! whatever the autoplay and data saver settings say.
#![deny(clippy::indexing_slicing)]

use super::{InlineVideos, Key, Slot, State};
use quill::ids::ChatId;
use std::collections::HashMap;

/// A clip asked to play by a click, whatever the autoplay settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum Demand {
    /// A round video message: once from the start, with sound.
    Sound,
    /// An MP4 GIF: a muted loop.
    Loop,
}

#[derive(Clone, Copy)]
pub(super) struct OnDemand {
    demand: Demand,
    /// The player opened and was started as asked.
    started: bool,
    /// The window it was asked in (`None`: the main window).
    pub(super) window: Option<ChatId>,
}

impl State {
    /// The slots in `dropped` go: so do their demands.
    pub(super) fn forget(&mut self, dropped: &HashMap<Key, Slot>) {
        for key in dropped.keys() {
            self.on_demand.remove(key);
        }
    }
}

impl InlineVideos {
    /// The window's display scale as of the last frame (0 before the
    /// first), for sizes decoded elsewhere (`gif_clip`).
    pub(in crate::ui) fn scale(&self) -> f32 {
        self.with(|state| state.scale).unwrap_or(0.)
    }

    /// Ask for `key`'s clip to play (`demand`) once its row renders it
    /// downloaded, whatever the autoplay settings; asked in `window`.
    pub(in crate::ui) fn request(&self, window: Option<ChatId>, key: Key, demand: Demand) {
        self.with(|state| {
            state.on_demand.insert(
                key,
                OnDemand {
                    demand,
                    started: false,
                    window,
                },
            )
        });
    }

    /// What was asked of `key`'s clip, if anything.
    pub(in crate::ui) fn demand(&self, key: Key) -> Option<Demand> {
        self.with(|state| state.on_demand.get(&key).map(|asked| asked.demand))
            .flatten()
    }

    /// Note that `key`'s demand is being carried out: true the first time
    /// only (the caller starts it then).
    pub(in crate::ui) fn start_demand(&self, key: Key) -> bool {
        self.with(|state| {
            state.on_demand.get_mut(&key).is_some_and(|asked| {
                let first = !asked.started;
                asked.started = true;
                first
            })
        })
        .unwrap_or(false)
    }

    /// `key`'s demand is done (a round video played to its end).
    pub(in crate::ui) fn end_demand(&self, key: Key) {
        self.with(|state| state.on_demand.remove(&key));
    }
}
