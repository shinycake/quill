//! The accounts whose background client is being prepared. Reading an
//! account's database key runs on a worker; a client starts from it only if
//! the account is still wanted when it ends, and nothing may delete the
//! account's folders while a worker still runs, wanted or not.

use quill::ids::AccountKey;
use std::collections::HashMap;

/// One account's preparation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Prepare {
    /// A client starts when a worker ends; `workers` are running.
    Wanted { workers: u32 },
    /// The account was removed, stopped or switched to meanwhile: nothing
    /// starts, but `workers` still run.
    Cancelled { workers: u32 },
    /// The disk is below the open floor: the start was refused. A reading
    /// with room asks again (`take_refused`). `workers` still run (a cancel
    /// and a new begin left more than one); none usually.
    DiskRefused { workers: u32 },
}

/// What the preparations of all accounts are doing.
#[derive(Debug, Default)]
pub(super) struct PrepareSet {
    accounts: HashMap<AccountKey, Prepare>,
}

impl PrepareSet {
    /// A worker prepares `key`, and a client starts from it.
    pub(super) fn begin(&mut self, key: &AccountKey) {
        let workers = match self.accounts.get(key) {
            Some(
                Prepare::Wanted { workers }
                | Prepare::Cancelled { workers }
                | Prepare::DiskRefused { workers },
            ) => *workers,
            None => 0,
        };
        self.accounts.insert(
            key.clone(),
            Prepare::Wanted {
                workers: workers.saturating_add(1),
            },
        );
    }

    /// A client is wanted for `key`, so the account is not started twice.
    pub(super) fn wanted(&self, key: &AccountKey) -> bool {
        matches!(self.accounts.get(key), Some(Prepare::Wanted { .. }))
    }

    pub(super) fn any_wanted(&self) -> bool {
        self.accounts
            .values()
            .any(|prepare| matches!(prepare, Prepare::Wanted { .. }))
    }

    /// A worker for `key` is running, wanted or not.
    pub(super) fn running(&self, key: &AccountKey) -> bool {
        matches!(
            self.accounts.get(key),
            Some(
                Prepare::Wanted { .. }
                    | Prepare::Cancelled { .. }
                    | Prepare::DiskRefused { workers: 1.. }
            )
        )
    }

    /// Every account with a worker running or a start wanted.
    pub(super) fn keys(&self) -> impl Iterator<Item = &AccountKey> {
        self.accounts
            .iter()
            .filter(|(_, prepare)| !matches!(prepare, Prepare::DiskRefused { workers: 0 }))
            .map(|(key, _)| key)
    }

    /// The open floor refused `key`'s start, which waits for a reading with
    /// room. An account whose start is wanted again keeps that; workers
    /// still running stay counted.
    pub(super) fn refuse(&mut self, key: &AccountKey) {
        match self.accounts.get(key).copied() {
            Some(Prepare::Wanted { .. } | Prepare::DiskRefused { .. }) => {}
            Some(Prepare::Cancelled { workers }) => {
                self.accounts
                    .insert(key.clone(), Prepare::DiskRefused { workers });
            }
            None => {
                self.accounts
                    .insert(key.clone(), Prepare::DiskRefused { workers: 0 });
            }
        }
    }

    pub(super) fn any_refused(&self) -> bool {
        self.accounts
            .values()
            .any(|prepare| matches!(prepare, Prepare::DiskRefused { .. }))
    }

    /// A reading with room: every refused start leaves its stage, and the
    /// pass that follows asks again (and refuses again what is still short).
    pub(super) fn take_refused(&mut self) {
        self.accounts.retain(|_, prepare| match *prepare {
            Prepare::DiskRefused { workers: 0 } => false,
            Prepare::DiskRefused { workers } => {
                *prepare = Prepare::Cancelled { workers };
                true
            }
            _ => true,
        });
    }

    /// No client starts for `key` any more; its workers run on.
    pub(super) fn cancel(&mut self, key: &AccountKey) {
        match self.accounts.get(key).copied() {
            Some(Prepare::Wanted { workers }) => {
                self.accounts
                    .insert(key.clone(), Prepare::Cancelled { workers });
            }
            Some(Prepare::DiskRefused { workers }) => {
                if workers == 0 {
                    self.accounts.remove(key);
                } else {
                    self.accounts
                        .insert(key.clone(), Prepare::Cancelled { workers });
                }
            }
            Some(Prepare::Cancelled { .. }) | None => {}
        }
    }

