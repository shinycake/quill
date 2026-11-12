//! The accounts whose background client is being prepared. Reading an
//! account's database key runs on a worker; a client starts from it only if
//! the account is still wanted when it ends, and nothing may delete the
//! account's folders while a worker still runs, wanted or not.

use gpui_kit::Task;
use quill::ids::AccountKey;
use std::collections::HashMap;
use std::sync::Arc;

/// One preparation running for an account. The token names it: the worker
/// hands it back when it ends, so the stage drops exactly that one. The
/// task is the worker's continuation, held here for as long as it runs.
#[derive(Debug)]
struct Worker {
    token: Arc<()>,
    task: Option<Task<()>>,
}

/// The preparations running for one account.
#[derive(Debug, Default)]
struct Workers(Vec<Worker>);

impl Workers {
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Drop the worker that `token` names.
    fn end(&mut self, token: &Arc<()>) {
        self.0.retain(|worker| !Arc::ptr_eq(&worker.token, token));
    }
}

/// One account's preparation.
#[derive(Debug)]
enum Prepare {
    /// A client starts when a worker ends; the `workers` are running.
    Wanted { workers: Workers },
    /// The account was removed, stopped or switched to meanwhile: nothing
    /// starts, but the `workers` still run.
    Cancelled { workers: Workers },
    /// The disk is below the open floor: the start was refused. A reading
    /// with room asks again (`take_refused`). The `workers` still run (a
    /// cancel and a new begin left more than one); none usually.
    DiskRefused { workers: Workers },
}

impl Prepare {
    fn workers(&self) -> &Workers {
        match self {
            Self::Wanted { workers }
            | Self::Cancelled { workers }
            | Self::DiskRefused { workers } => workers,
        }
    }

    fn into_workers(self) -> Workers {
        match self {
            Self::Wanted { workers }
            | Self::Cancelled { workers }
            | Self::DiskRefused { workers } => workers,
        }
    }
}

/// What the preparations of all accounts are doing.
#[derive(Debug, Default)]
pub(super) struct PrepareSet {
    accounts: HashMap<AccountKey, Prepare>,
}

impl PrepareSet {
    /// A worker prepares `key`, and a client starts from it. The returned
    /// token names the worker: it is handed back to [`Self::finished`], and
    /// the worker's task is given to [`Self::hold`].
    pub(super) fn begin(&mut self, key: &AccountKey) -> Arc<()> {
        let token = Arc::new(());
        let mut workers = self
            .accounts
            .remove(key)
            .map(Prepare::into_workers)
            .unwrap_or_default();
        workers.0.push(Worker {
            token: token.clone(),
            task: None,
        });
        self.accounts
            .insert(key.clone(), Prepare::Wanted { workers });
        token
    }

    /// The worker `token` names runs as `task`, which the stage keeps until
    /// the worker ends.
    pub(super) fn hold(&mut self, key: &AccountKey, token: &Arc<()>, task: Task<()>) {
        let Some(prepare) = self.accounts.get_mut(key) else {
            return;
        };
        let (Prepare::Wanted { workers }
        | Prepare::Cancelled { workers }
        | Prepare::DiskRefused { workers }) = prepare;
        if let Some(worker) = workers
            .0
            .iter_mut()
            .find(|worker| Arc::ptr_eq(&worker.token, token))
        {
            worker.task = Some(task);
        }
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
        self.accounts
            .get(key)
            .is_some_and(|prepare| !prepare.workers().is_empty())
    }

    /// Every account with a worker running or a start wanted.
    pub(super) fn keys(&self) -> impl Iterator<Item = &AccountKey> {
        self.accounts
            .iter()
            .filter(|(_, prepare)| {
                !matches!(prepare, Prepare::DiskRefused { workers } if workers.is_empty())
            })
            .map(|(key, _)| key)
    }

