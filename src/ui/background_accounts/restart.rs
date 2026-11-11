//! When a background client that died or failed to start is started again:
//! after a delay that doubles with every failure in a row, so a client that
//! cannot run is not restarted in a tight loop. The poll asks
//! [`Restarts::due_in`]; reaching `Ready` forgets the failures.
use quill::ids::AccountKey;
use std::collections::HashMap;
use std::time::{Duration, Instant};

const FIRST: Duration = Duration::from_secs(5);
const LONGEST: Duration = Duration::from_secs(300);

#[derive(Default)]
pub(super) struct Restarts {
    /// Failures in a row and the instant the next start may happen.
    failed: HashMap<AccountKey, (u32, Instant)>,
}

impl Restarts {
    /// `key`'s client failed at `now`: its next start waits longer.
    pub(super) fn failed(&mut self, key: &AccountKey, now: Instant) {
        let count = self.failed.get(key).map_or(0, |(count, _)| *count);
        let delay = FIRST
            .saturating_mul(1u32.checked_shl(count).unwrap_or(u32::MAX))
            .min(LONGEST);
        self.failed
            .insert(key.clone(), (count.saturating_add(1), now + delay));
    }

    /// `key`'s client reached `Ready`, or the account is gone.
    pub(super) fn forget(&mut self, key: &AccountKey) {
        self.failed.remove(key);
    }

    /// How long until `key` may start again; zero when it may now.
    pub(super) fn due_in(&self, key: &AccountKey, now: Instant) -> Duration {
        self.failed
            .get(key)
            .map_or(Duration::ZERO, |(_, at)| at.saturating_duration_since(now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> AccountKey {
        AccountKey::primary()
    }

    #[test]
    fn the_delay_doubles_with_every_failure_in_a_row_and_resets_on_ready() {
        let t0 = Instant::now();
        let mut restarts = Restarts::default();
        assert_eq!(restarts.due_in(&key(), t0), Duration::ZERO);
        restarts.failed(&key(), t0);
        assert_eq!(restarts.due_in(&key(), t0), Duration::from_secs(5));
        assert_eq!(
            restarts.due_in(&key(), t0 + Duration::from_secs(5)),
            Duration::ZERO
        );
        restarts.failed(&key(), t0);
        assert_eq!(restarts.due_in(&key(), t0), Duration::from_secs(10));
        for _ in 0..40 {
            restarts.failed(&key(), t0);
        }
        assert_eq!(restarts.due_in(&key(), t0), LONGEST);
        restarts.forget(&key());
        assert_eq!(restarts.due_in(&key(), t0), Duration::ZERO);
    }
}