    /// Every start is cancelled.
    pub(super) fn cancel_all(&mut self) {
        self.take_refused();
        for prepare in self.accounts.values_mut() {
            if let Prepare::Wanted { workers } = *prepare {
                *prepare = Prepare::Cancelled { workers };
            }
        }
    }

    /// A worker for `key` ended. `true` when a client was wanted: the
    /// first worker to end takes the start, a later one finds none.
    pub(super) fn finished(&mut self, key: &AccountKey) -> bool {
        let (wanted, workers) = match self.accounts.get(key).copied() {
            Some(Prepare::Wanted { workers }) => (true, workers),
            Some(Prepare::Cancelled { workers }) => (false, workers),
            Some(Prepare::DiskRefused { workers }) => {
                // The refusal stays (a reading with room asks again); only
                // the worker count goes down.
                self.accounts.insert(
                    key.clone(),
                    Prepare::DiskRefused {
                        workers: workers.saturating_sub(1),
                    },
                );
                return false;
            }
            None => return false,
        };
        match workers.saturating_sub(1) {
            0 => {
                self.accounts.remove(key);
            }
            left => {
                self.accounts
                    .insert(key.clone(), Prepare::Cancelled { workers: left });
            }
        }
        wanted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(name: &str) -> AccountKey {
        AccountKey(name.to_string())
    }

    #[test]
    fn the_first_worker_to_end_takes_the_start() {
        let mut set = PrepareSet::default();
        set.begin(&key("a"));
        set.begin(&key("a"));
        assert!(set.wanted(&key("a")) && set.running(&key("a")));
        assert!(set.finished(&key("a")), "the start goes to the first");
        assert!(!set.wanted(&key("a")) && set.running(&key("a")));
        assert!(!set.finished(&key("a")), "a later one finds none");
        assert!(!set.running(&key("a")));
    }

    #[test]
    fn a_cancelled_start_keeps_blocking_deletion_until_its_worker_ends() {
        let mut set = PrepareSet::default();
        set.begin(&key("a"));
        set.begin(&key("b"));
        set.cancel(&key("a"));
        assert!(!set.wanted(&key("a")) && set.running(&key("a")));
        assert!(set.any_wanted());
        set.cancel_all();
        assert!(!set.any_wanted());
        assert!(set.running(&key("b")));
        assert!(!set.finished(&key("a")) && !set.finished(&key("b")));
        assert_eq!(set.keys().count(), 0);
    }

    #[test]
    fn a_refused_start_waits_without_blocking_deletion_and_leaves_on_a_retry() {
        let mut set = PrepareSet::default();
        set.refuse(&key("a"));
        assert!(set.any_refused());
        assert!(!set.wanted(&key("a")), "not a start in flight");
        assert!(!set.running(&key("a")), "no worker: nothing to wait for");
        assert_eq!(set.keys().count(), 0);
        set.take_refused();
        assert!(!set.any_refused());
        // A stopped account forgets its refusal.
        set.refuse(&key("b"));
        set.cancel(&key("b"));
        assert!(!set.any_refused());
        // A worker still running keeps its stage.
        set.begin(&key("c"));
        set.refuse(&key("c"));
        assert!(set.wanted(&key("c")) && !set.any_refused());
        // A cancel and a re-begin left two workers; the first one's start
        // is refused: the refusal is kept while the second still runs.
        set.begin(&key("d"));
        set.cancel(&key("d"));
        set.begin(&key("d"));
        set.cancel(&key("d"));
        assert!(!set.finished(&key("d")), "cancelled: no start");
        set.refuse(&key("d"));
        assert!(set.any_refused() && set.running(&key("d")));
        assert!(!set.finished(&key("d")));
        assert!(set.any_refused(), "the refusal outlives the last worker");
        assert!(!set.running(&key("d")));
        set.take_refused();
        assert!(!set.any_refused());
        assert_eq!(set.keys().filter(|k| k.0 == "d").count(), 0);
    }
}