    /// The open floor refused `key`'s start, which waits for a reading with
    /// room. An account whose start is wanted again keeps that; workers
    /// still running stay counted.
    pub(super) fn refuse(&mut self, key: &AccountKey) {
        match self.accounts.remove(key) {
            Some(prepare @ (Prepare::Wanted { .. } | Prepare::DiskRefused { .. })) => {
                self.accounts.insert(key.clone(), prepare);
            }
            Some(Prepare::Cancelled { workers }) => {
                self.accounts
                    .insert(key.clone(), Prepare::DiskRefused { workers });
            }
            None => {
                self.accounts.insert(
                    key.clone(),
                    Prepare::DiskRefused {
                        workers: Workers::default(),
                    },
                );
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
        let before = std::mem::take(&mut self.accounts);
        for (key, prepare) in before {
            match prepare {
                Prepare::DiskRefused { workers } if workers.is_empty() => {}
                Prepare::DiskRefused { workers } => {
                    self.accounts.insert(key, Prepare::Cancelled { workers });
                }
                other => {
                    self.accounts.insert(key, other);
                }
            }
        }
    }

    /// No client starts for `key` any more; its workers run on.
    pub(super) fn cancel(&mut self, key: &AccountKey) {
        match self.accounts.remove(key) {
            Some(Prepare::Wanted { workers }) => {
                self.accounts
                    .insert(key.clone(), Prepare::Cancelled { workers });
            }
            Some(Prepare::DiskRefused { workers }) => {
                if !workers.is_empty() {
                    self.accounts
                        .insert(key.clone(), Prepare::Cancelled { workers });
                }
            }
            Some(other @ Prepare::Cancelled { .. }) => {
                self.accounts.insert(key.clone(), other);
            }
            None => {}
        }
    }

    /// Every start is cancelled.
    pub(super) fn cancel_all(&mut self) {
        self.take_refused();
        let before = std::mem::take(&mut self.accounts);
        for (key, prepare) in before {
            let prepare = match prepare {
                Prepare::Wanted { workers } => Prepare::Cancelled { workers },
                other => other,
            };
            self.accounts.insert(key, prepare);
        }
    }

    /// The worker `token` names ended for `key`. `true` when a client was
    /// wanted: the first worker to end takes the start, a later one finds
    /// none.
    pub(super) fn finished(&mut self, key: &AccountKey, token: &Arc<()>) -> bool {
        let Some(prepare) = self.accounts.remove(key) else {
            return false;
        };
        match prepare {
            Prepare::Wanted { mut workers } => {
                workers.end(token);
                if !workers.is_empty() {
                    self.accounts
                        .insert(key.clone(), Prepare::Cancelled { workers });
                }
                true
            }
            Prepare::Cancelled { mut workers } => {
                workers.end(token);
                if !workers.is_empty() {
                    self.accounts
                        .insert(key.clone(), Prepare::Cancelled { workers });
                }
                false
            }
            Prepare::DiskRefused { mut workers } => {
                // The refusal stays (a reading with room asks again); only
                // the worker goes.
                workers.end(token);
                self.accounts
                    .insert(key.clone(), Prepare::DiskRefused { workers });
                false
            }
        }
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
        let first = set.begin(&key("a"));
        let second = set.begin(&key("a"));
        assert!(set.wanted(&key("a")) && set.running(&key("a")));
        assert!(
            set.finished(&key("a"), &first),
            "the start goes to the first"
        );
        assert!(!set.wanted(&key("a")) && set.running(&key("a")));
        assert!(!set.finished(&key("a"), &second), "a later one finds none");
        assert!(!set.running(&key("a")));
    }

    #[test]
    fn a_cancelled_start_keeps_blocking_deletion_until_its_worker_ends() {
        let mut set = PrepareSet::default();
        let a = set.begin(&key("a"));
        let b = set.begin(&key("b"));
        set.cancel(&key("a"));
        assert!(!set.wanted(&key("a")) && set.running(&key("a")));
        assert!(set.any_wanted());
        set.cancel_all();
        assert!(!set.any_wanted());
        assert!(set.running(&key("b")));
        assert!(!set.finished(&key("a"), &a) && !set.finished(&key("b"), &b));
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
        let _c = set.begin(&key("c"));
        set.refuse(&key("c"));
        assert!(set.wanted(&key("c")) && !set.any_refused());
        // A cancel and a re-begin left two workers; the first one's start
        // is refused: the refusal is kept while the second still runs.
        let d1 = set.begin(&key("d"));
        set.cancel(&key("d"));
        let d2 = set.begin(&key("d"));
        set.cancel(&key("d"));
        assert!(!set.finished(&key("d"), &d1), "cancelled: no start");
        set.refuse(&key("d"));
        assert!(set.any_refused() && set.running(&key("d")));
        assert!(!set.finished(&key("d"), &d2));
        assert!(set.any_refused(), "the refusal outlives the last worker");
        assert!(!set.running(&key("d")));
        set.take_refused();
        assert!(!set.any_refused());
        assert_eq!(set.keys().filter(|k| k.0 == "d").count(), 0);
    }
}
