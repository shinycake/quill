//! The wallpaper renderer's requests and results, shared with its worker
//! thread. Each render is one entry by key, in one stage: queued, running
//! or done, so a key is never wanted and finished at once.
#![deny(clippy::indexing_slicing)]

use super::wallpaper::Wallpaper;
use super::wallpaper_source::Rendered;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Finished results nobody picked up are dropped past this many.
const DONE_CAP: usize = 48;

pub(super) struct Job {
    pub(super) key: u64,
    pub(super) slot: u64,
    pub(super) wallpaper: Wallpaper,
    pub(super) width: u32,
    pub(super) height: u32,
    /// Not before this instant (a resize settling).
    pub(super) not_before: Instant,
}

/// Where a render is. A key that is queued or running is wanted: a result
/// for a key that is no longer wanted is dropped.
enum Stage {
    /// Waiting for the worker; `order` is the request's place in line.
    Queued { job: Job, order: u64 },
    /// Handed to the worker, which ends it with `finish` (a render that
    /// panics still finishes, `render_guarded`).
    Claimed { slot: u64 },
    /// Finished, with its slot, until a lookup takes it.
    Done { slot: u64, result: Option<Rendered> },
}

impl Stage {
    fn slot(&self) -> u64 {
        match self {
            Self::Queued { job, .. } => job.slot,
            Self::Claimed { slot } | Self::Done { slot, .. } => *slot,
        }
    }
}

/// Requests and results shared with the worker thread.
#[derive(Default)]
pub(super) struct Board {
    stages: HashMap<u64, Stage>,
    next_order: u64,
}

impl Board {
    /// Queue `job`, replacing the slot's older request and any result for
    /// it nobody took: a size the window was at for one frame when its
    /// render finished (a full history picture) would otherwise wait
    /// finished until `DONE_CAP` clears it.
    pub(super) fn request(&mut self, job: Job, replaces: bool) {
        if self.stages.contains_key(&job.key) {
            return;
        }
        if replaces {
            let slot = job.slot;
            self.stages.retain(|_, stage| stage.slot() != slot);
        }
        self.next_order = self.next_order.wrapping_add(1);
        self.stages.insert(
            job.key,
            Stage::Queued {
                job,
                order: self.next_order,
            },
        );
    }

    /// The next job that may start now, or how long until one may.
    pub(super) fn next(&mut self, now: Instant) -> Result<Job, Option<Duration>> {
        let due = self
            .stages
            .iter()
            .filter_map(|(key, stage)| match stage {
                Stage::Queued { job, order } if job.not_before <= now => Some((*order, *key)),
                _ => None,
            })
            .min();
        if let Some((_, key)) = due
            && let Some(Stage::Queued { job, .. }) = self.stages.remove(&key)
        {
            self.stages
                .insert(job.key, Stage::Claimed { slot: job.slot });
            return Ok(job);
        }
        Err(self
            .stages
            .values()
            .filter_map(|stage| match stage {
                Stage::Queued { job, .. } => Some(job.not_before.saturating_duration_since(now)),
                _ => None,
            })
            .min())
    }

    pub(super) fn finish(&mut self, slot: u64, key: u64, result: Option<Rendered>) {
        // Wanted: still queued (asked for again while it ran) or running.
        // A replaced render is no longer here and its result is dropped.
        if matches!(
            self.stages.get(&key),
            Some(Stage::Queued { .. } | Stage::Claimed { .. })
        ) {
            if self.done() >= DONE_CAP {
                self.stages
                    .retain(|_, stage| !matches!(stage, Stage::Done { .. }));
            }
            self.stages.insert(key, Stage::Done { slot, result });
        }
    }

    /// The finished result for `key`, taken.
    pub(super) fn take_done(&mut self, key: u64) -> Option<Option<Rendered>> {
        match self.stages.remove(&key) {
            Some(Stage::Done { result, .. }) => Some(result),
            Some(other) => {
                self.stages.insert(key, other);
                None
            }
            None => None,
        }
    }

    fn done(&self) -> usize {
        self.stages
            .values()
            .filter(|stage| matches!(stage, Stage::Done { .. }))
            .count()
    }

    #[cfg(test)]
    pub(super) fn queued(&self) -> usize {
        self.stages
            .values()
            .filter(|stage| matches!(stage, Stage::Queued { .. }))
            .count()
    }

    #[cfg(test)]
    pub(super) fn wants(&self, key: u64) -> bool {
        matches!(
            self.stages.get(&key),
            Some(Stage::Queued { .. } | Stage::Claimed { .. })
        )
    }

    #[cfg(test)]
    pub(super) fn has_done(&self, key: &u64) -> bool {
        matches!(self.stages.get(key), Some(Stage::Done { .. }))
    }

    #[cfg(test)]
    pub(super) fn done_slot(&self, key: u64) -> Option<u64> {
        match self.stages.get(&key) {
            Some(Stage::Done { slot, result: None }) => Some(*slot),
            _ => None,
        }
    }
}
