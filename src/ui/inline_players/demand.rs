//! Clips played on demand (`Demand`): asked for by a click, they play
//! whatever the autoplay and data saver settings say.
#![deny(clippy::indexing_slicing)]

use super::windows::List;
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

/// Where a click's demand stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DemandStage {
    /// Asked after its window's history pass `pass` ended; waits for a row
    /// to render the clip. A pass that ends with no row for it expires it.
    Asked { pass: u64 },
    /// The player opened and was started as asked.
    Started,
}

#[derive(Clone, Copy)]
pub(super) struct OnDemand {
    demand: Demand,
    pub(super) stage: DemandStage,
    /// The window it was asked in (`None`: the main window).
    pub(super) window: Option<ChatId>,
}

impl State {
    /// `window` ended a pass of `list` (`pass` is its number now): the
    /// clicks asked before the last full pass that no player carries out
    /// (their row never rendered them) expire.
    pub(super) fn expire_demands(&mut self, window: Option<ChatId>, list: List, pass: u64) {
        let players = &self.players;
        self.on_demand.retain(|key, asked| {
            let stale = asked.window == window
                && List::of(*key) == list
                && matches!(asked.stage, DemandStage::Asked { pass: at } if pass > at.saturating_add(1))
                && !players.contains_key(key);
            !stale
        });
    }

    /// `window`'s history drew nothing, or a list of it is left: the
    /// clicks asked there that no player carries out expire.
    pub(super) fn expire_waiting_demands(&mut self, window: Option<ChatId>, list: Option<List>) {
        let players = &self.players;
        self.on_demand.retain(|key, asked| {
            let stale = asked.window == window
                && list.is_none_or(|list| List::of(*key) == list)
                && matches!(asked.stage, DemandStage::Asked { .. })
                && !players.contains_key(key);
            !stale
        });
    }

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
            let pass = state.pass(window, List::of(key));
            state.on_demand.insert(
                key,
                OnDemand {
                    demand,
                    stage: DemandStage::Asked { pass },
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

    /// `key`'s demand is waiting for its player to start (asked, not
    /// started yet).
    pub(in crate::ui) fn demand_waiting(&self, key: Key) -> bool {
        self.with(|state| {
            state
                .on_demand
                .get(&key)
                .is_some_and(|asked| matches!(asked.stage, DemandStage::Asked { .. }))
        })
        .unwrap_or(false)
    }

    /// `key`'s player started as asked: the demand is carried out. Called
    /// only once the sound really started, so a click the player refused
    /// stays asked.
    pub(in crate::ui) fn demand_started(&self, key: Key) {
        self.with(|state| {
            if let Some(asked) = state.on_demand.get_mut(&key) {
                asked.stage = DemandStage::Started;
            }
        });
    }

    /// `key`'s demand is done (a round video played to its end).
    pub(in crate::ui) fn end_demand(&self, key: Key) {
        self.with(|state| state.on_demand.remove(&key));
    }
}
